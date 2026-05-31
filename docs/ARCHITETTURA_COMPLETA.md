# Synapse Hyperplane - Architettura Completa 🚀

## Panoramica

Synapse Hyperplane è un motore di lettura ultra-low-latency per Agave 4.0.0 che implementa:
- **Geyser Plugin** per live streaming degli account updates
- **Delta Plane** per storage append-only degli aggiornamenti
- **Index Fabric** per indici bitmap compressi
- **Query Orchestrator** per query getProgramAccounts ottimizzate
- **Slot Reconciler** per commitment tracking e rollback
- **Runtime multi-threaded** per orchestrazione di tutti i componenti

## Struttura del Workspace

```
crates/
├── hyperplane-types/          # Tipi fondamentali (AccountView, Location, etc.)
├── account-file-mapper/       # Mapping account file Agave → strutture interne
├── base-locator/              # Localizzatore account per storage base
├── geyser-bridge/             # Geyser plugin + ring buffer shared memory
├── delta-plane/               # Delta store con segmenti append-only
├── index-fabric/              # Indici bitmap compressi (RoaringBitmap)
│   ├── program_index.rs       # Programma → accounts
│   ├── token_owner_index.rs   # Token owner → token accounts
│   ├── token_mint_index.rs    # Token mint → token accounts
│   ├── data_size_index.rs     # Data size → accounts
│   ├── memcmp_index.rs        # Offset+bytes → accounts
│   └── discriminator_index.rs # Discriminator Anchor → accounts
├── query-orchestrator/        # Query planner + executor
│   ├── query_planner.rs       # Pianificazione query getProgramAccounts
│   ├── bitmap_intersection.rs # Motore di intersezione bitmap
│   ├── cost_estimator.rs      # Stima costo query
│   └── pagination.rs          # Supporto paginazione
├── slot-reconciler/           # Commitment tracking e rollback
│   ├── lib.rs                 # SlotReconciler principale
│   ├── commitments.rs         # Commitment level tracking
│   ├── rollback.rs            # Rollback handler
│   ├── roots.rs               # Root slot per compaction
│   └── watermarks.rs          # Watermark tracker
├── cache-plane/               # Cache L1/L2 (memory + DragonflyDB)
├── rpc-read-provider/         # RPC server per query account
├── control-plane/             # Metrics, health checks, admin API
├── synapse-hyperplane/        # Runtime principale e binary
│   ├── runtime.rs             # Multi-threaded runtime
│   └── index_manager.rs       # Ponte Delta → Indexes
└── target/                    # Build artifacts
```

## Componenti Principali

### 1. Geyser Bridge
**File**: `crates/geyser-bridge/src/`
- **RingBufferWriter**: Scrive aggiornamenti account in shared memory
- **RingBufferReader**: Legge aggiornamenti per Delta Consumer
- **GeyserPlugin**: Implementa interfaccia Agave Geyser v4.0.0
- **Supporto**: V0_0_1, V0_0_2, V0_0_3 account updates

### 2. Delta Plane
**File**: `crates/delta-plane/src/`
- **DeltaConsumer**: Consuma aggiornamenti dal ring buffer
- **SegmentWriter**: Scrive segmenti append-only (100MB ciascuno)
- **DeltaLocator**: Traccia segmenti per pubkey+slot
- **UpdateReducer**: Riduce aggiornamenti multipli per stesso account
- **Compactor**: Compatta segmenti vecchi in base layer

### 3. Index Fabric
**File**: `crates/index-fabric/src/`

Tutti gli indici usano:
- **Pubkey Dictionary**: BTreeMap<Pubkey, u64> per comprimere pubkey → ID
- **RoaringBitmap**: Bitmap compresse per ID account (8 byte vs 32 byte)
- **Arc<RwLock<State>>**: Thread-safe con lock reading/writing

#### ProgramIndex
- **Key**: Program ID (Pubkey)
- **Value**: Bitmap di account ID
- **Query**: `get_program_accounts(program_id) -> Vec<Pubkey>`

#### TokenOwnerIndex
- **Key**: Token Owner (Pubkey)
- **Value**: Bitmap di token account ID
- **Query**: `get_token_accounts_by_owner(owner) -> Vec<Pubkey>`

#### TokenMintIndex
- **Key**: Token Mint (Pubkey)
- **Value**: Bitmap di token account ID
- **Query**: `get_token_accounts_by_mint(mint) -> Vec<Pubkey>`

#### DataSizeIndex
- **Key**: Data Size (u64)
- **Value**: Bitmap di account ID
- **Query**: `get_accounts_by_size(size) -> Vec<Pubkey>`

#### MemcmpIndex
- **Key**: MemcmpFilter { offset, bytes }
- **Value**: Bitmap di account ID
- **Query**: `get_accounts_by_memcmp(offset, bytes) -> Vec<Pubkey>`

