# 💰 Guía de Costos y Fees en Solana

## 📊 Costos Básicos de Transacciones

### Fee Base (Obligatorio)
```
Base Fee = 5,000 lamports por firma
         = 0.000005 SOL por transacción
         ≈ $0.0001 USD (a $20/SOL)
```

### Priority Fee (Opcional)
```
Priority Fee = Compute Units × Compute Unit Price
```

## 🧮 Fórmula de Cálculo

### Componentes:

1. **Compute Unit Limit**: Máximo de unidades computacionales que puede usar tu TX
   - Default: 200,000 CU por instrucción
   - Swap típico: 200,000 - 300,000 CU
   - Máximo por TX: 1,400,000 CU

2. **Compute Unit Price**: Precio en micro-lamports por CU
   - 1 micro-lamport = 0.000001 lamports
   - 1,000,000 micro-lamports = 1 lamport

3. **Priority Fee Total**:
```rust
priority_fee_lamports = (compute_limit × compute_unit_price_microlamports) / 1_000_000
```

## 💸 Ejemplos de Costos Reales

### Configuración 1: ULTRA ECONÓMICA (Para Testing)
```env
PRIORITY_FEE_LAMPORTS=1000        # 0.000001 SOL
# Esto equivale a ~0.05 micro-lamports/CU con 200k CU
```

**Costo total:**
- Base fee: 5,000 lamports
- Priority fee: 1,000 lamports
- **TOTAL: 6,000 lamports = 0.000006 SOL ≈ $0.00012 USD**

### Configuración 2: ECONÓMICA (Red Normal)
```env
PRIORITY_FEE_LAMPORTS=10000       # 0.00001 SOL
# Equivale a ~50 micro-lamports/CU con 200k CU
```

**Costo total:**
- Base fee: 5,000 lamports
- Priority fee: 10,000 lamports
- **TOTAL: 15,000 lamports = 0.000015 SOL ≈ $0.0003 USD**

### Configuración 3: MODERADA (Velocidad Normal)
```env
PRIORITY_FEE_LAMPORTS=100000      # 0.0001 SOL
# Equivale a ~500 micro-lamports/CU con 200k CU
```

**Costo total:**
- Base fee: 5,000 lamports
- Priority fee: 100,000 lamports
- **TOTAL: 105,000 lamports = 0.000105 SOL ≈ $0.0021 USD**

### Configuración 4: RÁPIDA (Alta Prioridad)
```env
PRIORITY_FEE_LAMPORTS=500000      # 0.0005 SOL
# Equivale a ~2,500 micro-lamports/CU con 200k CU
```

**Costo total:**
- Base fee: 5,000 lamports
- Priority fee: 500,000 lamports
- **TOTAL: 505,000 lamports = 0.000505 SOL ≈ $0.01 USD**

### Configuración 5: ULTRA RÁPIDA (Máxima Prioridad)
```env
PRIORITY_FEE_LAMPORTS=1000000     # 0.001 SOL
# Equivale a ~5,000 micro-lamports/CU con 200k CU
```

**Costo total:**
- Base fee: 5,000 lamports
- Priority fee: 1,000,000 lamports
- **TOTAL: 1,005,000 lamports = 0.001005 SOL ≈ $0.02 USD**

## 🎯 Configuración del Bot Actual

### En `src/trading/executor.rs`:

```rust
// Priority fee
let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
    self.config.priority_fee_lamports,  // De .env
);

// Compute limit
let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(
    300_000  // 300k CU
);
```

### Cómo funciona:

Si en `.env` tienes:
```env
PRIORITY_FEE_LAMPORTS=100000
```

Entonces:
- No se usa directamente como micro-lamports/CU
- Se usa como fee total en lamports
- El código lo establece con `set_compute_unit_price(100000)`

⚠️ **NOTA**: Hay una confusión en el código actual. `set_compute_unit_price()` espera **micro-lamports por CU**, no lamports totales.

## 🔧 Corrección para el Código

### Método Correcto:

```rust
// Opción 1: Especificar micro-lamports por CU directamente
let compute_price_microlamports = 500; // 500 micro-lamports/CU
let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
    compute_price_microlamports
);

// Con 200k CU:
// Priority Fee = 200,000 × 500 / 1,000,000 = 100,000 lamports
```

```rust
// Opción 2: Calcular desde lamports deseados
let desired_priority_fee_lamports = 100_000;
let compute_units = 300_000;
let price_per_cu = (desired_priority_fee_lamports * 1_000_000) / compute_units;

let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
    price_per_cu  // En micro-lamports
);
```

## 💡 Recomendaciones para Testing

### Fase 1: Testing Inicial (Sin Compras)
```env
AUTO_BUY_ENABLED=false
PRIORITY_FEE_LAMPORTS=1000        # Solo para observar
```
**Costo por observación: ~0.000006 SOL**

### Fase 2: Testing con Compras Mínimas
```env
AUTO_BUY_ENABLED=true
AUTO_BUY_AMOUNT_SOL=0.01          # 0.01 SOL por trade
PRIORITY_FEE_LAMPORTS=10000       # Económico
```
**Costo por transacción: ~0.000015 SOL + 0.01 SOL trade = 0.010015 SOL**

### Fase 3: Testing con Velocidad
```env
AUTO_BUY_ENABLED=true
AUTO_BUY_AMOUNT_SOL=0.05
PRIORITY_FEE_LAMPORTS=100000      # Prioridad moderada
```
**Costo por transacción: ~0.000105 SOL + 0.05 SOL trade = 0.050105 SOL**

