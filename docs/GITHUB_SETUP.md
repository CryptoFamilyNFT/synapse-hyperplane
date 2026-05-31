# GitHub Issues Templates e CI/CD Setup

## 📋 Templates Creati

### 1. **Bug Report** (`.github/ISSUE_TEMPLATE/bug_report.md`)
Template strutturato per report bug:
- Descrizione e steps per riprodurre
- Expected behavior
- Environment (OS, Rust version, backend)
- Error logs

### 2. **Feature Request** (`.github/ISSUE_TEMPLATE/feature_request.md`)
Template per nuove feature:
- Problem statement
- Proposed solution
- Alternative solutions
- Related phase (dropdown)
- Implementation details

### 3. **Phase 2 Task** (`.github/ISSUE_TEMPLATE/phase2_task.md`)
Template specifico per Phase 2:
- Categoria (Geyser Bridge, Delta Store, etc.)
- Acceptance criteria
- Priority
- Technical notes

---

## 🚀 CI/CD Workflow

### File: `.github/workflows/ci.yml`

**Jobs Implementati:**

#### 1. **build-macos** (redb backend)
- ✅ Build workspace con `redb-backend`
- ✅ Test suite
- ✅ Check formatting (`cargo fmt`)
- ✅ Clippy lints (`-D warnings`)

#### 2. **build-linux** (RocksDB backend)
- ✅ Installa dipendenze RocksDB
- ✅ Build con `rocksdb-backend --no-default-features`
- ✅ Test suite
- ✅ Formatting + Clippy

#### 3. **benchmarks** (opzionale, manual trigger)
- ⚠️ Solo su `workflow_dispatch`
- ⚠️ Performance tests con `--ignored` flag

**Trigger:**
- Push su `main` / `develop`
- Pull request su `main` / `develop`
- Manual trigger per benchmarks

---

## 📁 Struttura .github

```
.github/
├── workflows/
│   └── ci.yml              # CI/CD pipeline
└── ISSUE_TEMPLATE/
    ├── bug_report.md       # Template bug report
    ├── feature_request.md  # Template feature request
    └── phase2_task.md      # Template Phase 2 tasks
```

---

## ✅ Prossimi Step

### 1. GitHub Setup
- [ ] Abilitare GitHub Actions nel repository
- [ ] Configurare branch protection per `main`
- [ ] Richiedere CI passing per merge

### 2. Badge README
Aggiungere al README.md:
```markdown
[![CI](https://github.com/your-username/synapse-hyperplane/actions/workflows/ci.yml/badge.svg)](https://github.com/your-username/synapse-hyperplane/actions/workflows/ci.yml)
```

### 3. Monitoring
- [ ] Setup codecov per code coverage
- [ ] Integrare Dependabot per dependency updates
- [ ] Configurare release automation

---

## 🎯 Status Implementation

| Component | Status | Note |
|-----------|--------|------|
| CI Workflow | ✅ Complete | macOS + Linux, redb + RocksDB |
| Bug Template | ✅ Complete | Environment-aware |
| Feature Template | ✅ Complete | Phase-linked |
| Phase 2 Template | ✅ Complete | Task tracking |
| Benchmarks | ⚠️ Optional | Manual trigger only |

---

**Tutto pronto per tracciare Phase 2 issues! 🚀**
