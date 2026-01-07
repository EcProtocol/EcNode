// Peer Lifecycle Genesis Bootstrap Simulator
//
// This example demonstrates cold-start network bootstrap using deterministic genesis tokens.
// Peers start with no connections (Isolated) and discover each other through genesis token elections.

mod peer_lifecycle;

use peer_lifecycle::{
    PeerLifecycleConfig,
    PeerLifecycleRunner,
    InitialNetworkState,
    TokenDistributionConfig,
    TopologyMode,
    EventSchedule,
    ScheduledEvent,
    NetworkEvent,
    BootstrapMethod,
};
use ec_rust::ec_genesis::GenesisConfig;

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  Genesis Bootstrap Simulator                          ║");
    println!("║  Cold-start network from deterministic genesis tokens ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    // Create configuration
    let mut config = PeerLifecycleConfig::default();

    // Lower consensus thresholds for testing (easier to reach consensus with fewer Answers)
    config.peer_config.election_config.majority_threshold = 0.6; // 10% instead of 60%
    config.peer_config.election_config.consensus_threshold = 8; // 6/10 instead of 8/10

    // Simulation parameters
    config.rounds = 150;

    // Adjust peer management parameters
    config.peer_config.connection_timeout = 10000; // Long timeout to prevent premature disconnects
    config.peer_config.election_timeout = 100; // Give elections time to accumulate Answers
    config.peer_config.min_collection_time = 10; // Wait before checking election completion
    config.peer_config.pending_timeout = 1000; // Long timeout for discovered peers
    config.peer_config.elections_per_tick = 5; // More elections for faster discovery

    // Initial network: 30 peers starting with 4 random known peers each (match random mode test)
    config.initial_state = InitialNetworkState {
        num_peers: 30, // Start with 30 peers (matching random mode)
        // RandomIdentified: Each peer knows 4 random others (bootstrap scenario)
        initial_topology: TopologyMode::RandomIdentified {
            peers_per_node: 4  // Each peer knows 4 random others initially
        },
        bootstrap_rounds: 0, // No artificial bootstrap - let discovery happen naturally
    };

    // Token distribution: GENESIS MODE with full 100K tokens
    config.token_distribution = TokenDistributionConfig {
        // These fields are unused in genesis mode
        total_tokens: 0,
        neighbor_overlap: 0,
        coverage_fraction: 0.0,

        // Genesis configuration
        genesis_config: Some(GenesisConfig {
            block_count: 100_000, // Full 100K genesis tokens
            seed_string: "This is the Genesis of the Echo Consent Network".to_string(),
        }),
        genesis_storage_fraction: 0.3, // Each peer stores 3/4 of ring (~75K tokens) - match random mode overlap!
    };

    config.metrics.sample_interval = 10;

    // Schedule events to monitor progress and add late joiners
    config.events = EventSchedule {
        events: vec![
            ScheduledEvent {
                round: 25,
                event: NetworkEvent::ReportStats {
                    label: Some("Round 25: Early network formation".to_string()),
                },
            },
            ScheduledEvent {
                round: 50,
                event: NetworkEvent::ReportStats {
                    label: Some("Round 50: Before late joiners".to_string()),
                },
            },
            // Add 5 late-joining peers at round 50
            ScheduledEvent {
                round: 50,
                event: NetworkEvent::PeerJoin {
                    count: 5, // 5 new peers join
                    coverage_fraction: 0.90, // Same storage fraction as initial peers (90%)
                    bootstrap_method: BootstrapMethod::Random(4), // Each knows 4 random existing peers
                    group_name: Some("late-joiners".to_string()),
                },
            },
            ScheduledEvent {
                round: 75,
                event: NetworkEvent::ReportStats {
                    label: Some("Round 75: After late joiners integrated".to_string()),
                },
            },
            ScheduledEvent {
                round: 145,
                event: NetworkEvent::ReportStats {
                    label: Some("Round 145: Final state".to_string()),
                },
            },
        ],
    };

    println!("Starting genesis bootstrap simulation...");
    println!("  Initial peers: {}", config.initial_state.num_peers);
    println!("  Rounds: {}", config.rounds);
    println!("  Topology: {:?}", config.initial_state.initial_topology);
    println!("  Genesis config: {:?}\n", config.token_distribution.genesis_config);

    // Run simulation
    let runner = PeerLifecycleRunner::new(config);
    let result = runner.run();

    // Print results
    result.print_summary();

    println!("\n✓ Genesis bootstrap simulation complete!");
}
