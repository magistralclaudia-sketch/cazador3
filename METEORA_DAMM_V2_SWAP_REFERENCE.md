# Meteora DAMM v2 Swap Instruction - Complete Reference

**Program ID**: `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG`
**Last Updated**: 2025-11-16
**Sources**:
- https://github.com/MeteoraAg/damm-v2 (official program)
- https://github.com/MeteoraAg/damm-v2-sdk (official TypeScript SDK)
- Local implementation: `/home/user/cazador3/src/trading/swap_builder.rs`

---

## Table of Contents
1. [Instruction Discriminators](#instruction-discriminators)
2. [Instruction Data Structure](#instruction-data-structure)
3. [Required Accounts](#required-accounts)
4. [Rust Implementation](#rust-implementation)
5. [TypeScript SDK Reference](#typescript-sdk-reference)
6. [Important Notes](#important-notes)

---

## Instruction Discriminators

### Primary Swap Instruction: `swap`

**Discriminator Calculation**:
```
SHA256("global:swap")[0..8]
```

**Result**:
- **Hexadecimal**: `f8c69e91e17587c8`
- **Byte Array**: `[248, 198, 158, 145, 225, 117, 135, 200]`
- **Rust Constant**: `[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]`

### Advanced Swap Instruction: `swap2`

**Discriminator Calculation**:
```
SHA256("global:swap2")[0..8]
```

**Result**:
- **Hexadecimal**: `414b3f4ceb5b5b88`
- **Byte Array**: `[65, 75, 63, 76, 235, 91, 91, 136]`
- **Rust Constant**: `[0x41, 0x4b, 0x3f, 0x4c, 0xeb, 0x5b, 0x5b, 0x88]`

### Other Common Instructions (for reference)

| Instruction | Discriminator (hex) |
|------------|---------------------|
| initialize_pool | `5fb40aac54aee828` |
| add_liquidity | `b59d59438fb63448` |
| remove_liquidity | `5055d14818ceb16c` |

---

## Instruction Data Structure

### For `swap` Instruction

**Rust Structure**:
```rust
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct SwapParameters {
    /// Amount of input tokens to swap
    pub amount_in: u64,

    /// Minimum amount of output tokens (slippage protection)
    pub minimum_amount_out: u64,
}
```

**Binary Format**:
```
┌─────────────────────┬──────────────┬────────────────────────┐
│ Discriminator       │ amount_in    │ minimum_amount_out     │
│ 8 bytes             │ 8 bytes (u64)│ 8 bytes (u64)          │
└─────────────────────┴──────────────┴────────────────────────┘
Total: 24 bytes
```

**Example**:
```
Swap 100,000,000 lamports with minimum output of 1,000,000 tokens:

[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]  // discriminator
[0x00, 0xe1, 0xf5, 0x05, 0x00, 0x00, 0x00, 0x00]  // 100_000_000 (little-endian)
[0x40, 0x42, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00]  // 1_000_000 (little-endian)
```

### For `swap2` Instruction

**Rust Structure**:
```rust
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct SwapParameters2 {
    pub amount_0: u64,
    pub amount_1: u64,
    pub swap_mode: SwapMode,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub enum SwapMode {
    ExactIn = 0,       // Specify exact input amount
    ExactOut = 1,      // Specify exact output amount
    PartialFill = 2,   // Allow partial fills
}
```

**Binary Format**:
```
┌─────────────────────┬──────────────┬──────────────┬────────────┐
│ Discriminator       │ amount_0     │ amount_1     │ swap_mode  │
│ 8 bytes             │ 8 bytes (u64)│ 8 bytes (u64)│ 1 byte     │
└─────────────────────┴──────────────┴──────────────┴────────────┘
Total: 25 bytes
```

---

## Required Accounts

### Account Structure (SwapCtx)

The swap instruction requires **11 accounts** (with 1 optional):

| Index | Account Name | Type | Writable | Signer | Description |
|-------|-------------|------|----------|--------|-------------|
| 0 | `pool_authority` | Unchecked | No | No | Pool's PDA authority |
| 1 | `pool` | AccountLoader<Pool> | **Yes** | No | The liquidity pool account |
| 2 | `input_token_account` | TokenAccount | **Yes** | No | User's input token account |
| 3 | `output_token_account` | TokenAccount | **Yes** | No | User's output token account |
| 4 | `token_a_vault` | InterfaceAccount | **Yes** | No | Pool's vault for token A |
| 5 | `token_b_vault` | InterfaceAccount | **Yes** | No | Pool's vault for token B |
| 6 | `token_a_mint` | InterfaceAccount<Mint> | No | No | Token A mint address |
| 7 | `token_b_mint` | InterfaceAccount<Mint> | No | No | Token B mint address |
| 8 | `payer` | Signer | No | **Yes** | User executing the swap |
| 9 | `token_a_program` | Interface | No | No | Token program for A (Token/Token2022) |
| 10 | `token_b_program` | Interface | No | No | Token program for B (Token/Token2022) |
| 11 | `referral_token_account` | Optional TokenAccount | **Yes** | No | Optional referral fee account |

### Optional Remaining Accounts

If the pool has a rate limiter enabled, add:
- **Sysvar Instructions**: `Sysvar1nstructions1111111111111111111111111`

### Determining Pool Authority

The `pool_authority` is a PDA (Program Derived Address) calculated as:
```rust
Pubkey::find_program_address(
    &[b"authority", pool.to_bytes().as_ref()],
    &program_id
)
```

---

## Rust Implementation

### Complete Example

```rust
use anchor_lang::{AnchorSerialize, AnchorDeserialize};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

/// Swap instruction discriminator (SHA256("global:swap")[0..8])
const SWAP_DISCRIMINATOR: [u8; 8] = [248, 198, 158, 145, 225, 117, 135, 200];

/// Parameters for the swap instruction
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct SwapParameters {
    pub amount_in: u64,
    pub minimum_amount_out: u64,
}

/// Build a swap instruction for Meteora DAMM v2
pub fn build_swap_instruction(
    program_id: Pubkey,
    pool: Pubkey,
    pool_authority: Pubkey,
    input_token_account: Pubkey,
    output_token_account: Pubkey,
    token_a_vault: Pubkey,
    token_b_vault: Pubkey,
    token_a_mint: Pubkey,
    token_b_mint: Pubkey,
    payer: Pubkey,
    token_a_program: Pubkey,
    token_b_program: Pubkey,
    amount_in: u64,
    minimum_amount_out: u64,
    referral_token_account: Option<Pubkey>,
) -> Result<Instruction, std::io::Error> {
    // Serialize instruction parameters
    let params = SwapParameters {
        amount_in,
        minimum_amount_out,
    };

    let mut data = SWAP_DISCRIMINATOR.to_vec();
    params.serialize(&mut data)?;

    // Build accounts array
    let mut accounts = vec![
        AccountMeta::new_readonly(pool_authority, false),       // 0: pool_authority
        AccountMeta::new(pool, false),                          // 1: pool
        AccountMeta::new(input_token_account, false),           // 2: input_token_account
        AccountMeta::new(output_token_account, false),          // 3: output_token_account
        AccountMeta::new(token_a_vault, false),                 // 4: token_a_vault
        AccountMeta::new(token_b_vault, false),                 // 5: token_b_vault
        AccountMeta::new_readonly(token_a_mint, false),         // 6: token_a_mint
        AccountMeta::new_readonly(token_b_mint, false),         // 7: token_b_mint
        AccountMeta::new_readonly(payer, true),                 // 8: payer (signer)
        AccountMeta::new_readonly(token_a_program, false),      // 9: token_a_program
        AccountMeta::new_readonly(token_b_program, false),      // 10: token_b_program
    ];

    // Add optional referral account
    if let Some(referral) = referral_token_account {
        accounts.push(AccountMeta::new(referral, false));       // 11: referral_token_account
    }

    Ok(Instruction {
        program_id,
        accounts,
        data,
    })
}

/// Derive pool authority PDA
pub fn derive_pool_authority(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"authority", pool.as_ref()],
        program_id
    )
}
```

### Usage Example

```rust
use solana_sdk::pubkey::Pubkey;
use spl_token::id as token_program_id;

// Program and pool addresses
let program_id = Pubkey::from_str("cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG")?;
let pool = Pubkey::from_str("...")?;  // Your pool address

// Derive pool authority
let (pool_authority, _bump) = derive_pool_authority(&pool, &program_id);

// User accounts
let payer = wallet.pubkey();
let input_token_account = get_associated_token_address(&payer, &token_a_mint);
let output_token_account = get_associated_token_address(&payer, &token_b_mint);

// Build the swap instruction
let swap_ix = build_swap_instruction(
    program_id,
    pool,
    pool_authority,
    input_token_account,
    output_token_account,
    pool_data.token_a_vault,
    pool_data.token_b_vault,
    pool_data.token_a_mint,
    pool_data.token_b_mint,
    payer,
    token_program_id(),         // SPL Token program
    token_program_id(),         // SPL Token program
    100_000_000,                // Swap 0.1 SOL
    1_000_000,                  // Minimum 1M tokens out (99% slippage)
    None,                       // No referral
)?;

// Add to transaction with compute budget
let tx = Transaction::new_signed_with_payer(
    &[
        ComputeBudgetInstruction::set_compute_unit_price(micro_lamports_per_cu),
        ComputeBudgetInstruction::set_compute_unit_limit(300_000),
        swap_ix,
    ],
    Some(&payer),
    &[&wallet],
    recent_blockhash,
);
```

---

## TypeScript SDK Reference

### Using the Official SDK

```typescript
import { CpAmm } from '@meteora-ag/cp-amm-sdk';
import { Connection, PublicKey } from '@solana/web3.js';

const connection = new Connection('https://api.mainnet-beta.solana.com');
const cpAmm = new CpAmm(connection);

// Get quote for swap
const quote = await cpAmm.getQuote({
  poolAddress: new PublicKey('...'),
  inputTokenMint: tokenAMint,
  outputTokenMint: tokenBMint,
  inAmount: BigInt(100_000_000), // 0.1 SOL
  slippageBps: 9900, // 99% slippage
});

// Execute swap
const swapTx = await cpAmm.swap({
  poolAddress: new PublicKey('...'),
  inputTokenMint: tokenAMint,
  outputTokenMint: tokenBMint,
  amountIn: BigInt(100_000_000),
  minimumAmountOut: quote.minimumAmountOut,
  payer: wallet.publicKey,
});

// Sign and send
const signature = await wallet.sendTransaction(swapTx, connection);
```

### Manual Instruction Building (TypeScript)

```typescript
import { Program, BN } from '@project-serum/anchor';
import { PublicKey, TransactionInstruction } from '@solana/web3.js';

// Discriminator for swap instruction
const SWAP_DISCRIMINATOR = Buffer.from([248, 198, 158, 145, 225, 117, 135, 200]);

// Serialize parameters
function serializeSwapParams(amountIn: BN, minimumAmountOut: BN): Buffer {
  const data = Buffer.alloc(24);
  SWAP_DISCRIMINATOR.copy(data, 0);
  amountIn.toArrayLike(Buffer, 'le', 8).copy(data, 8);
  minimumAmountOut.toArrayLike(Buffer, 'le', 8).copy(data, 16);
  return data;
}

// Build swap instruction
const swapInstruction = new TransactionInstruction({
  programId: new PublicKey('cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG'),
  keys: [
    { pubkey: poolAuthority, isSigner: false, isWritable: false },
    { pubkey: pool, isSigner: false, isWritable: true },
    { pubkey: inputTokenAccount, isSigner: false, isWritable: true },
    { pubkey: outputTokenAccount, isSigner: false, isWritable: true },
    { pubkey: tokenAVault, isSigner: false, isWritable: true },
    { pubkey: tokenBVault, isSigner: false, isWritable: true },
    { pubkey: tokenAMint, isSigner: false, isWritable: false },
    { pubkey: tokenBMint, isSigner: false, isWritable: false },
    { pubkey: payer, isSigner: true, isWritable: false },
    { pubkey: tokenAProgram, isSigner: false, isWritable: false },
    { pubkey: tokenBProgram, isSigner: false, isWritable: false },
  ],
  data: serializeSwapParams(new BN(100_000_000), new BN(1_000_000)),
});
```

---

## Important Notes

### 1. Token Programs

Meteora DAMM v2 supports **both** Token and Token-2022 programs:
- `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA` (SPL Token)
- `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb` (Token-2022)

Make sure to specify the correct token program for each token.

### 2. Pool Authority

The `pool_authority` account is **always** a PDA derived from the pool address:
```rust
find_program_address(&[b"authority", pool.as_ref()], &program_id)
```

### 3. Input vs Output Direction

The instruction determines trade direction by comparing `input_token_account`'s mint:
- If input mint == token A mint → Swap A to B
- If input mint == token B mint → Swap B to A

### 4. SOL Wrapping

When swapping native SOL:
1. Wrap SOL to wSOL **before** the swap instruction
2. Unwrap wSOL back to SOL **after** the swap instruction

The TypeScript SDK handles this automatically.

### 5. Slippage Protection

The `minimum_amount_out` parameter protects against slippage:
```rust
// For 99% slippage tolerance (high for testing):
let estimated_out = calculate_swap_output(pool, amount_in);
let minimum_amount_out = (estimated_out as f64 * 0.01) as u64;

// For 5% slippage (normal):
let minimum_amount_out = (estimated_out as f64 * 0.95) as u64;
```

### 6. Priority Fees

For fast execution during network congestion:
```rust
// Set priority fee (micro-lamports per compute unit)
let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
    micro_lamports_per_cu  // e.g., 333 for ~100k lamports total
);

// Set compute limit
let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(300_000);
```

**Formula**:
```
total_priority_fee = (compute_units * micro_lamports_per_cu) / 1_000_000
```

### 7. Rate Limiter

Some pools have rate limiters enabled. If so, add the Sysvar Instructions account:
```rust
accounts.push(AccountMeta::new_readonly(
    solana_program::sysvar::instructions::ID,
    false
));
```

---

## Verification

This document was created by analyzing:

1. **Official Program Source**:
   - https://github.com/MeteoraAg/damm-v2/blob/main/programs/cp-amm/src/lib.rs
   - https://github.com/MeteoraAg/damm-v2/blob/main/programs/cp-amm/src/instructions/swap/ix_swap.rs

2. **Official TypeScript SDK**:
   - https://github.com/MeteoraAg/damm-v2-sdk/blob/main/src/CpAmm.ts (lines 1844-1912)

3. **Local Implementation**:
   - `/home/user/cazador3/src/trading/swap_builder.rs`

The discriminators were calculated using:
```python
import hashlib
discriminator = hashlib.sha256(b"global:swap").digest()[:8]
# Result: [248, 198, 158, 145, 225, 117, 135, 200]
```

This matches the Anchor framework's standard discriminator calculation method.

---

## Additional Resources

- **Meteora Docs**: https://docs.meteora.ag/
- **Program on Solana Explorer**: https://solscan.io/account/cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG
- **TypeScript SDK NPM**: https://www.npmjs.com/package/@meteora-ag/cp-amm-sdk
- **Anchor Documentation**: https://www.anchor-lang.com/

---

**Last verified**: 2025-11-16
**Program Version**: DAMM v2 (Constant Product AMM)
**Network**: Solana Mainnet Beta & Devnet
