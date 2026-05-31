# VPS Deployment Guide for Synapse Hyperplane

Complete guide for deploying Synapse Hyperplane to production VPS with Phase 4 optimizations.

## Prerequisites

### VPS Requirements

**Minimum Specs:**
- CPU: 8 cores (16 recommended)
- RAM: 32GB (64GB recommended)
- Storage: 500GB NVMe SSD
- Network: 1Gbps+
- OS: Ubuntu 22.04 LTS or Debian 12

**Recommended Providers:**
- Hetzner (best price/performance)
- OVH (good DDoS protection)
- AWS EC2 (c6i.4xlarge or larger)
- GCP (c2-standard-16 or larger)

### System Dependencies

```bash
# Update system
sudo apt-get update && sudo apt-get upgrade -y

# Install build dependencies
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    librocksdb-dev \
    libudev-dev \
    clang \
    llvm \
    curl \
    git

# Install Rust (stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env

# Verify Rust version
rustc --version  # Should be 1.75+
cargo --version
```

## Deployment Steps

### 1. Clone Repository

```bash
cd /opt
sudo git clone https://github.com/your-repo/synapse-hyperplane.git
sudo chown -R $USER:$USER synapse-hyperplane
cd synapse-hyperplane
```

### 2. Configure Build

```bash
# Create production config
cat > configs/production.toml << EOF
[geyser]
ring_buffer_path = "/dev/shm/synapse-geyser.ring"
ring_buffer_size_mb = 2048

[delta_plane]
delta_path = "/var/lib/synapse/delta"
segment_size_mb = 1024
compaction_threshold = 100

[index_fabric]
index_path = "/var/lib/synapse/indexes"
enable_lsm_bitmap = true
enable_hot_cold = true
hot_threshold_slots = 100
warm_threshold_slots = 10000

[query_orchestrator]
enable_cost_model = true
enable_cache = true
cache_ttl_secs = 3600
cache_l1_size = 10000
cache_l2_size = 100000

[shared_memory]
enabled = true
path = "/dev/shm/synapse-results"
capacity_mb = 512

[numa]
enabled = true  # Only effective on multi-socket systems
policy = "local"
EOF
```

### 3. Build for Production

```bash
# Release build with RocksDB backend
cargo build --release --features rocksdb-backend --no-default-features

# Verify build
ls -lh target/release/synapse-*
```

**Build time:** ~15-30 minutes on 8-core VPS

### 4. Create Systemd Services

```bash
# Main runtime service
sudo tee /etc/systemd/system/synapse-runtime.service > /dev/null << EOF
[Unit]
Description=Synapse Hyperplane Runtime
After=network.target

[Service]
Type=exec
User=synapse
Group=synapse
WorkingDirectory=/opt/synapse-hyperplane
ExecStart=/opt/synapse-hyperplane/target/release/synapse-hyperplane \
    --config /opt/synapse-hyperplane/configs/production.toml \
    --rpc-bind 0.0.0.0:8898
Restart=always
RestartSec=10
LimitNOFILE=1000000
LimitMEMLOCK=infinity

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=synapse

# Security
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/synapse /dev/shm

[Install]
WantedBy=multi-user.target
EOF

# Base scanner service
sudo tee /etc/systemd/system/synapse-scanner.service > /dev/null << EOF
[Unit]
Description=Synapse Base Scanner
After=network.target

[Service]
Type=exec
User=synapse
Group=synapse
WorkingDirectory=/opt/synapse-hyperplane
ExecStart=/opt/synapse-hyperplane/target/release/synapse-base-scanner \
    --input /var/lib/solana/ledger/accounts \
    --output /var/lib/synapse/base
Restart=on-failure
RestartSec=30

[Install]
WantedBy=multi-user.target
EOF
```

### 5. Create User and Directories

```bash
# Create synapse user
sudo useradd -r -s /bin/false synapse

# Create directories
sudo mkdir -p /var/lib/synapse/{delta,indexes,base}
sudo mkdir -p /dev/shm/synapse-results
sudo chown -R synapse:synapse /var/lib/synapse /dev/shm/synapse-results

# Set permissions
sudo chmod 755 /var/lib/synapse
sudo chmod 1777 /dev/shm/synapse-results  # Sticky bit for shared memory
```

### 6. Enable and Start Services

```bash
# Reload systemd
sudo systemctl daemon-reload

# Enable services
sudo systemctl enable synapse-runtime synapse-scanner

# Start services
sudo systemctl start synapse-scanner
sudo systemctl start synapse-runtime

# Check status
sudo systemctl status synapse-runtime
sudo systemctl status synapse-scanner
```

### 7. Configure Firewall

```bash
# Allow RPC port
sudo ufw allow 8898/tcp comment "Synapse RPC"

# Allow metrics port (optional)
sudo ufw allow 9090/tcp comment "Synapse Metrics"

# Enable firewall
sudo ufw enable
```

## Monitoring Setup

### 1. Install Monitoring Tools

```bash
# Install Prometheus node exporter
sudo apt-get install -y prometheus-node-exporter

# Install Grafana (optional)
sudo apt-get install -y grafana
sudo systemctl enable grafana-server
sudo systemctl start grafana-server
```

### 2. Configure Metrics Collection

```bash
# Create Prometheus config
sudo tee /etc/prometheus/synapse.yml > /dev/null << EOF
scrape_configs:
  - job_name: 'synapse'
    static_configs:
      - targets: ['localhost:9090']
  
  - job_name: 'node'
    static_configs:
      - targets: ['localhost:9100']
EOF

# Restart Prometheus
sudo systemctl restart prometheus
```

