# Deploy Checklist - Synapse Hyperplane su VPS

Checklist rapida per deployment in produzione su VPS con nodo Agave 4.0.0.

## Pre-Deployment

### 1. Verifica Requisiti Hardware

```bash
# CPU
lscpu | grep "CPU(s):"
# ✅ Minimo: 8 core
# ✅ Consigliato: 16-32 core

# RAM
free -h | grep Mem
# ✅ Minimo: 32GB
# ✅ Consigliato: 96-128GB

# Storage
df -h /mnt/nvme
# ✅ Minimo: 500GB NVMe
# ✅ Consigliato: 2TB NVMe separato

# Network
ethtool eth0 | grep Speed
# ✅ Minimo: 1Gbps
# ✅ Consigliato: 10Gbps
```

### 2. Verifica Software

```bash
# OS version
cat /etc/os-release
# ✅ Ubuntu 22.04 LTS o Debian 12

# Rust version
rustc --version
# ✅ >= 1.78

# Agave version
solana-validator --version
# ✅ 4.0.0+

# Shared memory
df -h /dev/shm
# ✅ >= 2GB disponibili
```

---

## Deployment Steps

### Step 1: Setup Sistema

```bash
# [ ] Aggiorna sistema
sudo apt update && sudo apt upgrade -y

# [ ] Installa dipendenze
sudo apt install -y build-essential pkg-config libssl-dev libclang-dev git curl wget

# [ ] Installa/aggiorna Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustup update

# [ ] Ottimizza kernel
echo "* soft nofile 1048576" | sudo tee -a /etc/security/limits.conf
echo "* hard nofile 1048576" | sudo tee -a /etc/security/limits.conf
sudo sysctl -p

# [ ] Configura shared memory (4GB)
echo "tmpfs /dev/shm tmpfs defaults,size=4G 0 0" | sudo tee -a /etc/fstab
sudo mount -o remount /dev/shm
```

### Step 2: Configura Agave Geyser Plugin

```bash
# [ ] Crea directory config
sudo mkdir -p /opt/synapse/config

# [ ] Crea config Geyser plugin
cat <<EOF | sudo tee /opt/synapse/config/geyser-plugin-config.json
{
  "libpath": "/opt/synapse/plugins/libgeyser_bridge.so",
  "log": { "level": "info" },
  "publish": {
    "accounts": {
      "enabled": true,
      "include_startup": true,
      "include_votes": false
    },
    "transactions": { "enabled": false },
    "entries": { "enabled": false },
    "blocks": { "enabled": false },
    "slots": { "enabled": true }
  },
  "ring_buffer": {
    "path": "/dev/shm/synapse-geyser.ring",
    "size_mb": 1024
  }
}
EOF

# [ ] Aggiungi flag al validatore
# Modifica script di avvio del validatore:
# --geyser-plugin-config /opt/synapse/config/geyser-plugin-config.json

# [ ] Riavvia validatore
sudo systemctl restart solana-validator

# [ ] Verifica plugin caricato
tail -f /var/log/solana/validator.log | grep geyser
# ✅ Deve mostrare: "Geyser plugin loaded"
```

### Step 3: Installazione Synapse

```bash
# [ ] Crea directory
sudo mkdir -p /opt/synapse
sudo chown $USER:$USER /opt/synapse
cd /opt/synapse

# [ ] Clona repository
git clone https://github.com/your-org/synapse-hyperplane.git .

# [ ] Build release
cargo build --release --features redb-backend
# ⏱ Tempo: 15-30 minuti

# [ ] Verifica binari
ls -lh target/release/synapse-sidecar
# ✅ Deve esistere
```

### Step 4: Configura Dati

```bash
# [ ] Crea directory dati
sudo mkdir -p /mnt/nvme/synapse/{delta,indexes,checkpoints,logs}
sudo chown -R $USER:$USER /mnt/nvme/synapse

# [ ] Verifica permessi
ls -ld /mnt/nvme/synapse
# ✅ Deve essere di proprietà dell'utente corrente
```

