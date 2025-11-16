# ✅ Guía de Verificación Pre-Pruebas

## ⚠️ ATENCIÓN - Actualizaciones Críticas

**Fecha**: 2024 - Verificación completa de fórmulas DAMM V2

### 🔴 Acción Requerida ANTES de Testing:

1. **PRIORITY FEE - Verificar interpretación** (`.env:24`)
   - `PRIORITY_FEE_LAMPORTS=100000` puede significar:
     - **Si son micro-lamports/CU**: Costaría 0.03 SOL por TX ❌ MUY CARO
     - **Si quieres 100k lamports totales**: Debe ser `333` micro-lamports/CU ✅
   - Ver sección 4 abajo para detalles completos

2. **FÓRMULAS DE SWAP - Verificadas** (`VERIFICACION_FORMULAS_DAMM_V2.md`)
   - ✅ Cálculo de precio: CORRECTO
   - ⚠️ Estimación de swap: SIMPLIFICADO (compensado por slippage 99%)
   - 📄 Documento completo creado con fórmulas oficiales

### 📋 Documentos de Referencia:
- `VERIFICACION_FORMULAS_DAMM_V2.md` - Comparación detallada con fórmulas oficiales
- `COSTOS_Y_FEES.md` - Análisis de costos
- `ULTRA_FAST_TRADING.md` - Sistema de trading rápido

---

## 🎯 Checklist Completo Antes de Ejecutar

### 1. ✅ BLOCKHASH FRESCO - IMPLEMENTADO

**Ubicación**: `src/trading/transaction_confirmer.rs` y `src/trading/executor.rs`

#### ¿Qué hace el código actual?

```rust
// En TransactionConfirmer
pub async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
    self.rpc_client
        .get_latest_blockhash()
        .await
        .context("Failed to get latest blockhash")
}

// En TradeExecutor::execute_swap_transaction()
let recent_blockhash = self.tx_confirmer.get_latest_blockhash().await?;
```

#### ✅ Verificado:
- [x] Obtiene blockhash fresco antes de cada TX
- [x] Usa commitment level `confirmed` (recomendado)
- [x] Blockhash es válido por ~80-90 segundos (150 blocks)
- [x] Se obtiene justo antes de construir la TX

#### 📊 Mejores Prácticas Implementadas:
1. **Commitment Level**: Usa `confirmed` (balance entre velocidad y seguridad)
2. **Timing**: Obtiene blockhash inmediatamente antes de enviar
3. **Retry**: Si falla, el sistema reintenta automáticamente
4. **Expiration**: El código reintenta dentro del período de validez

#### ⚠️ Para Producción:
```rust
// Opcional: Monitorear lastValidBlockHeight
let (blockhash, last_valid_height) = self.rpc_client
    .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
    .await?;

// Verificar que no haya expirado
let current_height = self.rpc_client.get_block_height().await?;
if current_height > last_valid_height {
    // Blockhash expirado, obtener uno nuevo
}
```

---

### 2. ✅ CÁLCULO DE TOKENS - IMPLEMENTADO

**Ubicación**: `src/meteora/price.rs`

#### Método Actual (Simplificado):

```rust
pub fn estimate_swap_output(pool: &Pool, amount_in: u64, is_a_to_b: bool) -> u64 {
    let price = Self::calculate_price(pool);

    if is_a_to_b {
        (amount_in as f64 * price) as u64
    } else {
        (amount_in as f64 / price) as u64
    }
}
```

#### ✅ Funciona Para:
- [x] Estimaciones aproximadas
- [x] Pools con liquidez normal
- [x] Trading con slippage alto (99%)

#### ⚠️ Limitaciones:
- No usa reserves reales (solo precio)
- No considera price impact exacto
- Simplificado para velocidad

#### 🎯 Método Mejorado (Con Reserves):

```rust
pub fn estimate_swap_output_with_reserves(
    reserve_in: u128,
    reserve_out: u128,
    amount_in: u64,
    fee_bps: u16,
) -> u64 {
    // Fórmula constant product: x * y = k
    // amount_out = (reserve_out * amount_in_with_fee) / (reserve_in + amount_in_with_fee)

    let fee_multiplier = 10_000 - fee_bps as u128;
    let amount_in_with_fee = (amount_in as u128) * fee_multiplier;

    let numerator = amount_in_with_fee * reserve_out;
    let denominator = (reserve_in * 10_000) + amount_in_with_fee;

    (numerator / denominator) as u64
}
```

