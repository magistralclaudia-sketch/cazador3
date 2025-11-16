use anyhow::{Context, Result};
use futures::StreamExt;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use yellowstone_grpc_client::{GeyserGrpcClient, Interceptor};
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof,
    CommitmentLevel,
    SubscribeRequest,
    SubscribeRequestFilterAccounts,
    SubscribeRequestFilterAccountsFilter,
    SubscribeUpdateAccount,
    subscribe_request_filter_accounts_filter,
};

use crate::config::Config;
use crate::meteora::{Pool, PoolInfo};

#[derive(Debug, Clone)]
pub enum PoolEvent {
    NewPool(PoolInfo),
    PoolUpdated(PoolInfo),
}

pub struct GeyserPoolMonitor {
    config: Config,
}

impl GeyserPoolMonitor {
    pub fn new(config: Config) -> Self {
        Self {
            config,
        }
    }

    /// Conectar al servidor Geyser gRPC y retornar el cliente
    async fn create_client(&self) -> Result<GeyserGrpcClient<impl Interceptor>> {
        info!("Conectando a Geyser gRPC: {}", self.config.geyser_endpoint);

        let client = GeyserGrpcClient::build_from_shared(self.config.geyser_endpoint.clone())?
            .x_token(self.config.geyser_x_token.clone())?
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(10))
            .max_decoding_message_size(1024 * 1024 * 1024)
            .connect()
            .await
            .context("Failed to connect to Geyser gRPC")?;

        Ok(client)
    }

    /// Conectar al servidor Geyser gRPC
    pub async fn connect(&mut self) -> Result<()> {
        let mut client = self.create_client().await?;

        // Verificar conexión con ping
        let _ = client
            .ping(1)
            .await
            .context("Geyser ping failed")?;

        info!("✓ Conectado a Geyser gRPC exitosamente");
        Ok(())
    }

    /// Iniciar monitoreo de nuevos pools de Meteora DAMM V2
    ///
    /// Retorna un canal donde se recibirán los eventos de pools
    pub async fn monitor_pools(
        &mut self,
    ) -> Result<mpsc::UnboundedReceiver<PoolEvent>> {
        // Crear un nuevo cliente
        let mut client = self.create_client().await?;

        let (tx, rx) = mpsc::unbounded_channel();

        info!("🔍 Iniciando monitoreo de pools Meteora DAMM V2...");
        info!("📊 Program ID: {}", self.config.meteora_program_id);

        // Crear subscripción a cuentas del programa Meteora
        let mut accounts_filter = HashMap::new();

        // Filtro para todas las cuentas del programa Meteora DAMM V2
        accounts_filter.insert(
            "meteora_pools".to_string(),
            SubscribeRequestFilterAccounts {
                account: vec![],
                owner: vec![self.config.meteora_program_id.to_string()],
                filters: vec![
                    // Filtrar por tamaño mínimo de cuenta (pools tienen cierto tamaño)
                    SubscribeRequestFilterAccountsFilter {
                        filter: Some(
                            subscribe_request_filter_accounts_filter::Filter::Datasize(
                                Pool::MIN_SIZE as u64
                            )
                        ),
                    },
                ],
            },
        );

        // Crear request de subscripción
        let request = SubscribeRequest {
            accounts: accounts_filter,
            slots: HashMap::new(),
            transactions: HashMap::new(),
            transactions_status: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta: HashMap::new(),
            entry: HashMap::new(),
            commitment: Some(CommitmentLevel::Confirmed as i32),
            accounts_data_slice: vec![],
            ping: None,
        };

        info!("📡 Enviando subscripción a Geyser...");

        // Subscribirse
        let (_, mut stream) = client
            .subscribe_with_request(Some(request))
            .await
            .context("Failed to subscribe to Geyser")?;

        info!("✓ Subscripción activa. Esperando nuevos pools...");

        // ⚡ Procesar stream en task separado para no bloquear
        tokio::spawn(async move {
            // Usar Arc para compartir el cache entre tasks
            let pool_cache = Arc::new(std::sync::RwLock::new(HashMap::<Pubkey, Pool>::new()));

            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Some(update) = msg.update_oneof {
                            match update {
                                UpdateOneof::Account(account_update) => {
                                    // ⚡ PARALELIZACIÓN: Procesar cada evento en su propio task
                                    // Esto evita que un evento lento bloquee los siguientes
                                    let tx_clone = tx.clone();
                                    let cache_clone = pool_cache.clone();

                                    tokio::spawn(async move {
                                        Self::process_account_update_parallel(
                                            account_update,
                                            cache_clone,
                                            tx_clone,
                                        );
                                    });
                                }
                                UpdateOneof::Ping(_) => {
                                    debug!("Received ping from Geyser");
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error en stream de Geyser: {:?}", e);
                        // TODO: Implementar reconexión automática
                    }
                }
            }

            warn!("Stream de Geyser terminado");
        });

        Ok(rx)
    }

    /// Procesar actualización de cuenta (versión paralelizada)
    fn process_account_update_parallel(
        account_update: SubscribeUpdateAccount,
        pool_cache: Arc<std::sync::RwLock<HashMap<Pubkey, Pool>>>,
        tx: mpsc::UnboundedSender<PoolEvent>,
    ) {
        let account_info = match account_update.account {
            Some(acc) => acc,
            None => return,
        };

        // Parsear pubkey
        let pubkey = match Pubkey::try_from(account_info.pubkey.as_slice()) {
            Ok(pk) => pk,
            Err(e) => {
                error!("Invalid pubkey: {:?}", e);
                return;
            }
        };

        // Intentar deserializar pool
        let pool = match Pool::try_deserialize(&account_info.data) {
            Ok(p) => p,
            Err(e) => {
                debug!("Failed to deserialize pool {}: {:?}", pubkey, e);
                return;
            }
        };

        // Verificar si es nuevo pool o actualización (con lock de lectura)
        let event = {
            let cache_read = pool_cache.read().unwrap();
            if let Some(old_pool) = cache_read.get(&pubkey) {
                // Pool existente actualizado
                if old_pool.sqrt_price != pool.sqrt_price {
                    debug!("Pool actualizado: {}", pubkey);
                    Some(PoolEvent::PoolUpdated(PoolInfo::new(pubkey, pool.clone())))
                } else {
                    // Sin cambios relevantes
                    None
                }
            } else {
                // Nuevo pool detectado!
                info!("🆕 ¡NUEVO POOL DETECTADO! {}", pubkey);
                info!("   Token A: {}", pool.token_a_mint);
                info!("   Token B: {}", pool.token_b_mint);
                info!("   Liquidez: {}", pool.liquidity);
                Some(PoolEvent::NewPool(PoolInfo::new(pubkey, pool.clone())))
            }
        };

        // Si hay un evento, actualizar cache y enviarlo
        if let Some(event) = event {
            // Actualizar cache (con lock de escritura)
            {
                let mut cache_write = pool_cache.write().unwrap();
                cache_write.insert(pubkey, pool);
            }

            // Enviar evento
            if let Err(e) = tx.send(event) {
                error!("Failed to send pool event: {:?}", e);
            }
        }
    }
}
