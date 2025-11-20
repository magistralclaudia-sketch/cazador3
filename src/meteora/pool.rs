use anchor_lang::prelude::Pubkey;
use solana_sdk::pubkey::Pubkey as SolanaPubkey;

/// Meteora DAMM V2 Pool State (OFICIAL - copiado del source code)
/// Tamaño exacto: 1104 bytes
/// Basado en: https://github.com/MeteoraAg/damm-v2/blob/main/programs/cp-amm/src/state/pool.rs
///
/// Usamos #[zero_copy] en lugar de #[account(zero_copy)] porque no estamos
/// en el contexto de un programa Anchor, solo queremos deserializar
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Pool {
    /// Pool fee
    pub pool_fees: PoolFeesStruct,
    /// token a mint
    pub token_a_mint: Pubkey,
    /// token b mint
    pub token_b_mint: Pubkey,
    /// token a vault
    pub token_a_vault: Pubkey,
    /// token b vault
    pub token_b_vault: Pubkey,
    /// Whitelisted vault to be able to buy pool before activation_point
    pub whitelisted_vault: Pubkey,
    /// partner
    pub partner: Pubkey,
    /// liquidity share
    pub liquidity: u128,
    /// padding, previous reserve amount, be careful to use that field
    pub _padding: u128,
    /// protocol a fee
    pub protocol_a_fee: u64,
    /// protocol b fee
    pub protocol_b_fee: u64,
    /// partner a fee
    pub partner_a_fee: u64,
    /// partner b fee
    pub partner_b_fee: u64,
    /// min price
    pub sqrt_min_price: u128,
    /// max price
    pub sqrt_max_price: u128,
    /// current price
    pub sqrt_price: u128,
    /// Activation point, can be slot or timestamp
    pub activation_point: u64,
    /// Activation type, 0 means by slot, 1 means by timestamp
    pub activation_type: u8,
    /// pool status, 0: enable, 1 disable
    pub pool_status: u8,
    /// token a flag
    pub token_a_flag: u8,
    /// token b flag
    pub token_b_flag: u8,
    /// 0 is collect fee in both token, 1 only collect fee in token a, 2 only collect fee in token b
    pub collect_fee_mode: u8,
    /// pool type
    pub pool_type: u8,
    /// pool version, 0: max_fee is still capped at 50%, 1: max_fee is capped at 99%
    pub version: u8,
    /// padding
    pub _padding_0: u8,
    /// cumulative
    pub fee_a_per_liquidity: [u8; 32], // U256
    /// cumulative
    pub fee_b_per_liquidity: [u8; 32], // U256
    pub permanent_lock_liquidity: u128,
    /// metrics
    pub metrics: PoolMetrics,
    /// pool creator
    pub creator: Pubkey,
    /// Padding for further use
    pub _padding_1: [u64; 6],
    /// Farming reward information
    pub reward_infos: [RewardInfo; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct PoolFeesStruct {
    /// Trade fees are extra token amounts that are held inside the token
    /// accounts during a trade, making the value of liquidity tokens rise.
    /// Trade fee numerator
    pub base_fee: BaseFeeStruct,

    /// Protocol trading fees are extra token amounts that are held inside the token
    /// accounts during a trade, with the equivalent in pool tokens minted to
    /// the protocol of the program.
    /// Protocol trade fee numerator
    pub protocol_fee_percent: u8,
    /// partner fee
    pub partner_fee_percent: u8,
    /// referral fee
    pub referral_fee_percent: u8,
    /// padding
    pub padding_0: [u8; 5],

    /// dynamic fee
    pub dynamic_fee: DynamicFeeStruct,

    /// padding
    pub padding_1: [u64; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BaseFeeStruct {
    pub cliff_fee_numerator: u64,
    pub base_fee_mode: u8,
    pub padding_0: [u8; 5],
    pub first_factor: u16,
    pub second_factor: [u8; 8],
    pub third_factor: u64,
    pub padding_1: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DynamicFeeStruct {
    pub initialized: u8,
    pub padding: [u8; 7],
    pub max_volatility_accumulator: u32,
    pub variable_fee_control: u32,
    pub bin_step: u16,
    pub filter_period: u16,
    pub decay_period: u16,
    pub reduction_factor: u16,
    pub last_update_timestamp: u64,
    pub bin_step_u128: u128,
    pub sqrt_price_reference: u128,
    pub volatility_accumulator: u128,
    pub volatility_reference: u128,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct PoolMetrics {
    pub total_lp_a_fee: u128,
    pub total_lp_b_fee: u128,
    pub total_protocol_a_fee: u64,
    pub total_protocol_b_fee: u64,
    pub total_partner_a_fee: u64,
    pub total_partner_b_fee: u64,
    pub total_position: u64,
    pub padding: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct RewardInfo {
    /// Indicates if the reward has been initialized (1 byte)
    pub initialized: u8,
    /// reward token flag (1 byte)
    pub reward_token_flag: u8,
    /// padding (6 bytes)
    pub _padding_0: [u8; 6],
    /// Padding to ensure reward_rate is 16-byte aligned (8 bytes)
    pub _padding_1: [u8; 8],
    /// Reward token mint (32 bytes)
    pub mint: Pubkey,
    /// Reward vault (32 bytes)
    pub vault: Pubkey,
    /// Funder (32 bytes)
    pub funder: Pubkey,
    /// reward duration (8 bytes)
    pub reward_duration: u64,
    /// reward duration end (8 bytes)
    pub reward_duration_end: u64,
    /// reward rate (16 bytes)
    pub reward_rate: u128,
    /// Reward per token stored (32 bytes - U256)
    pub reward_per_token_stored: [u8; 32],
    /// Last update time (8 bytes)
    pub last_update_time: u64,
    /// Cumulative seconds with empty liquidity reward (8 bytes)
    pub cumulative_seconds_with_empty_liquidity_reward: u64,
}

impl Pool {
    /// Discriminador Anchor para Pool
    pub const DISCRIMINATOR: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];

    /// Tamaño del Pool (sin discriminator)
    pub const SIZE: usize = 1104;

    /// Deserializar Pool desde bytes usando bytemuck (como lo hace Meteora oficialmente)
    pub fn try_from_bytes(data: &[u8]) -> Result<&Self, std::io::Error> {
        // Verificar tamaño mínimo
        if data.len() < Self::SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Data too short: {} bytes, expected at least {}", data.len(), Self::SIZE),
            ));
        }

        // Intentar primero con discriminator (8 bytes + 1104)
        if data.len() >= Self::SIZE + 8 {
            let disc = &data[0..8];
            if disc == Self::DISCRIMINATOR {
                // Tiene discriminator, saltar los primeros 8 bytes
                let pool_data = &data[8..8 + Self::SIZE];
                return bytemuck::try_from_bytes(pool_data)
                    .map_err(|e| std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to deserialize with discriminator: {:?}", e),
                    ));
            }
        }

        // Intentar sin discriminator (solo 1104 bytes)
        bytemuck::try_from_bytes(&data[0..Self::SIZE])
            .map_err(|e| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to deserialize: {:?}", e),
            ))
    }

    /// Método de compatibilidad que retorna copia del Pool
    pub fn try_deserialize(data: &[u8]) -> Result<Self, std::io::Error> {
        Self::try_from_bytes(data).map(|pool_ref| *pool_ref)
    }

    /// Convertir Anchor Pubkey a Solana SDK Pubkey
    pub fn token_a_mint_solana(&self) -> SolanaPubkey {
        SolanaPubkey::new_from_array(self.token_a_mint.to_bytes())
    }

    pub fn token_b_mint_solana(&self) -> SolanaPubkey {
        SolanaPubkey::new_from_array(self.token_b_mint.to_bytes())
    }

    pub fn token_a_vault_solana(&self) -> SolanaPubkey {
        SolanaPubkey::new_from_array(self.token_a_vault.to_bytes())
    }

    pub fn token_b_vault_solana(&self) -> SolanaPubkey {
        SolanaPubkey::new_from_array(self.token_b_vault.to_bytes())
    }

    /// Get reserves (token A and B amounts in the pool)
    /// NOTA: Para obtener las cantidades exactas necesitas consultar los vaults
    pub fn get_reserves(&self) -> (u64, u64) {
        // Estos valores deben obtenerse consultando las cuentas de vault
        // Aquí solo retornamos dummy values
        (0, 0)
    }
}

#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub address: SolanaPubkey,
    pub pool: Pool,
}

impl PoolInfo {
    pub fn new(address: SolanaPubkey, pool: Pool) -> Self {
        Self { address, pool }
    }
}

// Unsafe impls para que bytemuck funcione (como en el código oficial de Meteora)
unsafe impl bytemuck::Pod for Pool {}
unsafe impl bytemuck::Zeroable for Pool {}

unsafe impl bytemuck::Pod for PoolFeesStruct {}
unsafe impl bytemuck::Zeroable for PoolFeesStruct {}

unsafe impl bytemuck::Pod for BaseFeeStruct {}
unsafe impl bytemuck::Zeroable for BaseFeeStruct {}

unsafe impl bytemuck::Pod for DynamicFeeStruct {}
unsafe impl bytemuck::Zeroable for DynamicFeeStruct {}

unsafe impl bytemuck::Pod for PoolMetrics {}
unsafe impl bytemuck::Zeroable for PoolMetrics {}

unsafe impl bytemuck::Pod for RewardInfo {}
unsafe impl bytemuck::Zeroable for RewardInfo {}
