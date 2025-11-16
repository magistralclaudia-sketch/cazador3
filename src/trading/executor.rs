use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    system_instruction,
    transaction::Transaction,
};
use std::str::FromStr;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::meteora::{Pool, PriceCalculator};

pub struct TradeExecutor {
    config: Config,
    rpc_client: RpcClient,
    wallet: Keypair,
}

impl TradeExecutor {
    pub fn new(config: Config) -> Result<Self> {
        // Conectar a RPC
        let rpc_client = RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );

        // Cargar wallet
        let wallet = Self::load_wallet(&config.wallet_keypair_path)
            .context("Failed to load wallet")?;

        info!("💼 Wallet cargada: {}", wallet.pubkey());

        Ok(Self {
            config,
            rpc_client,
            wallet,
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

    /// Comprar tokens inmediatamente en un nuevo pool
    ///
    /// Esta función ejecuta el snipe (compra rápida)
    pub async fn snipe_buy(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
    ) -> Result<Signature> {
        info!("🎯 EJECUTANDO SNIPE BUY en pool {}", pool_address);

        let price = PriceCalculator::calculate_price(pool);
        info!("   Precio actual: {}", price);
        info!("   Monto: {} SOL", self.config.auto_buy_amount_sol);

        // Calcular cantidad de tokens a comprar
        let lamports_in = (self.config.auto_buy_amount_sol * 1_000_000_000.0) as u64;

        // Construir instrucciones de swap
        let swap_ix = self.build_swap_instruction(
            pool_address,
            pool,
            lamports_in,
            true, // SOL -> Token
        )?;

        // Agregar priority fee para ejecución más rápida
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
            self.config.priority_fee_lamports,
        );

        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(200_000);

        // Construir y enviar transacción
        let recent_blockhash = self.rpc_client.get_latest_blockhash()?;

        let tx = Transaction::new_signed_with_payer(
            &[compute_budget_ix, compute_limit_ix, swap_ix],
            Some(&self.wallet.pubkey()),
            &[&self.wallet],
            recent_blockhash,
        );

        info!("📤 Enviando transacción de compra...");

        let signature = self.rpc_client.send_and_confirm_transaction_with_spinner(&tx)?;

        info!("✅ Compra exitosa! Signature: {}", signature);

        Ok(signature)
    }

    /// Vender tokens (para take profit o stop loss)
    pub async fn sell(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
        amount: u64,
    ) -> Result<Signature> {
        info!("💰 Ejecutando venta en pool {}", pool_address);

        let swap_ix = self.build_swap_instruction(
            pool_address,
            pool,
            amount,
            false, // Token -> SOL
        )?;

        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
            self.config.priority_fee_lamports,
        );

        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(200_000);

        let recent_blockhash = self.rpc_client.get_latest_blockhash()?;

        let tx = Transaction::new_signed_with_payer(
            &[compute_budget_ix, compute_limit_ix, swap_ix],
            Some(&self.wallet.pubkey()),
            &[&self.wallet],
            recent_blockhash,
        );

        info!("📤 Enviando transacción de venta...");

        let signature = self.rpc_client.send_and_confirm_transaction_with_spinner(&tx)?;

        info!("✅ Venta exitosa! Signature: {}", signature);

        Ok(signature)
    }

    /// Construir instrucción de swap para Meteora DAMM V2
    ///
    /// NOTA: Esta es una implementación simplificada
    /// En producción, deberías usar el SDK oficial o construir
    /// la instrucción exacta según el IDL del programa
    fn build_swap_instruction(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
        amount_in: u64,
        is_a_to_b: bool,
    ) -> Result<Instruction> {
        // Program ID de Meteora DAMM V2
        let program_id = self.config.meteora_program_id;

        // Cuentas requeridas para swap en Meteora
        // IMPORTANTE: Esto es una estructura simplificada
        // Necesitarás ajustar según el IDL exacto de Meteora DAMM V2

        let accounts = vec![
            // Pool
            solana_sdk::instruction::AccountMeta::new(*pool_address, false),
            // User source token account
            solana_sdk::instruction::AccountMeta::new(self.wallet.pubkey(), true),
            // User destination token account
            solana_sdk::instruction::AccountMeta::new(self.wallet.pubkey(), false),
            // Pool token A vault
            solana_sdk::instruction::AccountMeta::new(pool.token_a_vault, false),
            // Pool token B vault
            solana_sdk::instruction::AccountMeta::new(pool.token_b_vault, false),
            // Token program
            solana_sdk::instruction::AccountMeta::new_readonly(
                spl_token::id(),
                false,
            ),
        ];

        // Construir data de instrucción
        // El discriminador de "swap" en Anchor suele ser el hash de "global:swap"
        // Los primeros 8 bytes identifican la instrucción

        let mut data = vec![
            0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8, // Discriminador de swap (ejemplo)
        ];

        // Serializar parámetros del swap
        // amount_in (u64)
        data.extend_from_slice(&amount_in.to_le_bytes());
        // minimum_amount_out (u64) - con slippage
        let min_out = self.calculate_min_amount_out(amount_in, pool, is_a_to_b);
        data.extend_from_slice(&min_out.to_le_bytes());

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Calcular mínimo amount out considerando slippage
    fn calculate_min_amount_out(&self, amount_in: u64, pool: &Pool, is_a_to_b: bool) -> u64 {
        let estimated_out = PriceCalculator::estimate_swap_output(pool, amount_in, is_a_to_b);

        // Aplicar slippage
        let slippage_multiplier = 1.0 - (self.config.max_slippage_bps as f64 / 10000.0);
        (estimated_out as f64 * slippage_multiplier) as u64
    }

    pub fn get_wallet_pubkey(&self) -> Pubkey {
        self.wallet.pubkey()
    }
}

// NOTA IMPORTANTE PARA EL USUARIO:
// ================================
// La función build_swap_instruction() es una implementación SIMPLIFICADA.
//
// Para que funcione en producción, necesitas:
// 1. Obtener el IDL del programa Meteora DAMM V2
// 2. Usar anchor-client o construir la instrucción exacta
// 3. Incluir todas las cuentas correctas (puede haber más de 6)
// 4. Usar el discriminador correcto de la instrucción
//
// Puedes obtener el IDL de:
// - https://github.com/MeteoraAg/damm-v2
// - O usando: anchor idl fetch <program_id>
//
// Alternativamente, puedes usar Jupiter Aggregator que soporta Meteora:
// - https://station.jup.ag/docs/apis/swap-api
