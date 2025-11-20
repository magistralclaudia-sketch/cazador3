pub mod pool;
pub mod dlmm_pool;
pub mod price;
pub mod damm_swap;
pub mod dlmm_swap;

pub use pool::{Pool, PoolInfo};
pub use dlmm_pool::{LbPair, LbPairInfo};
pub use price::PriceCalculator;
pub use damm_swap::DammSwapBuilder;
pub use dlmm_swap::DlmmSwapBuilder;

use solana_sdk::pubkey::Pubkey;

/// Enum para manejar tanto pools DAMM V2 como DLMM
#[derive(Debug, Clone)]
pub enum MeteoraPool {
    /// Pool DAMM V2 (Constant Product AMM)
    DammV2(Pool),
    /// Pool DLMM (Liquidity Book / Dynamic AMM)
    Dlmm(LbPair),
}

impl MeteoraPool {
    /// Obtener los mints de los tokens (token_a, token_b) o (token_x, token_y)
    pub fn get_token_mints(&self) -> (Pubkey, Pubkey) {
        match self {
            MeteoraPool::DammV2(pool) => (
                pool.token_a_mint_solana(),
                pool.token_b_mint_solana(),
            ),
            MeteoraPool::Dlmm(pair) => (
                pair.token_x_mint,
                pair.token_y_mint,
            ),
        }
    }

    /// Obtener los vaults/reserves
    pub fn get_vaults(&self) -> (Pubkey, Pubkey) {
        match self {
            MeteoraPool::DammV2(pool) => (
                pool.token_a_vault_solana(),
                pool.token_b_vault_solana(),
            ),
            MeteoraPool::Dlmm(pair) => pair.get_reserves(),
        }
    }

    /// Obtener tipo como string
    pub fn pool_type(&self) -> &str {
        match self {
            MeteoraPool::DammV2(_) => "DAMM V2",
            MeteoraPool::Dlmm(_) => "DLMM",
        }
    }
}
