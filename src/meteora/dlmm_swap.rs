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

use crate::meteora::LbPair;

/// DLMM Program ID (Meteora DLMM Mainnet)
pub const DLMM_PROGRAM_ID: &str = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";

/// Parámetros para swap de DLMM
#[derive(BorshSerialize, Debug, Clone)]
pub struct SwapParameters {
    /// Amount to swap in
    pub amount_in: u64,
    /// Minimum amount to receive out (slippage protection)
    pub min_amount_out: u64,
}

/// Constructor de instrucciones de swap para DLMM
pub struct DlmmSwapBuilder {
    program_id: Pubkey,
}

impl DlmmSwapBuilder {
    pub fn new() -> Result<Self> {
        let program_id = DLMM_PROGRAM_ID.parse()?;
        Ok(Self { program_id })
    }

    pub fn with_program_id(program_id: Pubkey) -> Self {
        Self { program_id }
    }

    /// Calcular discriminator de Anchor para "global:swap"
    fn get_swap_discriminator() -> [u8; 8] {
        let mut hasher = Sha256::new();
        hasher.update(b"global:swap");
        let result = hasher.finalize();
        let mut discriminator = [0u8; 8];
        discriminator.copy_from_slice(&result[..8]);
        discriminator
    }

    /// Derivar bin_array_bitmap_extension PDA si existe
    /// La extensión puede no existir en algunos pools, en ese caso usaremos
    /// la misma dirección del lb_pair como placeholder
    pub fn derive_bin_array_bitmap_extension(&self, lb_pair: &Pubkey) -> Pubkey {
        // Seed típica: ["bitmap", lb_pair]
        let (pda, _bump) = Pubkey::find_program_address(
            &[b"bitmap", lb_pair.as_ref()],
            &self.program_id,
        );
        pda
    }

    /// Derivar event_authority PDA
    pub fn derive_event_authority(&self) -> Pubkey {
        let (pda, _bump) = Pubkey::find_program_address(
            &[b"__event_authority"],
            &self.program_id,
        );
        pda
    }

