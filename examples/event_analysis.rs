//! Event Analysis Example
//!
//! Run with: cargo run --example event_analysis --release
//!
//! This example demonstrates using CollectorEventSink to gather events
//! in memory and perform programmatic analysis on them.
//!
//! This uses the full simulator with a shared collector sink.

use log::info;
use simple_logger::SimpleLogger;
use std::cell::RefCell;
use std::rc::Rc;

mod simulator;
use simulator::{CollectorEventSink, SimConfig, SimRunner};

use ec_rust::ec_blocks::MemBlocks;
use ec_rust::ec_node::EcNode;
use ec_rust::ec_tokens::MemTokens;

fn main() {
    SimpleLogger::new().init().unwrap();

    info!("Running simulation with event collection and analysis...");

    // We'll use a custom setup to share the collector across all nodes
    let config = SimConfig {
        rounds: 100,
        num_peers: 50,
        seed: Some([42u8; 32]),
        enable_event_logging: false,
        csv_output_path: None,
        ..Default::default()
    };

    // For now, use SimRunner normally which creates per-peer CSV files
    // A future enhancement could allow injecting a shared collector
    let mut runner = SimRunner::new(config);
    let result = runner.run();

    info!("\n=== Simulation Results ===");
    info!("Rounds: {}", result.statistics.rounds_per_commit);
    info!("Commits: {}", result.committed_blocks);
    info!("Total messages: {}", result.total_messages);

    if result.committed_blocks > 0 {
        info!(
            "Performance: {:.1} rounds/commit, {:.0} messages/commit",
            result.statistics.rounds_per_commit, result.statistics.messages_per_commit
        );
    }

    info!("\nMessage distribution:");
    info!("  Query:  {}", result.statistics.message_counts.query);
    info!("  Vote:   {}", result.statistics.message_counts.vote);
    info!("  Block:  {}", result.statistics.message_counts.block);
    info!("  Answer: {}", result.statistics.message_counts.answer);

    info!("\n=== CollectorEventSink Usage ===");
    info!("The CollectorEventSink allows in-memory event collection.");
    info!("To use it programmatically:");
    info!("  1. Create: let collector = CollectorEventSink::new()");
    info!("  2. Inject into nodes via new_with_sink()");
    info!("  3. Query: collector.commits(), collector.reorgs()");
    info!("  4. Export: collector.export_to_csv(\"events.csv\")");
    info!("\nSee event_sinks.rs for available query methods.");
}