### Step 5: Configura systemd

```bash
# [ ] Crea service file
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
Environment=RUST_LOG=info
Environment=RUST_BACKTRACE=1
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
  --log-level info \\
  --max-memory-gb 64 \\
  --checkpoint-interval-secs 300
Restart=always
RestartSec=10
LimitNOFILE=1048576
StandardOutput=journal
StandardError=journal
SyslogIdentifier=synapse-sidecar

[Install]
WantedBy=multi-user.target
EOF

# [ ] Ricarica systemd
sudo systemctl daemon-reload

# [ ] Abilita servizio
sudo systemctl enable synapse-sidecar
```

### Step 6: Avvio e Verifica

```bash
# [ ] Avvia sidecar
sudo systemctl start synapse-sidecar

# [ ] Verifica stato
sudo systemctl status synapse-sidecar
# ✅ Deve essere: active (running)

# [ ] Controlla log
journalctl -u synapse-sidecar -f
# ✅ Deve mostrare:
#   - "Ring buffer connected"
#   - "Delta consumer started"
#   - "Index fabric initialized"
#   - "HTTP server listening on 0.0.0.0:8899"

# [ ] Testa API
curl -X POST http://localhost:8899 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getProgramAccounts",
    "params": ["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"]
  }'
# ✅ Deve restituire JSON con account data
```

---

## Post-Deployment

### Step 7: Monitoring

```bash
# [ ] Installa Prometheus + Grafana (opzionale)
# Segui guida in VPS_DEPLOYMENT_GUIDE.md sezione Monitoring

# [ ] Verifica metrics endpoint
curl http://localhost:9090/metrics
# ✅ Deve restituire metriche Prometheus

# [ ] Configura alerting (opzionale)
# Segui guida in VPS_DEPLOYMENT_GUIDE.md sezione Alerting
```

### Step 8: Benchmark

```bash
# [ ] Esegui benchmark
cd /opt/synapse
cargo bench --release --bench query_benchmark

# [ ] Verifica performance
# ✅ Query latency: < 100ms
# ✅ Memory usage: < 80% RAM
# ✅ Throughput: > 1000 QPS
```

### Step 9: Ottimizzazione

```bash
# [ ] Monitora CPU durante query
htop

# [ ] Se CPU < 70%: aumenta query-workers
# Modifica /etc/systemd/system/synapse-sidecar.service
# --query-workers 24

# [ ] Se CPU > 90%: riduci query-workers
# --query-workers 12

# [ ] Ricarica configurazione
sudo systemctl daemon-reload
sudo systemctl restart synapse-sidecar
```

---

## Troubleshooting Rapido

### Sidecar non parte

```bash
# Verifica ring buffer
ls -lh /dev/shm/synapse-geyser.ring
# ❌ Se non esiste: Geyser plugin non caricato

# Verifica log validatore
tail -f /var/log/solana/validator.log | grep geyser

# Riavvia validatore
sudo systemctl restart solana-validator

# Attendi 30s e riprova
sudo systemctl start synapse-sidecar
```

### Memoria insufficiente

```bash
# Riduci workers
sudo nano /etc/systemd/system/synapse-sidecar.service
# --query-workers 8
# --max-memory-gb 32

sudo systemctl daemon-reload
sudo systemctl restart synapse-sidecar
```

### Query lente

```bash
# Verifica indici popolati
curl http://localhost:9090/metrics | grep synapse_index_count

# Attendi completamento snapshot
journalctl -u synapse-sidecar | grep "Index build complete"

# Verifica I/O
iotop -o
```

---

## Contatti Emergenza

- **GitHub Issues**: https://github.com/your-org/synapse-hyperplane/issues
- **Discord**: https://discord.gg/synapse-hyperplane
- **Status Page**: https://status.synapse-hyperplane.io

---

*Versione: 1.0.0*
*Ultimo aggiornamento: 31 Maggio 2026*
