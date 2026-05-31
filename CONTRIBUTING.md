# Contribuire a Synapse Hyperplane

Grazie per il tuo interesse in Synapse Hyperplane! Questo documento fornisce linee guida per contribuire al progetto in modo efficace.

## 📋 Indice

- [Codice di Condotta](#codice-di-condotta)
- [Come Contribuire](#come-contribuire)
- [Setup dell'Ambiente](#setup-dellambiente)
- [Standard di Codice](#standard-di-codice)
- [Processo di Pull Request](#processo-di-pull-request)
- [Testing](#testing)
- [Documentazione](#documentazione)
- [Comunicazione](#comunicazione)

---

## 🤝 Codice di Condotta

Sii rispettoso, inclusivo e professionale. Contribuiamo tutti a creare un ambiente accogliente per sviluppatori di tutti i livelli.

---

## 🚀 Come Contribuire

### Tipi di Contributi Accettati

1. **Bug Fixes** - Correzioni di bug critici o minori
2. **Feature Implementation** - Nuove funzionalità allineate alla roadmap
3. **Performance Improvements** - Ottimizzazioni di velocità/memory usage
4. **Documentation** - Migliorie alla documentazione, esempi, tutorial
5. **Testing** - Nuovi test, miglioramenti alla coverage
6. **Code Quality** - Refactoring, cleanup, standardizzazione

### Cosa NON Contribuire

- Feature non allineate all'architettura (vedi [ARCHITECTURE.md](docs/ARCHITECTURE.md))
- Dipendenze non necessarie senza giustificazione
- Breaking changes senza discussione preliminare

---

## 🛠️ Setup dell'Ambiente

### Prerequisites

```bash
# Rust toolchain (versione specificata in rust-toolchain.toml)
rustup install stable
rustup default stable

# Verifica la versione
rustc --version  # Deve corrispondere a rust-toolchain.toml
```

### macOS (Sviluppo)

```bash
# Nessuna dipendenza esterna richiesta per redb backend
git clone https://github.com/your-username/synapse-hyperplane.git
cd synapse-hyperplane
cargo build --workspace
```

### Linux (Produzione)

```bash
# Installa dipendenze per RocksDB
sudo apt-get install librocksdb-dev build-essential pkg-config libssl-dev

# Clona e builda
git clone https://github.com/your-username/synapse-hyperplane.git
cd synapse-hyperplane
cargo build --workspace --features rocksdb-backend --no-default-features
```

### Verifica Setup

```bash
# Esegui i test
cargo test --workspace --features redb-backend

# Build release
cargo build --workspace --release

# Verifica formatting
cargo fmt -- --check

# Verifica linting
cargo clippy --workspace --features redb-backend -- -D warnings
```

---

## 📝 Standard di Codice

### Rust Style Guide

Seguiamo le [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) con queste convenzioni specifiche:

#### 1. Nomenclatura

```rust
// ✅ CORRETTO
pub struct AccountLocation { ... }
pub fn serialize_location(location: &AccountLocation) -> Result<Vec<u8>> { ... }
pub const MAX_ACCOUNT_SIZE: usize = 10 * 1024 * 1024;

// ❌ SBAGLIATO
pub struct accountLocation { ... }  // snake_case per struct
pub fn SerializeLocation(...)      // PascalCase per funzioni
```

#### 2. Error Handling

```rust
// ✅ CORRETTO - Usa thiserror per errori pubblici
#[derive(Debug, thiserror::Error)]
pub enum LocatorError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),
    
    #[error("Serialization failed: {0}")]
    SerializationError(String),
}

// ✅ CORRETTO - Usa ? operator con From traits
pub fn get(&self, pubkey: &Pubkey) -> Result<Option<AccountLocation>> {
    let location = self.db.get()?.get(pubkey.as_ref())?;
    Ok(location)
}

// ❌ SBAGLIATO - unwrap() in produzione
let value = risky_operation().unwrap();  // Panico!
```

#### 3. Documentazione

```rust
/// Serializes an AccountLocation into a byte vector.
///
/// # Arguments
/// * `location` - The AccountLocation to serialize
///
/// # Returns
/// * `Result<Vec<u8>>` - Serialized bytes on success
///
/// # Example
/// ```
/// let location = AccountLocation { ... };
/// let bytes = serialize_location(&location)?;
/// assert_eq!(bytes.len(), 45);
/// ```
pub fn serialize_location(location: &AccountLocation) -> Result<Vec<u8>> { ... }
```

#### 4. Feature Flags

```rust
// ✅ CORRETTO - Feature gates chiare
#[cfg(feature = "redb-backend")]
mod redb_impl;

#[cfg(feature = "rocksdb-backend")]
mod rocksdb_impl;

// ❌ SBAGLIATO - Feature gates annidate o confuse
#[cfg(all(not(test), feature = "rocksdb-backend"))]
```

#### 5. Memory Management

```rust
// ✅ CORRETTO - Zero-copy dove possibile
pub fn parse_account_record(data: &[u8]) -> Result<AccountView> {
    // Usa slice, non copie
    let pubkey = Pubkey::try_from(&data[0..32])?;
    ...
}

// ✅ CORRETTO - Capacity pre-allocata per performance
let mut bytes = Vec::with_capacity(45);  // Dimensione esatta
```

### Commit Message Convention

Usiamo [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

#### Types

- `feat`: Nuova funzionalità
- `fix`: Bug fix
- `docs`: Documentazione
- `style`: Formatting, missing semicolons, etc.
- `refactor`: Refactoring senza cambiamenti funzionali
- `perf`: Performance improvements
- `test`: Aggiunta/modifica test
- `chore`: Build, tooling, CI/CD

#### Esempi

```bash
# ✅ CORRETTO
feat(base-locator): add RocksDB batch insert support
fix(rpc-read-provider): json_response type mismatch
docs: update CONTRIBUTING.md with commit conventions
refactor(cache-plane): simplify access tracker to single lock

# ❌ SBAGLIATO
fixed stuff
added new feature
updated docs
```

---

## 🔀 Processo di Pull Request

### 1. Fork e Branch

```bash
# Forka il repository su GitHub
# Clona il tuo fork
git clone https://github.com/your-username/synapse-hyperplane.git
cd synapse-hyperplane

# Crea un branch per la tua feature
git checkout -b feat/my-new-feature
```

### 2. Sviluppo

```bash
# Fai commit frequenti e atomici
git add crates/base-locator/src/rocksdb_impl.rs
git commit -m "feat(base-locator): implement RocksDB batch insert"

# Esegui test e lint prima di pushare
cargo test --workspace --features redb-backend
cargo clippy --workspace --features redb-backend -- -D warnings
cargo fmt -- --check
```

### 3. Pre-Submission Checklist

Prima di aprire una PR, verifica:

- [ ] Il codice compila senza warnings (`cargo build --workspace`)
- [ ] Tutti i test passano (`cargo test --workspace`)
- [ ] Il codice è formattato (`cargo fmt`)
- [ ] Non ci sono lint warnings (`cargo clippy`)
- [ ] La documentazione è aggiornata
- [ ] I commit message seguono le convenzioni
- [ ] Hai aggiunto test per le nuove feature

### 4. Apri la PR

1. Vai su GitHub e apri una Pull Request
2. Usa il template fornito
3. Descrivi chiaramente cosa fa la PR
4. Linka eventuali issue correlate

### 5. Review Process

- Un maintainer revisionerà il codice
- Potrebbero essere richiesti cambiamenti
- Dopo approvazione, la PR verrà mergiata

---

## 🧪 Testing

### Tipi di Test

#### 1. Unit Test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_location() {
        let location = AccountLocation {
            file_id: 1,
            offset: 1024,
            stored_size: 100,
            data_offset: 50,
            data_len: 100,
            slot: 100,
            write_version: 1,
            storage_type: StorageType::Base,
        };

        let bytes = serialize_location(&location).unwrap();
        let restored = deserialize_location(&bytes).unwrap();

        assert_eq!(location, restored);
        assert_eq!(bytes.len(), 45);
    }
}
```

#### 2. Integration Test

```rust
// tests/integration_test.rs
#[test]
fn test_base_scanner_end_to_end() {
    // Setup test accounts
    let accounts_path = generate_test_accounts(500);
    
    // Run scanner
    let scanner = BaseScanner::new(&accounts_path);
    let results = scanner.scan().unwrap();
    
    // Verify
    assert_eq!(results.len(), 500);
    assert!(results.iter().all(|r| r.is_ok()));
}
```

#### 3. Performance Test

```rust
#[test]
#[ignore]  // Solo per benchmark manuali
fn test_scan_performance() {
    let accounts = generate_test_accounts(100_000);
    let start = std::time::Instant::now();
    
    scan_accounts(&accounts).unwrap();
    
    let elapsed = start.elapsed();
    println!("Scanned 100k accounts in {:?}", elapsed);
    assert!(elapsed < std::time::Duration::from_secs(1));
}
```

### Esecuzione Test

```bash
# Tutti i test
cargo test --workspace --features redb-backend

# Test specifici
cargo test -p base-locator --features redb-backend

# Test con output
cargo test --workspace --features redb-backend -- --nocapture

# Performance test (ignored by default)
cargo test --workspace --features redb-backend -- --ignored
```

---

## 📚 Documentazione

### Dove Scrivere Documentazione

1. **Docstrings** - Nel codice, per funzioni pubbliche
2. **README.md** - Panoramica del progetto
3. **docs/** - Documentazione dettagliata
   - `ARCHITECTURE.md` - Architettura
   - `BUILD_CONFIG.md` - Guida al build
   - `PHASE1_COMPLETE.md` - Status report
4. **Esempi** - In `examples/` o nei docstrings

### Standard di Documentazione

```rust
/// Breve descrizione (prima riga)
///
/// Descrizione dettagliata se necessaria.
///
/// # Arguments
/// * `param` - Descrizione del parametro
///
/// # Returns
/// Descrizione del return value
///
/// # Errors
/// Errori possibili
///
/// # Example
/// ```
/// codice di esempio
/// ```
pub fn mia_funzione(param: u32) -> Result<String> { ... }
```

---

## 💬 Comunicazione

### Canali

- **GitHub Issues** - Bug report, feature request
- **GitHub Discussions** - Domande, idee, discussioni
- **PR Comments** - Discussioni specifiche sul codice

### Quando Aprire una Issue

- ✅ Bug riproducibile
- ✅ Feature request allineata alla roadmap
- ✅ Documentazione mancante o errata
- ❌ Domande su come usare il progetto (usa Discussions)

### Template Issue

```markdown
**Descrizione**
Breve descrizione del problema o della feature

**To Reproduce** (per bug)
Step per riprodurre il bug

**Expected Behavior**
Cosa ti aspettavi accadesse

**Screenshots** (se applicabile)

**Environment:**
- OS: macOS / Linux
- Rust version: 1.x.x
- Backend: redb / RocksDB
```

---

## 🎯 Roadmap e Priorità

### Phase 2 (In Progress)

1. **Geyser Bridge** - Live updates da Geyser plugin
2. **Delta Plane** - Segment store per delta layer
3. **Index Fabric** - Bitmap indexes per query
4. **Production Readiness** - RocksDB, monitoring, load testing

### Come Scegliere su Cosa Lavorare

1. Controlla le [GitHub Issues](https://github.com/your-username/synapse-hyperplane/issues)
2. Cerca issue etichettate con `good first issue` o `help wanted`
3. Discuti nelle Discussions se hai idee nuove
4. Allinea con la roadmap del progetto

---

## 📜 Licenza

Contribuendo accetti che il tuo codice sia distribuito sotto la licenza del progetto (vedi [LICENSE](LICENSE)).

---

## 🙏 Grazie!

Ogni contributo, grande o piccolo, aiuta a rendere Synapse Hyperplane migliore. Grazie per il tuo tempo e il tuo impegno!
