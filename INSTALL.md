# Guía de Instalación - Meteora Sniper Bot

## Resolución de Problemas de Dependencias

Debido a conflictos de versiones entre `solana-sdk`, `yellowstone-grpc-client` y `anchor-lang`, puedes encontrar errores al compilar.

### Solución 1: Usar Cargo con patch (Recomendado)

El `Cargo.toml` incluye un `[patch.crates-io]` para resolver conflictos. Si no funciona, intenta:

```bash
# Limpiar cache de cargo
cargo clean
rm -rf ~/.cargo/registry/index/*
rm -rf ~/.cargo/git/db/*

# Actualizar índice
cargo update

# Intentar compilar
cargo build --release
```

### Solución 2: Ajustar versiones manualmente

Edita `Cargo.toml` y ajusta las versiones según tu entorno:

#### Para Solana SDK 1.18 (Más estable)

```toml
solana-sdk = "1.18"
solana-client = "1.18"
yellowstone-grpc-client = { git = "https://github.com/rpcpool/yellowstone-grpc", branch = "master" }
yellowstone-grpc-proto = { git = "https://github.com/rpcpool/yellowstone-grpc", branch = "master" }
anchor-lang = "0.29"
```

#### Para Solana SDK 2.0+ (Más reciente)

```toml
solana-sdk = "2.0"
solana-client = "2.0"
yellowstone-grpc-client = "10.1"
yellowstone-grpc-proto = "10.1"
anchor-lang = "0.30"
```

### Solución 3: Usar versión específica de yellowstone desde Git

```toml
[dependencies]
yellowstone-grpc-client = { git = "https://github.com/rpcpool/yellowstone-grpc", tag = "v1.14.0" }
yellowstone-grpc-proto = { git = "https://github.com/rpcpool/yellowstone-grpc", tag = "v1.14.0" }
```

## Verificación de Compilación

```bash
# Verificar dependencias
cargo tree | grep -E "(solana-sdk|yellowstone|anchor)"

# Compilar en modo debug (más rápido para testing)
cargo build

# Compilar en modo release (para producción)
cargo build --release
```

## Si todo falla

Como última opción, puedes compilar sin algunas dependencias y usarlas directamente:

1. Clonar yellowstone-grpc localmente:
```bash
git clone https://github.com/rpcpool/yellowstone-grpc.git ../yellowstone-grpc
```

2. Usar como dependencia local en `Cargo.toml`:
```toml
yellowstone-grpc-client = { path = "../yellowstone-grpc/yellowstone-grpc-client" }
yellowstone-grpc-proto = { path = "../yellowstone-grpc/yellowstone-grpc-proto" }
```

## Errores Comunes

### Error: "failed to select a version for `zeroize`"

Agrega a `Cargo.toml`:
```toml
[patch.crates-io]
zeroize = "1.3"
curve25519-dalek = "3.2.1"
```

### Error: "failed to compile `solana-sdk`"

Asegúrate de tener instalados los build tools:
```bash
# Ubuntu/Debian
sudo apt-get install build-essential pkg-config libssl-dev

# macOS
xcode-select --install
```

### Error: "could not find `Geyser` in the list of imported items"

Verifica que yellowstone-grpc-client esté correctamente instalado:
```bash
cargo update -p yellowstone-grpc-client
```

## Contacto

Si continúas teniendo problemas, crea un issue en el repositorio con:
- Output completo de `cargo build`
- Output de `cargo --version` y `rustc --version`
- Tu sistema operativo
