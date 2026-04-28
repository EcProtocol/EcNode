// Peer Lifecycle Simulator Example

mod peer_lifecycle;

use std::env;

use ec_rust::ec_genesis::GenesisConfig;
use ec_rust::ec_peers::PeerShapeTargetConfig;
use peer_lifecycle::{
    EventSchedule, InitialNetworkState, NetworkEvent, PeerLifecycleConfig, PeerLifecycleRunner,
    PeerSelection, ScheduledEvent, TokenDistributionConfig, TopologyMode,
};

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(default)
}

fn fixed_seed(variant: u64) -> [u8; 32] {
    let mut seed = [0u8; 32];
    seed[..8].copy_from_slice(&variant.to_le_bytes());
    seed[8..16].copy_from_slice(&0x9e37_79b9_7f4a_7c15u64.to_le_bytes());
    seed[16..24].copy_from_slice(&0xbf58_476d_1ce4_e5b9u64.to_le_bytes());
    seed[24..32].copy_from_slice(&0x94d0_49bb_1331_11ebu64.to_le_bytes());
    seed
}

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║    Peer Lifecycle Simulator                            ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    let seed_variant = env_u64("EC_PEER_LIFECYCLE_SEED_VARIANT", 1);
    let rounds = env_usize("EC_PEER_LIFECYCLE_ROUNDS", 240);
    let initial_peers = env_usize("EC_PEER_LIFECYCLE_INITIAL_PEERS", 96);
    let bootstrap_peers = env_usize("EC_PEER_LIFECYCLE_BOOTSTRAP_PEERS", 6);
    let total_tokens = env_usize("EC_PEER_LIFECYCLE_TOTAL_TOKENS", 200_000);
    let neighbor_overlap = env_usize("EC_PEER_LIFECYCLE_NEIGHBOR_OVERLAP", 8);
    let coverage_fraction = env_f64("EC_PEER_LIFECYCLE_COVERAGE", 0.90);
    let crash_count = env_usize("EC_PEER_LIFECYCLE_CRASH_COUNT", initial_peers / 8);
    let return_count = env_usize("EC_PEER_LIFECYCLE_RETURN_COUNT", crash_count);
    let connected_target = env_usize("EC_PEER_LIFECYCLE_CONNECTED_TARGET", 0);
    let connected_hysteresis = env_usize("EC_PEER_LIFECYCLE_CONNECTED_HYSTERESIS", 0);
    let elections_when_over_target =
        env_usize("EC_PEER_LIFECYCLE_ELECTIONS_WHEN_OVER_TARGET", usize::MAX);
    let elections_per_tick = env_usize("EC_PEER_LIFECYCLE_ELECTIONS_PER_TICK", 3);
    let election_timeout = env_usize("EC_PEER_LIFECYCLE_ELECTION_TIMEOUT", 100) as u64;
    let min_collection_time = env_usize("EC_PEER_LIFECYCLE_MIN_COLLECTION_TIME", 10) as u64;
    let consensus_threshold = env_usize("EC_PEER_LIFECYCLE_CONSENSUS_THRESHOLD", 8);
    let majority_threshold = env_f64("EC_PEER_LIFECYCLE_MAJORITY_THRESHOLD", 0.6);
    let prune_protection_time = env_u64("EC_PEER_LIFECYCLE_PRUNE_PROTECTION_TIME", 600);
    let enable_dense_shape_target = env_bool("EC_PEER_LIFECYCLE_DENSE_SHAPE_TARGET", false);
    let dense_shape_neighbors = env_usize("EC_PEER_LIFECYCLE_DENSE_SHAPE_NEIGHBORS", 10);
    let dense_shape_far_prob = env_f64("EC_PEER_LIFECYCLE_DENSE_SHAPE_FAR_PROB", 0.2);
    let dense_shape_hysteresis = env_usize("EC_PEER_LIFECYCLE_DENSE_SHAPE_HYSTERESIS", 4);

    // Genesis mode: use shared genesis tokens instead of random distribution
    let enable_genesis = env_bool("EC_PEER_LIFECYCLE_GENESIS", false);
    let genesis_block_count = env_usize("EC_PEER_LIFECYCLE_GENESIS_BLOCKS", 100_000);
    let genesis_storage_fraction = env_f64("EC_PEER_LIFECYCLE_GENESIS_STORAGE", 0.25);

    // Create configuration
    let mut config = PeerLifecycleConfig::default();
    config.seed = Some(fixed_seed(seed_variant));

    // Lower consensus thresholds for testing (easier to reach consensus with fewer Answers)
    config.peer_config.election_config.majority_threshold = majority_threshold;
    config.peer_config.election_config.consensus_threshold = consensus_threshold;

    // Customize for test
    config.rounds = rounds;

    // Adjust peer management parameters for testing
    config.peer_config.connection_timeout = 10000; // Long timeout to prevent premature disconnects
    config.peer_config.election_timeout = election_timeout; // Give elections time to accumulate Answers via Referral chains
    config.peer_config.min_collection_time = min_collection_time; // Wait before checking election completion
    config.peer_config.pending_timeout = 1000; // Long timeout for discovered peers
    config.peer_config.elections_per_tick = elections_per_tick; // Trigger multiple elections per tick
    config.peer_config.prune_protection_time = prune_protection_time;
    if connected_target > 0 {
        config.peer_config.connected_target = Some(connected_target);
        config.peer_config.connected_target_hysteresis = connected_hysteresis;
        if elections_when_over_target != usize::MAX {
            config.peer_config.elections_per_tick_above_target = Some(elections_when_over_target);
        }
    }
    if enable_dense_shape_target {
        config.peer_config.shape_target = Some(PeerShapeTargetConfig {
            guaranteed_neighbors: dense_shape_neighbors,
            center_probability: 1.0,
            far_probability: dense_shape_far_prob,
            hysteresis: dense_shape_hysteresis,
        });
    }

    // Bootstrap scenario test: each peer starts with a handful of random
    // Identified peers and must discover the rest through referrals.
    config.initial_state = InitialNetworkState {
        num_peers: initial_peers,
        // RandomIdentified: Each peer knows 5 random other peers (Identified state)
        initial_topology: TopologyMode::RandomIdentified {
            peers_per_node: bootstrap_peers,
        },
        bootstrap_rounds: 100,
    };

    // Token distribution: genesis mode uses shared tokens, random mode uses per-peer views
    config.token_distribution = TokenDistributionConfig {
        total_tokens,
        neighbor_overlap,
        coverage_fraction,
        genesis_config: if enable_genesis {
            Some(GenesisConfig {
                block_count: genesis_block_count,
                ..GenesisConfig::default()
            })
        } else {
            None
        },
        genesis_storage_fraction,
    };
    config.metrics.sample_interval = 10;

    // Add scheduled events to monitor progress
    config.events = EventSchedule {
        events: vec![
            ScheduledEvent {
                round: 50,
                event: NetworkEvent::ReportStats {
                    label: Some("After bootstrap phase".to_string()),
                },
            },
            ScheduledEvent {
                round: 100,
                event: NetworkEvent::ReportStats {
                    label: Some("Mid-simulation checkpoint".to_string()),
                },
            },
            ScheduledEvent {
                round: 110,
                event: NetworkEvent::PeerCrash {
                    selection: PeerSelection::Random { count: crash_count },
                },
            },
            ScheduledEvent {
                round: 125,
                event: NetworkEvent::ReportStats {
                    label: Some("After crash wave".to_string()),
                },
            },
            ScheduledEvent {
                round: 140,
                event: NetworkEvent::PeerReturn {
                    selection: PeerSelection::Random {
                        count: return_count,
                    },
                    bootstrap_method: peer_lifecycle::BootstrapMethod::Random(bootstrap_peers),
                },
            },
            ScheduledEvent {
                round: 150,
                event: NetworkEvent::ReportStats {
                    label: Some("After return wave".to_string()),
                },
            },
        ],
    };

    println!("Starting simulation...");
    println!("  Seed variant: {}", seed_variant);
    println!("  Peers: {}", config.initial_state.num_peers);
    println!("  Rounds: {}", config.rounds);
    println!("  Topology: {:?}", config.initial_state.initial_topology);
    if connected_target > 0 {
        let elections_label = if elections_when_over_target == usize::MAX {
            "default".to_string()
        } else {
            elections_when_over_target.to_string()
        };
        println!(
            "  Connected target: {} ± {}, elections above high band: {}",
            connected_target, connected_hysteresis, elections_label
        );
    }
    if enable_dense_shape_target {
        println!(
            "  Dense shape target: core ±{}, far probability {:.2}, hysteresis {}",
            dense_shape_neighbors, dense_shape_far_prob, dense_shape_hysteresis
        );
    }
    println!("  Prune protection: {}", prune_protection_time);
    println!(
        "  Elections/tick: {}, threshold: {} answers @ {:.2} majority, timeout: {}, min collection: {}",
        elections_per_tick,
        consensus_threshold,
        majority_threshold,
        election_timeout,
        min_collection_time
    );
    if enable_genesis {
        println!(
            "  Genesis mode: {} blocks, {:.0}% storage per peer",
            genesis_block_count,
            genesis_storage_fraction * 100.0
        );
    } else {
        println!("  Random token mode (no shared genesis)");
    }
    println!("  Tokens: {:?}\n", config.token_distribution);

    // Run simulation
    let runner = PeerLifecycleRunner::new(config);
    let result = runner.run();

    // Print results
    result.print_summary();

    println!("\n✓ Simulation complete!");
}
