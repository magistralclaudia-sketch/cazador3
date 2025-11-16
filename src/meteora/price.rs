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
    pub fn calculate_price(pool: &Pool) -> f64 {
        let sqrt_price = pool.sqrt_price as f64;
        let q64_float = Self::Q64 as f64;

        // Convertir sqrt_price de Q64.64 a float
        let sqrt_p = sqrt_price / q64_float;

        // Elevar al cuadrado para obtener el precio
        let price = sqrt_p * sqrt_p;

        // Ajustar por decimales de los tokens
        let decimal_adjustment = 10f64.pow(
            (pool.token_a_decimals as i32 - pool.token_b_decimals as i32) as f64
        );

        price * decimal_adjustment
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

    /// Calcular el precio en formato más legible con ajuste de decimales
    ///
    /// Retorna: (precio, token_a_decimals, token_b_decimals)
    pub fn calculate_price_with_decimals(pool: &Pool) -> (f64, u8, u8) {
        let price = Self::calculate_price(pool);
        (price, pool.token_a_decimals, pool.token_b_decimals)
    }

    /// Estimar cuántos tokens B recibirás por una cantidad de tokens A
    /// Usando la fórmula de constant product: x * y = k
    ///
    /// Esto es una estimación aproximada sin considerar fees
    pub fn estimate_swap_output(
        pool: &Pool,
        amount_in: u64,
        is_a_to_b: bool,
    ) -> u64 {
        let price = Self::calculate_price(pool);

        if is_a_to_b {
            // Swapping A -> B
            (amount_in as f64 * price) as u64
        } else {
            // Swapping B -> A
            (amount_in as f64 / price) as u64
        }
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
        pool.sqrt_price >= pool.sqrt_price_min && pool.sqrt_price <= pool.sqrt_price_max
    }

    /// Calcular el precio mínimo del pool
    pub fn calculate_min_price(pool: &Pool) -> f64 {
        let sqrt_price_min = pool.sqrt_price_min as f64;
        let q64_float = Self::Q64 as f64;
        let sqrt_p = sqrt_price_min / q64_float;
        sqrt_p * sqrt_p
    }

    /// Calcular el precio máximo del pool
    pub fn calculate_max_price(pool: &Pool) -> f64 {
        let sqrt_price_max = pool.sqrt_price_max as f64;
        let q64_float = Self::Q64 as f64;
        let sqrt_p = sqrt_price_max / q64_float;
        sqrt_p * sqrt_p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_calculation() {
        // Crear un pool de prueba
        let mut pool = Pool {
            sqrt_price: 1u128 << 64, // sqrt_price = 1.0, entonces price = 1.0
            token_a_decimals: 9,
            token_b_decimals: 9,
            ..Default::default()
        };

        let price = PriceCalculator::calculate_price(&pool);
        assert!((price - 1.0).abs() < 0.0001);
    }
}
