# Synapse Hyper CLI

Production testing and monitoring interface for Synapse Hyperplane.

## Installation

```bash
cargo build --release -p synapse-hyper-cli
```

The binary will be available at:
```
target/release/synapse-hyper
```

## Usage

### Quick Start

```bash
# Run all tests with TUI monitoring
synapse-hyper all

# Run specific test type
synapse-hyper unit
synapse-hyper integration
synapse-hyper bench
```

### Commands

#### Unit Tests

```bash
# Run all unit tests
synapse-hyper unit

# Run tests matching pattern
synapse-hyper unit index_fabric
```

#### Integration Tests

```bash
# Run all integration tests
synapse-hyper integration

# Run specific integration test
synapse-hyper integration integration_cost_model

# Include performance tests (--ignored)
synapse-hyper integration --include-performance
```

#### Benchmarks

```bash
# Run all benchmarks
synapse-hyper bench

# Run specific benchmark
synapse-hyper bench bench_cost_model

# Custom sample count
synapse-hyper bench --sample-count 200
```

#### All Tests

```bash
# Run unit + integration + performance tests
synapse-hyper all

# Include benchmarks
synapse-hyper all --with-benchmarks
```

#### Monitor Mode

```bash
# Monitor existing test process by PID
synapse-hyper monitor --pid 12345
```

#### Reports

```bash
# View previous test report
synapse-hyper report --file vps_test_report_20260531_120000.md

# Show only summary
synapse-hyper report --summary
```

### Global Options

```bash
# Backend features
synapse-hyper --features rocksdb-backend unit

# Test profile
synapse-hyper --profile performance unit

# Timeout for performance tests
synapse-hyper --timeout 600 all

# Verbose output
synapse-hyper --verbose bench

# Save report to file
synapse-hyper --report my_report.md all
```

## TUI Interface

### Tabs

1. **Overview** - Test statistics and recent logs
2. **Live Output** - Real-time test output
3. **Results** - Detailed test results
4. **Metrics** - Performance metrics and charts

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `q` | Quit (press twice to confirm) |
| `SPACE` | Start test |
| `c` / `C` | Cancel running test |
| `r` | Restart test |
| `Tab` | Next tab |
| `Shift+Tab` | Previous tab |
| `1-4` | Jump to specific tab |
| `↑` / `k` | Scroll up |
| `↓` / `j` | Scroll down |

### Status Indicators

- **RUNNING** - Tests are executing
- **IDLE** - Ready to start tests
- Shows elapsed time and throughput (tests/sec)

## Configuration

### Feature Flags

Choose backend based on environment:

```bash
# Development (macOS)
synapse-hyper --features redb-backend all

# Production (Linux)
synapse-hyper --features rocksdb-backend all
```

### Test Profiles

- `unit` - Unit tests only
- `integration` - Integration tests only
- `performance` - Performance tests only (--ignored)
- `all` - All test types (default)

## Examples

### Development Workflow

```bash
# 1. Quick unit tests during development
synapse-hyper --profile unit

# 2. Full integration tests before commit
synapse-hyper --profile integration

# 3. Performance validation
synapse-hyper --profile performance --timeout 600
```

### Production Testing

```bash
# 1. Run complete test suite
synapse-hyper all --with-benchmarks --report production_report.md

# 2. Monitor in real-time
synapse-hyper monitor

# 3. Review results
synapse-hyper report --file production_report.md --summary
```

### CI/CD Integration

```bash
# Non-interactive mode (for CI)
cargo test --workspace --release --features rocksdb-backend

# With JUnit output
cargo test --workspace --release --features rocksdb-backend -- --format junit > test-results.xml
```

## Output Files

When using `--report`, generates:

- `test_results.log` - Unit test output
- `integration_results.log` - Integration test output
- `perf_test_results.log` - Performance test output
- `benchmark_results.log` - Benchmark results
- `<report_name>.md` - Summary report with metrics

## Troubleshooting

### Build Errors

```bash
# Clean and rebuild
cargo clean
cargo build --release -p synapse-hyper-cli

# Reinstall dependencies
cargo update
```

### Runtime Errors

```bash
# Enable debug logging
RUST_LOG=debug synapse-hyper all

# Check terminal compatibility
echo $TERM
# Should be: xterm-256color or similar
```

### Performance Issues

```bash
# Reduce sample count for benchmarks
synapse-hyper bench --sample-count 50

# Skip performance tests
synapse-hyper all --profile integration
```

## Architecture

```
src/
├── main.rs      # CLI entry point and command parsing
├── app.rs       # Application state management
├── ui.rs        # TUI rendering and widgets
├── runner.rs    # Test execution and output parsing
└── config.rs    # Configuration and profiles
```

## License

Apache-2.0
