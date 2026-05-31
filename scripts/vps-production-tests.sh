#!/bin/bash
# VPS Production Test & Benchmark Script
# Esegue tutti i test e benchmark su VPS in produzione

set -e

echo "=========================================="
echo "Synapse Hyperplane - VPS Production Tests"
echo "=========================================="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if running in release mode
if [ ! -f "target/release/synapse-hyperplane" ]; then
    echo -e "${YELLOW}Building release binary...${NC}"
    cargo build --release --features rocksdb-backend --no-default-features
fi

# 1. Unit Tests
echo ""
echo "=========================================="
echo "1. Running Unit Tests"
echo "=========================================="
echo ""

cargo test --workspace --release --features rocksdb-backend --no-default-features 2>&1 | tee test_results.log

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo -e "${GREEN}✅ Unit Tests PASSED${NC}"
else
    echo -e "${RED}❌ Unit Tests FAILED${NC}"
    exit 1
fi

# 2. Integration Tests
echo ""
echo "=========================================="
echo "2. Running Integration Tests"
echo "=========================================="
echo ""

cargo test --test integration_* --release 2>&1 | tee integration_results.log

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo -e "${GREEN}✅ Integration Tests PASSED${NC}"
else
    echo -e "${RED}❌ Integration Tests FAILED${NC}"
    exit 1
fi

# 3. Performance Tests (ignored by default)
echo ""
echo "=========================================="
echo "3. Running Performance Tests (--ignored)"
echo "=========================================="
echo ""

cargo test --test integration_* --release -- --ignored --nocapture 2>&1 | tee perf_test_results.log

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo -e "${GREEN}✅ Performance Tests PASSED${NC}"
else
    echo -e "${YELLOW}⚠️  Performance Tests had issues (check logs)${NC}"
fi

# 4. Benchmarks
echo ""
echo "=========================================="
echo "4. Running Benchmarks"
echo "=========================================="
echo ""

cargo bench --bench bench_* 2>&1 | tee benchmark_results.log

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo -e "${GREEN}✅ Benchmarks COMPLETED${NC}"
else
    echo -e "${YELLOW}⚠️  Benchmarks had issues (check logs)${NC}"
fi

# 5. Extract Key Metrics
echo ""
echo "=========================================="
echo "5. Performance Summary"
echo "=========================================="
echo ""

echo "Query Planning Performance:"
grep -E "query_plan.*time:" benchmark_results.log || echo "  No data available"

echo ""
echo "LSM Bitmap Throughput:"
grep -E "lsm_bitmap.*throughput:" benchmark_results.log || echo "  No data available"

echo ""
echo "Cache Hit Rate:"
grep -E "cache_hit_rate:" integration_results.log || echo "  No data available"

echo ""
echo "Shared Memory Throughput:"
grep -E "Shared Memory.*MB/s" perf_test_results.log || echo "  No data available"

# 6. Generate Report
echo ""
echo "=========================================="
echo "6. Generating Report"
echo "=========================================="
echo ""

REPORT_DATE=$(date +%Y%m%d_%H%M%S)
REPORT_FILE="vps_test_report_${REPORT_DATE}.md"

cat > $REPORT_FILE << EOF
# VPS Production Test Report

**Date:** $(date)
**Host:** $(hostname)
**Kernel:** $(uname -r)
**CPU:** $(lscpu | grep "Model name" | cut -d: -f2 | xargs)
**RAM:** $(free -h | grep Mem | awk '{print $2}')

## Test Results

### Unit Tests
\`\`\`
$(tail -20 test_results.log)
\`\`\`

### Integration Tests
\`\`\`
$(tail -20 integration_results.log)
\`\`\`

### Performance Tests
\`\`\`
$(tail -30 perf_test_results.log)
\`\`\`

### Benchmarks
\`\`\`
$(tail -50 benchmark_results.log)
\`\`\`

## Key Metrics

$(grep -E "Query|LSM|Cache|Shared" benchmark_results.log integration_results.log perf_test_results.log 2>/dev/null || echo "No metrics extracted")

## System Info

\`\`\`
CPU Cores: $(nproc)
Memory: $(free -h | grep Mem)
Disk: $(df -h / | tail -1)
\`\`\`

## Recommendations

$(
if grep -q "FAILED" test_results.log; then
    echo "- ❌ Fix failing unit tests"
fi
if grep -q "FAILED" integration_results.log; then
    echo "- ❌ Fix failing integration tests"
fi
if grep -q "cache_hit_rate.*[0-5][0-9]%" integration_results.log; then
    echo "- ⚠️  Cache hit rate below 60%, consider increasing cache size"
fi
if ! grep -q "FAILED" test_results.log && ! grep -q "FAILED" integration_results.log; then
    echo "- ✅ All tests passing, ready for production"
fi
)

---
Report generated: $(date)
EOF

echo -e "${GREEN}Report saved to: ${REPORT_FILE}${NC}"
echo ""

# 7. Display Report Location
echo "=========================================="
echo "Test Complete!"
echo "=========================================="
echo ""
echo "Results saved:"
echo "  - Unit tests:        test_results.log"
echo "  - Integration tests: integration_results.log"
echo "  - Performance tests: perf_test_results.log"
echo "  - Benchmarks:        benchmark_results.log"
echo "  - Full report:       ${REPORT_FILE}"
echo ""
echo "Next steps:"
echo "  1. Review ${REPORT_FILE}"
echo "  2. Check for any FAILED tests"
echo "  3. Compare benchmarks with baseline"
echo "  4. Deploy to production if all tests pass"
echo ""
echo -e "${GREEN}Done!${NC}"
