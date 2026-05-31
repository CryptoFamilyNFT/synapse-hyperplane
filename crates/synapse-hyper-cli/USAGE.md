# Synapse Hyper CLI - Usage Guide

## Quick Start

```bash
# Build
cargo build --release -p synapse-hyper-cli

# Run
./target/release/synapse-hyper all
```

## Commands

### 1. Unit Tests

```bash
# All unit tests
synapse-hyper unit

# Filter by pattern
synapse-hyper unit index_fabric
```

### 2. Integration Tests

```bash
# All integration tests
synapse-hyper integration

# Specific test
synapse-hyper integration integration_cost_model

# Include performance tests
synapse-hyper integration --include-performance
```

### 3. Benchmarks

```bash
# All benchmarks
synapse-hyper bench

# Specific benchmark
synapse-hyper bench bench_cost_model

# Custom samples
synapse-hyper bench --sample-count 200
```

### 4. All Tests

```bash
# Complete test suite
synapse-hyper all

# With benchmarks
synapse-hyper all --with-benchmarks
```

## TUI Controls

| Key | Action |
|-----|--------|
| `q` | Quit (confirm twice) |
| `SPACE` | Start test |
| `c` | Cancel test |
| `r` | Restart |
| `Tab` | Next tab |
| `1-4` | Jump to tab |
| `↑/k` | Scroll up |
| `↓/j` | Scroll down |

## Tabs

1. **Overview** - Statistics and logs
2. **Live Output** - Real-time test output
3. **Results** - Detailed results
4. **Metrics** - Performance charts

## Options

```bash
# Backend
--features rocksdb-backend

# Profile
--profile unit|integration|performance|all

# Timeout
--timeout 300

# Verbose
--verbose

# Save report
--report output.md
```

## Examples

### Development

```bash
# Quick tests
synapse-hyper unit

# Before commit
synapse-hyper integration
```

### Production

```bash
# Full suite
synapse-hyper all --with-benchmarks --report prod_report.md

# Monitor
synapse-hyper monitor --pid 12345
```

## Output

Generates:
- `test_results.log`
- `integration_results.log`
- `benchmark_results.log`
- `<report>.md`

## Troubleshooting

```bash
# Clean build
cargo clean && cargo build --release -p synapse-hyper-cli

# Debug mode
RUST_LOG=debug synapse-hyper all
```
