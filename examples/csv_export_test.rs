//! CSV Export Example
//!
//! Run with: cargo run --example csv_export_test --release
//!
//! This example demonstrates CSV export of events for external analysis.
//! Events are exported per-peer to separate CSV files for easy analysis.

use log::info;
use simple_logger::SimpleLogger;

mod simulator;
use simulator::{SimConfig, SimRunner};

fn main() {
    SimpleLogger::new().init().unwrap();

    info!("Running simulation with CSV export enabled...");

    // Small simulation with CSV export
    let config = SimConfig {
        rounds: 100,
        num_peers: 20,
        seed: Some([42u8; 32]),
        enable_event_logging: false, // Disable console logging for cleaner output
        csv_output_path: Some("sim_events.csv".to_string()), // All events in one file
        ..Default::default()
    };

    info!("Configuration:");
    info!("  Rounds: {}", config.rounds);
    info!("  Peers: {}", config.num_peers);
    info!("  CSV output: sim_events.csv (single file, all peers)");

    let mut runner = SimRunner::new(config);
    let result = runner.run();

    info!("\n=== Simulation Results ===");
    info!("Seed used: {:?}", result.seed_used);
    info!("Commits: {}", result.committed_blocks);
    info!("Total messages: {}", result.total_messages);

    if result.committed_blocks > 0 {
        info!(
            "Performance: {:.1} rounds/commit, {:.0} messages/commit",
            result.statistics.rounds_per_commit, result.statistics.messages_per_commit
        );
    }

    info!("\n=== CSV File Generated ===");
    info!("All events exported to: sim_events.csv");
    info!("\nAnalysis examples:");
    info!("  # Count total commits");
    info!("  grep BlockCommitted sim_events.csv | wc -l");
    info!("  ");
    info!("  # Find all reorgs");
    info!("  grep Reorg sim_events.csv");
    info!("  ");
    info!("  # View first 20 events");
    info!("  head -20 sim_events.csv");
    info!("  ");
    info!("  # Filter by event type with awk");
    info!("  awk -F',' '$3==\"BlockCommitted\"' sim_events.csv");
    info!("  ");
    info!("  # Python analysis");
    info!("  df = pd.read_csv('sim_events.csv')");
    info!("  commits_per_peer = df[df['event_type']=='BlockCommitted'].groupby('peer').size()");
}
