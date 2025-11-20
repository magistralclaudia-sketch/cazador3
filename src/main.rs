mod config;
mod geyser_client;
mod meteora;
mod trading;

use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::Config;
use geyser_client::{GeyserPoolMonitor, PoolEvent, fetch_pool_from_transaction};
use meteora::{PriceCalculator, PoolInfo, MeteoraPool, LbPair};
use trading::{Position, PositionManager, TradeExecutor};
use solana_sdk::pubkey::Pubkey;
use solana_client::rpc_client::RpcClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Inicializar logger
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .init();

    // Banner
    print_banner();

    // Cargar configuración
    info!("📋 Cargando configuración...");
    let config = Config::from_env()?;

    info!("✓ Configuración cargada:");
    info!("  RPC: {}", config.rpc_url);
    info!("  Geyser: {}", config.geyser_endpoint);
    info!("  Auto-buy: {} ({}  SOL)",
        if config.auto_buy_enabled { "ENABLED" } else { "DISABLED" },
        config.auto_buy_amount_sol
    );
    info!("  Take Profit: {}%", config.take_profit_percent);
    info!("  Stop Loss: {}%", config.stop_loss_percent);

    // Crear trade executor
    info!("💼 Inicializando Trade Executor...");
    let executor = Arc::new(TradeExecutor::new(config.clone()).await?);
    info!("✓ Wallet: {}", executor.get_wallet_pubkey());

    // Nota: El ATA se crea automáticamente cuando detectamos el token a comprar
    // en ensure_ata_exists() dentro de snipe_buy() - más eficiente que pre-crear

    // Crear position manager (clonamos el Arc que es barato)
    let position_manager = PositionManager::new(config.clone(), executor.clone());

    // Crear Geyser client
    info!("🔌 Conectando a Geyser gRPC...");
    let mut geyser_monitor = GeyserPoolMonitor::new(config.clone());
    geyser_monitor.connect().await?;

    // Iniciar monitoreo de pools
    info!("🚀 Iniciando monitoreo de pools Meteora DAMM V2...");
    let pool_rx = geyser_monitor.monitor_pools().await?;

    // Ejecutar bot principal (pasar el executor ya creado)
    run_sniper_bot(config, pool_rx, position_manager, executor).await?;

    Ok(())
}

