# Synapse Hyperplane Accounts Engine

**Ultra-low-latency AccountsDB-compatible read engine for Agave 4.0.0**

## Vision

```
Agave continues to do validator/runtime.
Synapse Hyperplane reads, indexes, materializes, and serves queries.
```

An external read fabric that uses Agave's `/accounts` files as zero-copy base layer, Geyser as live delta stream, compressed bitmap indexes as query fabric, and materialized RPC read mesh to serve billions of Solana account queries **without pushing secondary-index pressure into the validator process**.

---

## Architecture

```
+--------------------------------------------------------------+
|                         Agave 4.0.0                           |
|  /accounts files     AccountsDB     Replay     Banking       |
|        |                 |             |          |           |
|        +------ read-only snapshot scan +          |           |
|                                      Geyser account updates   |
+-------------------------------^------------------------------+
                                |
                                v
+--------------------------------------------------------------+
|              Synapse Hyperplane Accounts Engine              |
|                                                              |
|  1. Account File Mapper (mmap, zero-copy)                   |
|  2. External Pubkey Locator (RocksDB)                       |
|  3. Program/Token Secondary Indexer (RoaringBitmap)         |
|  4. Delta Write Store (append-only segments)                |
|  5. Adaptive Query Materializer                             |
|  6. Hot Cache Plane (L1 + DragonflyDB)                      |
|  7. RPC Read Provider Mesh                                  |
|  8. Slot/Root Reconciler                                    |
+--------------------------------------------------------------+
```

---

## Core Design

**Base state** = read directly from Agave `/accounts` files where possible  
**Live state** = Geyser deltas  
**Query state** = merged view: **delta first, base second**

---

## Workspace Structure

```
synapse-hyperplane/
├── Cargo.toml (workspace)
├── README.md
├── CONTRIBUTING.md
├── rust-toolchain.toml
├── docs/
│   ├── ARCHITETTURA_COMPLETA.md
│   ├── BUILD_CONFIG.md
│   ├── VPS_DEPLOYMENT.md
│   └── VPS_TESTS_GUIDE.md
├── crates/
│   ├── hyperplane-types/       # Core types
│   ├── account-file-mapper/    # Phase 1: mmap scanner
│   ├── base-locator/           # Phase 1: RocksDB locator
│   ├── geyser-bridge/          # Phase 2: Geyser plugin
│   ├── delta-plane/            # Phase 2: delta store
│   ├── index-fabric/           # Phase 3: bitmap indexes
│   ├── query-orchestrator/     # Phase 4: query planner
│   ├── rpc-read-provider/      # RPC server
│   ├── cache-plane/            # L1 cache
│   ├── slot-reconciler/        # Commitment tracking
│   ├── control-plane/          # Metrics and health
│   ├── synapse-hyperplane/     # Main runtime
│   └── synapse-hyper-cli/      # Testing CLI
├── configs/
│   └── production.toml
├── scripts/
│   ├── generate_test_accounts.py
│   └── vps-production-tests.sh
├── benches/
│   ├── bench_cost_model.rs
│   └── bench_lsm_hotcold.rs
└── tests/
    ├── integration_cost_model.rs
    ├── integration_lsm_hotcold.rs
    └── integration_shared_numa_cache.rs
```

---

## Development Phases

### Phase 1: Account Seed + getAccountInfo [COMPLETE]

**Status:** Production-ready

- [x] Account file mapper (mmap scanner)
- [x] Base locator (pubkey -> location) - redb/RocksDB dual backend
- [x] Pubkey dictionary (compression 32→8 bytes)
- [x] RPC `getAccountInfo` + `getMultipleAccounts` (stub)
- [x] L1 cache (DashMap)
- [x] Binaries: `synapse-base-scanner`, `synapse-rpc`, `synapse-delta-plane`

**Initial Benchmarks:** 500k accounts/sec scan throughput (500 accounts in 1ms)

### Phase 2: Geyser Delta [IN PROGRESS]

**Status:** Development

- [ ] Geyser bridge plugin (ring buffer writer)
- [ ] Delta segment store (append-only)
- [ ] Delta locator
- [ ] Merge read path (delta first, base second)
- [ ] Compactor for merge delta → base

### Phase 3: Program + Token Indexes [PARTIAL]

**Status:** Foundation complete, indexes pending

- [x] Pubkey dictionary (base structure)
- [ ] Program bitmap (RoaringTreemap)
- [ ] Token owner bitmap
- [ ] Token mint bitmap

### Phase 4: getProgramAccounts Planner [COMPLETE]

**Status:** Implementation complete, production benchmarks pending

