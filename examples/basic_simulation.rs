//! Basic simulation example for ecRust consensus
//!
//! Run with: cargo run --example basic_simulation

use log::info;
use simple_logger::SimpleLogger;

mod simulator;
use simulator::{NetworkConfig, SimConfig, SimRunner, TopologyConfig, TopologyMode, TransactionConfig};

fn main() {
    SimpleLogger::new().init().unwrap();

    info!("Setting up simulation...");

    // Configure simulation using the simulator module
    let config = SimConfig {
        rounds: 2000,
        num_peers: 2000,
        seed: None, // Will be auto-generated
        network: NetworkConfig {
            delay_fraction: 0.5,
            loss_fraction: 0.02,
        },
        topology: TopologyConfig {
            mode: TopologyMode::RingGradient {
                min_prob: 0.1,
                max_prob: 0.7,
            },
        },
        transactions: TransactionConfig {
            initial_tokens: 1,
            block_size_range: (1, 3),
        },
        enable_event_logging: true, // Enable to see consensus events
        csv_output_path: None,      // Set to Some("events.csv") to export all events
    };

    info!("Starting simulation...");

    let mut runner = SimRunner::new(config);
    let result = runner.run();

    // Display results
    info!("Simulation complete!");
    info!("Seed used: {:?}", result.seed_used);

    info!(
        "Peers: max: {} min: {} avg: {:.1}",
        result.statistics.peer_stats.max_peers,
        result.statistics.peer_stats.min_peers,
        result.statistics.peer_stats.avg_peers
    );

    if result.committed_blocks > 0 {
        info!(
            "Messages: {}. Commits: {}, avg: {:.1} rounds/commit, {:.0} messages/commit",
            result.total_messages,
            result.committed_blocks,
            result.statistics.rounds_per_commit,
            result.statistics.messages_per_commit
        );
        info!(
            "Message distribution: Query: {}, Vote: {}, Block: {}, Answer: {}",
            result.statistics.message_counts.query,
            result.statistics.message_counts.vote,
            result.statistics.message_counts.block,
            result.statistics.message_counts.answer
        );
    } else {
        info!(
            "Messages: {}. Commits: NONE",
            result.total_messages
        );
        info!(
            "Message distribution: Query: {}, Vote: {}, Block: {}, Answer: {}",
            result.statistics.message_counts.query,
            result.statistics.message_counts.vote,
            result.statistics.message_counts.block,
            result.statistics.message_counts.answer
        );
    }
}
