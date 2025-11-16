use anyhow::Result;
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    // RPC Configuration
    pub rpc_url: String,
    pub rpc_ws_url: String,

    // Geyser Configuration
    pub geyser_endpoint: String,
    pub geyser_x_token: Option<String>,

    // Wallet
    pub wallet_keypair_path: String,

    // Meteora Program
    pub meteora_program_id: Pubkey,

    // Trading Configuration
    pub auto_buy_enabled: bool,
    pub auto_buy_amount_sol: f64,
    pub buy_slippage_bps: u16,
    pub sell_slippage_bps: u16,
    pub priority_fee_lamports: u64,

    // Risk Management
    pub take_profit_percent: f64,
    pub stop_loss_percent: f64,

    // Monitoring
    pub min_liquidity_sol: f64,

    // Jito Configuration (optional, for ultra-fast execution)
    pub jito_enabled: bool,
    pub jito_endpoint: Option<String>,
    pub jito_tip_lamports: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        let config = Config {
            rpc_url: std::env::var("RPC_URL")?,
            rpc_ws_url: std::env::var("RPC_WS_URL")?,
            geyser_endpoint: std::env::var("GEYSER_ENDPOINT")?,
            geyser_x_token: std::env::var("GEYSER_X_TOKEN").ok(),
            wallet_keypair_path: std::env::var("WALLET_KEYPAIR_PATH")?,
            meteora_program_id: Pubkey::from_str(&std::env::var("METEORA_PROGRAM_ID")?)?,
            auto_buy_enabled: std::env::var("AUTO_BUY_ENABLED")?.parse()?,
            auto_buy_amount_sol: std::env::var("AUTO_BUY_AMOUNT_SOL")?.parse()?,
            buy_slippage_bps: std::env::var("BUY_SLIPPAGE_BPS")?.parse()?,
            sell_slippage_bps: std::env::var("SELL_SLIPPAGE_BPS")?.parse()?,
            priority_fee_lamports: std::env::var("PRIORITY_FEE_LAMPORTS")?.parse()?,
            take_profit_percent: std::env::var("TAKE_PROFIT_PERCENT")?.parse()?,
            stop_loss_percent: std::env::var("STOP_LOSS_PERCENT")?.parse()?,
            min_liquidity_sol: std::env::var("MIN_LIQUIDITY_SOL")?.parse()?,
            jito_enabled: std::env::var("JITO_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()?,
            jito_endpoint: std::env::var("JITO_ENDPOINT").ok(),
            jito_tip_lamports: std::env::var("JITO_TIP_LAMPORTS")
                .unwrap_or_else(|_| "10000".to_string())
                .parse()?,
        };

        Ok(config)
    }
}