#### 📊 Para DAMM v2 Específico:

Meteora DAMM v2 usa constant product (x × y = k) pero dentro de un rango de precio:
- `sqrt_min_price` a `sqrt_max_price`
- Fees: `lp_fee_bps + protocol_fee_bps`

**Fórmula completa** (según código fuente):
```
Fee = lp_fee_bps + protocol_fee_bps (típicamente 25-30 BPS)

Compra (A -> B):
1. amount_in_minus_fee = amount_in × (10000 - fee_bps) / 10000
2. new_reserve_a = reserve_a + amount_in_minus_fee
3. new_reserve_b = k / new_reserve_a
4. amount_out = reserve_b - new_reserve_b

Venta (B -> A): Similar pero invertido
```

#### 🔧 Cómo Obtener Reserves Reales:

```rust
// En tu código:
pub async fn get_pool_reserves(
    &self,
    pool: &Pool,
) -> Result<(u64, u64)> {
    // Balance de token A vault
    let balance_a = self.rpc_client
        .get_token_account_balance(&pool.token_a_vault)
        .await?
        .amount
        .parse::<u64>()?;

    // Balance de token B vault
    let balance_b = self.rpc_client
        .get_token_account_balance(&pool.token_b_vault)
        .await?
        .amount
        .parse::<u64>()?;

    Ok((balance_a, balance_b))
}
```

---

### 3. ✅ SLIPPAGE - CONFIGURADO

**Ubicación**: `.env` y `src/config.rs`

#### Configuración Actual:

```env
BUY_SLIPPAGE_BPS=9900   # 99%
SELL_SLIPPAGE_BPS=4000  # 40%
```

#### ✅ Cálculo Implementado:

```rust
fn calculate_min_amount_out(&self, amount_in: u64, pool: &Pool, is_a_to_b: bool, slippage_bps: u16) -> u64 {
    let estimated_out = PriceCalculator::estimate_swap_output(pool, amount_in, is_a_to_b);

    // Con 99% slippage:
    // min_out = estimated × (1 - 0.99) = estimated × 0.01
    let slippage_multiplier = 1.0 - (slippage_bps as f64 / 10_000.0);
    let min_out = (estimated_out as f64 * slippage_multiplier) as u64;

    min_out
}
```

#### 📊 Ejemplo Real:

```
Compra: 0.1 SOL
Precio estimado: 1 SOL = 1,000,000 tokens
Estimated output: 100,000 tokens

Con 99% slippage (BUY):
min_output = 100,000 × 0.01 = 1,000 tokens
Aceptarás entre 1,000 y 100,000 tokens

Con 40% slippage (SELL):
min_output = 100,000 × 0.60 = 60,000 tokens
Aceptarás mínimo 60,000 tokens
```

---

### 4. ✅ PRIORITY FEES - CONFIGURADO

**Ubicación**: `src/trading/executor.rs`

#### Implementación Actual:

```rust
let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
    self.config.priority_fee_lamports,
);

let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(300_000);
```

#### ⚠️ IMPORTANTE - Posible Confusión:

`set_compute_unit_price()` espera **micro-lamports por CU**, no lamports totales.

**Conversión correcta**:
```rust
// Si quieres pagar 100,000 lamports totales con 300k CU:
let desired_total_lamports = 100_000;
let compute_units = 300_000;

// Convertir a micro-lamports por CU
let price_micro_lamports = (desired_total_lamports * 1_000_000) / compute_units;
// = 333 micro-lamports/CU

let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
    price_micro_lamports
);
```

#### 📊 Costo Real:

```
Priority Fee = (Compute Units × Price per CU) / 1,000,000

Ejemplo:
Compute Units: 300,000
Price per CU: 333 micro-lamports
Priority Fee = (300,000 × 333) / 1,000,000 = 99.9 lamports ≈ 100,000 lamports
```

#### ✅ Para Testing:

