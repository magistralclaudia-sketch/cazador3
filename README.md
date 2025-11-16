# 🚀 Meteora DAMM V2 Sniper Bot

Bot ultra-rápido de sniper para Meteora DAMM V2 en Solana usando Yellowstone Geyser gRPC para detección instantánea de nuevos pools.

## ✨ Características

- **⚡ Velocidad Máxima**: Usa Yellowstone Geyser gRPC para streaming en tiempo real directo desde el validador
- **🎯 Auto-Snipe**: Compra automática en nuevos pools detectados
- **💰 Take Profit / Stop Loss**: Sistema automático de gestión de riesgo
  - Take Profit: 10% por defecto
  - Stop Loss: -2% por defecto
- **📊 Monitoreo en Tiempo Real**: Seguimiento continuo de todas las posiciones
- **🔒 RPC Propio**: Optimizado para usar tu nodo RPC local

## 🏗️ Arquitectura

```
src/
├── config.rs              # Configuración del bot
├── geyser_client.rs       # Cliente Geyser para monitoreo ultra-rápido
├── meteora/
│   ├── pool.rs           # Estructuras de Pool DAMM V2
│   └── price.rs          # Cálculos de precio (sqrtPrice -> price)
├── trading/
│   ├── executor.rs       # Ejecución de trades
│   └── position_manager.rs  # Gestión de posiciones y TP/SL
└── main.rs               # Orquestación principal
```

## 📋 Requisitos Previos

### 1. Nodo Solana con Yellowstone Geyser gRPC

Tu nodo RPC propio debe tener configurado el plugin Yellowstone gRPC:

```bash
# Instalar Yellowstone gRPC plugin en tu nodo
git clone https://github.com/rpcpool/yellowstone-grpc.git
cd yellowstone-grpc
cargo build --release

# Configurar en tu solana-validator
# Agregar en validator.sh o config.toml:
--geyser-plugin-config /path/to/geyser-config.json
```

**Ejemplo de geyser-config.json:**
```json
{
  "libpath": "/path/to/libyellowstone_grpc_geyser.so",
  "address": "0.0.0.0:10000",
  "log": {
    "level": "info"
  }
}
```

### 2. Wallet con SOL

- Crea un wallet nuevo o usa uno existente
- Asegúrate de tener suficiente SOL para trades

```bash
# Generar nueva wallet
solana-keygen new --outfile wallet.json

# O exportar wallet existente a JSON
# El formato debe ser un array de bytes: [123, 45, 67, ...]
```

## 🚀 Instalación

### 1. Clonar y compilar

```bash
cd cazador3

# Compilar en modo release para máxima velocidad
cargo build --release
```

### 2. Configurar variables de entorno

Edita el archivo `.env`:

```bash
# RPC Configuration
RPC_URL=http://192.168.0.50:8899
RPC_WS_URL=ws://192.168.0.50:8900

# Geyser gRPC endpoint (CRÍTICO para velocidad)
GEYSER_ENDPOINT=http://192.168.0.50:10000
GEYSER_X_TOKEN=

# Wallet
WALLET_KEYPAIR_PATH=./wallet.json

# Meteora DAMM V2 Program ID (NO CAMBIAR)
METEORA_PROGRAM_ID=cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG

# Trading Configuration
AUTO_BUY_ENABLED=true          # false para solo monitorear
AUTO_BUY_AMOUNT_SOL=0.1        # Cantidad a comprar automáticamente
MAX_SLIPPAGE_BPS=300           # 3% slippage máximo
PRIORITY_FEE_LAMPORTS=100000   # Fee de prioridad para velocidad

# Risk Management
TAKE_PROFIT_PERCENT=10.0       # Vender al +10%
STOP_LOSS_PERCENT=-2.0         # Vender al -2%

# Monitoring
MIN_LIQUIDITY_SOL=1.0          # Liquidez mínima requerida
```

### 3. Preparar wallet

```bash
# Copiar tu wallet
cp /path/to/your/wallet.json ./wallet.json

# O crear nueva
solana-keygen new --outfile wallet.json

# Fondear wallet
solana transfer <WALLET_ADDRESS> 1 --url http://192.168.0.50:8899
```

## ▶️ Uso

### Modo Producción (Auto-compra)

```bash
# Ejecutar con auto-compra habilitada
cargo run --release
```

El bot:
1. Se conectará a Geyser gRPC
2. Monitoreará nuevos pools de Meteora DAMM V2
3. Comprará automáticamente cuando detecte un nuevo pool
4. Monitoreará la posición con Take Profit 10% y Stop Loss -2%
5. Venderá automáticamente cuando se cumpla TP o SL

### Modo Monitoreo (Solo observar)

```bash
# Editar .env
AUTO_BUY_ENABLED=false

# Ejecutar
cargo run --release
```

El bot solo reportará nuevos pools sin comprar.

## ⚙️ Configuración Avanzada

### Ajustar Velocidad

Para máxima velocidad:

1. **Priority Fee alto**: Aumenta `PRIORITY_FEE_LAMPORTS` en .env
2. **RPC local**: Usa tu nodo local, no RPC público
3. **Geyser Commitment**: Ya configurado en `confirmed` para balance velocidad/seguridad

### Optimizar Take Profit / Stop Loss

```bash
# Trading conservador
TAKE_PROFIT_PERCENT=5.0
STOP_LOSS_PERCENT=-1.0

# Trading agresivo
TAKE_PROFIT_PERCENT=20.0
STOP_LOSS_PERCENT=-5.0
```

### Filtrar por Liquidez

