# Deploy Guida: Synapse Hyperplane su VPS con Nodo Agave 4.0.0

Questa guida ti accompagna nel deployment di Synapse Hyperplane su una VPS che esegue un validatore Agave 4.0.0.

## Indice

1. [Prerequisiti](#prerequisiti)
2. [Architettura di Deployment](#architettura-di-deployment)
3. [Setup della VPS](#setup-della-vps)
4. [Configurazione Agave Geyser Plugin](#configurazione-agave-geyser-plugin)
5. [Installazione Synapse Hyperplane](#installazione-synapse-hyperplane)
6. [Configurazione Shared Memory Ring Buffer](#configurazione-shared-memory-ring-buffer)
7. [Avvio del Sidecar](#avvio-del-sidecar)
8. [Monitoring e Alerting](#monitoring-e-alerting)
9. [Troubleshooting](#troubleshooting)

---

## Prerequisiti

### Hardware Richiesto

**Minimo (per testing):**
- CPU: 8 core
- RAM: 32GB
- Storage: 500GB NVMe SSD
- Network: 1Gbps

**Produzione (consigliato):**
- CPU: 16-32 core
- RAM: 96-128GB
- Storage: 2TB NVMe SSD (separato per Synapse)
- Network: 10Gbps

### Software Richiesto

- **OS**: Ubuntu 22.04 LTS o Debian 12
- **Agave Validator**: v4.0.0+
- **Rust**: 1.78+
- **Docker** (opzionale, per containerized deploy)

### Permessi Richiesti

- Accesso root/sudo alla VPS
- Accesso alla directory del validatore Agave
- Permessi per creare shared memory (`/dev/shm`)

---

## Architettura di Deployment

```
┌─────────────────────────────────────────────────────────────────┐
│                        VPS (Validator Node)                     │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │              Agave Validator 4.0.0                       │  │
│  │                                                           │  │
│  │  ┌────────────────────────────────────────────────────┐  │  │
│  │  │  Geyser Plugin                                     │  │  │
│  │  │                                                     │  │  │
│  │  │  update_account() ─────────────────────────────┐   │  │  │
│  │  └────────────────────────────────────────────────┼───┘  │  │
│  │                                                    ↓      │  │
│  │                                            ┌──────────┐  │  │
│  │                                            │ Ring     │  │  │
│  │                                            │ Buffer   │  │  │
│  │                                            │ (1GB)    │  │  │
│  │                                            │ /dev/shm │  │  │
│  │                                            └────┬─────┘  │  │
│  └─────────────────────────────────────────────────┼────────┘  │
│                                                    │           │
│  ┌─────────────────────────────────────────────────┼────────┐  │
│  │           Synapse Hyperplane Sidecar            │        │  │
│  │                                                  ↓        │  │
│  │  ┌──────────────────────────────────────────────────┐   │  │
│  │  │  Delta Consumer Thread                           │   │  │
│  │  │     ↓                                            │   │  │
│  │  │  Index Fabric (Bitmaps)                          │   │  │
│  │  │     ↓                                            │   │  │
│  │  │  Query Orchestrator                              │   │  │
│  │  └──────────────────────────────────────────────────┘   │  │
│  │                                                           │  │
│  │  HTTP API: 8899 (Synapse RPC)                            │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Setup della VPS

### 1. Aggiornamento Sistema

```bash
# Aggiorna pacchetti
sudo apt update && sudo apt upgrade -y

# Installa dipendenze
sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libclang-dev \
    git \
    curl \
    wget \
    htop \
    iotop \
    net-tools
```

### 2. Installazione Rust

```bash
# Installa Rust (se non presente)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Verifica versione (deve essere >= 1.78)
rustc --version

# Installa toolchain stabile
rustup default stable
rustup update
```

### 3. Configurazione Kernel per Shared Memory

```bash
# Verifica dimensione /dev/shm (deve essere >= 2GB)
df -h /dev/shm

# Se necessario, aumenta in /etc/fstab
echo "tmpfs /dev/shm tmpfs defaults,size=4G 0 0" | sudo tee -a /etc/fstab

# Remount
sudo mount -o remount /dev/shm

# Verifica
df -h /dev/shm
# Output: tmpfs 4.0G ...
```

### 4. Ottimizzazioni di Sistema

```bash
# /etc/security/limits.conf - Aumenta file descriptors
echo "* soft nofile 1048576" | sudo tee -a /etc/security/limits.conf
echo "* hard nofile 1048576" | sudo tee -a /etc/security/limits.conf

# /etc/sysctl.conf - Ottimizzazioni network e memory
cat <<EOF | sudo tee -a /etc/sysctl.conf
# Network optimizations
net.core.rmem_max = 134217728
net.core.wmem_max = 134217728
net.ipv4.tcp_rmem = 4096 87380 134217728
net.ipv4.tcp_wmem = 4096 65536 134217728
net.core.netdev_max_backlog = 5000

# Memory optimizations
vm.swappiness = 1
vm.dirty_ratio = 40
vm.dirty_background_ratio = 10
vm.overcommit_memory = 1

# File system
fs.file-max = 2097152
fs.inotify.max_user_watches = 524288
EOF

# Applica
sudo sysctl -p
```

---

## Configurazione Agave Geyser Plugin

### 1. Compilazione Geyser Plugin

Il Geyser plugin di Synapse deve essere compilato e caricato dal validatore Agave.

```bash
# Vai alla directory del validatore
cd /path/to/agave-validator

# Compila con supporto Geyser
cargo build --release --features geysers
```

### 2. Configurazione Plugin

Crea il file di configurazione del Geyser plugin:

```bash
# Crea directory per config
mkdir -p /opt/synapse/config

# Crea config file
cat <<EOF > /opt/synapse/config/geyser-plugin-config.json
{
  "libpath": "/opt/synapse/plugins/libgeyser_bridge.so",
  "log": {
    "level": "info"
  },
  "publish": {
    "accounts": {
      "enabled": true,
      "include_startup": true,
      "include_votes": false
    },
    "transactions": {
      "enabled": false
    },
    "entries": {
      "enabled": false
    },
    "blocks": {
      "enabled": false
    },
    "slots": {
      "enabled": true
    }
  },
  "ring_buffer": {
    "path": "/dev/shm/synapse-geyser.ring",
    "size_mb": 1024
  }
}
EOF
```

### 3. Modifica Avvio Validatore

Aggiungi il flag del Geyser plugin all'avvio del validatore:

```bash
# Esempio di comando di avvio (modifica il tuo script esistente)
solana-validator \
  --identity /path/to/identity.json \
  --log-path /var/log/solana \
  --ledger /mnt/ledger \
  --entrypoint entrypoint.mainnet-beta.solana.com:8001 \
  --expected-genesis-hash 5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d \
  --rpc-port 8899 \
  --private-rpc \
  --enable-rpc-transaction-history \
  --limit-ledger-size \
  --geyser-plugin-config /opt/synapse/config/geyser-plugin-config.json
```

### 4. Verifica Plugin Caricato

```bash
# Controlla i log del validatore
tail -f /var/log/solana/validator.log | grep -i geyser

# Dovresti vedere:
# "Geyser plugin loaded: geyser_bridge"
# "Ring buffer initialized at /dev/shm/synapse-geyser.ring"
```

---

## Installazione Synapse Hyperplane

### 1. Clone Repository

```bash
# Crea directory per Synapse
mkdir -p /opt/synapse
cd /opt/synapse

# Clona repository
git clone https://github.com/your-org/synapse-hyperplane.git .
```

### 2. Build Release

```bash
# Build con feature redb-backend (default)
cargo build --release --features redb-backend

# Tempo stimato: 15-30 minuti (prima build)

# Verifica binari compilati
ls -lh target/release/synapse-*
# Output:
# synapse-sidecar
# synapse-rpc
# synapse-delta-plane
# synapse-base-scanner
```

### 3. Crea Directory per Dati

```bash
# Directory per dati Synapse (separate dal ledger Agave)
mkdir -p /mnt/nvme/synapse/{delta,indexes,checkpoints,logs}

# Imposta permessi
chown -R $USER:$USER /mnt/nvme/synapse
chmod 755 /mnt/nvme/synapse
```

---

## Configurazione Shared Memory Ring Buffer

### 1. Verifica Shared Memory

```bash
# Verifica che il ring buffer esista (creato dal Geyser plugin)
ls -lh /dev/shm/synapse-geyser.ring

# Se non esiste, il Geyser plugin non è stato caricato correttamente
# Controlla i log del validatore
```

### 2. Test Ring Buffer

```bash
# Utility per testare il ring buffer
cd /opt/synapse

# Esegui test
cargo test -p geyser-bridge --release -- --nocapture

# Dovresti vedere:
# "Ring buffer test passed: 1024 MB"
# "Write latency: 2.3 μs"
# "Read latency: 1.8 μs"
```

---

## Avvio del Sidecar

### 1. Script di Avvio

Crea uno script systemd per gestire il sidecar:

```bash
# Crea service file
cat <<EOF | sudo tee /etc/systemd/system/synapse-sidecar.service
[Unit]
Description=Synapse Hyperplane Sidecar
After=network.target solana-validator.service
Wants=solana-validator.service

[Service]
Type=simple
User=solana
Group=solana
WorkingDirectory=/opt/synapse

# Environment
Environment=RUST_LOG=info
Environment=RUST_BACKTRACE=1

# Executable
ExecStart=/opt/synapse/target/release/synapse-sidecar \\
  --ring-path /dev/shm/synapse-geyser.ring \\
  --delta-path /mnt/nvme/synapse/delta \\
  --index-path /mnt/nvme/synapse/indexes \\
  --checkpoint-path /mnt/nvme/synapse/checkpoints \\
  --query-workers 16 \\
  --index-workers 8 \\
  --enable-metrics true \\
  --metrics-port 9090 \\
  --enable-tracing true \\
  --tracing-sampling-rate 0.1 \\
  --log-level info \\
  --max-memory-gb 64 \\
  --checkpoint-interval-secs 300 \\
  --enable-postgres-backend false \\
  --enable-redis-cache false

# Restart policy
Restart=always
RestartSec=10

# File descriptors
LimitNOFILE=1048576

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=synapse-sidecar

[Install]
WantedBy=multi-user.target
EOF
```

### 2. Avvio Sidecar

```bash
# Ricarica systemd
sudo systemctl daemon-reload

# Abilita avvio automatico
sudo systemctl enable synapse-sidecar

# Avvia sidecar
sudo systemctl start synapse-sidecar

# Verifica stato
sudo systemctl status synapse-sidecar

# Dovresti vedere:
# ● synapse-sidecar.service - Synapse Hyperplane Sidecar
#    Loaded: loaded (/etc/systemd/system/synapse-sidecar.service; enabled)
#    Active: active (running)
```

### 3. Verifica Operatività

```bash
# Controlla log
journalctl -u synapse-sidecar -f

# Dovresti vedere:
# "Ring buffer connected"
# "Delta consumer started"
# "Index fabric initialized"
# "Query orchestrator ready"
# "HTTP server listening on 0.0.0.0:8899"

# Testa API
curl -X POST http://localhost:8899 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getProgramAccounts",
    "params": [
      "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
      {
        "filters": [
          {"dataSize": 165}
        ]
      }
    ]
  }'

# Dovresti ricevere una risposta JSON con account data
```

---

## Monitoring e Alerting

### 1. Installazione Prometheus + Grafana

```bash
# Crea directory
mkdir -p /opt/monitoring/{prometheus,grafana}

# Download Prometheus
cd /opt/monitoring/prometheus
wget https://github.com/prometheus/prometheus/releases/download/v2.50.0/prometheus-2.50.0.linux-amd64.tar.gz
tar xvfz prometheus-*.tar.gz
mv prometheus-*/{prometheus,promtool} .
rm -rf prometheus-*

# Configura Prometheus
cat <<EOF > /opt/monitoring/prometheus/prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'synapse-sidecar'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'

  - job_name: 'agave-validator'
    static_configs:
      - targets: ['localhost:8899']
    metrics_path: '/metrics'
EOF
```

### 2. Dashboard Grafana

Importa il dashboard preconfigurato:

```bash
# Dashboard JSON disponibile in /opt/synapse/monitoring/grafana-dashboard.json
# Importa via UI: Grafana → Dashboards → Import → Upload JSON
```

### 3. Metriche Chiave da Monitorare

**Synapse Sidecar:**
- `synapse_query_latency_seconds` - Latenza query (target: < 100ms)
- `synapse_index_update_latency_seconds` - Latenza update indici (target: < 10ms)
- `synapse_memory_usage_bytes` - Memoria utilizzata (alert: > 80%)
- `synapse_ring_buffer_fill_ratio` - Ring buffer fill ratio (alert: > 90%)
- `synapse_query_errors_total` - Errori query (alert: > 0)

**Agave Validator:**
- `validator_slot_height` - Slot corrente
- `validator_transaction_count` - Transaction processate
- `validator_geyser_plugin_status` - Stato plugin (deve essere 1)

### 4. Alerting (Prometheus Alertmanager)

```yaml
# /opt/monitoring/prometheus/alerts.yml
groups:
  - name: synapse
    rules:
      - alert: HighMemoryUsage
        expr: synapse_memory_usage_bytes / 1073741824 > 60
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Synapse memory usage high"
          description: "Memory usage is {{ $value }}GB"

      - alert: QueryLatencyHigh
        expr: histogram_quantile(0.99, rate(synapse_query_latency_seconds_bucket[5m])) > 0.5
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Query latency too high"
          description: "P99 latency is {{ $value }}s"

      - alert: RingBufferFull
        expr: synapse_ring_buffer_fill_ratio > 0.9
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Ring buffer almost full"
          description: "Fill ratio is {{ $value | humanizePercentage }}"
```

---

## Troubleshooting

### Problema: Sidecar non si avvia

**Sintomi:**
```
journalctl -u synapse-sidecar: "Failed to connect to ring buffer"
```

**Soluzione:**
```bash
# 1. Verifica che il Geyser plugin sia caricato
ps aux | grep solana-validator
tail -f /var/log/solana/validator.log | grep geyser

# 2. Verifica che il ring buffer esista
ls -lh /dev/shm/synapse-geyser.ring

# 3. Se non esiste, riavvia il validatore
sudo systemctl restart solana-validator

# 4. Attendi 30 secondi e riprova
sudo systemctl start synapse-sidecar
```

### Problema: Memoria insufficiente

**Sintomi:**
```
journalctl: "Out of memory: Killed process synapse-sidecar"
```

**Soluzione:**
```bash
# 1. Riduci query-workers
# Modifica /etc/systemd/system/synapse-sidecar.service
--query-workers 8  # Da 16 a 8
--max-memory-gb 32 # Da 64 a 32

# 2. Ricarica e riavvia
sudo systemctl daemon-reload
sudo systemctl restart synapse-sidecar

# 3. Monitora memoria
htop
```

### Problema: Query lente

**Sintomi:**
```
curl getProgramAccounts: 5+ secondi
```

**Soluzione:**
```bash
# 1. Verifica che gli indici siano popolati
curl http://localhost:9090/metrics | grep synapse_index_count

# 2. Se gli indici sono vuoti, attendi il completamento dello snapshot iniziale
journalctl -u synapse-sidecar | grep "Index build complete"

# 3. Verifica I/O disk
iotop -o

# 4. Se I/O è saturato, sposta su NVMe più veloce
# Modifica --delta-path e --index-path su disk diverso
```

### Problema: Geyser plugin crasha

**Sintomi:**
```
validator.log: "Geyser plugin error: ring buffer write failed"
```

**Soluzione:**
```bash
# 1. Aumenta dimensione ring buffer
# Modifica /opt/synapse/config/geyser-plugin-config.json
"ring_buffer": {
  "path": "/dev/shm/synapse-geyser.ring",
  "size_mb": 2048  # Da 1024 a 2048
}

# 2. Aumenta /dev/shm
sudo mount -o remount,size=8G /dev/shm

# 3. Riavvia validatore
sudo systemctl restart solana-validator
```

---

## Performance Tuning Post-Deploy

### 1. Benchmark Iniziale

```bash
# Esegui benchmark per validare le performance
cd /opt/synapse

cargo bench --release --bench query_benchmark

# Risultati attesi:
# getProgramAccounts (100k results): < 50ms
# getProgramAccounts + filters: < 100ms
# Account lookup: < 5ms
```

### 2. Ottimizzazione Query Workers

```bash
# Monitora CPU usage durante query
htop

# Se CPU < 70%: aumenta query-workers
# Se CPU > 90%: riduci query-workers

# Modifica /etc/systemd/system/synapse-sidecar.service
--query-workers 24  # Aumenta o riduci

sudo systemctl daemon-reload
sudo systemctl restart synapse-sidecar
```

### 3. Tuning Ring Buffer

```bash
# Monitora fill ratio
curl http://localhost:9090/metrics | grep synapse_ring_buffer_fill_ratio

# Se > 50%: aumenta dimensione buffer
# Modifica geysers-plugin-config.json: size_mb: 2048

# Se < 10%: puoi ridurre (risparmio RAM)
# size_mb: 512
```

---

## Checklist Finale

Prima di considerare il deployment completo:

- [ ] Geyser plugin caricato correttamente
- [ ] Ring buffer creato e accessibile
- [ ] Sidecar in esecuzione (systemctl status)
- [ ] API rispondono (curl test)
- [ ] Prometheus raccoglie metrics
- [ ] Grafana dashboard configurato
- [ ] Alerting configurato
- [ ] Benchmark eseguiti (performance validate)
- [ ] Log rotation configurata
- [ ] Backup script configurato
- [ ] Documentazione operativa creata

---

## Supporto

Per problemi o domande:
- **GitHub Issues**: https://github.com/your-org/synapse-hyperplane/issues
- **Discord**: https://discord.gg/synapse-hyperplane
- **Documentazione**: https://docs.synapse-hyperplane.io

---

*Ultimo aggiornamento: 31 Maggio 2026*
*Versione: 1.0.0*
