# Synapse Hyperplane - Build Configuration Guide

## Storage Backend Configuration

Synapse Hyperplane supports two storage backends via feature flags:

### 1. Redb Backend (Default - macOS Development)

**Use case**: macOS development, quick setup, no C++ compilation

```bash
# Build with default redb backend
cargo build --workspace --features redb-backend

# Or simply (redb-backend is default)
cargo build --workspace
```

**Pros**:
- Pure Rust, no external dependencies
- Fast compilation on macOS
- Good for development and testing

**Cons**:
- Slightly slower than RocksDB for very large datasets
- Less battle-tested in production

### 2. RocksDB Backend (Production - Linux/Ubuntu)

**Use case**: Production deployments on Linux, maximum performance

```bash
# Install RocksDB on Ubuntu/Debian
sudo apt-get install librocksdb-dev

# Or on macOS with Homebrew (if you want to test)
brew install rocksdb

# Build with RocksDB backend
cargo build --workspace --features rocksdb-backend --no-default-features
```

**Environment variables for Linux**:
```bash
export ROCKSDB_LIB_DIR=/usr/lib
export ROCKSDB_INCLUDE_DIR=/usr/include
```

**Pros**:
- Production-grade performance
- Better compression
- More mature ecosystem

**Cons**:
- Requires C++ compilation
- Slower build times on macOS

## Feature Flag Summary

| Feature | Backend | Use Case | Default |
|---------|---------|----------|---------|
| `redb-backend` | Redb (pure Rust) | macOS dev, quick setup | ✅ Yes |
| `rocksdb-backend` | RocksDB (C++) | Linux production | ❌ No |

## Building for Different Platforms

### macOS (Development)
```bash
cargo build --workspace
```

### Linux (Production)
```bash
# Install dependencies
sudo apt-get install librocksdb-dev build-essential

# Build with RocksDB
cargo build --workspace --release --features rocksdb-backend --no-default-features
```

### Cross-compilation
```bash
# Add target
rustup target add x86_64-unknown-linux-gnu

# Build for Linux from macOS (requires cross-compilation setup)
cargo build --workspace --target x86_64-unknown-linux-gnu --features rocksdb-backend --no-default-features
```

## Running Binaries

### Base Scanner (Phase 1 MVP)
```bash
# Scan Agave /accounts files and build base locator
cargo run --bin synapse-base-scanner --features redb-backend -- \
  --accounts-path /mnt/accounts \
  --locator-path /mnt/nvme/synapse/base-locator \
  --dictionary-path /mnt/nvme/synapse/pubkey-dict
```

### RPC Server (Phase 1 MVP)
```bash
# Start RPC read provider
cargo run --bin synapse-rpc --features redb-backend -- \
  --bind 0.0.0.0:8898 \
  --workers 32 \
  --locator-path /mnt/nvme/synapse/base-locator
```

## Testing

```bash
# Run tests with redb backend
cargo test --workspace --features redb-backend

# Run tests with RocksDB backend
cargo test --workspace --features rocksdb-backend --no-default-features
```

## Production Deployment (Linux)

```bash
# 1. Install dependencies
sudo apt-get update
sudo apt-get install -y librocksdb-dev build-essential pkg-config

# 2. Build release binary
cargo build --release --features rocksdb-backend --no-default-features

# 3. Deploy binaries
cp target/release/synapse-base-scanner /usr/local/bin/
cp target/release/synapse-rpc /usr/local/bin/

# 4. Create systemd services (see deploy/ directory)
```

## Memory Budget (312 GB RAM Machine)

```
Agave validator:      160-200 GB
Synapse engine:        80-110 GB
  - Locator DB:        20-40 GB (RocksDB cache)
  - Hot account cache: 16-32 GB
  - Bitmap indexes:    24-48 GB
  - Query workers:      8-16 GB
OS page cache:         20-40 GB
```

## Troubleshooting

### RocksDB compilation errors on macOS
```
error: 'cstdint' file not found
```
**Solution**: Use redb-backend for macOS development, or install Xcode Command Line Tools:
```bash
xcode-select --install
```

### Missing RocksDB library on Linux
```
error: couldn't find rocksdb library
```
**Solution**: Install librocksdb-dev:
```bash
sudo apt-get install librocksdb-dev
```

### Redb database corruption
```
error: database file is corrupted
```
**Solution**: Delete and rebuild:
```bash
rm -rf /mnt/nvme/synapse/base-locator/*.redb
cargo run --bin synapse-base-scanner -- ...
```

## Architecture Overview

```
┌─────────────────────────────────────────┐
│         Agave 4.0.0 Validator           │
│  /accounts files (read-only seed)       │
│  Geyser plugin → live updates           │
└──────────────┬──────────────────────────┘
               │
               v
┌─────────────────────────────────────────┐
│    Synapse Hyperplane Accounts Engine   │
│                                         │
│  Base Locator (RocksDB/Redb)            │
│    └─> pubkey → AccountLocation         │
│                                         │
│  Delta Store (append-only segments)     │
│    └─> live Geyser updates              │
│                                         │
│  Index Fabric (RoaringBitmap)           │
│    └─> program_id, token_owner, etc.    │
│                                         │
│  RPC Read Provider                      │
│    └─> getAccountInfo, getProgramAccts  │
└─────────────────────────────────────────┘
```

## Next Steps (Phase 2+)

1. **Geyser Bridge**: Implement live update ingestion
2. **Delta Plane**: Build append-only segment store
3. **Index Fabric**: Implement bitmap indexes
4. **Query Orchestrator**: Bitmap query planner
5. **Materialized Views**: Cache repeated queries

See `docs/ARCHITECTURE.md` for full details.
