//! Application State Management
//!
//! Handles TUI state, test statistics, and user interactions.

use crate::config::TestConfig;
use chrono::{DateTime, Local};

/// TUI application state
pub struct App {
    /// Test configuration
    pub config: TestConfig,
    
    /// Active tab (0=Overview, 1=Live, 2=Results, 3=Metrics)
    pub active_tab: usize,
    
    /// Test execution in progress
    pub running: bool,
    
    /// Confirm quit
    pub confirm_quit: bool,
    
    /// Log output
    pub logs: Vec<String>,
    
    /// Scroll position
    pub scroll_offset: i16,
    
    /// Test start timestamp
    #[allow(dead_code)]
    pub start_time: Option<DateTime<Local>>,
    
    /// Test statistics
    pub stats: TestStats,
}

/// Real-time test statistics
#[derive(Default, Clone)]
pub struct TestStats {
    /// Total tests
    pub total_tests: u32,
    /// Passed tests
    pub passed_tests: u32,
    /// Failed tests
    pub failed_tests: u32,
    /// Ignored tests
    pub ignored_tests: u32,
    /// Running tests
    pub running_tests: u32,
    /// Elapsed time
    pub elapsed_secs: u64,
    /// Throughput (test/sec)
    pub throughput: f64,
    /// Benchmark results
    pub benchmark_results: Vec<BenchmarkResult>,
    /// Recent errors
    #[allow(dead_code)]
    pub recent_errors: Vec<String>,
}

/// Benchmark result
#[derive(Clone, Debug)]
pub struct BenchmarkResult {
    pub name: String,
    pub time_ns: u64,
    pub throughput: Option<f64>,
    pub samples: u32,
}

impl App {
    pub fn new(config: TestConfig) -> Self {
        Self {
            config,
            active_tab: 0,
            running: false,
            confirm_quit: false,
            logs: Vec::new(),
            scroll_offset: 0,
            start_time: None,
            stats: TestStats::default(),
        }
    }

    /// Start test
    pub fn start_test(&mut self) {
        if !self.running {
            self.running = true;
            self.start_time = Some(Local::now());
            self.logs.clear();
            self.stats = TestStats::default();
            
            self.logs.push("▶ Starting test run...".to_string());
        }
    }

    /// Stop test
    pub fn stop_test(&mut self) {
        if self.running {
            self.running = false;
            self.logs.push("◼ Test stopped by user".to_string());
        }
    }

    /// Restart test
    pub fn restart_test(&mut self) {
        self.stop_test();
        self.start_test();
    }

    /// Switch to next tab
    pub fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % 4;
    }

    /// Switch to previous tab
    pub fn prev_tab(&mut self) {
        self.active_tab = if self.active_tab == 0 { 3 } else { self.active_tab - 1 };
    }

    /// Scroll up
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll down
    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    /// Update state (called during test execution)
    pub fn update(&mut self) {
        if self.running {
            // Update elapsed time
            if let Some(start) = self.start_time {
                self.stats.elapsed_secs = Local::now()
                    .signed_duration_since(start)
                    .num_seconds() as u64;
            }
            
            // Calculate throughput
            if self.stats.elapsed_secs > 0 {
                self.stats.throughput = 
                    (self.stats.passed_tests + self.stats.failed_tests) as f64 
                    / self.stats.elapsed_secs as f64;
            }
        }
    }

    /// Aggiungi log
    #[allow(dead_code)]
    pub fn add_log(&mut self, message: String) {
        self.logs.push(message);
        // Mantieni solo ultimi 1000 log
        if self.logs.len() > 1000 {
            self.logs.remove(0);
        }
    }
}