- [x] Bitmap intersection engine
- [x] DataSize index
- [x] Memcmp accelerator (pre-indexing common offsets)
- [x] Query cost model (cardinality estimation and plan optimization)
- [x] LSM Bitmap (Delta Architecture for lock-free writes)
- [x] Hot/Cold Index Separation (tiered storage)
- [x] Shared Memory API (zero-copy query results)
- [x] NUMA-Aware Fabric (memory pinning)
- [x] Adaptive Secondary Indexes (automatic migration)
- [x] Tiered Dictionary (L1/L2/L3 cache)
- [x] SIMD Bitmap Engine (vectorized operations)
- [x] Query Result Cache (multi-level with TTL)

**Note:** Full production benchmarks will be published in v0.2.0 after VPS testing.

### Phase 5: Materialized Query Engine [PLANNED]

**Status:** Design phase

- [ ] Query hash system
- [ ] Result pages
- [ ] Invalidation graph

---

## Memory Budget (312 GB RAM Machine)

```
Agave:             160-200 GB
Synapse engine:     80-110 GB
  - L1 hot accounts:  16-32 GB
  - Bitmaps:          24-48 GB
  - Pubkey dict:      8-16 GB
  - Encoded cache:    8-16 GB
  - Ingest buffers:   4-8 GB
  - Query workers:    8-16 GB
OS page cache:      20-40 GB
```

**Key principle:** Never load all accounts in RAM. Use mmap + indexes + caches.

---

## Technical Choices

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| Locator stores | RocksDB | Prefix bloom, fast point lookups |
| Account bytes | mmap + custom segments | Zero-copy reads, append-only |
| Indexes | RoaringBitmap | Compression, fast intersections |
| Hot cache | DashMap + DragonflyDB | L1 (local) + L2 (distributed) |
| Query brain | Postgres | Analytics, registry, billing |

---

## Performance Targets

| Method | Hot (p50) | Warm (p50) | Cold (p50) |
|--------|-----------|------------|------------|
| `getAccountInfo` | <1ms | 1-3ms | 3-8ms |
| `getMultipleAccounts` (100) | <5ms | 5-15ms | 15-50ms |
| `getTokenAccountsByOwner` | 1-3ms | 3-10ms | 10-30ms |
| `getProgramAccounts` | 10-50ms | 50-200ms | 200-1000ms |

**Note:** Targets are design specifications. Full benchmark results will be published in v0.2.0 after production VPS testing.

---

## Phase 4 Components

### Implemented Features

1. **Query Cost Model** - Query plan optimization with cardinality estimation
2. **Memcmp Accelerator** - Pre-indexing common offsets for O(1) lookup
3. **LSM Bitmap** - Bitmap Delta Architecture for lock-free writes
4. **Hot/Cold Index** - Tiered storage for frequent access patterns
5. **Shared Memory API** - Zero-copy query results via mmap
6. **NUMA-Aware Fabric** - Memory pinning for NUMA nodes
7. **Adaptive Indexes** - Automatic Bitmap/B-Tree/Hash migration
8. **Tiered Dictionary** - L1/L2/L3 cache for discriminators
9. **SIMD Bitmap Engine** - Vectorized bitmap operations
10. **Query Result Cache** - Multi-level cache with TTL/slot invalidation

### Test Coverage

```bash
# Run integration tests
cargo test --test integration_* --release

# Results:
# - index-fabric: 37 tests passed
# - query-orchestrator: 8 tests passed
# - synapse-hyperplane: 2 tests passed
# Total: 47/47 tests passing
```

### Production Testing CLI

A professional TUI-based testing and monitoring interface:

```bash
# Build
cargo build --release -p synapse-hyper-cli

# Run all tests with monitoring
./target/release/synapse-hyper all

# Run specific test type
./target/release/synapse-hyper unit
./target/release/synapse-hyper integration
./target/release/synapse-hyper bench

# Documentation
cat crates/synapse-hyper-cli/README.md
cat crates/synapse-hyper-cli/USAGE.md
```

**Keyboard Controls:**

| Key | Action |
|-----|--------|
| `q` (2x) | Quit |
| `SPACE` | Start test |
| `c` | Cancel test |
| `r` | Restart |
| `Tab` | Next tab |
| `1-4` | Jump to tab |
| `Up/Down` or `k/j` | Scroll |

---

## Getting Started

### Prerequisites

```bash
# Rust toolchain (see rust-toolchain.toml)
rustup install stable
rustup default stable

# macOS (no external dependencies - redb backend)
# No additional installation required

# Linux (for RocksDB backend)
sudo apt-get install librocksdb-dev build-essential pkg-config libssl-dev
```

### Build

