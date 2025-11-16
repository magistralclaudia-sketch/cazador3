use anyhow::{Context, Result};
use solana_client::nonblocking::rpc_client::RpcClient as AsyncRpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::meteora::{Pool, PriceCalculator};
use super::swap_builder::{SwapInstructionBuilder, get_associated_token_address};
use super::transaction_confirmer::TransactionConfirmer;

/// Executor de trades ultra-rápido para Meteora DAMM V2
///
/// Features:
/// - Async/await para máxima performance
/// - Confirmación ultra-rápida de transacciones
/// - Construcción correcta de instrucciones de swap
/// - Priority fees configurables
/// - Retry automático en fallos
pub struct TradeExecutor {
    config: Config,
    rpc_client: Arc<AsyncRpcClient>,
    wallet: Arc<Keypair>,
    swap_builder: SwapInstructionBuilder,
    tx_confirmer: TransactionConfirmer,
}

impl TradeExecutor {
    pub async fn new(config: Config) -> Result<Self> {
        // Cliente RPC async
        let rpc_client = Arc::new(AsyncRpcClient::new(config.rpc_url.clone()));

        // Cargar wallet
        let wallet = Arc::new(Self::load_wallet(&config.wallet_keypair_path)
            .context("Failed to load wallet")?);

        info!("💼 Wallet cargada: {}", wallet.pubkey());

        // Swap builder
        let swap_builder = SwapInstructionBuilder::new(config.meteora_program_id);

        // Transaction confirmer con configuración ultra-rápida
        let tx_confirmer = TransactionConfirmer::new(
            config.rpc_url.clone(),
            3,  // 3 reintentos
            30, // 30 segundos de timeout
        );

        Ok(Self {
            config,
            rpc_client,
            wallet,
            swap_builder,
            tx_confirmer,
        })
    }

    fn load_wallet(path: &str) -> Result<Keypair> {
        let data = std::fs::read_to_string(path)
            .context("Failed to read wallet file")?;

        let bytes: Vec<u8> = serde_json::from_str(&data)
            .context("Failed to parse wallet JSON")?;

        Keypair::from_bytes(&bytes)
            .context("Failed to create keypair from bytes")
    }

    /// 🎯 SNIPE BUY - Compra ultra-rápida en nuevo pool
    ///
    /// Optimizaciones:
    /// - Skip preflight para máxima velocidad
    /// - Priority fees altos
    /// - Confirmación agresiva
    /// - Construcción optimizada de instrucciones
    pub async fn snipe_buy(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
    ) -> Result<Signature> {
        info!("🎯 EJECUTANDO SNIPE BUY en pool {}", pool_address);

        let price = PriceCalculator::calculate_price(pool);
        info!("   💰 Precio actual: {:.6}", price);
        info!("   💵 Monto: {} SOL", self.config.auto_buy_amount_sol);

        // Calcular amounts
        let amount_in_lamports = (self.config.auto_buy_amount_sol * 1_000_000_000.0) as u64;
        let minimum_amount_out = self.calculate_min_amount_out(
            amount_in_lamports,
            pool,
            true, // SOL -> Token
        );

        info!("   📊 Amount in: {} lamports", amount_in_lamports);
        info!("   📉 Min amount out: {}", minimum_amount_out);

        // Obtener o crear cuentas de token
        let user_sol_account = self.wallet.pubkey();
        let user_token_account = get_associated_token_address(
            &self.wallet.pubkey(),
            &pool.token_b_mint, // Asumiendo que compramos token B con SOL (token A)
        );

        info!("   👤 User: {}", user_sol_account);
        info!("   🪙 Token account: {}", user_token_account);

        // TODO: Verificar si la cuenta de token existe, si no, crear ATA
        // Por ahora asumimos que existe o se creará automáticamente

        // Construir instrucción de swap
        let swap_ix = self.swap_builder.build_simple_swap_instruction(
            pool_address,
            pool,
            &self.wallet.pubkey(),
            &user_sol_account,
            &user_token_account,
            amount_in_lamports,
            minimum_amount_out,
        )?;

        // Construir y enviar transacción
        self.execute_swap_transaction(swap_ix, "COMPRA").await
    }

    /// 💸 SELL - Venta ultra-rápida
    pub async fn sell(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
        amount: u64,
    ) -> Result<Signature> {
        info!("💸 EJECUTANDO VENTA en pool {}", pool_address);
        info!("   📊 Cantidad: {}", amount);

        let minimum_amount_out = self.calculate_min_amount_out(
            amount,
            pool,
            false, // Token -> SOL
        );

        // Cuentas de token
        let user_token_account = get_associated_token_address(
            &self.wallet.pubkey(),
            &pool.token_b_mint,
        );
        let user_sol_account = self.wallet.pubkey();

        // Construir instrucción de swap
        let swap_ix = self.swap_builder.build_simple_swap_instruction(
            pool_address,
            pool,
            &self.wallet.pubkey(),
            &user_token_account,
            &user_sol_account,
            amount,
            minimum_amount_out,
        )?;

        // Ejecutar transacción
        self.execute_swap_transaction(swap_ix, "VENTA").await
    }

