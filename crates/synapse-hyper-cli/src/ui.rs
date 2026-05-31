//! TUI Rendering
//!
//! Renders all UI components and widgets for the test monitoring interface.

use crate::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Gauge, Clear};

/// Render the interface
pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Tabs
            Constraint::Min(0),     // Main content
            Constraint::Length(3),  // Status bar
        ])
        .split(f.area());

    // Tabs
    let titles = vec!["Overview", "Live Output", "Results", "Metrics"];
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Synapse Hyper - Production Testing "))
        .select(app.active_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    // Main content
    match app.active_tab {
        0 => draw_overview(f, app, chunks[1]),
        1 => draw_live_output(f, app, chunks[1]),
        2 => draw_results(f, app, chunks[1]),
        3 => draw_metrics(f, app, chunks[1]),
        _ => {}
    }

    // Status bar
    let status = if app.running {
        format!("▶ RUNNING │ {}s elapsed │ {:.2} tests/sec", 
            app.stats.elapsed_secs, 
            app.stats.throughput)
    } else {
        "◼ IDLE │ Press SPACE to start │ 'q' to quit".to_string()
    };
    
    let status_bar = Paragraph::new(status)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(status_bar, chunks[2]);
}

/// Tab 1: Overview
fn draw_overview(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(7),  // Stats
            Constraint::Min(0),     // Logs preview
        ])
        .split(area);

    // Statistics
    let stats_text = format!(
        "Total: {} │ ✓ Passed: {} │ ✗ Failed: {} │ Ignored: {}\n\
         Running: {} | Elapsed: {}s | Throughput: {:.2}/sec",
        app.stats.total_tests,
        app.stats.passed_tests,
        app.stats.failed_tests,
        app.stats.ignored_tests,
        app.stats.running_tests,
        app.stats.elapsed_secs,
        app.stats.throughput
    );

    let stats = Paragraph::new(stats_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(" Test Statistics "));
    f.render_widget(stats, chunks[0]);

    // Logs preview
    let logs: Vec<ListItem> = app.logs
        .iter()
        .skip(app.scroll_offset as usize)
        .take(10)
        .map(|line| ListItem::new(line.clone()))
        .collect();
    
    let logs_widget = List::new(logs)
        .block(Block::default().borders(Borders::ALL).title(" Recent Logs "));
    f.render_widget(logs_widget, chunks[1]);
}

/// Tab 2: Live Output
fn draw_live_output(f: &mut Frame, app: &App, area: Rect) {
    let logs: Vec<ListItem> = app.logs
        .iter()
        .skip(app.scroll_offset as usize)
        .map(|line| ListItem::new(line.clone()))
        .collect();
    
    let logs_widget = List::new(logs)
        .block(Block::default().borders(Borders::ALL).title("Live Output"))
        .style(Style::default().fg(Color::Green));
    f.render_widget(logs_widget, area);
}

/// Tab 3: Results
fn draw_results(f: &mut Frame, app: &App, area: Rect) {
    if app.stats.benchmark_results.is_empty() {
        let msg = Paragraph::new("No benchmark results yet. Run tests to see results.")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(msg, area);
        return;
    }

    let results: Vec<ListItem> = app.stats.benchmark_results
        .iter()
        .map(|b| {
            let time_ms = b.time_ns as f64 / 1_000_000.0;
            let throughput = b.throughput
                .map(|t| format!(" | {:.2} MB/s", t))
                .unwrap_or_default();
            ListItem::new(format!(
                "{}: {:.2}ms ({} samples){}",
                b.name, time_ms, b.samples, throughput
            ))
        })
        .collect();
    
    let results_widget = List::new(results)
        .block(Block::default().borders(Borders::ALL).title("Benchmark Results"));
    f.render_widget(results_widget, area);
}

/// Tab 4: Metrics
fn draw_metrics(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Success rate
            Constraint::Length(3),  // Memory usage (placeholder)
            Constraint::Min(0),     // System info
        ])
        .split(area);

    // Success rate gauge
    let total = app.stats.total_tests.max(1);
    let success_rate = (app.stats.passed_tests as f64 / total as f64 * 100.0) as u16;
    
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Success Rate"))
        .gauge_style(Style::default().fg(Color::Green))
        .percent(success_rate);
    f.render_widget(gauge, chunks[0]);

    // Memory gauge (placeholder)
    let mem_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Memory Usage"))
        .gauge_style(Style::default().fg(Color::Blue))
        .percent(42); // Placeholder
    f.render_widget(mem_gauge, chunks[1]);

    // System info
    let sys_info = format!(
        "Features: {}\n\
         Profile: {}\n\
         Timeout: {}s\n\
         Verbose: {}\n\
         Report: {}",
        app.config.features,
        app.config.profile,
        app.config.timeout,
        if app.config.verbose { "Yes" } else { "No" },
        app.config.report_file.as_deref().unwrap_or("None")
    );

    let sys_widget = Paragraph::new(sys_info)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Configuration"));
    f.render_widget(sys_widget, chunks[2]);
}
