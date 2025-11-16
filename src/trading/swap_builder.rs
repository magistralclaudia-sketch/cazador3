use anchor_lang::{AnchorDeserialize, AnchorSerialize};
use anyhow::{Context, Result};
use sha2::{Sha256, Digest};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::meteora::Pool;

/// Parámetros para swap en Meteora DAMM V2
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct SwapParameters {
    /// Cantidad exacta de entrada
    pub amount_in: u64,
    /// Cantidad mínima de salida (protección de slippage)
    pub minimum_amount_out: u64,
}

/// Constructor de instrucciones de swap para Meteora DAMM V2
pub struct SwapInstructionBuilder {
    program_id: Pubkey,
}

impl SwapInstructionBuilder {
    pub fn new(program_id: Pubkey) -> Self {
        Self { program_id }
    }

    /// Construir instrucción de swap para Meteora DAMM V2
    ///
    /// Basado en el programa cp-amm de Meteora:
    /// https://github.com/MeteoraAg/damm-v2
    ///
    /// Estructura de cuentas (según CPI example):
    /// 1. pool - La cuenta del pool (mut)
    /// 2. user_source_token - Cuenta de token de origen del usuario (mut)
    /// 3. user_destination_token - Cuenta de token de destino del usuario (mut)
    /// 4. a_vault - Vault de token A del pool (mut)
    /// 5. b_vault - Vault de token B del pool (mut)
    /// 6. a_token_vault - Token account del vault A (mut)
    /// 7. b_token_vault - Token account del vault B (mut)
    /// 8. a_vault_lp_mint - LP mint del vault A
    /// 9. b_vault_lp_mint - LP mint del vault B
    /// 10. a_vault_lp - LP account del vault A
    /// 11. b_vault_lp - LP account del vault B
    /// 12. protocol_token_fee - Cuenta para fees de protocolo (mut)
    /// 13. user - Usuario que firma la transacción (signer)
    /// 14. vault_program - Programa de vault
    /// 15. token_program - Programa de tokens
    pub fn build_swap_instruction(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
        user: &Pubkey,
        user_source_token: &Pubkey,
        user_destination_token: &Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<Instruction> {
        // Discriminador para la instrucción "swap" en Anchor
        // Esto es el hash SHA256 de "global:swap" truncado a 8 bytes
        // Nota: El discriminador exacto puede variar, obtenerlo del IDL oficial
        let discriminator = self.get_swap_discriminator();

        // Parámetros del swap
        let params = SwapParameters {
            amount_in,
            minimum_amount_out,
        };

        // Serializar data de la instrucción
        let mut data = discriminator.to_vec();
        params.serialize(&mut data)
            .context("Failed to serialize swap parameters")?;

        // Construir cuentas necesarias para el swap
        let accounts = vec![
            // 1. Pool (mut)
            AccountMeta::new(*pool_address, false),

            // 2. User source token account (mut)
            AccountMeta::new(*user_source_token, false),

            // 3. User destination token account (mut)
            AccountMeta::new(*user_destination_token, false),

            // 4-5. Vaults A y B del pool (mut)
            AccountMeta::new(pool.token_a_vault, false),
            AccountMeta::new(pool.token_b_vault, false),

            // 6-7. Token accounts de los vaults (derivados o del pool state)
            // Nota: Estos pueden necesitar derivación de PDAs
            AccountMeta::new(pool.token_a_vault, false),  // Placeholder
            AccountMeta::new(pool.token_b_vault, false),  // Placeholder

            // 8-9. LP mints
            AccountMeta::new_readonly(pool.pool_token_mint, false),
            AccountMeta::new_readonly(pool.pool_token_mint, false),  // Placeholder

            // 10-11. LP accounts
            AccountMeta::new(pool.token_a_vault, false),  // Placeholder
            AccountMeta::new(pool.token_b_vault, false),  // Placeholder

            // 12. Protocol fee receiver (mut)
            AccountMeta::new(pool.fee_receiver, false),

            // 13. User (signer)
            AccountMeta::new(*user, true),

            // 14. Vault program (si existe, sino puede ser el mismo program_id)
            AccountMeta::new_readonly(self.program_id, false),

            // 15. Token program
            AccountMeta::new_readonly(spl_token::id(), false),
        ];

        Ok(Instruction {
            program_id: self.program_id,
            accounts,
            data,
        })
    }

    /// Obtener discriminador de la instrucción swap
    ///
    /// Calcula el discriminador usando el método estándar de Anchor:
    /// discriminador = sha256("global:swap")[0..8]
    ///
    /// Meteora DAMM v2 usa instrucciones Anchor, por lo que este método es correcto.
    fn get_swap_discriminator(&self) -> [u8; 8] {
        Self::calculate_anchor_discriminator("global", "swap")
    }

    /// Calcular discriminador Anchor para cualquier instrucción
    ///
    /// Anchor usa: sha256("namespace:instruction_name")[0..8]
    fn calculate_anchor_discriminator(namespace: &str, name: &str) -> [u8; 8] {
        let preimage = format!("{}:{}", namespace, name);
        let mut hasher = Sha256::new();
        hasher.update(preimage.as_bytes());
        let hash = hasher.finalize();

        let mut discriminator = [0u8; 8];
        discriminator.copy_from_slice(&hash[..8]);
        discriminator
    }

    /// Versión simplificada del swap usando solo las cuentas esenciales
    ///
    /// Esta es una implementación mínima que puede funcionar
    /// dependiendo de cómo esté configurado el programa
    pub fn build_simple_swap_instruction(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
        user: &Pubkey,
        user_source_token: &Pubkey,
        user_destination_token: &Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<Instruction> {
        let discriminator = self.get_swap_discriminator();

        let params = SwapParameters {
            amount_in,
            minimum_amount_out,
        };

        let mut data = discriminator.to_vec();
        params.serialize(&mut data)?;

        // Versión simplificada con cuentas esenciales
        let accounts = vec![
            AccountMeta::new(*pool_address, false),
            AccountMeta::new(*user_source_token, false),
            AccountMeta::new(*user_destination_token, false),
            AccountMeta::new(pool.token_a_vault, false),
            AccountMeta::new(pool.token_b_vault, false),
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(spl_token::id(), false),
        ];

        Ok(Instruction {
            program_id: self.program_id,
            accounts,
            data,
        })
    }
}

/// Helper para encontrar cuentas de token asociadas
pub fn get_associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(wallet, mint)
}

// NOTA CRÍTICA PARA EL USUARIO:
// ===============================
// Esta implementación es basada en patrones comunes de Anchor y el análisis
// del CPI example de Meteora. Sin embargo, para garantizar que funcione en
// producción, DEBES:
//
// 1. Obtener el IDL oficial del programa:
//    anchor idl fetch cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG -o meteora.json
//
// 2. Verificar el discriminador exacto de la instrucción "swap"
//
// 3. Verificar la estructura exacta de cuentas requeridas
//
// 4. Considerar usar Jupiter Aggregator que maneja Meteora automáticamente:
//    - Más confiable
//    - Maneja routing automático
//    - Siempre actualizado
//
// Alternativamente, puedes usar el SDK de TypeScript como referencia:
// https://github.com/MeteoraAg/damm-v2-sdk