#### DiscriminatorIndex
- **Key**: Discriminator [u8; 8]
- **Value**: Bitmap di account ID
- **Query**: `get_accounts_by_discriminator(disc) -> Vec<Pubkey>`
- **Ottimizzazione**: Anchor programs discriminator lookup

### 4. Query Orchestrator
**File**: `crates/query-orchestrator/src/`

#### QueryPlanner
```rust
pub fn plan_query(
    program_id: Pubkey,
    filters: Vec<GpaFilter>,
) -> QueryPlan
```

Filtri supportati:
- `GpaFilter::DataSize(u64)`
- `GpaFilter::Memcmp { offset, bytes }`
- `GpaFilter::Discriminator([u8; 8])`

#### BitmapIntersectionEngine
```rust
pub fn intersect(&self, bitmaps: &[&PubkeyBitmap]) -> IntersectionResult
```
- Intersezione efficiente di N bitmap
- Stima tempo: `(result.len() * num_bitmaps) / 1000` μs

#### QueryCostEstimator
- Stima cardinalità risultato
- Decide streaming vs in-memory
- Threshold: >100K accounts → streaming

#### PaginationHelper
- Cursor-based pagination
- Supporto offset + limit
- Serializzazione JSON cursori

### 5. Slot Reconciler
**File**: `crates/slot-reconciler/src/`

#### Commitment Levels
```rust
pub enum CommitmentLevel {
    Processed,  // Most recent slot
    Confirmed,  // 2/3 cluster confirmation
    Finalized,  // Supermajority finalized
}
```

#### SlotReconciler
- **Atomic slot tracking**: `AtomicU64` per processed/confirmed/finalized
- **Rollback handling**: Revert a previous root slot
- **Root tracking**: Safe compaction point

#### WatermarkTracker
- Lock-free watermark updates
- Compare-and-swap per advance

### 6. Index Manager
**File**: `crates/synapse-hyperplane/src/index_manager.rs`

Ponte tra Delta Plane e Index Fabric:
```rust
pub fn update_account(&self, account: &AccountView, slot: u64)
```

Aggiorna in parallelo:
1. Program Index
2. Token Owner Index (se Token Program)
3. Token Mint Index (se Token Program)
4. Data Size Index
5. Memcmp Index (offset 0, 8 bytes + 32 bytes)
6. Discriminator Index (primi 8 bytes)

### 7. Synapse Runtime
**File**: `crates/synapse-hyperplane/src/runtime.rs`

Multi-threaded runtime che orchestra:

#### Thread Architecture
- **1x Geyser Consumer Thread**: Legge ring buffer, aggiorna indici
- **Nx Query Worker Threads**: Processa query (default: 8)
- **1x Metrics Thread**: Espone metriche Prometheus (opzionale)

#### RuntimeConfig
```rust
pub struct RuntimeConfig {
    pub ring_buffer_path: String,        // "/dev/shm/synapse-geyser.ring"
    pub delta_path: PathBuf,             // "/tmp/synapse/delta"
    pub index_path: PathBuf,             // "/tmp/synapse/indexes"
    pub query_workers: usize,            // 8 threads
    pub index_workers: usize,            // 4 threads
    pub rpc_bind: String,                // "0.0.0.0:8898"
    pub enable_metrics: bool,            // true
    pub metrics_port: u16,               // 9090
}
```

#### Initialization Flow
```rust
let runtime = SynapseRuntime::new(config)?;
runtime.initialize_query_planner()?;
let handles = runtime.start()?;
```

## Flusso End-to-End

### 1. Account Update Path (Geyser → Index)
```
Agave Validator
    ↓ update_account()
Geyser Plugin (geyser-bridge)
    ↓ write()
Ring Buffer (shared memory /dev/shm)
    ↓ read_next()
Geyser Consumer Thread (runtime.rs)
    ↓ update_account()
Index Manager
    ├→ ProgramIndex.add_account()
    ├→ TokenOwnerIndex.add_token_account()
    ├→ TokenMintIndex.add_token_account()
    ├→ DataSizeIndex.add_account()
    ├→ MemcmpIndex.add_account_memcmp()
    └→ DiscriminatorIndex.add_account()
        ↓
Indexes Updated (in-memory)
```

### 2. Query Path (RPC → Result)
```
RPC Client
    ↓ getProgramAccounts(program_id, filters)
RPC Server (rpc-read-provider)
    ↓ query_planner.plan_query()
QueryPlanner
    ├→ Get program bitmap
    ├→ Apply dataSize filters
    ├→ Apply memcmp filters
    └→ Apply discriminator filters
        ↓
BitmapIntersectionEngine.intersect()
    ↓
QueryCostEstimator.estimate()
    ↓
PaginationHelper.paginate()
    ↓
Vec<Pubkey> (results)
    ↓
RPC Response
```