### 3. Set Up Log Rotation

```bash
sudo tee /etc/logrotate.d/synapse > /dev/null << EOF
/var/log/synapse/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 0640 synapse synapse
    postrotate
        systemctl reload synapse-runtime
    endscript
}
EOF
```

## Performance Tuning

### 1. Kernel Parameters

```bash
# Optimize for high-throughput
sudo tee /etc/sysctl.d/99-synapse.conf > /dev/null << EOF
# Increase file descriptor limit
fs.file-max = 2097152

# Increase memory map areas
vm.max_map_count = 262144

# Optimize network
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
net.ipv4.tcp_rmem = 4096 87380 16777216
net.ipv4.tcp_wmem = 4096 65536 16777216

# Reduce swap tendency
vm.swappiness = 1
EOF

sudo sysctl --system
```

### 2. CPU Governor

```bash
# Install cpufrequtils
sudo apt-get install -y cpufrequtils

# Set to performance mode
sudo tee /etc/default/cpufrequtils > /dev/null << EOF
GOVERNOR="performance"
EOF

sudo systemctl restart cpufrequtils
```

### 3. I/O Scheduler

```bash
# Set deadline scheduler for NVMe
echo "deadline" | sudo tee /sys/block/nvme0n1/queue/scheduler
```

## Verification

### 1. Run Integration Tests

```bash
cd /opt/synapse-hyperplane
cargo test --test integration_* --release -- --ignored
```

**Expected Results:**
- All tests pass ✅
- Query latency <10ms (P99)
- Throughput >100K queries/sec
- Memory usage <16GB

### 2. Check Metrics

```bash
# View runtime metrics
curl localhost:9090/metrics | grep -E "synapse_|process_"

# Check key metrics:
# - synapse_queries_total
# - synapse_query_latency_seconds
# - synapse_cache_hit_rate
# - process_resident_memory_bytes
```

### 3. Monitor Logs

```bash
# Watch for errors
sudo journalctl -u synapse-runtime -f --grep="ERROR"

# Watch for tier migrations
sudo journalctl -u synapse-runtime -f --grep="tier"

# Watch compaction events
sudo journalctl -u synapse-runtime -f --grep="compaction"
```

## Maintenance

### 1. Backup Strategy

```bash
# Create backup script
sudo tee /opt/synapse-hyperplane/scripts/backup.sh > /dev/null << 'EOF'
#!/bin/bash
BACKUP_DIR="/var/backups/synapse"
DATE=$(date +%Y%m%d_%H%M%S)

mkdir -p $BACKUP_DIR

# Backup indexes
tar -czf $BACKUP_DIR/indexes_$DATE.tar.gz /var/lib/synapse/indexes

# Backup config
cp /opt/synapse-hyperplane/configs/production.toml $BACKUP_DIR/config_$DATE.toml

# Keep only last 7 days
find $BACKUP_DIR -name "*.tar.gz" -mtime +7 -delete

echo "Backup completed: $DATE"
EOF

chmod +x /opt/synapse-hyperplane/scripts/backup.sh

# Schedule daily backup
(crontab -l 2>/dev/null; echo "0 2 * * * /opt/synapse-hyperplane/scripts/backup.sh") | crontab -
```

### 2. Update Procedure

```bash
# Stop services
sudo systemctl stop synapse-runtime synapse-scanner

# Pull latest code
cd /opt/synapse-hyperplane
git pull

# Rebuild
cargo build --release --features rocksdb-backend --no-default-features

# Restart
sudo systemctl start synapse-scanner
sudo systemctl start synapse-runtime

# Verify
sudo systemctl status synapse-runtime
```

### 3. Troubleshooting

**High Memory Usage:**
```bash
# Check cache sizes
curl localhost:9090/metrics | grep cache

# Reduce cache if needed
# Edit configs/production.toml: cache_l1_size, cache_l2_size
```

**Slow Queries:**
```bash
# Check cache hit rate
curl localhost:9090/metrics | grep hit_rate

# Should be >70%, if lower:
# - Increase cache sizes
# - Check query patterns
```

**Compaction Issues:**
```bash
# Check compaction status
sudo journalctl -u synapse-runtime | grep compaction

# If compaction is too frequent:
# Increase max_deltas or max_delta_size in code
```

## Performance Benchmarks

### Expected Performance (on recommended hardware)

| Metric | Target | Measurement |
|--------|--------|-------------|
| Query Latency (P50) | <5ms | `synapse_query_latency_seconds` |
| Query Latency (P99) | <10ms | `synapse_query_latency_seconds` |
| Throughput | >100K qps | `synapse_queries_total` |
| Cache Hit Rate | >80% | `synapse_cache_hit_rate` |
| Memory Usage | <16GB | `process_resident_memory_bytes` |
| CPU Usage | <50% | `process_cpu_seconds_total` |

### Benchmarking Command

```bash
# Run benchmarks
cd /opt/synapse-hyperplane
cargo bench --bench bench_* 2>&1 | tee benchmarks.log

# Compare with baseline
cat benchmarks.log | grep -E "time:|throughput:"
```

## Support

For issues or questions:
- GitHub Issues: https://github.com/your-repo/synapse-hyperplane/issues
- Documentation: https://github.com/your-repo/synapse-hyperplane/docs
- Discord: [your-discord-invite]
