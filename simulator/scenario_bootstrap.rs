// Scenario 1: Bootstrap with Shared State (High Token Coverage)
//
// This scenario demonstrates that peers with high token coverage (95%)
// can bootstrap into a working network even with minimal initial peer knowledge.
//
// Configuration:
// - 30 peers start with only 3 random Identified peers each
// - 95% token coverage (high quality nodes)
// - Monitor progress at rounds 50, 100, 150

mod peer_lifecycle;

use peer_lifecycle::{
    PeerLifecycleConfig,
    PeerLifecycleRunner,
    InitialNetworkState,
    TokenDistributionConfig,
    TopologyMode,
    ScenarioBuilder,
};

fn main() {
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  SCENARIO 1: Bootstrap with Shared State              ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    println!("Hypothesis:");
    println!("  Peers with high token coverage (95%) should bootstrap into a");
    println!("  functional network despite having minimal initial peer knowledge.");
    println!("\nSetup:");
    println!("  - 30 peers, each knowing only 3 random other peers (Identified)");
    println!("  - 95% token coverage (excellent shared state knowledge)");
    println!("  - Monitoring at rounds 50, 100, 150, 200\n");

    // Create base configuration
    let mut config = PeerLifecycleConfig::default();

    // Election tuning for faster convergence
    config.peer_config.election_config.majority_threshold = 0.1;
    config.peer_config.election_config.consensus_threshold = 6;
    config.peer_config.connection_timeout = 10000;
    config.peer_config.election_timeout = 100;
    config.peer_config.min_collection_time = 10;
    config.peer_config.pending_timeout = 1000;
    config.peer_config.elections_per_tick = 3;

    // Scenario-specific configuration
    config.rounds = 200;
    config.initial_state = InitialNetworkState {
        num_peers: 30,
        // Each peer starts knowing only 3 random peers
        initial_topology: TopologyMode::RandomIdentified {
            peers_per_node: 3,  // MINIMAL peer knowledge
        },
        bootstrap_rounds: 100,
    };

    // HIGH token coverage - this is the key factor
    config.token_distribution = TokenDistributionConfig {
        total_tokens: 100_000,
        neighbor_overlap: 10,
        coverage_fraction: 0.95,  // 95% - excellent shared state knowledge
    };

    config.metrics.sample_interval = 10;

    // Use ScenarioBuilder to add checkpoints
    config.events = ScenarioBuilder::bootstrap_shared_state().build();

    println!("Starting simulation...\n");

    // Run simulation
    let runner = PeerLifecycleRunner::new(config);
    let result = runner.run();

    // Print results
    result.print_summary();

    // Print conclusion
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  ANALYSIS                                              ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    let final_connected_avg = result.final_metrics.network_health.avg_connected_peers;
    let final_locality = result.final_metrics.network_health.gradient_distribution
        .as_ref()
        .map(|g| g.avg_steepness)
        .unwrap_or(0.0);

    if let Some(ref dist) = result.final_metrics.network_health.gradient_distribution {
        println!("Final Network State:");
        println!("  Average Connected Peers: {:.1}", final_connected_avg);
        println!("  Locality Coefficient: {:.3}", final_locality);
        println!("  Strong Locality (≥0.7): {:.1}%", dist.near_ideal_percent);
        println!();

        if final_connected_avg > 15.0 && dist.near_ideal_percent > 5.0 {
            println!("✓ SUCCESS: Network bootstrapped successfully!");
            println!("  Despite minimal initial peer knowledge (3 peers), high token");
            println!("  coverage (95%) enabled the network to form connections through");
            println!("  successful elections and mutual discovery.");
        } else {
            println!("⚠ PARTIAL: Network formed but connectivity is lower than expected.");
            println!("  Token coverage alone may not be sufficient - peer knowledge or");
            println!("  network conditions may be limiting factors.");
        }
    }

    println!("\n✓ Scenario complete!\n");
}