    /// Ejecutar transacción de swap con máxima velocidad
    async fn execute_swap_transaction(
        &self,
        swap_ix: Instruction,
        operation_name: &str,
    ) -> Result<Signature> {
        // Priority fees para ejecución rápida
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
            self.config.priority_fee_lamports,
        );

        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(300_000);

        // Obtener blockhash reciente
        let recent_blockhash = self.tx_confirmer.get_latest_blockhash().await?;

        // Construir transacción
        let tx = Transaction::new_signed_with_payer(
            &[compute_budget_ix, compute_limit_ix, swap_ix],
            Some(&self.wallet.pubkey()),
            &[&*self.wallet],
            recent_blockhash,
        );

        info!("📤 Enviando transacción de {}...", operation_name);
        info!("   ⚡ Priority fee: {} lamports", self.config.priority_fee_lamports);
        info!("   💻 Compute limit: 300,000 units");

        // Enviar y confirmar con máxima velocidad
        let signature = self.tx_confirmer
            .send_with_retries(&tx)
            .await
            .context(format!("Failed to execute {}", operation_name))?;

        info!("✅ {} exitosa! Signature: {}", operation_name, signature);

        Ok(signature)
    }

    /// Calcular minimum amount out con slippage protection
    fn calculate_min_amount_out(&self, amount_in: u64, pool: &Pool, is_a_to_b: bool) -> u64 {
        // Estimar output usando precio del pool
        let estimated_out = PriceCalculator::estimate_swap_output(pool, amount_in, is_a_to_b);

        // Aplicar slippage tolerance
        let slippage_multiplier = 1.0 - (self.config.max_slippage_bps as f64 / 10_000.0);
        let min_out = (estimated_out as f64 * slippage_multiplier) as u64;

        info!("   📈 Estimated output: {}", estimated_out);
        info!("   📉 Min output ({}% slippage): {}",
            self.config.max_slippage_bps as f64 / 100.0,
            min_out
        );

        min_out
    }

    /// Obtener balance de SOL de la wallet
    pub async fn get_sol_balance(&self) -> Result<u64> {
        self.rpc_client
            .get_balance(&self.wallet.pubkey())
            .await
            .context("Failed to get SOL balance")
    }

    /// Obtener balance de un token SPL
    pub async fn get_token_balance(&self, mint: &Pubkey) -> Result<u64> {
        let token_account = get_associated_token_address(
            &self.wallet.pubkey(),
            mint,
        );

        match self.rpc_client
            .get_token_account_balance(&token_account)
            .await
        {
            Ok(balance) => {
                Ok(balance.amount.parse::<u64>().unwrap_or(0))
            }
            Err(e) => {
                warn!("Error obteniendo balance de token: {:?}", e);
                Ok(0)
            }
        }
    }

    pub fn get_wallet_pubkey(&self) -> Pubkey {
        self.wallet.pubkey()
    }
}

// NOTAS IMPORTANTES PARA PRODUCCIÓN:
// ===================================
//
// 1. INSTRUCCIÓN DE SWAP:
//    La función build_simple_swap_instruction() es una implementación simplificada.
//    Para producción, obtén el IDL oficial:
//    ```bash
//    anchor idl fetch cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG
//    ```
//
// 2. CUENTAS DE TOKEN ASOCIADAS:
//    Este código asume que las ATAs ya existen. En producción, deberías:
//    - Verificar si existen
//    - Crear ATAs si no existen
//    - Manejar wrapped SOL correctamente
//
// 3. JUPITER AGGREGATOR (Alternativa Recomendada):
//    Para máxima confiabilidad, considera usar Jupiter que maneja Meteora:
//    ```toml
//    jupiter-swap-api-client = "1.0"
//    ```
//    Jupiter maneja automáticamente:
//    - Routing óptimo
//    - Todas las DEX incluyendo Meteora
//    - Slippage y fees
//    - Actualizaciones del programa
//
// 4. WRAPPING/UNWRAPPING DE SOL:
//    Si intercambias SOL, necesitas:
//    - Wrap SOL a wSOL para usar en SPL Token
//    - Unwrap después si quieres SOL nativo
//
// 5. PRIORIDAD DE FEES:
//    Para máxima velocidad durante congestión:
//    - Monitorea fees de red en tiempo real
//    - Ajusta priority_fee_lamports dinámicamente
//    - Considera usar 500,000+ durante alta congestión
