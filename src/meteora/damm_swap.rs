use anyhow::{Context, Result};
use borsh::BorshSerialize;
use sha2::{Digest, Sha256};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use spl_token::ID as TOKEN_PROGRAM_ID;
use spl_associated_token_account::get_associated_token_address;

use crate::meteora::Pool;

/// DAMM V2 Program ID
pub const DAMM_V2_PROGRAM_ID: &str = "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG";

/// Parámetros para swap2 (versión actual)
#[derive(BorshSerialize, Debug, Clone)]
pub struct SwapParameters2 {
    /// Cuando es exact in: amount_in. Cuando es exact out: amount_out
    pub amount_0: u64,
    /// Cuando es exact in: minimum_amount_out. Cuando es exact out: maximum_amount_in
    pub amount_1: u64,
    /// Swap mode: 0 = ExactIn, 1 = PartialFill, 2 = ExactOut
    pub swap_mode: u8,
}

#[derive(Debug, Clone, Copy)]
pub enum SwapMode {
    ExactIn = 0,
    PartialFill = 1,
    ExactOut = 2,
}

/// Constructor de instrucciones de swap para DAMM V2
pub struct DammSwapBuilder {
    program_id: Pubkey,
}

impl DammSwapBuilder {
    pub fn new() -> Result<Self> {
        let program_id = DAMM_V2_PROGRAM_ID.parse()?;
        Ok(Self { program_id })
    }

    pub fn with_program_id(program_id: Pubkey) -> Self {
        Self { program_id }
    }

    /// Calcular discriminator de Anchor para "global:swap2"
    fn get_swap2_discriminator() -> [u8; 8] {
        let mut hasher = Sha256::new();
        hasher.update(b"global:swap2");
        let result = hasher.finalize();
        let mut discriminator = [0u8; 8];
        discriminator.copy_from_slice(&result[..8]);
        discriminator
    }