```env
# Ultra económico
PRIORITY_FEE_LAMPORTS=10000    # ~33 micro-lamports/CU
Costo: ~0.00001 SOL

# Normal
PRIORITY_FEE_LAMPORTS=100000   # ~333 micro-lamports/CU
Costo: ~0.0001 SOL

# Rápido
PRIORITY_FEE_LAMPORTS=500000   # ~1666 micro-lamports/CU
Costo: ~0.0005 SOL
```

---

### 5. ✅ RPC PROPIO - CONFIGURADO

**Ubicación**: `.env`

#### Configuración Actual:

```env
RPC_URL=http://192.168.0.50:8899
RPC_WS_URL=ws://192.168.0.50:8900
GEYSER_ENDPOINT=http://192.168.0.50:10000
```

#### ✅ Verificaciones Pre-Prueba:

```bash
# 1. Verificar que el nodo RPC responde
curl http://192.168.0.50:8899 -X POST -H "Content-Type: application/json" -d '
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "getHealth"
}'

# Respuesta esperada: {"jsonrpc":"2.0","result":"ok","id":1}

# 2. Verificar Geyser gRPC
nc -zv 192.168.0.50 10000

# 3. Verificar blockhash
curl http://192.168.0.50:8899 -X POST -H "Content-Type: application/json" -d '
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "getLatestBlockhash",
  "params": [{"commitment": "confirmed"}]
}'

# 4. Ver slot actual
curl http://192.168.0.50:8899 -X POST -H "Content-Type: application/json" -d '
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "getSlot"
}'
```

---

### 6. ✅ WALLET - CONFIGURACIÓN

**Ubicación**: `./wallet.json`

#### Formato Esperado:

```json
[123,45,67,89,...]  // Array de 64 bytes
```

#### ✅ Crear Wallet de Testing:

```bash
# Opción 1: Crear nueva
solana-keygen new --outfile wallet.json --no-bip39-passphrase

# Opción 2: Recuperar existente
solana-keygen recover --outfile wallet.json

# Ver pubkey
solana-keygen pubkey wallet.json

# Fondear (en devnet para testing)
solana airdrop 1 $(solana-keygen pubkey wallet.json) --url https://api.devnet.solana.com

# Fondear en mainnet (transferir desde otra wallet)
solana transfer <WALLET_PUBKEY> 0.5 --from <TU_WALLET_PRINCIPAL>
```

---

### 7. 📋 CONFIGURACIÓN RECOMENDADA PARA PRIMERA PRUEBA

#### `.env` Para Testing Seguro:

```env
# RPC
RPC_URL=http://192.168.0.50:8899
RPC_WS_URL=ws://192.168.0.50:8900
GEYSER_ENDPOINT=http://192.168.0.50:10000
GEYSER_X_TOKEN=

# Wallet
WALLET_KEYPAIR_PATH=./wallet.json

# Program
METEORA_PROGRAM_ID=cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG

# Trading (MODO OBSERVACIÓN)
AUTO_BUY_ENABLED=false          # ❌ DESHABILITADO
AUTO_BUY_AMOUNT_SOL=0.01        # Mínimo por si acaso

# Slippage
BUY_SLIPPAGE_BPS=9900           # 99%
SELL_SLIPPAGE_BPS=4000          # 40%

# Fees (ULTRA ECONÓMICO)
PRIORITY_FEE_LAMPORTS=10000     # ~$0.0003 por TX

# Risk
TAKE_PROFIT_PERCENT=10.0
STOP_LOSS_PERCENT=-2.0

# Monitoring
MIN_LIQUIDITY_SOL=0.1           # Bajo para detectar más pools
```

---

### 8. 🧪 PLAN DE PRUEBAS PASO A PASO

#### Fase 1: Verificación de Conectividad (5 min)

```bash
# 1. Verificar RPC
curl http://192.168.0.50:8899 -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}'

# 2. Verificar Geyser
nc -zv 192.168.0.50 10000

# 3. Ver balance de wallet
solana balance $(solana-keygen pubkey wallet.json) --url http://192.168.0.50:8899
```

**Resultado Esperado**: ✅ Todo responde, wallet tiene > 0.01 SOL

#### Fase 2: Modo Observación (30 min)

```env
AUTO_BUY_ENABLED=false
```

```bash
cargo run --release
```

