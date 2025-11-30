// Peer Lifecycle Simulator Example

mod peer_lifecycle;

use peer_lifecycle::{
    PeerLifecycleConfig,
    PeerLifecycleRunner,
    InitialNetworkState,
    TokenDistribution,
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

    // Increase connection timeout to prevent premature timeouts during testing
    // In real system, normal traffic would provide keepalives
    config.peer_config.connection_timeout = 10000;

    // Make elections more frequent to speed up ring building
    config.peer_config.election_interval = 10; // Every 10 ticks instead of 60

    // Give elections MUCH more time to accumulate Answers via Referral chains
    config.peer_config.election_timeout = 100; // 100 ticks instead of 8
    config.peer_config.min_collection_time = 10; // Wait 10 ticks before checking

    // Increase pending timeout so discovered peers don't immediately timeout
    // In production, we'd send Invitations, but for testing just give them time
    config.peer_config.pending_timeout = 1000; // Long enough to see discovery working
    config.initial_state = InitialNetworkState {
        num_peers: 20,
        // Start with a well-connected seed network (ring + some random connections)
        // This simulates joining an existing network rather than cold start
        initial_topology: TopologyMode::Ring { neighbors: 3 },
        bootstrap_rounds: 20,
    };
    // Use Clustered distribution - tokens near peer ID for realistic DHT behavior
    // Need enough tokens for proof-of-storage signatures (10 tokens with specific 10-bit patterns)
    // With 1024 possible patterns, need ~10,000+ tokens for good coverage
    config.token_distribution = TokenDistribution::Clustered {
        tokens_per_peer: 10_000,
        cluster_radius: 1_000_000, // Cluster within 1M of peer ID
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
