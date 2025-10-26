//! Test event logging in simulator
//!
//! Run with: cargo run --example event_logging_test --release
//!
//! This example demonstrates the event logging system with a smaller network
//! to make it easier to see individual consensus events.

use log::info;
use simple_logger::SimpleLogger;

mod simulator;
use simulator::{SimConfig, SimRunner};

fn main() {
    SimpleLogger::new().init().unwrap();

    info!("Running small simulation with event logging enabled...");

    // Small simulation to see events clearly
    let config = SimConfig {
        rounds: 50,
        num_peers: 50,              // Minimum ~20 peers needed for topology to work
        seed: Some([42u8; 32]),     // Fixed seed for reproducibility
        enable_event_logging: true, // Enable event logging
        csv_output_path: None,      // Set to Some("event_test.csv") for CSV export
        ..Default::default()
    };

    let mut runner = SimRunner::new(config);
    let result = runner.run();

    info!("\nSimulation complete!");
    info!("Commits: {}", result.committed_blocks);
    info!("Total messages: {}", result.total_messages);
}
