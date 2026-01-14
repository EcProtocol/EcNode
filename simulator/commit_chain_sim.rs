//! Commit Chain Simulation Example
//!
//! Run with: cargo run --bin commit_chain_sim

mod commit_chain;

use commit_chain::{BlockInjectionConfig, CommitChainSimConfig, NetworkConfig};
use ec_rust::ec_commit_chain::CommitChainConfig;
use log::info;
use simple_logger::SimpleLogger;

fn main() {
    SimpleLogger::new().init().unwrap();

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║        Commit Chain Simulator                          ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    info!("Setting up commit chain simulation...");

    // Configure simulation
    let config = CommitChainSimConfig {
        rounds: 500,
        num_peers: 20,
        seed: None, // Will be auto-generated

        commit_chain: CommitChainConfig {
            sync_target: 100, // Sync back 100 rounds
            confirmation_threshold: 2,
            ..Default::default()
        },

        block_injection: BlockInjectionConfig {
            blocks_per_round: 2.0, // Average 2 blocks per round total
            block_size_range: (1, 3),
            total_tokens: 10000,
        },

        network: NetworkConfig {
            delay_fraction: 0.3,
            loss_fraction: 0.01,
        },
    };

    info!("Configuration:");
    info!("  Peers: {}", config.num_peers);
    info!("  Rounds: {}", config.rounds);
    info!("  Blocks per round: {}", config.block_injection.blocks_per_round);
    info!("  Network delay: {}", config.network.delay_fraction);
    info!("  Network loss: {}", config.network.loss_fraction);
    info!("");

    info!("Starting simulation...");

    let runner = commit_chain::CommitChainRunner::new(config);
    let result = runner.run();

    // Display results
    result.print_summary();

    info!("✓ Simulation complete!");
}