    /// Construir instrucción de swap para comprar token nuevo
    ///
    /// Compra: SOL (input) -> TOKEN (output)
    /// - input_mint = WSOL (So11111111111111111111111111111111111111112)
    /// - output_mint = El token nuevo del pool
    ///
    /// # Arguments
    /// * `pool_address` - Dirección del pool (lb_pair)
    /// * `pool` - Datos del pool DLMM
    /// * `user` - Wallet del usuario (signer)
    /// * `amount_sol_in` - Cantidad de SOL a gastar (en lamports)
    /// * `minimum_token_out` - Mínima cantidad de tokens a recibir
    pub fn build_buy_instruction(
        &self,
        pool_address: &Pubkey,
        pool: &LbPair,
        user: &Pubkey,
        amount_sol_in: u64,
        minimum_token_out: u64,
    ) -> Result<Instruction> {
        // WSOL y Native SOL mint addresses
        let wsol_mint = "So11111111111111111111111111111111111111112".parse::<Pubkey>()?;
        let native_sol_mint = solana_sdk::system_program::ID; // 11111111111111111111111111111111

        // Determinar cuál token es SOL y cuál es el nuevo token
        let token_x_mint = pool.token_x_mint;
        let token_y_mint = pool.token_y_mint;

        let is_sol_x = token_x_mint == wsol_mint || token_x_mint == native_sol_mint;
        let is_sol_y = token_y_mint == wsol_mint || token_y_mint == native_sol_mint;

        let (user_token_in, user_token_out, _is_x_to_y) = if is_sol_x {
            // SOL es X, compramos Y
            // Siempre usamos WSOL para el swap
            (
                get_associated_token_address(user, &wsol_mint),
                get_associated_token_address(user, &token_y_mint),
                true,
            )
        } else if is_sol_y {
            // SOL es Y, compramos X
            (
                get_associated_token_address(user, &wsol_mint),
                get_associated_token_address(user, &token_x_mint),
                false,
            )
        } else {
            return Err(anyhow::anyhow!("Pool no contiene SOL/WSOL, no se puede comprar con SOL"));
        };

        // Reserves
        let (reserve_x, reserve_y) = pool.get_reserves();

        // Derivar PDAs
        let bin_array_bitmap_extension = self.derive_bin_array_bitmap_extension(pool_address);
        let event_authority = self.derive_event_authority();

        // Parámetros del swap
        let params = SwapParameters {
            amount_in: amount_sol_in,
            min_amount_out: minimum_token_out,
        };

        // Serializar data
        let discriminator = Self::get_swap_discriminator();
        let mut data = discriminator.to_vec();
        params.serialize(&mut data)?;

        // Para las cuentas del swap, si el mint es native SOL, usar WSOL
        let swap_token_x_mint = if token_x_mint == native_sol_mint { wsol_mint } else { token_x_mint };
        let swap_token_y_mint = if token_y_mint == native_sol_mint { wsol_mint } else { token_y_mint };

        // Construir cuentas según SwapKeys de DLMM
        // Orden exacto de cazador1:
        // 0. lb_pair (mut)
        // 1. bin_array_bitmap_extension (readonly)
        // 2. reserve_x (mut)
        // 3. reserve_y (mut)
        // 4. user_token_in (mut)
        // 5. user_token_out (mut)
        // 6. token_x_mint (readonly)
        // 7. token_y_mint (readonly)
        // 8. oracle (readonly) - es el campo pool.oracle
        // 9. host_fee_in (mut) - puede ser user_token_in o una cuenta específica
        // 10. user (signer)
        // 11. token_x_program (readonly) - Token Program
        // 12. token_y_program (readonly) - Token Program
        // 13. event_authority (readonly)
        // 14. program (readonly) - DLMM Program
        let accounts = vec![
            // 0. lb_pair (mut)
            AccountMeta::new(*pool_address, false),

            // 1. bin_array_bitmap_extension (readonly)
            AccountMeta::new_readonly(bin_array_bitmap_extension, false),

            // 2. reserve_x (mut)
            AccountMeta::new(reserve_x, false),

            // 3. reserve_y (mut)
            AccountMeta::new(reserve_y, false),

            // 4. user_token_in (mut) - WSOL ATA del usuario
            AccountMeta::new(user_token_in, false),

            // 5. user_token_out (mut) - Token ATA del usuario
            AccountMeta::new(user_token_out, false),

            // 6. token_x_mint (readonly) - WSOL si es native SOL
            AccountMeta::new_readonly(swap_token_x_mint, false),

            // 7. token_y_mint (readonly) - WSOL si es native SOL
            AccountMeta::new_readonly(swap_token_y_mint, false),

            // 8. oracle (mut) - writable según IDL
            AccountMeta::new(pool.oracle, false),

            // 9. host_fee_in (mut) - usamos user_token_in
            AccountMeta::new(user_token_in, false),

            // 10. user (signer)
            AccountMeta::new_readonly(*user, true),

            // 11. token_x_program (Token Program)
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),

            // 12. token_y_program (Token Program)
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),

            // 13. event_authority (readonly)
            AccountMeta::new_readonly(event_authority, false),

