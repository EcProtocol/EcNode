//! Test simulation with fixed seed for reproducibility
//!
//! Run with: cargo run --example fixed_seed_test

use log::info;
use simple_logger::SimpleLogger;

mod simulator;
use simulator::{SimConfig, SimRunner};

fn main() {
    SimpleLogger::new().init().unwrap();

    // Use a fixed seed for reproducible results
    let fixed_seed = [42u8; 32];

    info!("Running simulation with fixed seed: {:?}", fixed_seed);

    let config = SimConfig {
        rounds: 100,
        num_peers: 50,
        seed: Some(fixed_seed),
        ..Default::default()
    };

    let mut runner = SimRunner::new(config);
    let result = runner.run();

    info!("Simulation complete!");
    info!("Seed used: {:?}", result.seed_used);
    info!("Commits: {}", result.committed_blocks);
    info!("Total messages: {}", result.total_messages);

    // Verify the seed was used correctly
    assert_eq!(result.seed_used, fixed_seed, "Seed mismatch!");
    info!("✓ Seed verification passed!");
}
