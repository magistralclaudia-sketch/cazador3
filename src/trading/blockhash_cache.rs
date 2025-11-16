use anyhow::{Context, Result};
use solana_client::nonblocking::rpc_client::RpcClient as AsyncRpcClient;
use solana_sdk::hash::Hash;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info};

/// Cache de blockhash con auto-refresh para máxima velocidad
///
/// En lugar de pedir un blockhash fresco en cada transacción (10-50ms),
/// mantenemos uno en cache y lo refrescamos en background cada 500ms.
///
/// Ahorro: 10-50ms por transacción
pub struct BlockhashCache {
    cache: Arc<RwLock<(Hash, Instant)>>,
    rpc_client: Arc<AsyncRpcClient>,
    refresh_interval: Duration,
}

impl BlockhashCache {
    pub fn new(rpc_url: String, refresh_interval_ms: u64) -> Self {
        let rpc_client = Arc::new(AsyncRpcClient::new(rpc_url));

        Self {
            cache: Arc::new(RwLock::new((Hash::default(), Instant::now()))),
            rpc_client,
            refresh_interval: Duration::from_millis(refresh_interval_ms),
        }
    }

    /// Obtener blockhash del cache
    ///
    /// Si el blockhash tiene menos de 1 segundo, se usa del cache (ultra rápido).
    /// Si es más viejo, se obtiene uno nuevo.
    pub async fn get_blockhash(&self) -> Result<Hash> {
        // Intentar leer del cache
        {
            let (hash, timestamp) = *self.cache.read().unwrap();

            // Si es fresco (< 1 segundo), usarlo
            if timestamp.elapsed() < Duration::from_secs(1) && hash != Hash::default() {
                debug!("⚡ Blockhash del cache ({}ms)", timestamp.elapsed().as_millis());
                return Ok(hash);
            }
        }

        // Necesitamos uno nuevo
        debug!("🔄 Obteniendo blockhash fresco del RPC...");
        let new_hash = self.rpc_client
            .get_latest_blockhash()
            .await
            .context("Failed to get latest blockhash")?;

        // Actualizar cache
        {
            let mut cache = self.cache.write().unwrap();
            *cache = (new_hash, Instant::now());
        }

        Ok(new_hash)
    }

    /// Background task que refresca el blockhash automáticamente
    ///
    /// Llamar esto en un tokio::spawn() al inicio del bot.
    pub async fn auto_refresh_loop(self: Arc<Self>) {
        info!("🔄 Iniciando auto-refresh de blockhash cada {}ms", self.refresh_interval.as_millis());

        loop {
            sleep(self.refresh_interval).await;

            match self.rpc_client.get_latest_blockhash().await {
                Ok(hash) => {
                    let mut cache = self.cache.write().unwrap();
                    *cache = (hash, Instant::now());
                    debug!("✓ Blockhash actualizado en background");
                }
                Err(e) => {
                    debug!("⚠️ Error refrescando blockhash: {:?}", e);
                }
            }
        }
    }

    /// Forzar refresh inmediato
    pub async fn force_refresh(&self) -> Result<Hash> {
        let new_hash = self.rpc_client
            .get_latest_blockhash()
            .await
            .context("Failed to get latest blockhash")?;

        {
            let mut cache = self.cache.write().unwrap();
            *cache = (new_hash, Instant::now());
        }

        Ok(new_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_blockhash_cache_creation() {
        let cache = BlockhashCache::new(
            "http://localhost:8899".to_string(),
            500,
        );

        assert_eq!(cache.refresh_interval, Duration::from_millis(500));
    }
}