            // 14. program (readonly)
            AccountMeta::new_readonly(self.program_id, false),
        ];

        Ok(Instruction {
            program_id: self.program_id,
            accounts,
            data,
        })
    }

    /// Construir instrucciones completas para comprar incluyendo creación de ATAs
    pub fn build_buy_instructions_with_atas(
        &self,
        pool_address: &Pubkey,
        pool: &LbPair,
        user: &Pubkey,
        amount_sol_in: u64,
        minimum_token_out: u64,
    ) -> Result<Vec<Instruction>> {
        let mut instructions = Vec::new();

        let token_x_mint = pool.token_x_mint;
        let token_y_mint = pool.token_y_mint;
        let wsol_mint = "So11111111111111111111111111111111111111112".parse::<Pubkey>()?;
        let native_sol_mint = solana_sdk::system_program::ID;

        let is_sol_x = token_x_mint == wsol_mint || token_x_mint == native_sol_mint;
        let is_sol_y = token_y_mint == wsol_mint || token_y_mint == native_sol_mint;

        // Determinar cuál es el token output (el que NO es SOL/WSOL)
        let output_mint = if is_sol_x {
            token_y_mint
        } else if is_sol_y {
            token_x_mint
        } else {
            return Err(anyhow::anyhow!("Pool no contiene SOL/WSOL"));
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

    /// Construir instrucción de venta (TOKEN -> SOL)
    pub fn build_sell_instruction(
        &self,
        pool_address: &Pubkey,
        pool: &LbPair,
        user: &Pubkey,
        amount_token_in: u64,
        minimum_sol_out: u64,
    ) -> Result<Instruction> {
        // WSOL y Native SOL mint addresses
        let wsol_mint = "So11111111111111111111111111111111111111112".parse::<Pubkey>()?;
        let native_sol_mint = solana_sdk::system_program::ID;

        let token_x_mint = pool.token_x_mint;
        let token_y_mint = pool.token_y_mint;

        let is_sol_x = token_x_mint == wsol_mint || token_x_mint == native_sol_mint;
        let is_sol_y = token_y_mint == wsol_mint || token_y_mint == native_sol_mint;

        let (user_token_in, user_token_out) = if is_sol_x {
            // Vendemos Y por X (SOL)
            (
                get_associated_token_address(user, &token_y_mint),
                get_associated_token_address(user, &wsol_mint),
            )
        } else if is_sol_y {
            // Vendemos X por Y (SOL)
            (
                get_associated_token_address(user, &token_x_mint),
                get_associated_token_address(user, &wsol_mint),
            )
        } else {
            return Err(anyhow::anyhow!("Pool no contiene SOL/WSOL"));
        };

        let (reserve_x, reserve_y) = pool.get_reserves();
        let bin_array_bitmap_extension = self.derive_bin_array_bitmap_extension(pool_address);
        let event_authority = self.derive_event_authority();

        let params = SwapParameters {
            amount_in: amount_token_in,
            min_amount_out: minimum_sol_out,
        };

        let discriminator = Self::get_swap_discriminator();
        let mut data = discriminator.to_vec();
        params.serialize(&mut data)?;

        let accounts = vec![
            AccountMeta::new(*pool_address, false),
            AccountMeta::new_readonly(bin_array_bitmap_extension, false),
            AccountMeta::new(reserve_x, false),
            AccountMeta::new(reserve_y, false),
            AccountMeta::new(user_token_in, false),
            AccountMeta::new(user_token_out, false),
            AccountMeta::new_readonly(token_x_mint, false),
            AccountMeta::new_readonly(token_y_mint, false),
            AccountMeta::new(pool.oracle, false), // mut según IDL
            AccountMeta::new(user_token_in, false),
            AccountMeta::new_readonly(*user, true),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(self.program_id, false),
        ];

        Ok(Instruction {
            program_id: self.program_id,
            accounts,
            data,
        })
    }

    /// Construir instrucciones completas para vender (TOKEN -> SOL)
    pub fn build_sell_instructions_with_atas(
        &self,
        pool_address: &Pubkey,
        pool: &LbPair,
        user: &Pubkey,
        amount_token_in: u64,
        minimum_sol_out: u64,
    ) -> Result<Vec<Instruction>> {
        let mut instructions = Vec::new();

        let wsol_mint = "So11111111111111111111111111111111111111112".parse::<Pubkey>()?;
        let user_wsol_ata = get_associated_token_address(user, &wsol_mint);

        // Crear WSOL ATA si no existe
        instructions.push(
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                user,
                user,
                &wsol_mint,
                &TOKEN_PROGRAM_ID,
            )
        );

        // Instrucción de swap (venta)
        instructions.push(self.build_sell_instruction(
            pool_address,
            pool,
            user,
            amount_token_in,
            minimum_sol_out,
        )?);

        // Close WSOL account para recuperar SOL
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

impl Default for DlmmSwapBuilder {
    fn default() -> Self {
        Self::new().expect("Failed to create DlmmSwapBuilder")
    }
}
