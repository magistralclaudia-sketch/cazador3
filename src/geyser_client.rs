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
use solana_client::rpc_client::RpcClient;

#[derive(Debug, Clone)]
pub enum PoolEvent {
    NewPool(PoolInfo),
    PoolUpdated(PoolInfo),
    PoolCreationDetected {
        signature: String,
        slot: u64,
    },
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

        let mut builder = GeyserGrpcClient::build_from_shared(self.config.geyser_endpoint.clone())?;

        // X-Token opcional (solo si está presente y no vacío)
        if let Some(ref token) = self.config.geyser_x_token {
            if !token.is_empty() {
                builder = builder.x_token(Some(token.clone()))?;
            }
        }

        let client = builder
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

        info!("🔍 Iniciando monitoreo de pools Meteora DAMM V2 + DLMM...");
        info!("📊 DAMM V2: cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG");
        info!("📊 DLMM:    dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN");

        // ⚡ ESTRATEGIA: Monitorear TRANSACCIONES en vez de cambios de cuenta
        // Esto nos permite detectar la creación de pools de forma más confiable

        use yellowstone_grpc_proto::prelude::{
            SubscribeRequestFilterTransactions,
        };

        // Program IDs de Meteora
        const DAMM_V2_PROGRAM: &str = "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG";
        const DLMM_PROGRAM: &str = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";

        let mut transactions_filter = HashMap::new();

