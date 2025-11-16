mod config;
mod geyser_client;
mod meteora;
mod trading;

use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::Config;
use geyser_client::{GeyserPoolMonitor, PoolEvent};
use meteora::{PriceCalculator, PoolInfo};
use trading::{Position, PositionManager, TradeExecutor};

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

    // ⚡ Pre-crear ATAs para tokens comunes (ahorra ~500ms en primera compra)
    executor.pre_create_common_atas().await?;

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
    // ⚡ OPTIMIZACIÓN: Verificaciones rápidas SIN logs
    let min_liquidity = (config.min_liquidity_sol * 1_000_000_000.0) as u128;
    let should_buy = config.auto_buy_enabled && pool_info.pool.liquidity >= min_liquidity;

    if should_buy {
        // ⚡ COMPRAR INMEDIATAMENTE - Sin calcular precio ni loggear
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
            }
            Err(e) => {
                error!("");
                error!("❌ ERROR EN COMPRA");
                error!("📍 Pool: {}", pool_info.address);
                error!("Error: {:?}", e);
                error!("");
            }
        }
    } else {
        // Solo observación - aquí sí podemos loggear
        if !config.auto_buy_enabled {
            let price = PriceCalculator::calculate_price(&pool_info.pool);
            info!("");
            info!("🆕 Pool detectado (solo observación): {}", pool_info.address);
            info!("   💰 Precio: {:.8}", price);
            info!("   💧 Liquidez: {}", pool_info.pool.liquidity);
            info!("");
        } else if pool_info.pool.liquidity < min_liquidity {
            info!("⚠️  Pool {} ignorado: liquidez insuficiente ({} < {})",
                pool_info.address,
                pool_info.pool.liquidity,
                min_liquidity
            );
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

        // Verificar TP/SL
        if let Err(e) = position_manager.check_positions(&pool_info).await {
            error!("Error checking positions: {:?}", e);
        }
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
