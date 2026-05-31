# Benchmark Results

Questa cartella contiene i risultati dei benchmark eseguiti su Synapse Hyperplane.

## Benchmark Eseguiti

### 1. Query Performance Benchmark

**Data**: [Inserisci data]
**Hardware**: [Specificare hardware]
**Dataset**: [Numero di account]

#### Risultati

| Query Type | Agave (baseline) | Synapse | Speedup |
|------------|-----------------|---------|---------|
| getProgramAccounts (no filters) | [time] | [time] | [x] |
| getProgramAccounts + dataSize | [time] | [time] | [x] |
| getProgramAccounts + memcmp | [time] | [time] | [x] |
| getProgramAccounts + discriminator | [time] | [time] | [x] |
| getProgramAccounts + all filters | [time] | [time] | [x] |

#### Grafici

![Query Latency Comparison](./charts/query_latency_comparison.png)
![Throughput over Time](./charts/throughput_over_time.png)

---

### 2. Memory Usage Benchmark

**Data**: [Inserisci data]
**Hardware**: [Specificare hardware]

#### Risultati

| Component | Memory Usage |
|-----------|-------------|
| Pubkey Dictionary | [MB] |
| Program Index | [MB] |
| Token Owner Index | [MB] |
| DataSize Index | [MB] |
| Memcmp Index | [MB] |
| Discriminator Index | [MB] |
| **Total** | **[MB]** |

#### Confronto con Agave

| System | Peak Memory | Compression Ratio |
|--------|-------------|-------------------|
| Agave 4.0.0 | [GB] | 1x |
| Synapse Hyperplane | [MB] | [x] |

---

### 3. Index Update Latency Benchmark

**Data**: [Inserisci data]
**Test**: Simulazione di 1M account updates

#### Risultati

| Metric | Value |
|--------|-------|
| Average Latency | [μs] |
| P50 Latency | [μs] |
| P95 Latency | [μs] |
| P99 Latency | [μs] |
| Max Latency | [μs] |

#### Grafici

![Update Latency Distribution](./charts/update_latency_distribution.png)

---

### 4. Concurrency Benchmark

**Data**: [Inserisci data]
**Test**: Query parallele con N worker

#### Risultati

| Concurrent Queries | Avg Latency | P99 Latency | Throughput (QPS) |
|-------------------|-------------|-------------|------------------|
| 1 | [ms] | [ms] | [QPS] |
| 10 | [ms] | [ms] | [QPS] |
| 50 | [ms] | [ms] | [QPS] |
| 100 | [ms] | [ms] | [QPS] |
| 500 | [ms] | [ms] | [QPS] |

#### Grafici

![Concurrency vs Latency](./charts/concurrency_vs_latency.png)
![Concurrency vs Throughput](./charts/concurrency_vs_throughput.png)

---

## Come Eseguire i Benchmark

### Prerequisiti

```bash
# Installa cargo-bench (opzionale, per benchmark avanzati)
cargo install cargo-bench
```

### Esecuzione

```bash
# Benchmark base
cargo bench --release

# Benchmark specifico
cargo bench --release --bench query_benchmark

# Benchmark con output JSON
cargo bench --release -- --output-format json > benchmark_results.json
```

### Custom Dataset

```bash
# Genera dataset personalizzato
python3 scripts/generate_test_accounts.py --count 1000000 --output /tmp/test_accounts

# Esegui benchmark sul dataset
SYNAPSE_TEST_ACCOUNTS=/tmp/test_accounts cargo bench --release
```

---

## Analisi dei Risultati

### Metriche Chiave

1. **Query Latency**: Deve essere < 100ms per il 99% delle query
2. **Memory Usage**: Deve essere < 10% della RAM totale
3. **Index Update Latency**: Deve essere < 10ms (consistency requirement)
4. **Throughput**: Deve supportare 1000+ QPS per produzione

### Quando Ottimizzare

- Query latency > 100ms: Aumenta query-workers o ottimizza indici
- Memory usage > 80%: Riduci pool size o abilita caching
- Update latency > 10ms: Verifica I/O disk e ring buffer
- Throughput < 1000 QPS: Aumenta worker threads o scala orizzontalmente

---

## Strumenti di Analisi

### 1. Flamegraph per CPU Profiling

```bash
# Installa flamegraph
cargo install flamegraph

# Esegui benchmark con profiling
cargo flamegraph --bench query_benchmark

# Visualizza
flamegraph.svg
```

### 2. Memory Profiling con DHAT

```bash
# Compila con DHAT
RUSTFLAGS="-Z sanitizer=memory" cargo bench --release

# Analisi output
dhat_analyzer target/release/deps/query_benchmark-*
```

### 3. I/O Profiling con iostat

```bash
# Monitora I/O durante benchmark
iostat -x 1

# Cerca:
# - %util: Deve essere < 80%
# - await: Deve essere < 10ms
# - r/s, w/s: Throughput I/O
```

---

## Template per Report

Copia e compila questo template per ogni sessione di benchmark:

```markdown
# Benchmark Report - [DATA]

## Configurazione

- **Hardware**: [CPU, RAM, Storage]
- **OS**: [Ubuntu/Debian version]
- **Rust Version**: [rustc --version]
- **Synapse Version**: [git rev-parse HEAD]
- **Agave Version**: [4.0.0]

## Dataset

- **Account Count**: [numero]
- **Program Accounts**: [numero]
- **Token Accounts**: [numero]

## Risultati

[Incolla risultati]

## Analisi

[Incolla analisi]

## Conclusioni

[Punti chiave e azioni raccomandate]
```

---

*Ultimo aggiornamento: 31 Maggio 2026*
