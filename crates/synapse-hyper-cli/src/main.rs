//! Synapse Hyper CLI - Production Testing & Monitoring Interface
//! 
//! Monitora test e benchmark in tempo reale con interfaccia terminale interattiva.

mod app;
mod ui;
mod runner;
mod config;

use app::App;
use clap::{Parser, Subcommand};
use config::TestConfig;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use runner::TestRunner;
use std::{io, time::Duration};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Synapse Hyper CLI - Production Testing & Monitoring
#[derive(Parser)]
#[command(name = "synapse-hyper")]
#[command(author = "Synapse Team", version = "0.1.0")]
#[command(about = "Production testing and monitoring interface", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Feature flags per backend (es: rocksdb-backend, redb-backend)
    #[arg(long, default_value = "rocksdb-backend")]
    features: String,

    /// Profilo di test (unit, integration, performance, all)
    #[arg(short, long, default_value = "all")]
    profile: String,

    /// Timeout in secondi per test performance
    #[arg(long, default_value = "300")]
    timeout: u64,

    /// Abilita output dettagliato
    #[arg(short, long)]
    verbose: bool,

    /// Salva report su file
    #[arg(long)]
    report: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Esegui unit test con monitoraggio TUI
    Unit {
        /// Pattern filtro (es: "index_fabric")
        #[arg(default_value = "")]
        pattern: String,
    },
    /// Esegui integration test con monitoraggio TUI
    Integration {
        /// Test specifico (es: "integration_cost_model")
        #[arg(default_value = "")]
        test: String,

        /// Includi performance test (--ignored)
        #[arg(long)]
        include_performance: bool,
    },
    /// Esegui benchmark con monitoraggio TUI
    Bench {
        /// Benchmark specifico (es: "bench_cost_model")
        #[arg(default_value = "")]
        bench: String,

        /// Campioni per benchmark
        #[arg(long, default_value = "100")]
        sample_count: u32,
    },
    /// Esegui tutti i test (unit + integration + performance)
    All {
        /// Includi benchmark
        #[arg(long)]
        with_benchmarks: bool,
    },
    /// Monitora test in corso
    Monitor {
        /// PID del processo cargo test
        #[arg(short, long)]
        pid: Option<u32>,
    },
    /// Mostra report test precedenti
    Report {
        /// File report (es: "vps_test_report_20260531_120000.md")
        #[arg(short, long)]
        file: Option<String>,

        /// Mostra solo metriche chiave
        #[arg(long)]
        summary: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let config = TestConfig {
        features: cli.features.clone(),
        profile: cli.profile.clone(),
        timeout: cli.timeout,
        verbose: cli.verbose,
        report_file: cli.report.clone(),
    };

    let mut app = App::new(config);

    // Main loop
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Handle result
    match res {
        Ok(_) => {
            println!("Test session completed successfully");
            if let Some(report_path) = &app.config.report_file {
                println!("Report saved to: {}", report_path);
            }
        }
        Err(err) => println!("Error: {:?}", err),
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => {
                    if app.confirm_quit {
                        return Ok(());
                    } else {
                        app.confirm_quit = true;
                    }
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    if app.running {
                        app.stop_test();
                    }
                }
                KeyCode::Char('r') => {
                    app.restart_test();
                }
                KeyCode::Char(' ') => {
                    if !app.running {
                        app.start_test();
                    }
                }
                KeyCode::Tab => {
                    app.next_tab();
                }
                KeyCode::BackTab => {
                    app.prev_tab();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    app.scroll_up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.scroll_down();
                }
                KeyCode::Char('1') => app.active_tab = 0,
                KeyCode::Char('2') => app.active_tab = 1,
                KeyCode::Char('3') => app.active_tab = 2,
                KeyCode::Char('4') => app.active_tab = 3,
                _ => {}
            }
        }

        // Auto-refresh durante test
        if app.running {
            tokio::time::sleep(Duration::from_millis(100)).await;
            app.update();
        }
    }
}
