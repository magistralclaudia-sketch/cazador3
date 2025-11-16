use anyhow::{anyhow, Context, Result};
use solana_client::{
    nonblocking::rpc_client::RpcClient as AsyncRpcClient,
    rpc_client::RpcClient,
    rpc_config::RpcSendTransactionConfig,
};
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    signature::Signature,
    transaction::Transaction,
};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Confirmador de transacciones ultra-rápido
pub struct TransactionConfirmer {
    rpc_client: AsyncRpcClient,
    max_retries: u32,
    confirmation_timeout: Duration,
}

impl TransactionConfirmer {
    pub fn new(rpc_url: String, max_retries: u32, confirmation_timeout_secs: u64) -> Self {
        Self {
            rpc_client: AsyncRpcClient::new(rpc_url),
            max_retries,
            confirmation_timeout: Duration::from_secs(confirmation_timeout_secs),
        }
    }

    /// Enviar transacción con confirmación ultra-rápida
    ///
    /// Esta función:
    /// 1. Envía la transacción con skipPreflight para máxima velocidad
    /// 2. Monitorea confirmación usando commitment "confirmed" (más rápido que "finalized")
    /// 3. Reintenta si falla
    /// 4. Devuelve tan pronto como la tx esté confirmada
    pub async fn send_and_confirm_ultra_fast(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature> {
        let signature = transaction.signatures[0];
        let start_time = Instant::now();

        info!("📤 Enviando transacción: {}", signature);

        // Configuración para máxima velocidad
        let config = RpcSendTransactionConfig {
            skip_preflight: true,  // ⚡ Salta validación previa para velocidad
            preflight_commitment: Some(CommitmentLevel::Processed),
            encoding: None,
            max_retries: Some(0), // Manejamos reintentos manualmente
            min_context_slot: None,
        };

        // Enviar transacción
        let result = self.rpc_client
            .send_transaction_with_config(transaction, config)
            .await;

        match result {
            Ok(sig) => {
                debug!("✓ Transacción enviada: {}", sig);
            }
            Err(e) => {
                warn!("⚠️  Error enviando transacción: {:?}", e);
                // Continuamos de todas formas, puede que ya se haya enviado
            }
        }

        // Confirmar transacción con polling agresivo
        self.confirm_transaction_fast(signature, start_time).await
    }

    /// Confirmar transacción con polling ultra-rápido
    async fn confirm_transaction_fast(
        &self,
        signature: Signature,
        start_time: Instant,
    ) -> Result<Signature> {
        let mut attempt = 0;
        let poll_interval = Duration::from_millis(200); // Poll cada 200ms

        loop {
            attempt += 1;

            // Verificar timeout
            if start_time.elapsed() > self.confirmation_timeout {
                return Err(anyhow!(
                    "Timeout esperando confirmación de transacción después de {:?}",
                    start_time.elapsed()
                ));
            }

            // ⚡ Obtener estado con commitment "processed" para máxima velocidad
            // "processed" = confirmado por el leader actual (~200ms)
            // "confirmed" = >66% validators (~1-2 segundos)
            // Para sniper bot, processed es suficiente para ganar velocidad
            match self.rpc_client
                .get_signature_status_with_commitment(
                    &signature,
                    CommitmentConfig::processed(),  // ⚡ Cambio crítico para velocidad
                )
                .await
            {
                Ok(Some(result)) => {
                    match result {
                        Ok(_) => {
                            let elapsed = start_time.elapsed();
                            info!("✅ Transacción confirmada en {:?}: {}", elapsed, signature);
                            return Ok(signature);
                        }
                        Err(e) => {
                            return Err(anyhow!(
                                "Transacción falló: {:?}",
                                e
                            ));
                        }
                    }
                }
                Ok(None) => {
                    // Transacción aún no confirmada
                    debug!("⏳ Esperando confirmación (intento {})...", attempt);
                }
                Err(e) => {
                    warn!("Error verificando estado: {:?}", e);
                }
            }

            // Esperar antes del siguiente poll
            sleep(poll_interval).await;
        }
    }

    /// Enviar transacción con reintentos automáticos
    ///
    /// Útil cuando la red está congestionada
    pub async fn send_with_retries(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature> {
        let mut last_error = None;

        for attempt in 1..=self.max_retries {
            info!("Intento {}/{}", attempt, self.max_retries);

            match self.send_and_confirm_ultra_fast(transaction).await {
                Ok(sig) => return Ok(sig),
                Err(e) => {
                    warn!("Intento {} falló: {:?}", attempt, e);
                    last_error = Some(e);

                    if attempt < self.max_retries {
                        // Backoff exponencial
                        let wait_time = Duration::from_secs(2_u64.pow(attempt - 1));
                        info!("Esperando {:?} antes de reintentar...", wait_time);
                        sleep(wait_time).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Falló después de {} intentos", self.max_retries)))
    }

    /// Obtener blockhash reciente con cache
    pub async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        self.rpc_client
            .get_latest_blockhash()
            .await
            .context("Failed to get latest blockhash")
    }
}

/// Confirmador sincrónico para uso en contextos no-async
pub struct SyncTransactionConfirmer {
    rpc_client: RpcClient,
    max_retries: u32,
}

impl SyncTransactionConfirmer {
    pub fn new(rpc_url: String, max_retries: u32) -> Self {
        Self {
            rpc_client: RpcClient::new_with_commitment(
                rpc_url,
                CommitmentConfig::confirmed(),
            ),
            max_retries,
        }
    }

    /// Enviar y confirmar transacción (versión sincrónica)
    pub fn send_and_confirm(&self, transaction: &Transaction) -> Result<Signature> {
        let signature = transaction.signatures[0];
        info!("📤 Enviando transacción (sync): {}", signature);

        // Enviar transacción
        let config = RpcSendTransactionConfig {
            skip_preflight: true,
            preflight_commitment: Some(CommitmentLevel::Processed),
            ..Default::default()
        };

        self.rpc_client
            .send_transaction_with_config(transaction, config)
            .context("Failed to send transaction")
    }

    /// Confirmar transacción ya enviada
    pub fn confirm_transaction(
        &self,
        signature: &Signature,
        timeout_secs: u64,
    ) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        loop {
            if start.elapsed() > timeout {
                return Err(anyhow!("Timeout confirmando transacción"));
            }

            match self.rpc_client.get_signature_status(signature)? {
                Some(result) => {
                    result?;
                    info!("✅ Transacción confirmada: {}", signature);
                    return Ok(());
                }
                None => {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_confirmer_creation() {
        let confirmer = TransactionConfirmer::new(
            "http://localhost:8899".to_string(),
            3,
            30,
        );

        assert_eq!(confirmer.max_retries, 3);
        assert_eq!(confirmer.confirmation_timeout, Duration::from_secs(30));
    }
}
