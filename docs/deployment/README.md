# Deployment Guides

Questa cartella contiene le guide per il deployment di Synapse Hyperplane in produzione.

## 📖 Guide Disponibili

### 1. VPS Deployment Guide (`VPS_DEPLOYMENT_GUIDE.md`)

**Guida completa** per deployare Synapse Hyperplane su una VPS che esegue un validatore Agave 4.0.0.

**Contenuto**:
- ✅ Prerequisiti hardware e software
- ✅ Architettura di deployment
- ✅ Setup della VPS (kernel tuning, ottimizzazioni)
- ✅ Configurazione Agave Geyser Plugin
- ✅ Installazione Synapse Hyperplane
- ✅ Configurazione Shared Memory Ring Buffer
- ✅ Avvio del Sidecar con systemd
- ✅ Monitoring con Prometheus + Grafana
- ✅ Alerting configuration
- ✅ Troubleshooting dettagliato
- ✅ Performance tuning post-deploy

**Tempo di lettura**: 30 minuti
**Tempo di deployment**: 2-4 ore (prima volta)

**Per chi**: DevOps, SRE, System Administrator

---

### 2. Deploy Checklist (`DEPLOY_CHECKLIST.md`)

**Checklist rapida** per deployment in produzione.

**Contenuto**:
- ✅ Pre-deployment verification
- ✅ 9 step di deployment
- ✅ Post-deployment checks
- ✅ Troubleshooting rapido
- ✅ Contatti emergenza

**Tempo di lettura**: 5 minuti
**Tempo di deployment**: 30 minuti (se si segue checklist)

**Per chi**: DevOps esperti (uso operativo)

---

### 3. Phase Reports

Report delle varie fasi di sviluppo del progetto.

#### PHASE1_COMPLETE.md
- MVP: Account seed + getAccountInfo
- 500k accounts/sec
- 81 errori → 0 errori

#### PHASE1_COMPLETION.md
- Completion report Phase 1
- Test results
- Next steps

#### PHASE1_STATUS.md
- Status update Phase 1
- Issues risolti
- Work in progress

#### PHASE4_COMPLETE.md
- Query Orchestrator completo
- Bitmap intersection engine
- 7/7 tests passing
- Performance targets raggiunti

**Per chi**: Project manager, sviluppatori, stakeholder

---

## 🚀 Percorsi di Deployment

### Deployment Rapido (Testing)

**Tempo**: 1 ora
**Requisiti**: VPS 8 core, 32GB RAM

1. Segui `DEPLOY_CHECKLIST.md`
2. Usa configurazione minimale
3. Skip monitoring (opzionale)

---

### Deployment Produzione

**Tempo**: 4 ore
**Requisiti**: VPS 16-32 core, 96-128GB RAM

1. Leggi `VPS_DEPLOYMENT_GUIDE.md` (30 min)
2. Segui checklist `DEPLOY_CHECKLIST.md` (2 ore)
3. Configura monitoring (1 ora)
4. Esegui benchmark (30 min)
5. Ottimizza configurazione (30 min)

---

### Deployment Enterprise

**Tempo**: 1-2 giorni
**Requisiti**: Cluster Kubernetes, HA

1. Segui `VPS_DEPLOYMENT_GUIDE.md` (sezione Kubernetes)
2. Configura multi-node sharding
3. Setup PostgreSQL backend
4. Configura Redis caching
5. Implementa circuit breaker
6. Test load testing

---

## 📋 Checklist Pre-Deployment

Prima di iniziare il deployment, verifica:

### Hardware

- [ ] CPU: >= 16 core (produzione)
- [ ] RAM: >= 96GB (produzione)
- [ ] Storage: >= 2TB NVMe (separato per Synapse)
- [ ] Network: >= 10Gbps

### Software

- [ ] OS: Ubuntu 22.04 LTS o Debian 12
- [ ] Rust: >= 1.78
- [ ] Agave: 4.0.0+
- [ ] Docker: 24+ (opzionale)

### Permessi

- [ ] Accesso root/sudo alla VPS
- [ ] Accesso directory validatore Agave
- [ ] Permessi shared memory (`/dev/shm`)

### Network

- [ ] Porte aperte: 8899 (RPC), 9090 (metrics)
- [ ] Firewall configurato
- [ ] SSL/TLS certificate (produzione)

---

## 🎯 Post-Deployment

Dopo il deployment, esegui:

### 1. Verifica Operatività

```bash
# Stato sidecar
sudo systemctl status synapse-sidecar

# Log
journalctl -u synapse-sidecar -f

# Test API
curl http://localhost:8899
```

### 2. Benchmark

```bash
cd /opt/synapse
cargo bench --release --bench query_benchmark
```

### 3. Monitoring

- [ ] Prometheus raccoglie metrics
- [ ] Grafana dashboard configurato
- [ ] Alerting attivo

### 4. Ottimizzazione

- [ ] CPU usage durante query
- [ ] Memory usage
- [ ] Query latency
- [ ] Throughput

---

## 🆘 Troubleshooting

### Sidecar non parte

→ Vedi `VPS_DEPLOYMENT_GUIDE.md` sezione Troubleshooting

### Memoria insufficiente

→ Vedi `DEPLOY_CHECKLIST.md` sezione Troubleshooting

### Query lente

→ Vedi `VPS_DEPLOYMENT_GUIDE.md` sezione Troubleshooting

### Geyser plugin crasha

→ Vedi `VPS_DEPLOYMENT_GUIDE.md` sezione Troubleshooting

---

## 📞 Supporto

- **GitHub Issues**: https://github.com/your-org/synapse-hyperplane/issues
- **Discord**: https://discord.gg/synapse-hyperplane
- **Email**: support@synapse-hyperplane.io

---

*Ultimo aggiornamento: 31 Maggio 2026*
*Versione: 1.0.0*
