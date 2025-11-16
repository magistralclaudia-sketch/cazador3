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
use super::blockhash_cache::BlockhashCache;
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
    blockhash_cache: Arc<BlockhashCache>,
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

        // ⚡ Blockhash cache con auto-refresh cada 500ms
        let blockhash_cache = Arc::new(BlockhashCache::new(
            config.rpc_url.clone(),
            500, // Refresh cada 500ms
        ));

        // Iniciar background task de auto-refresh
        let cache_clone = blockhash_cache.clone();
        tokio::spawn(async move {
            cache_clone.auto_refresh_loop().await;
        });

        // Obtener primer blockhash
        blockhash_cache.force_refresh().await?;
        info!("✓ Blockhash cache inicializado con auto-refresh");

        Ok(Self {
            config,
            rpc_client,
            wallet,
            swap_builder,
            tx_confirmer,
            blockhash_cache,
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
    /// - Creación automática de ATA si no existe
    /// - Skip preflight para máxima velocidad
    /// - Priority fees altos
    /// - Confirmación agresiva
    /// - Construcción optimizada de instrucciones
    pub async fn snipe_buy(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
    ) -> Result<Signature> {
        // Calcular amounts (SIN logs para velocidad)
        let amount_in_lamports = (self.config.auto_buy_amount_sol * 1_000_000_000.0) as u64;
        let minimum_amount_out = self.calculate_min_amount_out(
            amount_in_lamports,
            pool,
            true, // SOL -> Token
            self.config.buy_slippage_bps, // 99% slippage para compra
        );

        // Obtener cuentas
        let user_sol_account = self.wallet.pubkey();
        let user_token_account = get_associated_token_address(
            &self.wallet.pubkey(),
            &pool.token_b_mint,
        );

        // ⚡ CRÍTICO: Verificar y crear ATA si no existe
        // Esto DEBE hacerse antes del swap o fallará
        self.ensure_ata_exists(&pool.token_b_mint).await?;

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

        // Ejecutar transacción ultra-rápida
        self.execute_swap_transaction(swap_ix, "COMPRA").await
    }

    /// Asegurar que la Associated Token Account existe
    ///
    /// Si no existe, la crea en una transacción separada.
    /// Esto es CRÍTICO para que el swap no falle.
    async fn ensure_ata_exists(&self, mint: &Pubkey) -> Result<()> {
        let ata = get_associated_token_address(&self.wallet.pubkey(), mint);

        // Verificar si ya existe
        match self.rpc_client.get_account(&ata).await {
            Ok(_) => {
                // ATA existe, todo bien
                Ok(())
            }
            Err(_) => {
                // ATA no existe, necesitamos crearla
                // IMPORTANTE: Esto agrega ~500ms, pero es necesario

                let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
                    &self.wallet.pubkey(),  // payer
                    &self.wallet.pubkey(),  // wallet
                    mint,                   // mint
                    &spl_token::id(),       // token program
                );

                // ⚡ Obtener blockhash del cache (ultra rápido)
                let blockhash = self.blockhash_cache.get_blockhash().await?;

                // Crear y firmar transacción
                let create_tx = Transaction::new_signed_with_payer(
                    &[create_ata_ix],
                    Some(&self.wallet.pubkey()),
                    &[&*self.wallet],
                    blockhash,
                );

                // Enviar y confirmar
                self.tx_confirmer
                    .send_and_confirm_ultra_fast(&create_tx)
                    .await
                    .context("Failed to create ATA")?;

                Ok(())
            }
        }
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
            self.config.sell_slippage_bps, // 40% slippage para venta
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

        // ⚡ Obtener blockhash del cache (ahorra 10-50ms)
        let recent_blockhash = self.blockhash_cache.get_blockhash().await?;

        // Construir transacción
        let tx = Transaction::new_signed_with_payer(
            &[compute_budget_ix, compute_limit_ix, swap_ix],
            Some(&self.wallet.pubkey()),
            &[&*self.wallet],
            recent_blockhash,
        );

        // ⚡ Enviar y confirmar con máxima velocidad (sin logs para velocidad)
        let signature = self.tx_confirmer
            .send_with_retries(&tx)
            .await
            .context(format!("Failed to execute {}", operation_name))?;

        Ok(signature)
    }

    /// Calcular minimum amount out con slippage protection
    fn calculate_min_amount_out(&self, amount_in: u64, pool: &Pool, is_a_to_b: bool, slippage_bps: u16) -> u64 {
        // Estimar output usando precio del pool
        let estimated_out = PriceCalculator::estimate_swap_output(pool, amount_in, is_a_to_b);

        // Aplicar slippage tolerance
        let slippage_multiplier = 1.0 - (slippage_bps as f64 / 10_000.0);
        let min_out = (estimated_out as f64 * slippage_multiplier) as u64;

        info!("   📈 Estimated output: {}", estimated_out);
        info!("   📉 Min output ({}% slippage): {}",
            slippage_bps as f64 / 100.0,
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