```bash
# macOS (redb backend default - pure Rust, no C++ dependencies)
cargo build --workspace --release

# Linux production (RocksDB backend)
cargo build --workspace --release --features rocksdb-backend --no-default-features

# Verify build
cargo build --workspace --features redb-backend
```

### Quick Start

```bash
# 1. Generate test accounts (500 fake accounts)
python scripts/generate_test_accounts.py

# 2. Run base scanner
./target/release/synapse-base-scanner \
  --accounts-path /tmp/synapse-test/accounts \
  --locator-path /tmp/synapse-test/locator \
  --dictionary-path /tmp/synapse-test/dictionary

# Output: "Scanned 500 accounts in 1ms (500k accounts/sec)"

# 3. Start RPC server
./target/release/synapse-rpc \
  --bind 0.0.0.0:8898 \
  --locator-path /tmp/synapse-test/locator

# 4. Test query
curl -X POST http://localhost:8898 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":["11111111111111111111111111111111"]}'
```

### Test

```bash
# Run test suite
cargo test --workspace --features redb-backend

# Specific crate tests
cargo test -p base-locator --features redb-backend
cargo test -p account-file-mapper --features redb-backend

# Performance tests (ignored by default)
cargo test --workspace --features redb-backend -- --ignored
```

---

## Documentation

| Document | Description |
|----------|-------------|
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution guidelines |
| [docs/ARCHITETTURA_COMPLETA.md](docs/ARCHITETTURA_COMPLETA.md) | Complete architecture |
| [docs/BUILD_CONFIG.md](docs/BUILD_CONFIG.md) | Build configuration guide |
| [docs/VPS_DEPLOYMENT.md](docs/VPS_DEPLOYMENT.md) | VPS deployment guide |
| [docs/VPS_TESTS_GUIDE.md](docs/VPS_TESTS_GUIDE.md) | Production testing guide |
| [crates/synapse-hyper-cli/README.md](crates/synapse-hyper-cli/README.md) | Testing CLI documentation |
| [crates/synapse-hyper-cli/USAGE.md](crates/synapse-hyper-cli/USAGE.md) | CLI quick start |

---

## Deployment

### VPS Production Deployment

For production deployment on VPS infrastructure:

```bash
# 1. Clone repository
git clone https://github.com/your-org/synapse-hyperplane.git
cd synapse-hyperplane

# 2. Build release
cargo build --release --features rocksdb-backend --no-default-features

# 3. Configure
cp configs/production.toml configs/local.toml
# Edit configs/local.toml with your paths

# 4. Run
./target/release/synapse-hyperplane --config configs/local.toml
```

See [docs/VPS_DEPLOYMENT.md](docs/VPS_DEPLOYMENT.md) for complete deployment guide.

### Systemd Services

Production deployment includes systemd service definitions:

- `synapse-runtime.service` - Main runtime process
- `synapse-scanner.service` - Background account scanner
- `synapse-monitor.service` - Health monitoring

Configuration templates available in `docs/deployment/`.

---

## Configuration

See `configs/production.toml` for full configuration options:

```toml
[engine]
memory_budget_gb = 96
num_shards = 64

[agave]
accounts_path = "/mnt/accounts"
rpc_fallback = "http://127.0.0.1:8899"

[base_mapper]
mmap = true
scan_threads = 24

[cache]
l1_hot_accounts_gb = 24
l1_bitmaps_gb = 32
```

---

## Monitoring

### Metrics Endpoints

```bash
# Prometheus metrics
curl http://localhost:9090/metrics

# Health check
curl http://localhost:9090/health

# Statistics
curl http://localhost:9090/stats
```

### Key Metrics

| Metric | Description |
|--------|-------------|
| `synapse_queries_total` | Total queries served |
| `synapse_query_latency_seconds` | Query latency histogram |
| `synapse_cache_hits_total` | Cache hit counter |
| `synapse_accounts_scanned_total` | Accounts scanned from files |
| `synapse_geyser_deltas_total` | Geyser updates received |

---

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Commit your changes (`git commit -am 'Add new feature'`)
4. Push to the branch (`git push origin feature/my-feature`)
5. Create a Pull Request

### Development Workflow

```bash
# Run tests before committing
cargo test --workspace

# Format code
cargo fmt --all

# Check clippy lints
cargo clippy --workspace -- -D warnings

# Build documentation
cargo doc --workspace --no-deps
```

---

## License

Apache-2.0

---

## Contact

| Resource | Link |
|----------|------|
| GitHub Issues | https://github.com/your-org/synapse-hyperplane/issues |
| Documentation | https://docs.synapse-hyperplane.dev |
| Website | https://synapse-hyperplane.dev |

---

**Version:** 0.1.0-beta  
**Last Updated:** May 2026  
**Status:** Beta Release - Production Benchmarks Pending
