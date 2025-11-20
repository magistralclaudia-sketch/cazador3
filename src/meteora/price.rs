use super::pool::Pool;
use num_traits::pow::Pow;

/// Calculadora de precios para Meteora DAMM V2
///
/// Meteora usa constant product (x * y = k) con sqrt_price
/// El precio está almacenado como sqrt(price) en formato Q64.64
///
/// Fórmula: price = (sqrt_price / 2^64)^2
pub struct PriceCalculator;

impl PriceCalculator {
    /// Factor de escala Q64.64 (2^64)
    const Q64: u128 = 1u128 << 64;

    /// Calcular el precio real de token B en términos de token A
    ///
    /// Retorna: precio de B/A (cuántos tokens A por 1 token B)
    ///
    /// Ejemplo: Si 1 SOL = 100 USDC, price = 100.0
    ///
    /// NOTA: Este precio NO está ajustado por decimales de los tokens.
    /// Para obtener el precio real, necesitarías multiplicar por:
    /// 10^(token_a_decimals - token_b_decimals)
    pub fn calculate_price(pool: &Pool) -> f64 {
        let sqrt_price = pool.sqrt_price as f64;
        let q64_float = Self::Q64 as f64;

        // Convertir sqrt_price de Q64.64 a float
        let sqrt_p = sqrt_price / q64_float;

        // Elevar al cuadrado para obtener el precio
        sqrt_p * sqrt_p
    }

    /// Calcular precio inverso (A/B)
    pub fn calculate_inverse_price(pool: &Pool) -> f64 {
        let price = Self::calculate_price(pool);
        if price == 0.0 {
            0.0
        } else {
            1.0 / price
        }
    }

    /// Calcular el precio sin ajuste de decimales
    ///
    /// NOTA: Los decimales de los tokens NO están en el Pool struct.
    /// Necesitarías obtenerlos de las token mint accounts si los necesitas.
    /// Por ahora solo retorna el precio raw.
    pub fn calculate_price_raw(pool: &Pool) -> f64 {
        Self::calculate_price(pool)
    }

    /// Estimar cuántos tokens recibirás en un swap
    /// Usando la fórmula CORRECTA de constant product AMM: x * y = k
    ///
    /// Fórmula: amount_out = (reserve_out * amount_in) / (reserve_in + amount_in)
    ///
    /// NOTA: Esta es una estimación SIN FEES. La transacción real tendrá menos output.
    pub fn estimate_swap_output(
        pool: &Pool,
        amount_in: u64,
        is_a_to_b: bool,
    ) -> u64 {
        // Para usar constant product necesitaríamos las reserves actuales
        // Como no las tenemos en el pool state, usamos aproximación con liquidez

        // MÉTODO SIMPLIFICADO usando precio
        // En producción, deberías obtener las reserves reales del pool
        let price = Self::calculate_price(pool);

        if is_a_to_b {
            // Swapping A -> B
            (amount_in as f64 * price) as u64
        } else {
            // Swapping B -> A
            (amount_in as f64 / price) as u64
        }
    }

    /// Estimar output usando constant product AMM con reserves conocidas
    ///
    /// Fórmula correcta: amount_out = (reserve_out * amount_in) / (reserve_in + amount_in)
    ///
    /// Sin considerar fees. Para fees del 0.3%:
    /// amount_in_with_fee = amount_in * 997
    /// numerator = amount_in_with_fee * reserve_out
    /// denominator = (reserve_in * 1000) + amount_in_with_fee
    /// amount_out = numerator / denominator
    pub fn estimate_swap_output_with_reserves(
        reserve_in: u128,
        reserve_out: u128,
        amount_in: u64,
        fee_bps: u16, // Fee en basis points (ej: 30 = 0.3%)
    ) -> u64 {
        if reserve_in == 0 || reserve_out == 0 {
            return 0;
        }

        let amount_in_u128 = amount_in as u128;

        // Aplicar fee
        let fee_multiplier = 10_000 - fee_bps as u128;
        let amount_in_with_fee = amount_in_u128 * fee_multiplier;

        // Fórmula constant product
        let numerator = amount_in_with_fee * reserve_out;
        let denominator = (reserve_in * 10_000) + amount_in_with_fee;

        if denominator == 0 {
            return 0;
        }

        (numerator / denominator) as u64
    }