### 3. Commitment Path (Slot Reconciliation)
```
Geyser Update (slot N)
    ↓
SlotReconciler.update_processed(N)
    ↓
Cluster Confirmation
    ↓
SlotReconciler.update_confirmed(N)
    ↓
Supermajority Finalization
    ↓
SlotReconciler.update_finalized(N)
    ↓
Root Advance (safe to compact < N)
```

## Performance Targets

| Metric | Target | Implementazione |
|--------|--------|-----------------|
| Index Update Latency | < 10μs | Arc<RwLock> + RoaringBitmap |
| Query Planning | < 100μs | Bitmap intersection O(1) |
| Bitmap Intersection | < 1ms (1M accounts) | RoaringBitmap SIMD |
| Memory Overhead | < 1 byte/account | Pubkey dictionary compression |
| Pagination Overhead | < 50μs | Cursor-based, no scan |
| Geyser Throughput | 1M updates/sec | Ring buffer lock-free |
| Query Concurrency | 10K QPS | Multi-threaded workers |

## Test Coverage

### Index Fabric (7 test)
```bash
cargo test -p index-fabric
# ✅ program_index::test_program_index_basic
# ✅ token_owner_index::test_token_owner_index_basic
# ✅ token_mint_index::test_token_mint_index_basic
# ✅ data_size_index::test_data_size_index_basic
# ✅ memcmp_index::test_memcmp_index_basic
# ✅ discriminator_index::test_discriminator_index_basic
# ✅ bitmap_store::test_bitmap_store_basic
```

### Query Orchestrator (7 test)
```bash
cargo test -p query-orchestrator
# ✅ bitmap_intersection::test_bitmap_intersection_basic
# ✅ bitmap_intersection::test_bitmap_union_basic
# ✅ cost_estimator::test_cost_estimator_basic
# ✅ cost_estimator::test_cost_estimator_with_filters
# ✅ pagination::test_pagination_helper_basic
# ✅ pagination::test_pagination_cursor_serialization
# ✅ query_planner::test_query_planner_basic
```

### Slot Reconciler (2 test)
```bash
cargo test -p slot-reconciler
# ✅ slot_reconciler::test_slot_reconciler_basic
# ✅ slot_reconciler::test_slot_reconciler_rollback
```

### Synapse Runtime (2 test)
```bash
cargo test -p synapse-hyperplane --lib
# ✅ runtime::test_runtime_creation
# ✅ runtime::test_runtime_stats
```

## Binary Disponibili

### synapse-base-scanner
Scansiona account file Agave per costruire base layer:
```bash
./target/release/synapse-base-scanner \
  --accounts-dir /mnt/nvme/validator/accounts \
  --output-dir /tmp/synapse/base-locator
```

### synapse-delta-plane
Processa aggiornamenti Geyser in tempo reale:
```bash
./target/release/synapse-delta-plane \
  --ring-path /dev/shm/synapse-geyser.ring \
  --segment-path /tmp/synapse/delta-segments \
  --locator-path /tmp/synapse/base-locator
```

### synapse-rpc
RPC server per query account:
```bash
./target/release/synapse-rpc \
  --bind 0.0.0.0:8898 \
  --workers 32 \
  --locator-path /tmp/synapse/base-locator \
  --dragonfly-url redis://localhost:6379
```

## Prossimi Passi - Phase 5

### 1. Load Testing
- [ ] Benchmark con 10M accounts
- [ ] Stress test query concorrenti (10K QPS)
- [ ] Memory profiling con mass load
- [ ] Latency percentile (p50, p95, p99)

### 2. Production Deployment
- [ ] Integrazione Agave 4.0.0 validator
- [ ] Geyser plugin configuration YAML
- [ ] Kubernetes deployment manifests
- [ ] Monitoring dashboard (Grafana)

### 3. Ottimizzazioni
- [ ] SIMD bitmap intersection (AVX2/NEON)
- [ ] Parallel query execution (rayon)
- [ ] Cache-aware data layout
- [ ] Zero-copy deserialization

### 4. Feature Avanzate
- [ ] Account sorting (lamports, slots)
- [ ] Field projection (partial data fetch)
- [ ] Secondary index compositi
- [ ] Query caching (L1/L2)

## Compilation

```bash
# Build completo
cargo build --features redb-backend --release

# Test workspace
cargo test --workspace

# Run binary
./target/release/synapse-delta-plane --help
```

## Conclusioni

Synapse Hyperplane ora ha:
- ✅ **Tutti i componenti implementati** (nessun file vuoto)
- ✅ **Runtime multi-threaded** per orchestrazione
- ✅ **Slot Reconciler** completo (commitments, rollback, roots, watermarks)
- ✅ **Index Manager** per aggiornamenti real-time
- ✅ **Query Orchestrator** con bitmap intersection
- ✅ **Test coverage** su tutti i crate principali

Il sistema è pronto per l'integrazione con Agave validator e load testing production! 🎉
