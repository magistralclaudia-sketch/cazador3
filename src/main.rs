mod config;
mod geyser_client;
mod meteora;
mod trading;

use anyhow::Result;
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
    let executor = TradeExecutor::new(config.clone()).await?;
    info!("✓ Wallet: {}", executor.get_wallet_pubkey());

    // Crear position manager
    let position_manager = PositionManager::new(config.clone(), executor);

    // Crear Geyser client
    info!("🔌 Conectando a Geyser gRPC...");
    let mut geyser_monitor = GeyserPoolMonitor::new(config.clone());
    geyser_monitor.connect().await?;

    // Iniciar monitoreo de pools
    info!("🚀 Iniciando monitoreo de pools Meteora DAMM V2...");
    let pool_rx = geyser_monitor.monitor_pools().await?;

    // Ejecutar bot principal
    run_sniper_bot(config, pool_rx, position_manager).await?;

    Ok(())
}

async fn run_sniper_bot(
    config: Config,
    mut pool_rx: tokio::sync::mpsc::UnboundedReceiver<PoolEvent>,
    mut position_manager: PositionManager,
) -> Result<()> {
    info!("🎯 BOT DE SNIPER ACTIVO");
    info!("========================================");
    info!("Esperando nuevos pools...");
    info!("");

    let executor = TradeExecutor::new(config.clone()).await?;

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
    info!("");
    info!("🆕 ═══════════════════════════════════════");
    info!("   NUEVO POOL DETECTADO!");
    info!("═══════════════════════════════════════");
    info!("📍 Address: {}", pool_info.address);
    info!("🪙 Token A: {}", pool_info.pool.token_a_mint);
    info!("🪙 Token B: {}", pool_info.pool.token_b_mint);
    info!("💧 Liquidez: {}", pool_info.pool.liquidity);

    let price = PriceCalculator::calculate_price(&pool_info.pool);
    info!("💰 Precio: {}", price);

    // Verificar liquidez mínima
    let min_liquidity = (config.min_liquidity_sol * 1_000_000_000.0) as u128;
    if pool_info.pool.liquidity < min_liquidity {
        warn!("⚠️  Liquidez insuficiente. Mínimo: {} SOL", config.min_liquidity_sol);
        return;
    }

    // Auto-compra si está habilitada
    if config.auto_buy_enabled {
        info!("🎯 EJECUTANDO AUTO-COMPRA...");

        match executor.snipe_buy(&pool_info.address, &pool_info.pool).await {
            Ok(signature) => {
                info!("✅ ¡COMPRA EXITOSA!");
                info!("📝 Signature: {}", signature);

                // Agregar posición al position manager
                let amount = (config.auto_buy_amount_sol * 1_000_000_000.0) as u64;
                let position = Position::new(
                    pool_info.address,
                    price,
                    amount,
                    pool_info.pool.token_b_mint, // Asumiendo que compramos token B
                );

                position_manager.add_position(position);
            }
            Err(e) => {
                error!("❌ Error en compra: {:?}", e);
            }
        }
    } else {
        info!("ℹ️  Auto-compra deshabilitada. Solo monitoreando.");
    }

    info!("═══════════════════════════════════════");
    info!("");
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