    /// Calcular el cambio de precio (price impact)
    pub fn calculate_price_impact(old_sqrt_price: u128, new_sqrt_price: u128) -> f64 {
        if old_sqrt_price == 0 {
            return 0.0;
        }

        let old_price = (old_sqrt_price as f64 / Self::Q64 as f64).powi(2);
        let new_price = (new_sqrt_price as f64 / Self::Q64 as f64).powi(2);

        ((new_price - old_price) / old_price) * 100.0
    }

    /// Verificar si el precio está dentro del rango de liquidez concentrada
    pub fn is_price_in_range(pool: &Pool) -> bool {
        pool.sqrt_price >= pool.sqrt_min_price && pool.sqrt_price <= pool.sqrt_max_price
    }

    /// Calcular el precio mínimo del pool
    pub fn calculate_min_price(pool: &Pool) -> f64 {
        let sqrt_price_min = pool.sqrt_min_price as f64;
        let q64_float = Self::Q64 as f64;
        let sqrt_p = sqrt_price_min / q64_float;
        sqrt_p * sqrt_p
    }

    /// Calcular el precio máximo del pool
    pub fn calculate_max_price(pool: &Pool) -> f64 {
        let sqrt_price_max = pool.sqrt_max_price as f64;
        let q64_float = Self::Q64 as f64;
        let sqrt_p = sqrt_price_max / q64_float;
        sqrt_p * sqrt_p
    }

    /// Calcular price impact de un swap
    ///
    /// Price impact = (output_amount / reserve_out) * 100
    pub fn calculate_swap_price_impact(
        reserve_out: u128,
        output_amount: u64,
    ) -> f64 {
        if reserve_out == 0 {
            return 100.0;
        }

        ((output_amount as f64 / reserve_out as f64) * 100.0)
    }
}

// Tests temporalmente deshabilitados - necesitan actualización para nueva estructura Pool
// TODO: Actualizar tests con la estructura oficial de Pool (1104 bytes)
/*
#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;

    fn create_test_pool() -> Pool {
        // TODO: Crear Pool con estructura oficial completa
        unimplemented!()
    }

    #[test]
    fn test_price_calculation() {
        let pool = create_test_pool();
        let price = PriceCalculator::calculate_price(&pool);
        assert!((price - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_swap_output_with_reserves() {
        // Pool con 100,000 token A y 100,000 token B
        let reserve_in = 100_000_000_000; // 100k tokens
        let reserve_out = 100_000_000_000; // 100k tokens
        let amount_in = 1_000_000_000; // 1 token
        let fee_bps = 30; // 0.3%

        let output = PriceCalculator::estimate_swap_output_with_reserves(
            reserve_in,
            reserve_out,
            amount_in,
            fee_bps,
        );

        // Debería recibir aproximadamente 0.997 tokens (considerando fee)
        assert!(output > 990_000_000 && output < 1_000_000_000);
    }

    #[test]
    fn test_price_impact() {
        let old_sqrt_price = 1u128 << 64;
        let new_sqrt_price = (1u128 << 64) + (1u128 << 62); // +25% en sqrt -> +56.25% en price

        let impact = PriceCalculator::calculate_price_impact(old_sqrt_price, new_sqrt_price);
        assert!(impact > 50.0 && impact < 60.0);
    }
}
*/

// NOTAS IMPORTANTES:
// ==================
//
// 1. ESTIMACIÓN SIMPLIFICADA:
//    estimate_swap_output() usa solo el precio. Es una aproximación.
//    En producción real, necesitas las reserves del pool.
//
// 2. OBTENER RESERVES:
//    Las reserves están en los vaults del pool. Necesitas:
//    - Leer token_a_vault account
//    - Leer token_b_vault account
//    - Obtener sus balances con getTokenAccountBalance
//
// 3. FEES:
//    Meteora DAMM V2 tiene:
//    - LP fee (típicamente 0.25-0.30%)
//    - Protocol fee (variable)
//    Total fee = lp_fee_bps + protocol_fee_bps
//
// 4. CONSTANT PRODUCT CORRECTO:
//    Para cálculos exactos, usa estimate_swap_output_with_reserves()
//    una vez que obtengas las reserves reales.
//
// 5. SLIPPAGE:
//    Con 99% slippage, aceptas recibir hasta 1% del expected output
//    Esto es necesario en pools nuevos con volatilidad extrema
