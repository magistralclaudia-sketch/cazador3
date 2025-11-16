use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{interval, Duration};
use tracing::{info, warn};

use crate::config::Config;
use crate::meteora::{Pool, PoolInfo, PriceCalculator};
use crate::trading::executor::TradeExecutor;

#[derive(Debug, Clone)]
pub struct Position {
    pub pool_address: Pubkey,
    pub entry_price: f64,
    pub entry_time: u64,
    pub amount: u64,
    pub token_mint: Pubkey,
}

impl Position {
    pub fn new(pool_address: Pubkey, entry_price: f64, amount: u64, token_mint: Pubkey) -> Self {
        let entry_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            pool_address,
            entry_price,
            entry_time,
            amount,
            token_mint,
        }
    }

    /// Calcular PnL (Profit and Loss) en porcentaje
    pub fn calculate_pnl(&self, current_price: f64) -> f64 {
        ((current_price - self.entry_price) / self.entry_price) * 100.0
    }

    /// Verificar si debe ejecutar take profit
    pub fn should_take_profit(&self, current_price: f64, take_profit_percent: f64) -> bool {
        self.calculate_pnl(current_price) >= take_profit_percent
    }

    /// Verificar si debe ejecutar stop loss
    pub fn should_stop_loss(&self, current_price: f64, stop_loss_percent: f64) -> bool {
        self.calculate_pnl(current_price) <= stop_loss_percent
    }
}

pub struct PositionManager {
    config: Config,
    positions: HashMap<Pubkey, Position>,
    executor: Arc<TradeExecutor>,
}

impl PositionManager {
    pub fn new(config: Config, executor: Arc<TradeExecutor>) -> Self {
        Self {
            config,
            positions: HashMap::new(),
            executor,
        }
    }

    /// Agregar una nueva posición después de comprar
    pub fn add_position(&mut self, position: Position) {
        info!("📊 Nueva posición agregada:");
        info!("   Pool: {}", position.pool_address);
        info!("   Precio de entrada: {}", position.entry_price);
        info!("   Cantidad: {}", position.amount);

        self.positions.insert(position.pool_address, position);
    }

    /// Remover una posición después de vender
    pub fn remove_position(&mut self, pool_address: &Pubkey) -> Option<Position> {
        self.positions.remove(pool_address)
    }

    /// Obtener una posición
    pub fn get_position(&self, pool_address: &Pubkey) -> Option<&Position> {
        self.positions.get(pool_address)
    }

    /// Verificar todas las posiciones y ejecutar TP/SL si es necesario
    pub async fn check_positions(&mut self, pool_info: &PoolInfo) -> Result<()> {
        // Clonar la posición para evitar problemas con el borrow checker
        if let Some(position) = self.positions.get(&pool_info.address).cloned() {
            let current_price = PriceCalculator::calculate_price(&pool_info.pool);
            let pnl = position.calculate_pnl(current_price);

            info!("📈 Pool {}: PnL = {:.2}%", pool_info.address, pnl);

            // Verificar Take Profit
            if position.should_take_profit(current_price, self.config.take_profit_percent) {
                info!("🎉 ¡TAKE PROFIT! PnL = {:.2}%", pnl);
                self.execute_sell(pool_info, &position, "Take Profit").await?;
            }
            // Verificar Stop Loss
            else if position.should_stop_loss(current_price, self.config.stop_loss_percent) {
                warn!("🛑 STOP LOSS activado. PnL = {:.2}%", pnl);
                self.execute_sell(pool_info, &position, "Stop Loss").await?;
            }
        }

        Ok(())
    }

    /// Ejecutar venta
    async fn execute_sell(
        &mut self,
        pool_info: &PoolInfo,
        position: &Position,
        reason: &str,
    ) -> Result<()> {
        info!("💸 Ejecutando venta: {}", reason);

        match self.executor.sell(
            &pool_info.address,
            &pool_info.pool,
            position.amount,
        ).await {
            Ok(signature) => {
                info!("✅ Venta exitosa! Signature: {}", signature);
                self.remove_position(&pool_info.address);
                Ok(())
            }
            Err(e) => {
                warn!("❌ Error al vender: {:?}", e);
                Err(e)
            }
        }
    }

    /// Monitorear todas las posiciones continuamente
    pub async fn monitor_loop(&mut self, mut pool_rx: tokio::sync::mpsc::UnboundedReceiver<PoolInfo>) {
        info!("👀 Iniciando monitoreo de posiciones...");
        info!("   Take Profit: {}%", self.config.take_profit_percent);
        info!("   Stop Loss: {}%", self.config.stop_loss_percent);

        let mut check_interval = interval(Duration::from_secs(1)); // Verificar cada segundo

        loop {
            tokio::select! {
                // Actualización de pool recibida
                Some(pool_info) = pool_rx.recv() => {
                    if let Err(e) = self.check_positions(&pool_info).await {
                        warn!("Error checking positions: {:?}", e);
                    }
                }
                // Intervalo de verificación periódica
                _ = check_interval.tick() => {
                    // Aquí podrías hacer verificaciones adicionales periódicas
                    if !self.positions.is_empty() {
                        info!("📊 Posiciones activas: {}", self.positions.len());
                    }
                }
            }
        }
    }

    /// Obtener número de posiciones activas
    pub fn active_positions_count(&self) -> usize {
        self.positions.len()
    }

    /// Cerrar todas las posiciones
    pub async fn close_all_positions(&mut self) -> Result<()> {
        info!("🚪 Cerrando todas las posiciones...");

        let pool_addresses: Vec<Pubkey> = self.positions.keys().copied().collect();

        for pool_address in pool_addresses {
            // Aquí necesitarías obtener el pool state actual para ejecutar la venta
            // Por simplicidad, solo removemos de la lista
            warn!("TODO: Implementar cierre de posición para {}", pool_address);
            self.remove_position(&pool_address);
        }

        Ok(())
    }
}