```bash
# Solo pools con > 10 SOL de liquidez
MIN_LIQUIDITY_SOL=10.0
```

## 🔧 Componentes Técnicos Importantes

### Cálculo de Precio

Meteora DAMM V2 usa **constant product** (x * y = k) con precio almacenado como `sqrt_price` en formato Q64.64:

```rust
// Fórmula en src/meteora/price.rs
price = (sqrt_price / 2^64)^2
```

### Detección de Pools

El bot se suscribe a **TODAS** las cuentas nuevas del programa Meteora usando filtros de Geyser:

```rust
// En src/geyser_client.rs
- Filtro por owner: METEORA_PROGRAM_ID
- Filtro por tamaño mínimo de cuenta
- Commitment: confirmed
```

### Ejecución de Swap

**⚠️ IMPORTANTE**: La función `build_swap_instruction()` en `src/trading/executor.rs` es **SIMPLIFICADA**.

Para producción necesitas:

#### Opción 1: Obtener IDL de Meteora

```bash
# Descargar IDL del programa
anchor idl fetch cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG -o meteora_damm_v2.json

# Usar con anchor-client
```

#### Opción 2: Usar Jupiter Aggregator

Jupiter soporta Meteora automáticamente:

```bash
# Agregar Jupiter SDK
cargo add jupiter-swap-api-client
```

Ejemplo:
```rust
// Usar Jupiter para swap
let quote = jupiter_client.quote(
    &pool_info.pool.token_a_mint,
    &pool_info.pool.token_b_mint,
    amount,
).await?;

let swap_tx = jupiter_client.swap(quote).await?;
```

#### Opción 3: Construir instrucción manualmente

Necesitas:
1. Estudiar el código fuente: https://github.com/MeteoraAg/damm-v2
2. Identificar el discriminador correcto de la instrucción `swap`
3. Serializar parámetros correctamente
4. Incluir todas las cuentas necesarias

## 🐛 Troubleshooting

### Error: "Failed to connect to Geyser gRPC"

```bash
# Verificar que Geyser esté corriendo
netstat -tuln | grep 10000

# Verificar logs del nodo
tail -f /path/to/solana/logs/validator.log | grep geyser
```

### Error: "Invalid pubkey"

- Verifica que METEORA_PROGRAM_ID sea correcto
- No cambies este valor a menos que estés en devnet

### Error: "Failed to deserialize pool"

- Normal para cuentas que no son pools
- El bot filtra y solo procesa pools válidos

### Sin pools detectados

- Meteora DAMM V2 pools son menos frecuentes que DLMM
- Puedes testear con devnet primero
- Verifica que el filtro de tamaño no sea muy restrictivo

## 📊 Logs y Monitoreo

El bot muestra información detallada:

```
🆕 ═══════════════════════════════════════
   NUEVO POOL DETECTADO!
═══════════════════════════════════════
📍 Address: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
🪙 Token A: So11111111111111111111111111111111111111112
🪙 Token B: EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v
💧 Liquidez: 5000000000
💰 Precio: 150.5
🎯 EJECUTANDO AUTO-COMPRA...
✅ ¡COMPRA EXITOSA!
📝 Signature: 2ZE7Rw...
```

### Logs de posición:

```
📊 Pool actualizado: 7xKX... | PnL: +5.2%
🎉 ¡TAKE PROFIT! PnL = +10.1%
💸 Ejecutando venta: Take Profit
✅ Venta exitosa! Signature: 3YT8...
```

## ⚠️ Advertencias y Consideraciones

1. **Riesgo de Rug Pulls**: Los nuevos pools pueden ser scams. El bot NO verifica legitimidad del token.

2. **Slippage**: En pools nuevos con poca liquidez, el slippage puede ser alto.

3. **MEV Bots**: Otros bots pueden ser más rápidos. Geyser ayuda pero no garantiza ser primero.

4. **Fees de Red**: Durante congestión, las fees pueden ser muy altas.

5. **Liquidez Insuficiente**: Verifica que MIN_LIQUIDITY_SOL esté configurado apropiadamente.

6. **Testing**: SIEMPRE testea con cantidades pequeñas primero.

## 🔐 Seguridad

- **Nunca** compartas tu `wallet.json`
- **Nunca** comitees `wallet.json` al git
- Usa una wallet separada solo para el bot
- Mantén solo el capital necesario en la wallet

## 📚 Recursos

- [Meteora Docs](https://docs.meteora.ag/)
- [Yellowstone gRPC](https://github.com/rpcpool/yellowstone-grpc)
- [Solana Cookbook](https://solanacookbook.com/)
- [Anchor Lang](https://www.anchor-lang.com/)

## 🤝 Contribuciones

Para mejorar el bot:

1. Implementar instrucción de swap completa con IDL
2. Agregar soporte para Jupiter Aggregator
3. Implementar reconexión automática a Geyser
4. Agregar backtesting con datos históricos
5. Dashboard web para monitoreo

## 📝 TODO

- [ ] Implementar swap instruction completa (actualmente simplificada)
- [ ] Agregar soporte para Jupiter
- [ ] Reconexión automática de Geyser
- [ ] Tests unitarios
- [ ] Dashboard web
- [ ] Soporte para DLMM además de DAMM V2
- [ ] Multi-wallet support
- [ ] Telegram notifications

## ⚖️ License

MIT

## ⚠️ Disclaimer

Este bot es solo para fines educativos. Trading de criptomonedas conlleva riesgos significativos. Usa bajo tu propio riesgo.

---

**¡Buena suerte cazando! 🎯**
