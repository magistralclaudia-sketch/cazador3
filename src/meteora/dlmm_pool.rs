use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::pubkey::Pubkey as SolanaPubkey;
use rust_decimal::prelude::*;

/// Meteora DLMM LbPair (Liquidity Book Pair) - Pool DLMM oficial
/// Tamaño: 1048 bytes (sin discriminator de 8 bytes = 1040 bytes data)
/// Basado en: https://github.com/MeteoraAg/dlmm-sdk/
///
/// Este es el pool tipo "virtual" o "permissionless" que se crea con
/// InitializeVirtualPoolWithSplToken

// Discriminator para LbPair
pub const LB_PAIR_DISCRIMINATOR: [u8; 8] = [33, 11, 49, 98, 181, 101, 177, 13];

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, PartialEq)]
pub struct LbPair {
    pub parameters: StaticParameters,
    pub v_parameters: VariableParameters,
    pub bump_seed: [u8; 1],
    pub bin_step_seed: [u8; 2],
    pub pair_type: u8,
    pub active_id: i32,
    pub bin_step: u16,
    pub status: u8,
    pub require_base_factor_seed: u8,
    pub base_factor_seed: [u8; 2],
    pub activation_type: u8,
    pub padding0: u8,
    pub token_x_mint: SolanaPubkey,
    pub token_y_mint: SolanaPubkey,
    pub reserve_x: SolanaPubkey,
    pub reserve_y: SolanaPubkey,
    pub protocol_fee: ProtocolFee,
    pub padding1: [u8; 32],
    pub reward_infos: [RewardInfo; 2],
    pub oracle: SolanaPubkey,
    pub bin_array_bitmap: [u64; 16],
    pub last_updated_at: i64,
    pub padding2: [u8; 32],
    pub pre_activation_swap_address: SolanaPubkey,
    pub base_key: SolanaPubkey,
    pub activation_point: u64,
    pub pre_activation_duration: u64,
    pub padding3: [u8; 8],
    pub padding4: u64,
    pub creator: SolanaPubkey,
    pub reserved: [u8; 24],
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, PartialEq)]
pub struct StaticParameters {
    pub base_factor: u16,
    pub filter_period: u16,
    pub decay_period: u16,
    pub reduction_factor: u16,
    pub variable_fee_control: u32,
    pub max_volatility_accumulator: u32,
    pub min_bin_id: i32,
    pub max_bin_id: i32,
    pub protocol_share: u16,
    pub padding: [u8; 6],
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, PartialEq)]
pub struct VariableParameters {
    pub volatility_accumulator: u32,
    pub volatility_reference: u32,
    pub index_reference: i32,
    pub padding: [u8; 4],
    pub last_update_timestamp: i64,
    pub padding1: [u8; 8],
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, PartialEq)]
pub struct ProtocolFee {
    pub amount_x: u64,
    pub amount_y: u64,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, PartialEq)]
pub struct RewardInfo {
    pub mint: SolanaPubkey,
    pub vault: SolanaPubkey,
    pub funder: SolanaPubkey,
    pub reward_duration: u64,
    pub reward_duration_end: u64,
    pub reward_rate: u128,
    pub last_update_time: u64,
    pub cumulative_seconds_with_empty_liquidity_reward: u64,
}

impl LbPair {
    /// Tamaño del LbPair (sin discriminator)
    pub const SIZE: usize = 1040;

    /// Deserializar LbPair desde bytes usando Borsh
    pub fn try_from_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        use std::io::Read;

        // Verificar tamaño mínimo
        if data.len() < Self::SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Data too short: {} bytes, expected at least {}", data.len(), Self::SIZE),
            ));
        }

        // Intentar primero con discriminator (8 bytes + 1040)
        if data.len() >= Self::SIZE + 8 {
            let disc = &data[0..8];
            if disc == LB_PAIR_DISCRIMINATOR {
                // Tiene discriminator, deserializar desde byte 8
                let mut reader = &data[8..];
                return LbPair::deserialize(&mut reader);
            }
        }

        // Intentar sin discriminator
        let mut reader = &data[0..];
        LbPair::deserialize(&mut reader)
    }

    /// Método de compatibilidad
    pub fn try_deserialize(data: &[u8]) -> Result<Self, std::io::Error> {
        Self::try_from_bytes(data)
    }

    /// Obtener el precio actual del pool DLMM
    /// Fórmula oficial Meteora DLMM: price = (1 + bin_step / 10000) ^ active_id
    /// Ver: https://docs.meteora.ag/overview/products/dlmm/dlmm-formulas
    ///
    /// Usa rust_decimal para manejar active_id extremos (fuera del rango ±443636)
    /// que no se pueden calcular con f64::powi() o Q64.64 fixed-point.
    pub fn get_price(&self) -> f64 {
        // Calcular usando Decimal para manejar exponentes extremos
        // Mismo approach que el SDK de TypeScript con decimal.js
        let bin_step_fraction = Decimal::from(self.bin_step) / Decimal::from(10000);
        let base = Decimal::ONE + bin_step_fraction;

        // Para exponentes muy grandes, usar método logarítmico: price = exp(active_id * ln(base))
        // Esto evita overflow incluso con active_id en millones
        match Self::pow_with_log(base, self.active_id) {
            Some(decimal_price) => {
                decimal_price.to_f64().unwrap_or(f64::NAN)
            }
            None => f64::NAN,
        }
    }

    /// Calcular base^exp usando logaritmos para manejar exponentes extremos
    /// Formula: base^exp = exp(exp * ln(base))
    fn pow_with_log(base: Decimal, exp: i32) -> Option<Decimal> {
        if base <= Decimal::ZERO {
            return None;
        }

        if exp == 0 {
            return Some(Decimal::ONE);
        }

        // Convertir a f64 para usar ln() y exp()
        let base_f64 = base.to_f64()?;
        let ln_base = base_f64.ln();
        let result_ln = exp as f64 * ln_base;

        // Verificar overflow antes de calcular exp()
        if result_ln.abs() > 700.0 {  // e^700 ~ 10^304, límite de f64
            return None;
        }

        let result_f64 = result_ln.exp();
        Decimal::from_f64(result_f64)
    }

    /// Obtener las reservas (necesitas consultar las cuentas reserve_x y reserve_y)
    pub fn get_reserves(&self) -> (SolanaPubkey, SolanaPubkey) {
        (self.reserve_x, self.reserve_y)
    }
}

#[derive(Debug, Clone)]
pub struct LbPairInfo {
    pub address: SolanaPubkey,
    pub pool: LbPair,
}

impl LbPairInfo {
    pub fn new(address: SolanaPubkey, pool: LbPair) -> Self {
        Self { address, pool }
    }
}
