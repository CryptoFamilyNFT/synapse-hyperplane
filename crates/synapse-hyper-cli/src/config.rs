//! Configurazione test

/// Configurazione per test
#[derive(Clone)]
pub struct TestConfig {
    /// Feature flags (es: "rocksdb-backend")
    pub features: String,
    
    /// Profilo test (unit, integration, performance, all)
    pub profile: String,
    
    /// Timeout in secondi
    pub timeout: u64,
    
    /// Output dettagliato
    pub verbose: bool,
    
    /// File report
    pub report_file: Option<String>,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            features: "rocksdb-backend".to_string(),
            profile: "all".to_string(),
            timeout: 300,
            verbose: false,
            report_file: None,
        }
    }
}
