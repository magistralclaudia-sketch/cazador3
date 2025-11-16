use anyhow::{Context, Result};
use futures::StreamExt;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::*;

use crate::config::Config;
use crate::meteora::{Pool, PoolInfo};

#[derive(Debug, Clone)]
pub enum PoolEvent {
    NewPool(PoolInfo),
    PoolUpdated(PoolInfo),
}

pub struct GeyserPoolMonitor {
    config: Config,
    client: Option<GeyserGrpcClient<impl tonic::codegen::InterceptedService<
        tonic::transport::Channel,
        impl tonic::service::Interceptor,
    >>>,
}

impl GeyserPoolMonitor {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            client: None,
        }
    }

    /// Conectar al servidor Geyser gRPC
    pub async fn connect(&mut self) -> Result<()> {
        info!("Conectando a Geyser gRPC: {}", self.config.geyser_endpoint);

        let mut client = GeyserGrpcClient::connect(
            self.config.geyser_endpoint.clone(),
            self.config.geyser_x_token.clone(),
            None, // No TLS para conexión local
        )
        .await
        .context("Failed to connect to Geyser gRPC")?;

        // Verificar conexión con ping
        let _ = client
            .ping(1)
            .await
            .context("Geyser ping failed")?;

        info!("✓ Conectado a Geyser gRPC exitosamente");

        self.client = Some(client);
        Ok(())
    }

    /// Iniciar monitoreo de nuevos pools de Meteora DAMM V2
    ///
    /// Retorna un canal donde se recibirán los eventos de pools
    pub async fn monitor_pools(
        &mut self,
    ) -> Result<mpsc::UnboundedReceiver<PoolEvent>> {
        let client = self.client.as_mut()
            .context("Not connected to Geyser. Call connect() first")?;

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
        let mut request = HashMap::new();
        request.insert("client".to_string(), SubscribeRequest {
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
        });

        info!("📡 Enviando subscripción a Geyser...");

        // Subscribirse
        let (_, mut stream) = client
            .subscribe_with_request(Some(request))
            .await
            .context("Failed to subscribe to Geyser")?;

        info!("✓ Subscripción activa. Esperando nuevos pools...");

        // Procesar stream en task separado para no bloquear
        tokio::spawn(async move {
            let mut pool_cache: HashMap<Pubkey, Pool> = HashMap::new();

            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Some(update) = msg.update_oneof {
                            match update {
                                UpdateOneof::Account(account_update) => {
                                    Self::process_account_update(
                                        account_update,
                                        &mut pool_cache,
                                        &tx,
                                    );
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

    /// Procesar actualización de cuenta
    fn process_account_update(
        account_update: SubscribeUpdateAccount,
        pool_cache: &mut HashMap<Pubkey, Pool>,
        tx: &mpsc::UnboundedSender<PoolEvent>,
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

        // Verificar si es nuevo pool o actualización
        let event = if let Some(old_pool) = pool_cache.get(&pubkey) {
            // Pool existente actualizado
            if old_pool.sqrt_price != pool.sqrt_price {
                debug!("Pool actualizado: {}", pubkey);
                PoolEvent::PoolUpdated(PoolInfo::new(pubkey, pool.clone()))
            } else {
                // Sin cambios relevantes
                return;
            }
        } else {
            // Nuevo pool detectado!
            info!("🆕 ¡NUEVO POOL DETECTADO! {}", pubkey);
            info!("   Token A: {}", pool.token_a_mint);
            info!("   Token B: {}", pool.token_b_mint);
            info!("   Liquidez: {}", pool.liquidity);
            PoolEvent::NewPool(PoolInfo::new(pubkey, pool.clone()))
        };

        // Actualizar cache
        pool_cache.insert(pubkey, pool);

        // Enviar evento
        if let Err(e) = tx.send(event) {
            error!("Failed to send pool event: {:?}", e);
        }
    }
}