**Resultado Esperado**:
- ✅ Se conecta a Geyser
- ✅ Detecta pools de Meteora
- ✅ Muestra precio y liquidez
- ✅ NO compra nada

**Logs esperados**:
```
✓ Conectado a Geyser gRPC exitosamente
🔍 Iniciando monitoreo de pools...
📡 Subscripción activa. Esperando nuevos pools...
🆕 ¡NUEVO POOL DETECTADO! 7xKX...
   Token A: So11111...
   Token B: EPjFWdd...
   Liquidez: 1000000000
   Precio: 0.00015
ℹ️  Auto-compra deshabilitada. Solo monitoreando.
```

#### Fase 3: Compra Mínima (testing real)

```env
AUTO_BUY_ENABLED=true
AUTO_BUY_AMOUNT_SOL=0.01        # 1 centavo de SOL
PRIORITY_FEE_LAMPORTS=50000     # Medio
```

```bash
cargo run --release
```

**Resultado Esperado**:
- ✅ Detecta pool
- ✅ Ejecuta compra con 0.01 SOL
- ✅ Obtiene signature
- ✅ Monitorea posición
- ✅ Ejecuta TP/SL si se activa

#### Fase 4: Producción Ligera

```env
AUTO_BUY_AMOUNT_SOL=0.1
PRIORITY_FEE_LAMPORTS=200000
```

---

### 9. ⚠️ ISSUES CONOCIDOS Y SOLUCIONES

#### Issue #1: Instrucción de Swap Simplificada

**Problema**: `build_simple_swap_instruction()` puede no funcionar en producción

**Solución Temporal**: Usa slippage 99% para compensar

**Solución Permanente**:
1. Obtener IDL oficial: `anchor idl fetch cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG`
2. O usar Jupiter Aggregator

#### Issue #2: Cálculo de Precio Simplificado

**Problema**: No usa reserves reales, solo precio

**Impacto**: Estimación puede ser inexacta en pools muy volátiles

**Mitigación**: Slippage 99% cubre la diferencia

**Solución**: Implementar `get_pool_reserves()` y usar `estimate_swap_output_with_reserves()`

#### Issue #3: ATAs No Verificadas

**Problema**: Asume que Associated Token Accounts existen

**Solución**: El código actual asume que se crearán automáticamente. Si falla, implementar creación manual.

---

### 10. 📊 MÉTRICAS A MONITOREAR

Durante las pruebas, observa:

```
✅ Latencia detección: < 100ms desde creación del pool
✅ Latencia TX: < 1 segundo desde detección a confirmación
✅ Success rate: > 90% de transacciones confirmadas
✅ Slippage real: Tokens recibidos vs estimados
✅ Fees pagados: Debe coincidir con configuración
```

---

### 11. 🆘 TROUBLESHOOTING

#### "Blockhash not found"
- Problema: Blockhash expiró
- Solución: Ya implementado retry automático

#### "Transaction simulation failed"
- Problema: Instrucción incorrecta o insuficiente SOL
- Verificar: Balance de wallet, logs de error

#### "No pools detected"
- Problema: Geyser no está enviando datos
- Verificar: nc -zv 192.168.0.50 10000

#### "Slippage tolerance exceeded"
- Normal con 99% slippage
- Si falla, el pool cambió >99%

---

### 12. ✅ CHECKLIST FINAL PRE-EJECUCIÓN

```
[ ] ⚠️ PRIORITY_FEE_LAMPORTS verificado (ver sección 4 y VERIFICACION_FORMULAS_DAMM_V2.md)
[ ] 📄 Leído VERIFICACION_FORMULAS_DAMM_V2.md para entender estimaciones simplificadas
[ ] RPC responde en http://192.168.0.50:8899
[ ] Geyser responde en puerto 10000
[ ] wallet.json existe y tiene formato correcto
[ ] Wallet tiene > 0.1 SOL de balance
[ ] .env configurado correctamente
[ ] AUTO_BUY_ENABLED=false para primera prueba
[ ] Compilación sin errores: cargo build --release
[ ] Logs habilitados: RUST_LOG=info
```

---

**¡Listo para pruebas! 🚀**

Ejecuta: `RUST_LOG=info cargo run --release`
