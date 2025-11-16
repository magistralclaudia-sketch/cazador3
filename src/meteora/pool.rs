use anchor_lang::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};

/// Meteora DAMM V2 Pool State
/// Esta estructura representa el estado de un pool de Meteora DAMM V2
/// Basado en el programa Constant Product AMM
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct Pool {
    /// Bump seed for PDA
    pub bump: u8,

    /// Liquidity provider fee in basis points
    pub lp_fee_bps: u16,

    /// Protocol fee in basis points
    pub protocol_fee_bps: u16,

    /// Current square root price (Q64.64 format)
    /// price = (sqrt_price / 2^64)^2
    pub sqrt_price: u128,

    /// Total liquidity in the pool
    pub liquidity: u128,

    /// Token A mint
    pub token_a_mint: Pubkey,

    /// Token B mint
    pub token_b_mint: Pubkey,

    /// Token A vault
    pub token_a_vault: Pubkey,

    /// Token B vault
    pub token_b_vault: Pubkey,

    /// Pool token mint (LP token)
    pub pool_token_mint: Pubkey,

    /// Fee receiver
    pub fee_receiver: Pubkey,

    /// Minimum sqrt price (for concentrated liquidity)
    pub sqrt_price_min: u128,

    /// Maximum sqrt price (for concentrated liquidity)
    pub sqrt_price_max: u128,

    /// Token A decimals
    pub token_a_decimals: u8,

    /// Token B decimals
    pub token_b_decimals: u8,

    /// Pool creation timestamp
    pub created_at: i64,

    /// Last update timestamp
    pub updated_at: i64,

    /// Reserved space for future upgrades
    pub reserved: [u64; 16],
}

impl Pool {
    /// Discriminador de cuenta de Anchor para Pool
    /// Los primeros 8 bytes de la cuenta identifican el tipo
    pub const DISCRIMINATOR: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];

    /// Tamaño mínimo esperado de la cuenta del pool
    pub const MIN_SIZE: usize = 8 + std::mem::size_of::<Pool>();

    /// Intentar deserializar un pool desde datos de cuenta
    pub fn try_deserialize(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < 8 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Data too short for discriminator",
            ));
        }

        // Verificar discriminador
        let disc = &data[0..8];
        if disc != Self::DISCRIMINATOR {
            // Si no coincide, intentar deserializar sin discriminador
            // (algunos programas pueden tener diferentes formatos)
            return Pool::try_from_slice(data);
        }

        // Deserializar después del discriminador
        Pool::try_from_slice(&data[8..])
    }

    /// Verificar si es un nuevo pool (creado recientemente)
    pub fn is_new(&self, current_timestamp: i64, threshold_seconds: i64) -> bool {
        current_timestamp - self.created_at < threshold_seconds
    }

    /// Verificar si el pool tiene liquidez mínima
    pub fn has_min_liquidity(&self, min_liquidity: u128) -> bool {
        self.liquidity >= min_liquidity
    }
}

#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub address: Pubkey,
    pub pool: Pool,
}

impl PoolInfo {
    pub fn new(address: Pubkey, pool: Pool) -> Self {
        Self { address, pool }
    }
}
