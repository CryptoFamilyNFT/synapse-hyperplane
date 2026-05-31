//! Test Runner - Executes tests and benchmarks
//!
//! Handles test execution, output parsing, and event streaming.

use crate::app::BenchmarkResult;
use crate::config::TestConfig;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;
use tokio::sync::mpsc;

/// Test execution runner
pub struct TestRunner {
    config: TestConfig,
    tx: mpsc::Sender<TestEvent>,
}

/// Events from runner
#[derive(Debug, Clone)]
pub enum TestEvent {
    TestStarted(String),
    TestPassed(String),
    TestFailed(String, String),
    TestIgnored(String),
    BenchmarkResult(BenchmarkResult),
    Log(String),
    Finished { total: u32, passed: u32, failed: u32 },
}

impl TestRunner {
    pub fn new(config: TestConfig, tx: mpsc::Sender<TestEvent>) -> Self {
        Self { config, tx }
    }

    /// Run unit tests
    pub async fn run_unit_tests(&self, pattern: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.tx.send(TestEvent::Log("Starting unit tests...".to_string())).await?;

        let mut args = vec![
            "test".to_string(),
            "--release".to_string(),
            "--workspace".to_string(),
            format!("--features={}", self.config.features),
            "--no-default-features".to_string(),
            "--color=always".to_string(),
        ];

        if !pattern.is_empty() {
            args.push(pattern.to_string());
        }

        let mut cmd = Command::new("cargo")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Read output
        if let Some(stdout) = cmd.stdout.take() {
            let reader = BufReader::new(stdout);
            let tx_clone = self.tx.clone();
            
            // Leggi stdout in thread separato
            thread::spawn(move || {
                for line in reader.lines() {
                    if let Ok(line) = line {
                        let _ = tx_clone.blocking_send(TestEvent::Log(line));
                    }
                }
            });
        }

        let status = cmd.wait()?;
        
        self.tx.send(TestEvent::Finished {
            total: 0, // TODO: track properly
            passed: 0,
            failed: 0,
        }).await?;

        if status.success() {
            self.tx.send(TestEvent::Log("✓ Unit tests completed".to_string())).await?;
        } else {
            self.tx.send(TestEvent::Log("✗ Unit tests failed".to_string())).await?;
        }

        Ok(())
    }

    /// Run integration tests
    pub async fn run_integration(&self, test_name: &str, include_performance: bool) -> Result<(), Box<dyn std::error::Error>> {
        self.tx.send(TestEvent::Log("Starting integration tests...".to_string())).await?;

        let mut args = vec![
            "test".to_string(),
            "--release".to_string(),
            format!("--features={}", self.config.features),
            "--no-default-features".to_string(),
            "--color=always".to_string(),
        ];

        if !test_name.is_empty() {
            args.push("--test".to_string());
            args.push(test_name.to_string());
        } else {
            args.push("--test".to_string());
            args.push("integration_*".to_string());
        }

        if include_performance {
            args.push("--".to_string());
            args.push("--ignored".to_string());
            args.push("--nocapture".to_string());
        }

        let mut cmd = Command::new("cargo")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Leggi stdout in thread separato
        if let Some(stdout) = cmd.stdout.take() {
            let reader = BufReader::new(stdout);
            let tx_clone = self.tx.clone();
            
            thread::spawn(move || {
                for line in reader.lines() {
                    if let Ok(line) = line {
                        let _ = tx_clone.blocking_send(TestEvent::Log(line));
                    }
                }
            });
        }

        let status = cmd.wait()?;
        
        if status.success() {
            self.tx.send(TestEvent::Log("✓ Integration tests completed".to_string())).await?;
        } else {
            self.tx.send(TestEvent::Log("✗ Integration tests failed".to_string())).await?;
        }

        Ok(())
    }

    /// Run benchmarks
    pub async fn run_benchmarks(&self, bench_name: &str, sample_count: u32) -> Result<(), Box<dyn std::error::Error>> {
        self.tx.send(TestEvent::Log("Starting benchmarks...".to_string())).await?;

        let mut args = vec![
            "bench".to_string(),
            "--release".to_string(),
            format!("--features={}", self.config.features),
            "--no-default-features".to_string(),
        ];

        if !bench_name.is_empty() {
            args.push("--bench".to_string());
            args.push(bench_name.to_string());
        }

        args.push("--".to_string());
        args.push(format!("--sample-size={}", sample_count));

        let mut cmd = Command::new("cargo")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Leggi stdout in thread separato
        if let Some(stdout) = cmd.stdout.take() {
            let reader = BufReader::new(stdout);
            let tx_clone = self.tx.clone();
            
            thread::spawn(move || {
                for line in reader.lines() {
                    if let Ok(line) = line {
                        let _ = tx_clone.blocking_send(TestEvent::Log(line));
                    }
                }
            });
        }

        let status = cmd.wait()?;
        
        if status.success() {
            self.tx.send(TestEvent::Log("✓ Benchmarks completed".to_string())).await?;
        } else {
            self.tx.send(TestEvent::Log("✗ Benchmarks failed".to_string())).await?;
        }

        Ok(())
    }

    /// Parse test output line
    async fn parse_test_line(&self, line: &str) -> Result<(), Box<dyn std::error::Error>> {
        if line.contains("test ") {
            if line.contains(" ... ok") {
                let test_name = line.split("test ").nth(1)
                    .and_then(|s| s.split(" ... ok").next())
                    .unwrap_or("unknown");
                self.tx.send(TestEvent::TestPassed(test_name.to_string())).await?;
            } else if line.contains(" ... FAILED") {
                let test_name = line.split("test ").nth(1)
                    .and_then(|s| s.split(" ... FAILED").next())
                    .unwrap_or("unknown");
                self.tx.send(TestEvent::TestFailed(test_name.to_string(), line.to_string())).await?;
            } else if line.contains(" ... ignored") {
                let test_name = line.split("test ").nth(1)
                    .and_then(|s| s.split(" ... ignored").next())
                    .unwrap_or("unknown");
                self.tx.send(TestEvent::TestIgnored(test_name.to_string())).await?;
            }
        }
        
        // General log
        self.tx.send(TestEvent::Log(line.to_string())).await?;
        Ok(())
    }

    /// Parse linea output benchmark
    async fn parse_bench_line(&self, line: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Esempio: "bench_lsm_insert        1.234 ms (100 samples)"
        if line.contains("ms") && line.contains("samples") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let name = parts[0].to_string();
                let time_ms: f64 = parts[1].parse().unwrap_or(0.0);
                let samples: u32 = parts[3].trim_start_matches('(').trim_end_matches(')').parse().unwrap_or(0);
                
                let result = BenchmarkResult {
                    name,
                    time_ns: (time_ms * 1_000_000.0) as u64,
                    throughput: None,
                    samples,
                };
                
                self.tx.send(TestEvent::BenchmarkResult(result)).await?;
            }
        }
        
        self.tx.send(TestEvent::Log(line.to_string())).await?;
        Ok(())
    }
}
