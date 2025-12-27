// Peer Lifecycle Simulator Example

mod peer_lifecycle;

use peer_lifecycle::{
    PeerLifecycleConfig,
    PeerLifecycleRunner,
    InitialNetworkState,
    TokenDistributionConfig,
    TopologyMode,
};

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║    Peer Lifecycle Simulator                            ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    // Create configuration
    let mut config = PeerLifecycleConfig::default();

    // Lower consensus thresholds for testing (easier to reach consensus with fewer Answers)
    config.peer_config.election_config.majority_threshold = 0.1; // 10% instead of 60%
    config.peer_config.election_config.consensus_threshold = 6; // 6/10 instead of 8/10

    // Customize for test
    config.rounds = 2000;

    // Adjust peer management parameters for testing
    config.peer_config.connection_timeout = 10000; // Long timeout to prevent premature disconnects
    config.peer_config.election_timeout = 100; // Give elections time to accumulate Answers via Referral chains
    config.peer_config.min_collection_time = 10; // Wait 10 ticks before checking election completion
    config.peer_config.pending_timeout = 1000; // Long timeout for discovered peers
    config.peer_config.elections_per_tick = 3; // Trigger multiple elections per tick
    // Network configuration matching Design/peer_lifecycle_simulator.md
    // Test scenario: 30 connected peers, 90% token coverage, 20% peer knowledge
    config.initial_state = InitialNetworkState {
        num_peers: 30, // Start with 30 connected peers
        // LocalKnowledge: Peers know subset of neighbors, some are Connected
        initial_topology: TopologyMode::LocalKnowledge {
            peer_knowledge_fraction: 0.5, // Know 50% of nearby peers (Identified)
            connected_fraction: 0.4,       // 40% of known peers start as Connected
        },
        bootstrap_rounds: 100,
    };

    // Token distribution with 90% coverage (high quality nodes)
    // neighbor_overlap controls view width - each peer overlaps with N neighbors
    config.token_distribution = TokenDistributionConfig {
        total_tokens: 10_000,     // 10K tokens + peer IDs automatically injected
        neighbor_overlap: 10,      // Overlap with 10 neighbors on each side (gives ~12 nearby)
        coverage_fraction: 0.9,    // Know 90% of nearby tokens (high quality)
    };
    config.metrics.sample_interval = 10;

    println!("Starting simulation...");
    println!("  Peers: {}", config.initial_state.num_peers);
    println!("  Rounds: {}", config.rounds);
    println!("  Topology: {:?}", config.initial_state.initial_topology);
    println!("  Tokens: {:?}\n", config.token_distribution);

    // Run simulation
    let runner = PeerLifecycleRunner::new(config);
    let result = runner.run();

    // Print results
    result.print_summary();

    println!("\n✓ Simulation complete!");
}
