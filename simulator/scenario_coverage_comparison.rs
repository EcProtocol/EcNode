// Scenario 2: Token Coverage Impact on Connectivity
//
// This scenario runs TWO simulations with identical peer configurations but
// different token coverage levels, demonstrating how token coverage affects
// the ability to form connections.
//
// Simulation A: 95% token coverage (high quality)
// Simulation B: 50% token coverage (medium quality)
//
// Both start with same topology: 5 random Identified peers each

mod peer_lifecycle;

use peer_lifecycle::{
    PeerLifecycleConfig,
    PeerLifecycleRunner,
    InitialNetworkState,
    TokenDistributionConfig,
    TopologyMode,
    ScenarioBuilder,
};

fn run_simulation(coverage: f64, label: &str) -> (f64, f64, f64) {
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  {}                                            ║", label);
    println!("╚════════════════════════════════════════════════════════╝\n");

    let mut config = PeerLifecycleConfig::default();

    // Election tuning
    config.peer_config.election_config.majority_threshold = 0.1;
    config.peer_config.election_config.consensus_threshold = 6;
    config.peer_config.connection_timeout = 10000;
    config.peer_config.election_timeout = 100;
    config.peer_config.min_collection_time = 10;
    config.peer_config.pending_timeout = 1000;
    config.peer_config.elections_per_tick = 3;

    config.rounds = 200;
    config.initial_state = InitialNetworkState {
        num_peers: 30,
        initial_topology: TopologyMode::RandomIdentified {
            peers_per_node: 5,  // Same for both
        },
        bootstrap_rounds: 100,
    };

    // THE KEY DIFFERENCE: token coverage
    config.token_distribution = TokenDistributionConfig {
        total_tokens: 100_000,
        neighbor_overlap: 10,
        coverage_fraction: coverage,  // VARIED
    };

    config.metrics.sample_interval = 10;

    // Add reporting checkpoints
    config.events = ScenarioBuilder::new()
        .at_round(100).report_stats("Mid-simulation")
        .at_round(200).report_stats("Final state")
        .build();

    println!("Configuration: {}% token coverage\n", (coverage * 100.0) as usize);

    // Run simulation
    let runner = PeerLifecycleRunner::new(config);
    let result = runner.run();

    // Extract key metrics
    let avg_connected = result.final_metrics.network_health.avg_connected_peers;
    let locality = result.final_metrics.network_health.gradient_distribution
        .as_ref()
        .map(|g| g.avg_steepness)
        .unwrap_or(0.0);
    let strong_locality_pct = result.final_metrics.network_health.gradient_distribution
        .as_ref()
        .map(|g| g.near_ideal_percent)
        .unwrap_or(0.0);

    let election_success_rate = if result.final_metrics.election_stats.total_elections_started > 0 {
        (result.final_metrics.election_stats.total_elections_completed as f64 /
         result.final_metrics.election_stats.total_elections_started as f64) * 100.0
    } else {
        0.0
    };

    println!("\nKey Metrics:");
    println!("  Avg Connected Peers: {:.1}", avg_connected);
    println!("  Locality Coefficient: {:.3}", locality);
    println!("  Strong Locality (≥0.7): {:.1}%", strong_locality_pct);
    println!("  Election Success Rate: {:.1}%", election_success_rate);

    (avg_connected, locality, election_success_rate)
}

fn main() {
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  SCENARIO 2: Token Coverage Impact Analysis           ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    println!("Hypothesis:");
    println!("  Peers with higher token coverage should achieve better");
    println!("  connectivity and more successful elections.\n");

    println!("Setup:");
    println!("  - Two simulations with identical peer topology");
    println!("  - 30 peers, each knowing 5 random others (Identified)");
    println!("  - Simulation A: 95% token coverage");
    println!("  - Simulation B: 50% token coverage");
    println!("  - Compare final connectivity and election success\n");

    // Run both simulations
    let (high_connected, high_locality, high_success) =
        run_simulation(0.95, "SIMULATION A: High Coverage (95%)");

    let (med_connected, med_locality, med_success) =
        run_simulation(0.50, "SIMULATION B: Medium Coverage (50%)");

    // Comparative analysis
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  COMPARATIVE ANALYSIS                                  ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    println!("┌────────────────────────────────┬──────────┬──────────┬──────────┐");
    println!("│ Metric                         │   95%    │   50%    │  Δ (%)   │");
    println!("├────────────────────────────────┼──────────┼──────────┼──────────┤");

    let connected_delta = ((high_connected - med_connected) / med_connected) * 100.0;
    println!("│ Avg Connected Peers            │  {:6.1}  │  {:6.1}  │  {:+6.1}  │",
             high_connected, med_connected, connected_delta);

    let locality_delta = ((high_locality - med_locality) / med_locality) * 100.0;
    println!("│ Locality Coefficient           │  {:6.3}  │  {:6.3}  │  {:+6.1}  │",
             high_locality, med_locality, locality_delta);

    let success_delta = high_success - med_success;
    println!("│ Election Success Rate (%)      │  {:6.1}  │  {:6.1}  │  {:+6.1}  │",
             high_success, med_success, success_delta);

    println!("└────────────────────────────────┴──────────┴──────────┴──────────┘\n");

    // Conclusions
    println!("Findings:\n");

    if high_connected > med_connected * 1.1 {
        println!("✓ High token coverage ({:.0}%) resulted in {:.0}% more connections",
                 95.0, connected_delta);
    } else {
        println!("⚠ Token coverage had minimal impact on connection count");
    }

    if high_success > med_success + 5.0 {
        println!("✓ High coverage improved election success rate by {:.1} percentage points",
                 success_delta);
    } else {
        println!("⚠ Election success rates were similar regardless of coverage");
    }

    println!("\nConclusion:");
    if connected_delta > 10.0 && success_delta > 5.0 {
        println!("  Token coverage is a CRITICAL factor for network connectivity.");
        println!("  Higher coverage enables more successful elections, leading to");
        println!("  more connections and better overall network health.");
    } else {
        println!("  Token coverage has MODERATE impact on connectivity.");
        println!("  Other factors (network topology, election parameters) may");
        println!("  be equally or more important.");
    }

    println!("\n✓ Scenario complete!\n");
}