### Fase 4: Producción
```env
AUTO_BUY_ENABLED=true
AUTO_BUY_AMOUNT_SOL=0.1
PRIORITY_FEE_LAMPORTS=500000      # Alta prioridad
```
**Costo por transacción: ~0.000505 SOL + 0.1 SOL trade = 0.100505 SOL**

## 📈 Calculadora de Costos

### Por 100 Transacciones:

| Config | Priority Fee/TX | Total Fees (100 TX) | En SOL | En USD ($20/SOL) |
|--------|-----------------|---------------------|--------|------------------|
| Ultra Económica | 1,000 lamports | 600,000 lamports | 0.0006 SOL | $0.012 |
| Económica | 10,000 lamports | 1,500,000 lamports | 0.0015 SOL | $0.03 |
| Moderada | 100,000 lamports | 10,500,000 lamports | 0.0105 SOL | $0.21 |
| Rápida | 500,000 lamports | 50,500,000 lamports | 0.0505 SOL | $1.01 |
| Ultra Rápida | 1,000,000 lamports | 100,500,000 lamports | 0.1005 SOL | $2.01 |

### Por Día (Asumiendo 50 trades/día):

| Config | Costo Diario | Costo Mensual | Costo Anual |
|--------|--------------|---------------|-------------|
| Ultra Económica | $0.006 | $0.18 | $2.19 |
| Económica | $0.015 | $0.45 | $5.48 |
| Moderada | $0.105 | $3.15 | $38.33 |
| Rápida | $0.505 | $15.15 | $184.33 |
| Ultra Rápida | $1.005 | $30.15 | $366.83 |

## 🔍 Monitorear Fees en Tiempo Real

### Usando Solana CLI:
```bash
# Ver fees recientes
solana fees

# Ver priority fees del mercado
curl http://192.168.0.50:8899 -X POST -H "Content-Type: application/json" -d '
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "getRecentPrioritizationFees",
  "params": [[]]
}'
```

### APIs Útiles:
- QuickNode Priority Fee Tracker: https://www.quicknode.com/gas-tracker/solana
- Helius Priority Fee API
- Triton Priority Fee Estimator

## ⚡ Optimizar Costos vs Velocidad

### Para Máxima Economía:
```env
PRIORITY_FEE_LAMPORTS=0           # Sin priority fee (MÁS LENTO)
```
- Costo: Solo 5,000 lamports (base fee)
- Tiempo: 2-10 segundos o más
- **Riesgo**: Puede no incluirse si red está congestionada

### Para Balance:
```env
PRIORITY_FEE_LAMPORTS=50000       # ~250 micro-lamports/CU
```
- Costo: ~55,000 lamports
- Tiempo: ~1-2 segundos
- **Recomendado para testing**

### Para Velocidad:
```env
PRIORITY_FEE_LAMPORTS=200000      # ~1,000 micro-lamports/CU
```
- Costo: ~205,000 lamports
- Tiempo: ~300-800ms
- **Recomendado para sniper bot**

## 💾 Presupuesto para Testing

### Budget Inicial Recomendado:
```
Wallet funding: 0.5 SOL

Desglose:
- Testing sin compras (100 TX): 0.001 SOL
- Testing con compras (20 TX × 0.01 SOL): 0.2 SOL
- Testing con compras (10 TX × 0.05 SOL): 0.5 SOL
- Reserve para fees: 0.05 SOL
- Buffer de seguridad: Resto

Total recomendado: 0.5-1 SOL para testing completo
```

## 🎓 Conversiones Útiles

```
1 SOL = 1,000,000,000 lamports
1 lamport = 0.000000001 SOL
1 lamport = 1,000,000 micro-lamports
1 micro-lamport = 0.000001 lamports

Ejemplos:
100,000 lamports = 0.0001 SOL
1,000,000 lamports = 0.001 SOL
10,000,000 lamports = 0.01 SOL
```

## 🔧 Código Corregido Sugerido

Crear un helper para calcular fees correctamente:

```rust
// En src/trading/executor.rs

/// Calcular precio por CU desde fee total deseado
fn calculate_compute_unit_price(
    desired_priority_fee_lamports: u64,
    compute_limit: u32,
) -> u64 {
    // Convertir a micro-lamports por CU
    (desired_priority_fee_lamports * 1_000_000) / compute_limit as u64
}

// Uso:
let compute_limit = 300_000;
let desired_fee = self.config.priority_fee_lamports;

let price_per_cu = calculate_compute_unit_price(desired_fee, compute_limit);

let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
    price_per_cu  // Micro-lamports por CU (correcto)
);
```

## 📊 Resumen Ejecutivo

**Para Testing Económico:**
```env
PRIORITY_FEE_LAMPORTS=10000       # ~$0.0003 por TX
AUTO_BUY_AMOUNT_SOL=0.01          # $0.20 por trade
# Costo total por prueba: ~$0.20
```

**Para Sniper Bot en Producción:**
```env
PRIORITY_FEE_LAMPORTS=200000      # ~$0.004 por TX
AUTO_BUY_AMOUNT_SOL=0.1           # $2.00 por trade
# Costo total por snipe: ~$2.004
```

**Budget Diario Recomendado:**
- Testing: 0.1 SOL ($2)
- Producción ligera: 1 SOL ($20)
- Producción activa: 5-10 SOL ($100-200)

---

**Última actualización: Enero 2025**
**Precio SOL de referencia: $20 USD**
