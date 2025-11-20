use anyhow::{Context, Result};
use borsh::{BorshSerialize, BorshDeserialize};
use sha2::{Sha256, Digest};
use std::str::FromStr;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::meteora::Pool;

/// Parámetros para swap en Meteora DAMM V2
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
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

            // 8-9. LP mints - NO EXISTEN EN DAMM V2 (constant product AMM)
            // AccountMeta::new_readonly(pool.pool_token_mint, false),
            // AccountMeta::new_readonly(pool.pool_token_mint, false),  // Placeholder
            AccountMeta::new(pool.token_a_vault, false),  // Placeholder - FIXME
            AccountMeta::new(pool.token_b_vault, false),  // Placeholder - FIXME

            // 10-11. LP accounts - NO EXISTEN EN DAMM V2
            AccountMeta::new(pool.token_a_vault, false),  // Placeholder - FIXME
            AccountMeta::new(pool.token_b_vault, false),  // Placeholder - FIXME

            // 12. Protocol fee receiver (mut) - TEMPORAL: usar creator
            AccountMeta::new(pool.creator, false),  // FIXME: should be protocol fee receiver

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
    /// Discriminador exacto obtenido del IDL oficial de Meteora DAMM V2:
    /// https://solscan.io/account/cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG
    ///
    /// ✅ VERIFICADO: [248, 198, 158, 145, 225, 117, 135, 200]
    fn get_swap_discriminator(&self) -> [u8; 8] {
        // Discriminador exacto del IDL oficial
        [248, 198, 158, 145, 225, 117, 135, 200]
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

    /// Versión COMPLETA del swap con todas las 14 cuentas requeridas
    ///
    /// Basado en la estructura oficial de Meteora DAMM V2 (Shyft + SDK oficial)
    pub fn build_complete_swap_instruction(
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

        // Pool authority es una dirección FIJA (no un PDA derivado)
        // Valor del IDL oficial: HLnpSz9h2S4hiLQ43rnSD9XkcUThA7B8hQMKmDaiTLcC
        let pool_authority = Pubkey::from_str("HLnpSz9h2S4hiLQ43rnSD9XkcUThA7B8hQMKmDaiTLcC")
            .expect("Invalid pool authority address");

        // Derivar event_authority PDA
        let (event_authority, _) = Pubkey::find_program_address(
            &[b"__event_authority"],
            &self.program_id,
        );

        // Las 14 cuentas requeridas en ORDEN EXACTO
        let accounts = vec![
            // 0. pool_authority (PDA)
            AccountMeta::new_readonly(pool_authority, false),

            // 1. pool
            AccountMeta::new(*pool_address, false),

            // 2. input_token_account (user source)
            AccountMeta::new(*user_source_token, false),

            // 3. output_token_account (user destination)
            AccountMeta::new(*user_destination_token, false),

            // 4. token_a_vault
            AccountMeta::new(pool.token_a_vault, false),

            // 5. token_b_vault
            AccountMeta::new(pool.token_b_vault, false),

            // 6. token_a_mint
            AccountMeta::new_readonly(pool.token_a_mint, false),

            // 7. token_b_mint
            AccountMeta::new_readonly(pool.token_b_mint, false),

            // 8. payer (user, signer) - WRITABLE
            AccountMeta::new(*user, true),

            // 9. token_a_program (SPL Token o Token-2022)
            AccountMeta::new_readonly(spl_token::id(), false),

            // 10. token_b_program (SPL Token o Token-2022)
            AccountMeta::new_readonly(spl_token::id(), false),

            // 11. referral_token_account (opcional pero DEBE estar presente)
            // Si no hay referral, usar la misma cuenta que output o Pubkey::default()
            AccountMeta::new(*user_destination_token, false),

            // 12. event_authority (PDA)
            AccountMeta::new_readonly(event_authority, false),

            // 13. program (el programa mismo)
            AccountMeta::new_readonly(self.program_id, false),
        ];

        Ok(Instruction {
            program_id: self.program_id,
            accounts,
            data,
        })
    }

    /// Versión simplificada del swap usando solo las cuentas esenciales
    ///
    /// ADVERTENCIA: Esta versión está DESACTUALIZADA y puede fallar
    /// USA build_complete_swap_instruction() en su lugar
    #[deprecated(note = "Use build_complete_swap_instruction instead")]
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
