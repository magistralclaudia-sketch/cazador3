use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    system_instruction,
    transaction::Transaction,
};
use std::str::FromStr;
use tracing::{error, info, warn};

/// Jito Bundle Sender para ejecución ultra-rápida
///
/// Jito Labs provee acceso directo a los block builders de Solana,
/// garantizando que tus transacciones se incluyan en el próximo bloque.
///
/// Ventajas:
/// - Latencia mínima (~100-300ms vs ~1-2s normal)
/// - Garantía de inclusión si pagas el tip
/// - Sin competencia en mempool público
pub struct JitoBundleSender {
    jito_endpoint: String,
    tip_account: Pubkey,
    min_tip_lamports: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct BundleRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BundleResponse {
    jsonrpc: String,
    id: u64,
    result: Option<String>,
    error: Option<BundleError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BundleError {
    code: i32,
    message: String,
}

impl JitoBundleSender {
    /// Crear nuevo bundle sender
    ///
    /// Endpoints de Jito (mainnet-beta):
    /// - NY: https://ny.mainnet.block-engine.jito.wtf
    /// - Amsterdam: https://amsterdam.mainnet.block-engine.jito.wtf
    /// - Frankfurt: https://frankfurt.mainnet.block-engine.jito.wtf
    /// - Tokyo: https://tokyo.mainnet.block-engine.jito.wtf
    pub fn new(jito_endpoint: String, min_tip_lamports: u64) -> Self {
        // Tip accounts de Jito (rotan entre estos 8)
        // https://jito-labs.gitbook.io/mev/searcher-resources/tips
        let tip_accounts = [
            "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
            "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
            "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
            "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
            "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
            "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
            "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
            "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
        ];

        // Usar la primera por defecto
        let tip_account = Pubkey::from_str(tip_accounts[0])
            .expect("Invalid tip account");

        Self {
            jito_endpoint,
            tip_account,
            min_tip_lamports,
        }
    }

    /// Enviar bundle con transacción de swap + tip
    ///
    /// El bundle garantiza que ambas transacciones se ejecutan juntas:
    /// 1. Tu swap
    /// 2. Tip a Jito
    ///
    /// IMPORTANTE: El bundle se ejecuta atómicamente - si el swap falla, no pagas el tip.
    pub async fn send_bundle(
        &self,
        swap_tx: &Transaction,
        tip_tx: &Transaction,
    ) -> Result<String> {
        info!("📦 Enviando Jito bundle con 2 transacciones");

        // Serializar ambas transacciones a base58
        let swap_tx_base58 = bs58::encode(
            bincode::serialize(swap_tx)
                .context("Failed to serialize swap transaction")?
        ).into_string();

        let tip_tx_base58 = bs58::encode(
            bincode::serialize(tip_tx)
                .context("Failed to serialize tip transaction")?
        ).into_string();

        // Crear request JSON-RPC
        // IMPORTANTE: El tip DEBE estar en el bundle para que Jito lo procese
        let request = BundleRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "sendBundle".to_string(),
            params: vec![vec![swap_tx_base58, tip_tx_base58]],
        };

        // Endpoint correcto de Jito: /api/v1/bundles
        let api_url = format!("{}/api/v1/bundles", self.jito_endpoint);

        // Enviar via HTTP POST
        let client = reqwest::Client::new();
        let response = client
            .post(&api_url)
            .json(&request)
            .send()
            .await
            .context("Failed to send bundle to Jito")?;

        // Verificar status code
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!(
                "Jito bundle failed with status {}: {}",
                status,
                text
            ));
        }

        let bundle_response: BundleResponse = response
            .json()
            .await
            .context("Failed to parse Jito response")?;

        // Verificar resultado
        if let Some(error) = bundle_response.error {
            return Err(anyhow::anyhow!(
                "Jito bundle failed: {} (code: {})",
                error.message,
                error.code
            ));
        }

        let bundle_id = bundle_response.result
            .context("No bundle ID in response")?;

        info!("✅ Bundle enviado exitosamente: {}", bundle_id);
        Ok(bundle_id)
    }

    /// Crear transacción de tip firmada
    ///
    /// Esta transacción transfiere SOL a la cuenta de tips de Jito.
    /// IMPORTANTE: El tip es lo que paga por la prioridad en Jito (no los priority fees normales).
    pub fn create_tip_transaction(
        &self,
        payer_keypair: &solana_sdk::signature::Keypair,
        recent_blockhash: solana_sdk::hash::Hash,
        tip_lamports: u64,
    ) -> Transaction {
        use solana_sdk::signer::Signer;

        let tip_ix = system_instruction::transfer(
            &payer_keypair.pubkey(),
            &self.tip_account,
            tip_lamports,
        );

        Transaction::new_signed_with_payer(
            &[tip_ix],
            Some(&payer_keypair.pubkey()),
            &[payer_keypair],
            recent_blockhash,
        )
    }

    /// Calcular tip dinámico basado en prioridad
    ///
    /// Tips recomendados:
    /// - Low: 1,000 lamports (0.000001 SOL)
    /// - Medium: 10,000 lamports (0.00001 SOL)
    /// - High: 100,000 lamports (0.0001 SOL)
    /// - Ultra: 1,000,000+ lamports (0.001+ SOL)
    pub fn calculate_tip(priority: TipPriority) -> u64 {
        match priority {
            TipPriority::Low => 1_000,
            TipPriority::Medium => 10_000,
            TipPriority::High => 100_000,
            TipPriority::Ultra => 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TipPriority {
    Low,
    Medium,
    High,
    Ultra,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tip_calculation() {
        assert_eq!(JitoBundleSender::calculate_tip(TipPriority::Low), 1_000);
        assert_eq!(JitoBundleSender::calculate_tip(TipPriority::Medium), 10_000);
        assert_eq!(JitoBundleSender::calculate_tip(TipPriority::High), 100_000);
        assert_eq!(JitoBundleSender::calculate_tip(TipPriority::Ultra), 1_000_000);
    }

    #[test]
    fn test_jito_sender_creation() {
        let sender = JitoBundleSender::new(
            "https://ny.mainnet.block-engine.jito.wtf".to_string(),
            10_000,
        );
        assert_eq!(sender.min_tip_lamports, 10_000);
    }
}
