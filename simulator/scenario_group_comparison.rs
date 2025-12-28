// Peer Group Comparison Scenario
//
// Demonstrates peer group tracking by having different cohorts join at different times
// with varying token coverage levels. Shows how group membership allows fine-grained
// analysis of network behavior.

mod peer_lifecycle;

use peer_lifecycle::{
    PeerLifecycleConfig,
    PeerLifecycleRunner,
    InitialNetworkState,
    TokenDistributionConfig,
    TopologyMode,
    ScenarioBuilder,
    BootstrapMethod,
};

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  SCENARIO: Peer Group Comparison                      ║");
    println!("╚════════════════════════════════════════════════════════╝\n");
    println!("Objective:");
    println!("  Track three distinct peer groups with different coverage");
    println!("  levels to understand how token coverage affects their");
    println!("  ability to integrate into an established network.\n");
    println!("Groups:");
    println!("  - 'initial': 20 peers, 95% coverage (bootstrap network)");
    println!("  - 'high-quality': 10 peers, 90% coverage (join at round 100)");
    println!("  - 'low-quality': 10 peers, 50% coverage (join at round 150)");
    println!("\n");

    // Configuration
    let mut config = PeerLifecycleConfig::default();

    // Initial network: 20 peers with high coverage
    config.initial_state = InitialNetworkState {
        num_peers: 20,
        initial_topology: TopologyMode::RandomIdentified {
            peers_per_node: 5,  // Each peer knows 5 random others
        },
        bootstrap_rounds: 100,
    };

    config.rounds = 250;  // Long enough to see all groups stabilize

    // High token coverage for initial group
    config.token_distribution = TokenDistributionConfig {
        total_tokens: 100_000,
        neighbor_overlap: 10,
        coverage_fraction: 0.95,  // 95% coverage
    };

    // Configure events: peers join at different rounds
    config.events = ScenarioBuilder::new()
        .at_round(50).report_stats("Initial network baseline")
        .at_round(100).peers_join(10, 0.90, BootstrapMethod::Random(3), "high-quality")
        .at_round(120).report_stats("After high-quality joins")
        .at_round(150).peers_join(10, 0.50, BootstrapMethod::Random(3), "low-quality")
        .at_round(170).report_stats("After low-quality joins")
        .at_round(200).report_stats("Mid-integration")
        .at_round(250).report_stats("Final state")
        .build();

    // Run simulation
    let runner = PeerLifecycleRunner::new(config);
    let _result = runner.run();

    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  Analysis                                              ║");
    println!("╚════════════════════════════════════════════════════════╝\n");
    println!("Key Observations:");
    println!("  1. The 'initial' group should establish strong connectivity");
    println!("     due to high coverage (95%) and sufficient bootstrap time.\n");
    println!("  2. The 'high-quality' group (90% coverage) should integrate");
    println!("     successfully, achieving near-initial levels of connectivity.\n");
    println!("  3. The 'low-quality' group (50% coverage) will struggle to");
    println!("     connect, demonstrating the critical role of token coverage.\n");
    println!("Expected Metrics (final state):");
    println!("  - initial: ~18-20 avg connections, ~80% election success");
    println!("  - high-quality: ~15-18 avg connections, ~75% election success");
    println!("  - low-quality: ~0-2 avg connections, ~1-5% election success\n");
    println!("✓ Scenario complete!");
}