    /// Derivar pool_authority PDA
    /// Seed: ["pool_authority"]
    pub fn derive_pool_authority(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"pool_authority"],
            &self.program_id,
        )
    }

    /// Construir instrucción de swap para comprar token nuevo
    ///
    /// Compra: SOL (input) -> TOKEN (output)
    /// - input_mint = WSOL (So11111111111111111111111111111111111111112)
    /// - output_mint = El token nuevo del pool
    ///
    /// # Arguments
    /// * `pool_address` - Dirección del pool
    /// * `pool` - Datos del pool
    /// * `user` - Wallet del usuario (signer)
    /// * `amount_sol_in` - Cantidad de SOL a gastar (en lamports)
    /// * `minimum_token_out` - Mínima cantidad de tokens a recibir (slippage protection)
    pub fn build_buy_instruction(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
        user: &Pubkey,
        amount_sol_in: u64,
        minimum_token_out: u64,
    ) -> Result<Instruction> {
        // WSOL mint address
        let wsol_mint = "So11111111111111111111111111111111111111112".parse::<Pubkey>()?;

        // Determinar cuál token es SOL y cuál es el nuevo token
        let token_a_mint = pool.token_a_mint_solana();
        let token_b_mint = pool.token_b_mint_solana();

        let (input_is_a, input_mint, output_mint) = if token_a_mint == wsol_mint {
            (true, token_a_mint, token_b_mint)
        } else if token_b_mint == wsol_mint {
            (false, token_b_mint, token_a_mint)
        } else {
            return Err(anyhow::anyhow!("Pool no contiene WSOL, no se puede comprar con SOL"));
        };

        // ATAs del usuario
        let user_input_token = get_associated_token_address(user, &input_mint);
        let user_output_token = get_associated_token_address(user, &output_mint);

        // Vaults del pool
        let (token_a_vault, token_b_vault) = (
            pool.token_a_vault_solana(),
            pool.token_b_vault_solana(),
        );

        // Pool authority PDA
        let (pool_authority, _bump) = self.derive_pool_authority();

        // Parámetros del swap
        let params = SwapParameters2 {
            amount_0: amount_sol_in,
            amount_1: minimum_token_out,
            swap_mode: SwapMode::ExactIn as u8,
        };

        // Serializar data
        let discriminator = Self::get_swap2_discriminator();
        let mut data = discriminator.to_vec();
        params.serialize(&mut data)?;

        // Construir cuentas según SwapCtx
        let accounts = vec![
            // 0. pool_authority (PDA)
            AccountMeta::new_readonly(pool_authority, false),

            // 1. pool (mut)
            AccountMeta::new(*pool_address, false),

            // 2. input_token_account (mut) - user's WSOL ATA
            AccountMeta::new(user_input_token, false),

            // 3. output_token_account (mut) - user's token ATA
            AccountMeta::new(user_output_token, false),

            // 4. token_a_vault (mut)
            AccountMeta::new(token_a_vault, false),

            // 5. token_b_vault (mut)
            AccountMeta::new(token_b_vault, false),

            // 6. token_a_mint (readonly)
            AccountMeta::new_readonly(token_a_mint, false),

            // 7. token_b_mint (readonly)
            AccountMeta::new_readonly(token_b_mint, false),

            // 8. payer (signer)
            AccountMeta::new_readonly(*user, true),

            // 9. token_a_program (Token Program)
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),

            // 10. token_b_program (Token Program)
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        ];

        Ok(Instruction {
            program_id: self.program_id,
            accounts,
            data,
        })
    }

    /// Construir instrucciones completas para comprar incluyendo creación de ATAs si es necesario
    pub fn build_buy_instructions_with_atas(
        &self,
        pool_address: &Pubkey,
        pool: &Pool,
        user: &Pubkey,
        amount_sol_in: u64,
        minimum_token_out: u64,
    ) -> Result<Vec<Instruction>> {
        let mut instructions = Vec::new();

        let token_a_mint = pool.token_a_mint_solana();
        let token_b_mint = pool.token_b_mint_solana();
        let wsol_mint = "So11111111111111111111111111111111111111112".parse::<Pubkey>()?;

        // Determinar cuál es el token output (el que NO es WSOL)
        let output_mint = if token_a_mint == wsol_mint {
            token_b_mint
        } else if token_b_mint == wsol_mint {
            token_a_mint
        } else {
            return Err(anyhow::anyhow!("Pool no contiene WSOL"));
        };

        // Crear ATA para el token de salida si no existe
        let user_output_ata = get_associated_token_address(user, &output_mint);
        instructions.push(
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                user,
                user,
                &output_mint,
                &TOKEN_PROGRAM_ID,
            )
        );

        // Crear WSOL ATA si no existe
        let user_wsol_ata = get_associated_token_address(user, &wsol_mint);
        instructions.push(
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                user,
                user,
                &wsol_mint,
                &TOKEN_PROGRAM_ID,
            )
        );

        // Wrap SOL -> WSOL (transferir SOL al ATA)
        instructions.push(
            solana_sdk::system_instruction::transfer(
                user,
                &user_wsol_ata,
                amount_sol_in,
            )
        );

        // Sync native (necesario después de transferir SOL)
        instructions.push(
            spl_token::instruction::sync_native(&TOKEN_PROGRAM_ID, &user_wsol_ata)?
        );

        // Instrucción de swap
        instructions.push(self.build_buy_instruction(
            pool_address,
            pool,
            user,
            amount_sol_in,
            minimum_token_out,
        )?);

        // Close WSOL account para recuperar SOL sobrante
        instructions.push(
            spl_token::instruction::close_account(
                &TOKEN_PROGRAM_ID,
                &user_wsol_ata,
                user,
                user,
                &[],
            )?
        );

        Ok(instructions)
    }
}

impl Default for DammSwapBuilder {
    fn default() -> Self {
        Self::new().expect("Failed to create DammSwapBuilder")
    }
}