        // ⚡ ESTRATEGIA: Filtrar transacciones que REQUIEREN el programa Meteora
        // account_required = La transacción DEBE incluir esta cuenta/programa
        transactions_filter.insert(
            "meteora_damm_txs".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include: vec![],
                account_exclude: vec![],
                account_required: vec![DAMM_V2_PROGRAM.to_string()],  // DEBE tener DAMM V2
            },
        );

        transactions_filter.insert(
            "meteora_dlmm_txs".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include: vec![],
                account_exclude: vec![],
                account_required: vec![DLMM_PROGRAM.to_string()],  // DEBE tener DLMM
            },
        );

        info!("⚡ Filtro configurado: Transacciones que REQUIEREN programas Meteora (account_required)");

        // Crear request de subscripción
        let request = SubscribeRequest {
            accounts: HashMap::new(),
            slots: HashMap::new(),
            transactions: transactions_filter,  // ← Volver a transactions
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
        let rpc_url = self.config.rpc_url.clone();
        tokio::spawn(async move {
            // Contador de transacciones
            let mut tx_count = 0u64;
            let mut last_log_time = std::time::Instant::now();

            // Usar Arc para compartir el cache entre tasks
            let pool_cache = Arc::new(std::sync::RwLock::new(HashMap::<Pubkey, Pool>::new()));

            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Some(update) = msg.update_oneof {
                            match update {
                                UpdateOneof::Transaction(tx_update) => {
                                    tx_count += 1;

                                    // Log cada 100 transacciones
                                    if tx_count % 100 == 0 || last_log_time.elapsed().as_secs() >= 30 {
                                        info!("📊 Transacciones Meteora recibidas: {} (últimos 30s)", tx_count);
                                        last_log_time = std::time::Instant::now();
                                    }

                                    // Procesar transacción
                                    if let Some(meta) = &tx_update.transaction.as_ref().and_then(|t| t.meta.as_ref()) {
                                        // Verificar que fue exitosa
                                        if meta.err.is_some() {
                                            continue;
                                        }

                                        // Buscar instrucciones de creación de pool en los logs
                                        let is_pool_creation = meta.log_messages.iter().any(|log| {
                                            log.contains("Instruction: InitializePool") ||
                                            log.contains("Instruction: InitializeCustomizablePool") ||
                                            log.contains("Instruction: InitializePoolWithDynamicConfig") ||
                                            log.contains("create pool") ||
                                            log.contains("Instruction: InitializeVirtualPoolWithSplToken") ||
                                            log.contains("Instruction: InitializePermissionLbPair")
                                        });

                                        if is_pool_creation {
                                            if let Some(transaction) = &tx_update.transaction {
                                                let signature_bytes = &transaction.signature;
                                                let signature_str = bs58::encode(signature_bytes).into_string();

                                                info!("🆕 ¡POOL CREATION DETECTED!");
                                                info!("   Signature: {}", signature_str);
                                                info!("   Slot: {}", tx_update.slot);

                                                // Enviar evento de nuevo pool
                                                // Por ahora solo loguear, el fetch del pool se hace en main.rs
                                                let _ = tx.send(PoolEvent::PoolCreationDetected {
                                                    signature: signature_str,
                                                    slot: tx_update.slot,
                                                });
                                            }
                                        }
                                    }
                                }
                                UpdateOneof::Account(account_update) => {
                                    // Ignorar account updates por ahora
                                    debug!("📥 Account update ignorado (usamos transactions)");
                                }
                                UpdateOneof::Ping(_) => {
                                    debug!("🏓 Ping recibido desde Geyser");
                                }
                                _ => {
                                    debug!("📨 Otro tipo de evento recibido desde Geyser");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("❌ Error en stream de Geyser: {:?}", e);
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

        // Determinar si es DAMM V2 o DLMM por el owner y tamaño
        const DAMM_V2_PROGRAM: &str = "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG";
        const DLMM_PROGRAM: &str = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";

        let damm_program_id = DAMM_V2_PROGRAM.parse::<Pubkey>().unwrap();
        let dlmm_program_id = DLMM_PROGRAM.parse::<Pubkey>().unwrap();

        let owner = match Pubkey::try_from(account_info.owner.as_slice()) {
            Ok(o) => o,
            Err(_) => return,
        };

        // Intentar deserializar según el owner
        if owner == damm_program_id && account_info.data.len() >= 1100 {
            // DAMM V2 Pool
            match Pool::try_deserialize(&account_info.data) {
                Ok(pool) => {
                    // Verificar si es nuevo pool
                    let is_new = {
                        let cache_read = pool_cache.read().unwrap();
                        !cache_read.contains_key(&pubkey)
                    };

                    if is_new {
                        info!("🆕 ¡NUEVO POOL DAMM V2 DETECTADO! {}", pubkey);
                        info!("   Token A: {}", pool.token_a_mint_solana());
                        info!("   Token B: {}", pool.token_b_mint_solana());
                        info!("   Liquidez: {}", pool.liquidity);

                        // Agregar al cache
                        {
                            let mut cache_write = pool_cache.write().unwrap();
                            cache_write.insert(pubkey, pool.clone());
                        }

                        // Enviar evento
                        let _ = tx.send(PoolEvent::NewPool(PoolInfo::new(pubkey, pool)));
                    } else {
                        // Pool actualizado
                        debug!("Pool DAMM V2 actualizado: {}", pubkey);
                        let _ = tx.send(PoolEvent::PoolUpdated(PoolInfo::new(pubkey, pool)));
                    }
                }
                Err(e) => {
                    debug!("Failed to deserialize DAMM V2 pool {}: {:?}", pubkey, e);
                }
            }
        } else if owner == dlmm_program_id && account_info.data.len() >= 1040 {
            // DLMM Pool
            debug!("📝 DLMM pool detectado: {} (tamaño: {})", pubkey, account_info.data.len());
            // TODO: Implementar deserialización DLMM cuando sea necesario
        }
    }

    // Resto del código...
    #[allow(dead_code)]
    fn old_process_account_update_parallel(
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

        // Intentar deserializar pool (OLD VERSION - solo DAMM V2)
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

        // Actualizar cache con lock de escritura
        if event.is_some() {
            let mut cache_write = pool_cache.write().unwrap();
            cache_write.insert(pubkey, pool);
        }

        // Enviar evento
        if let Some(ev) = event {
            let _ = tx.send(ev);
        }
    }
}

/// Fetch pool data from RPC (blocking)
fn fetch_pool_from_rpc_blocking(rpc_url: &str, pool_pubkey: &Pubkey) -> Result<Pool> {
    let rpc_client = RpcClient::new(rpc_url.to_string());

    // Get account data
    let account = rpc_client
        .get_account(pool_pubkey)
        .context("Failed to fetch pool account")?;

    // Deserialize pool
    let pool = Pool::try_deserialize(&account.data)
        .context("Failed to deserialize pool")?;

    Ok(pool)
}

/// Fetch pool from transaction by finding account owned by DAMM V2 or DLMM program
pub fn fetch_pool_from_transaction(rpc_url: &str, signature: &str) -> Result<Option<(Pubkey, crate::meteora::MeteoraPool)>> {
    use solana_client::rpc_config::RpcTransactionConfig;
    use solana_transaction_status::UiTransactionEncoding;
    use solana_sdk::commitment_config::CommitmentConfig;

    let rpc_client = RpcClient::new(rpc_url.to_string());

    // Get transaction with account keys
    let tx = rpc_client
        .get_transaction_with_config(
            &signature.parse()?,
            RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            },
        )
        .context("Failed to fetch transaction")?;

    // Extract account keys from transaction
    use solana_transaction_status::EncodedTransaction;
    if let EncodedTransaction::Json(ui_transaction) = tx.transaction.transaction {
        use solana_transaction_status::UiMessage;
        let account_keys = match ui_transaction.message {
            UiMessage::Parsed(parsed) => parsed.account_keys.iter().map(|k| k.pubkey.clone()).collect::<Vec<_>>(),
            UiMessage::Raw(raw) => raw.account_keys,
        };

        // DAMM V2 program ID
        let damm_program_id: Pubkey = "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG".parse()?;
        // DLMM program ID (para migraciones)
        let dlmm_program_id: Pubkey = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN".parse()?;

        info!("🔍 Buscando pool en {} cuentas de la transacción...", account_keys.len());

        // Iterate through all accounts to find the one owned by DAMM V2 or DLMM program
        for (i, account_key_str) in account_keys.iter().enumerate() {
            if let Ok(pool_pubkey) = account_key_str.parse::<Pubkey>() {
                // ⚡ CONSULTAR DIRECTAMENTE AL NODO LOCAL para obtener datos actuales
                match rpc_client.get_account(&pool_pubkey) {
                    Ok(account) => {
                        // Check if owned by DAMM V2 program
                        if account.owner == damm_program_id {
                            debug!("   [{}] DAMM V2 account: {}", i, pool_pubkey);

                            // Verificar tamaño de pool DAMM v2 (debe ser 1104 bytes + 8 discriminator = 1112)
                            if account.data.len() >= 1100 && account.data.len() <= 1120 {
                                // Intentar deserializar usando zero-copy (con Anchor)
                                match Pool::try_from_bytes(&account.data) {
                                    Ok(pool_ref) => {
                                        // Copiar el pool (Pool implementa Copy gracias a zero_copy)
                                        let pool = *pool_ref;

                                        info!("✅ Pool DAMM V2 encontrado en cuenta #{}: {}", i, pool_pubkey);
                                        info!("   Token A: {}", pool.token_a_mint_solana());
                                        info!("   Token B: {}", pool.token_b_mint_solana());
                                        info!("   Vault A: {}", pool.token_a_vault_solana());
                                        info!("   Vault B: {}", pool.token_b_vault_solana());
                                        info!("   Liquidez: {}", pool.liquidity);
                                        info!("   Precio: {}", pool.sqrt_price);

                                        return Ok(Some((pool_pubkey, crate::meteora::MeteoraPool::DammV2(pool))));
                                    }
                                    Err(e) => {
                                        debug!("      Failed to deserialize as Pool: {:?}", e);
                                    }
                                }
                            } else {
                                debug!("      Account size mismatch ({} bytes), expected ~1112", account.data.len());
                            }
                        }

                        // Check if owned by DLMM program
                        if account.owner == dlmm_program_id {
                            debug!("   [{}] DLMM account: {}", i, pool_pubkey);

                            // Verificar tamaño de pool DLMM (debe ser 1040 bytes + 8 discriminator = 1048)
                            if account.data.len() >= 1040 && account.data.len() <= 1060 {
                                // Intentar deserializar como LbPair usando Borsh
                                match crate::meteora::LbPair::try_from_bytes(&account.data) {
                                    Ok(lb_pair) => {
                                        info!("✅ Pool DLMM encontrado en cuenta #{}: {}", i, pool_pubkey);
                                        info!("   Token X: {}", lb_pair.token_x_mint);
                                        info!("   Token Y: {}", lb_pair.token_y_mint);
                                        info!("   Reserve X: {}", lb_pair.reserve_x);
                                        info!("   Reserve Y: {}", lb_pair.reserve_y);
                                        info!("   Active Bin ID: {}", lb_pair.active_id);
                                        info!("   Precio: {:.6}", lb_pair.get_price());

                                        return Ok(Some((pool_pubkey, crate::meteora::MeteoraPool::Dlmm(lb_pair))));
                                    }
                                    Err(e) => {
                                        debug!("      Failed to deserialize as LbPair: {:?}", e);
                                    }
                                }
                            } else {
                                debug!("      Account size mismatch ({} bytes), expected ~1048 for DLMM", account.data.len());
                            }
                        }
                    }
                    Err(e) => {
                        debug!("   [{}] Failed to fetch account {}: {:?}", i, pool_pubkey, e);
                    }
                }
            }
        }

        warn!("⚠️  No se encontró pool account válido en las {} cuentas", account_keys.len());
    }

    Ok(None)
}