async fn run_sniper_bot(
    config: Config,
    mut pool_rx: tokio::sync::mpsc::UnboundedReceiver<PoolEvent>,
    mut position_manager: PositionManager,
    executor: Arc<TradeExecutor>,  // ⚡ Recibir executor en lugar de crear uno nuevo
) -> Result<()> {
    info!("🎯 BOT DE SNIPER ACTIVO");
    info!("========================================");
    info!("Esperando nuevos pools...");
    info!("");

    while let Some(event) = pool_rx.recv().await {
        match event {
            PoolEvent::NewPool(pool_info) => {
                handle_new_pool(&config, &executor, &mut position_manager, pool_info).await;
            }
            PoolEvent::PoolUpdated(pool_info) => {
                handle_pool_update(&mut position_manager, pool_info).await;
            }
            PoolEvent::PoolCreationDetected { signature, slot } => {
                info!("🔍 Fetching pool data from signature: {}", signature);
                info!("   Slot: {}", slot);

                // Fetch pool desde la transacción (blocking operation)
                let rpc_url = config.rpc_url.clone();
                let sig_clone = signature.clone();

                match tokio::task::spawn_blocking(move || {
                    fetch_pool_from_transaction(&rpc_url, &sig_clone)
                }).await {
                    Ok(Ok(Some((pool_pubkey, meteora_pool)))) => {
                        info!("✅ Pool fetched: {}", pool_pubkey);
                        info!("   Type: {}", meteora_pool.pool_type());

                        // Procesar ambos tipos de pools: DAMM V2 y DLMM
                        match meteora_pool {
                            MeteoraPool::DammV2(pool) => {
                                let pool_info = PoolInfo::new(pool_pubkey, pool);
                                handle_new_pool(&config, &executor, &mut position_manager, pool_info).await;
                            }
                            MeteoraPool::Dlmm(pool) => {
                                info!("✅ DLMM pool detected - procesando...");
                                handle_new_dlmm_pool(&config, executor.clone(), pool_pubkey, pool).await;
                            }
                        }
                    }
                    Ok(Ok(None)) => {
                        warn!("⚠️  No pool account found in transaction {}", signature);
                    }
                    Ok(Err(e)) => {
                        error!("❌ Failed to fetch pool from transaction: {:?}", e);
                    }
                    Err(e) => {
                        error!("❌ Task join error: {:?}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_new_pool(
    config: &Config,
    executor: &TradeExecutor,
    position_manager: &mut PositionManager,
    pool_info: PoolInfo,
) {
    info!("🔔 Pool detectado: {}", pool_info.address);
    info!("   Token A: {}", pool_info.pool.token_a_mint);
    info!("   Token B: {}", pool_info.pool.token_b_mint);
    info!("   Liquidez: {}", pool_info.pool.liquidity);

    // ⚡ Verificaciones rápidas
    let min_liquidity = (config.min_liquidity_sol * 1_000_000_000.0) as u128;
    let should_buy = config.auto_buy_enabled && pool_info.pool.liquidity >= min_liquidity;

    if should_buy {
        info!("⚡ Iniciando compra automática...");

        // ⚡ COMPRAR INMEDIATAMENTE
        match executor.snipe_buy(&pool_info.address, &pool_info.pool).await {
            Ok(signature) => {
                // ✅ DESPUÉS de comprar, loggear TODO
                let price = PriceCalculator::calculate_price(&pool_info.pool);

                info!("");
                info!("🆕 ═══════════════════════════════════════");
                info!("   ✅ COMPRA EXITOSA!");
                info!("═══════════════════════════════════════");
                info!("📍 Pool: {}", pool_info.address);
                info!("📝 Signature: {}", signature);
                info!("💰 Precio: {:.8}", price);
                info!("💵 Monto: {} SOL", config.auto_buy_amount_sol);
                info!("🪙 Token A: {}", pool_info.pool.token_a_mint);
                info!("🪙 Token B: {}", pool_info.pool.token_b_mint);
                info!("💧 Liquidez: {}", pool_info.pool.liquidity);
                info!("═══════════════════════════════════════");
                info!("");

                // Agregar posición
                let amount = (config.auto_buy_amount_sol * 1_000_000_000.0) as u64;
                let position = Position::new(
                    pool_info.address,
                    price,
                    amount,
                    pool_info.pool.token_b_mint,
                );
                position_manager.add_position(position);
                info!("✅ Posición agregada al tracker");
            }
            Err(e) => {
                error!("");
                error!("❌ ═══════════════════════════════════════");
                error!("   ERROR EN COMPRA (BOT CONTINÚA)");
                error!("═══════════════════════════════════════");
                error!("📍 Pool: {}", pool_info.address);
                error!("🪙 Token A: {}", pool_info.pool.token_a_mint);
                error!("🪙 Token B: {}", pool_info.pool.token_b_mint);
                error!("💧 Liquidez: {}", pool_info.pool.liquidity);
                error!("❌ Error: {:?}", e);
                error!("═══════════════════════════════════════");
                error!("🔄 Bot continúa monitoreando nuevos pools...");
                error!("");
            }
        }
    } else {
        // Solo observación - aquí sí podemos loggear
        if !config.auto_buy_enabled {
            let price = PriceCalculator::calculate_price(&pool_info.pool);
            info!("   💰 Precio: {:.8}", price);
            info!("   ⚠️  Auto-buy DESHABILITADO (solo observación)");
            info!("");
        } else if pool_info.pool.liquidity < min_liquidity {
            info!("   ⚠️  Liquidez insuficiente: {} < {} (ignorado)",
                pool_info.pool.liquidity,
                min_liquidity
            );
            info!("");
        }
    }
}

async fn handle_pool_update(
    position_manager: &mut PositionManager,
    pool_info: PoolInfo,
) {
    // Verificar si tenemos una posición en este pool
    if let Some(position) = position_manager.get_position(&pool_info.address) {
        let current_price = PriceCalculator::calculate_price(&pool_info.pool);
        let pnl = position.calculate_pnl(current_price);

        info!("📊 Pool actualizado: {} | PnL: {:.2}%", pool_info.address, pnl);

        // Verificar TP/SL - NUNCA detener el bot por errores aquí
        if let Err(e) = position_manager.check_positions(&pool_info).await {
            error!("⚠️  Error verificando TP/SL (bot continúa): {:?}", e);
        }
    }
}

/// ⚡ Esperar a que la TX de pool creation complete y comprar INMEDIATAMENTE
///
/// Geyser nos notifica del pool DURANTE la ejecución de la TX, antes de que
/// todas las inner instructions (addLiquidity) completen. Por eso el active_id
/// es extremo y el precio inválido. Solución: esperar 1-2s y refetch el pool.
async fn wait_and_buy_dlmm(
    config: &Config,
    executor: Arc<TradeExecutor>,
    pool_address: Pubkey,
    _pool: LbPair,  // Pool inicial con estado intermedio
    _token_mint: Pubkey,  // El mint ya existe en la misma TX
) {
    let rpc = RpcClient::new(config.rpc_url.clone());
    let start = std::time::Instant::now();

    // Esperar 1.5 segundos para que la TX complete todas las inner instructions
    info!("   ⏱️  Esperando 1.5s para que TX complete...");
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    // Refetch el pool desde RPC para obtener el estado FINAL
    info!("   🔄 Refetching pool desde RPC...");
    match rpc.get_account_data(&pool_address) {
        Ok(pool_data) => {
            match LbPair::try_from_bytes(&pool_data) {
                Ok(updated_pool) => {
                    let updated_price = updated_pool.get_price();
                    let active_id = updated_pool.active_id;
                    let bin_step = updated_pool.bin_step;

                    info!("   📊 Pool actualizado:");
                    info!("      Active ID: {}", active_id);
                    info!("      Bin Step: {}", bin_step);
                    info!("      Precio: {:.10}", updated_price);

                    // Verificar que el precio sea válido
                    if !updated_price.is_finite() || updated_price == 0.0 {
                        warn!("   ⚠️  Precio aún inválido después de 1.5s: {:.8}", updated_price);
                        warn!("      Esperando 1s más y reintentando...");
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

                        // Segundo intento
                        match rpc.get_account_data(&pool_address) {
                            Ok(pool_data2) => {
                                match LbPair::try_from_bytes(&pool_data2) {
                                    Ok(updated_pool2) => {
                                        let price2 = updated_pool2.get_price();
                                        info!("   💰 Precio (2do intento): {:.10}", price2);

                                        // Solo comprar si el precio es válido
                                        if price2.is_finite() && price2 > 0.0 {
                                            let elapsed = start.elapsed();
                                            buy_with_updated_pool(config, &executor, pool_address, updated_pool2, elapsed).await;
                                        } else {
                                            warn!("   ⚠️  Pool sin liquidez después de 2.5s - SKIP");
                                        }
                                    }
                                    Err(e) => {
                                        error!("   ❌ Error deserializando pool (2do intento): {:?}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("   ❌ Error refetching pool (2do intento): {:?}", e);
                            }
                        }
                        return;
                    }

                    // ⚡⚡⚡ COMPRAR INMEDIATAMENTE con pool actualizado
                    let elapsed = start.elapsed();
                    buy_with_updated_pool(config, &executor, pool_address, updated_pool, elapsed).await;
                }
                Err(e) => {
                    error!("   ❌ Error deserializando pool actualizado: {:?}", e);
                }
            }
        }
        Err(e) => {
            error!("   ❌ Error refetching pool: {:?}", e);
        }
    }
}

/// Helper function para comprar con pool actualizado
async fn buy_with_updated_pool(
    config: &Config,
    executor: &TradeExecutor,
    pool_address: Pubkey,
    pool: LbPair,
    elapsed: std::time::Duration,
) {
    if config.auto_buy_enabled {
        info!("⚡ Iniciando compra automática DLMM ultra-rápida...");

        match executor.snipe_buy_dlmm(&pool_address, &pool).await {
            Ok(signature) => {
                let price = pool.get_price();
                info!("");
                info!("🆕 ═══════════════════════════════════════");
                info!("   ✅ COMPRA DLMM EXITOSA (ULTRA-FAST)!");
                info!("═══════════════════════════════════════");
                info!("📍 Pool: {}", pool_address);
                info!("📝 Signature: {}", signature);
                info!("💰 Precio: {:.8}", price);
                info!("💵 Monto: {} SOL", config.auto_buy_amount_sol);
                info!("🪙 Token X: {}", pool.token_x_mint);
                info!("🪙 Token Y: {}", pool.token_y_mint);
                info!("⏱️  Latencia: {}ms desde detección", elapsed.as_millis());
                info!("═══════════════════════════════════════");
                info!("");
            }
            Err(e) => {
                error!("");
                error!("❌ ═══════════════════════════════════════");
                error!("   ERROR EN COMPRA DLMM (BOT CONTINÚA)");
                error!("═══════════════════════════════════════");
                error!("📍 Pool: {}", pool_address);
                error!("🪙 Token X: {}", pool.token_x_mint);
                error!("🪙 Token Y: {}", pool.token_y_mint);
                error!("❌ Error: {:?}", e);
                error!("═══════════════════════════════════════");
                error!("");
            }
        }
    }
}

async fn handle_new_dlmm_pool(
    config: &Config,
    executor: Arc<TradeExecutor>,
    pool_address: Pubkey,
    pool: LbPair,
) {
    info!("🔔 DLMM Pool detectado: {}", pool_address);
    info!("   Token X: {}", pool.token_x_mint);
    info!("   Token Y: {}", pool.token_y_mint);
    info!("   Active Bin ID: {}", pool.active_id);
    info!("   Bin Step: {}", pool.bin_step);

    // Determinar cuál es el token nuevo (no SOL)
    let wsol_mint = "So11111111111111111111111111111111111111112".parse::<Pubkey>().unwrap();
    let native_sol = "11111111111111111111111111111111".parse::<Pubkey>().unwrap();

    let token_mint = if pool.token_x_mint == wsol_mint || pool.token_x_mint == native_sol {
        pool.token_y_mint
    } else {
        pool.token_x_mint
    };

    // ESTRATEGIA NUEVA: NO calcular precio con fórmula exponencial.
    // El programa DLMM calcula el precio internamente mirando los balances en el bin.
    // Simplemente hacemos swap con min_amount_out bajo y dejamos que el programa lo maneje.

    let should_buy = config.auto_buy_enabled;

    if should_buy {
        info!("⚡ Iniciando compra automática DLMM...");

        // ⚡ COMPRAR INMEDIATAMENTE
        match executor.snipe_buy_dlmm(&pool_address, &pool).await {
            Ok(signature) => {
                let price = pool.get_price();

                info!("");
                info!("🆕 ═══════════════════════════════════════");
                info!("   ✅ COMPRA DLMM EXITOSA!");
                info!("═══════════════════════════════════════");
                info!("📍 Pool: {}", pool_address);
                info!("📝 Signature: {}", signature);
                info!("💰 Precio: {:.8}", price);
                info!("💵 Monto: {} SOL", config.auto_buy_amount_sol);
                info!("🪙 Token X: {}", pool.token_x_mint);
                info!("🪙 Token Y: {}", pool.token_y_mint);
                info!("📊 Active Bin: {}", pool.active_id);
                info!("═══════════════════════════════════════");
                info!("");

                // TODO: Agregar position tracking para DLMM
                // Por ahora solo loggeamos la compra exitosa
            }
            Err(e) => {
                error!("");
                error!("❌ ═══════════════════════════════════════");
                error!("   ERROR EN COMPRA DLMM (BOT CONTINÚA)");
                error!("═══════════════════════════════════════");
                error!("📍 Pool: {}", pool_address);
                error!("🪙 Token X: {}", pool.token_x_mint);
                error!("🪙 Token Y: {}", pool.token_y_mint);
                error!("❌ Error: {:?}", e);
                error!("═══════════════════════════════════════");
                error!("🔄 Bot continúa monitoreando nuevos pools...");
                error!("");
            }
        }
    } else {
        info!("   ⚠️  Auto-buy DESHABILITADO (solo observación)");
        info!("");
    }
}

fn print_banner() {
    println!("");
    println!("╔═══════════════════════════════════════════════╗");
    println!("║                                               ║");
    println!("║        🚀 METEORA DAMM V2 SNIPER BOT 🚀      ║");
    println!("║                                               ║");
    println!("║  Ultra-rápido con Yellowstone Geyser gRPC    ║");
    println!("║  Detección instantánea de nuevos pools       ║");
    println!("║  Auto-compra con Take Profit & Stop Loss     ║");
    println!("║                                               ║");
    println!("╚═══════════════════════════════════════════════╝");
    println!("");
}
