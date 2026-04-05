#[allow(dead_code, unused_imports)]
mod integrated;
#[allow(dead_code, unused_imports)]
mod peer_lifecycle;

use std::env;

use ec_rust::ec_genesis::GenesisConfig;
use ec_rust::ec_peers::AdaptiveNeighborhoodConfig;

use integrated::{
    IntegratedRunner, IntegratedSimConfig, NetworkConfig, TransactionFlowConfig,
    TransactionSourcePolicy,
};
use peer_lifecycle::{
    BootstrapMethod, InitialNetworkState, NetworkEvent, PeerSelection, ScheduledEvent,
    TokenDistributionConfig, TopologyMode,
};

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(default)
}

fn env_string(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(default)
}

fn fixed_seed(variant: u64) -> [u8; 32] {
    let mut seed = [
        0x45, 0x63, 0x68, 0x6f, 0x2d, 0x43, 0x6f, 0x6e, 0x73, 0x65, 0x6e, 0x74, 0x2d, 0x50,
        0x4f, 0x43, 0x2d, 0x4c, 0x6f, 0x6e, 0x67, 0x2d, 0x52, 0x75, 0x6e, 0x2d, 0x30, 0x31,
        0x2d, 0x58, 0x59, 0x5a,
    ];

    for (idx, byte) in variant.to_le_bytes().iter().enumerate() {
        seed[24 + idx] ^= *byte;
    }

    seed
}

fn main() {
    let seed_variant = env_u64("EC_LONG_RUN_SEED_VARIANT", 0);
    let rounds = env_usize("EC_LONG_RUN_ROUNDS", 2400);
    let initial_peers = env_usize("EC_LONG_RUN_INITIAL_PEERS", 96);
    let join_count = env_usize("EC_LONG_RUN_JOIN_COUNT", 24);
    let crash_count = env_usize("EC_LONG_RUN_CRASH_COUNT", 12);
    let return_count = env_usize("EC_LONG_RUN_RETURN_COUNT", 8);
    let second_join_count = env_usize("EC_LONG_RUN_SECOND_JOIN_COUNT", 16);
    let second_crash_count = env_usize("EC_LONG_RUN_SECOND_CRASH_COUNT", 10);
    let genesis_blocks = env_usize("EC_LONG_RUN_GENESIS_BLOCKS", 50_000);
    let network_profile = env_string("EC_LONG_RUN_NETWORK_PROFILE", "cross_dc_normal");
    let neighborhood_width = env_usize("EC_LONG_RUN_NEIGHBORHOOD_WIDTH", 4);
    let vote_target_count = env_usize("EC_LONG_RUN_VOTE_TARGETS", 2);
    let adaptive_far_width = env_usize("EC_LONG_RUN_ADAPTIVE_FAR_WIDTH", 0);
    let adaptive_hop_threshold = env_usize("EC_LONG_RUN_ADAPTIVE_HOP_THRESHOLD", 0);
    let existing_token_fraction = env_f64("EC_LONG_RUN_EXISTING_TOKEN_FRACTION", 0.0);
    let enable_batching = env_bool("EC_LONG_RUN_BATCHING", true);
    let batch_vote_replies = env_bool("EC_LONG_RUN_BATCH_VOTE_REPLIES", false);

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  Integrated Long-Run Simulator                        ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!("Runs a longer genesis-backed lifecycle scenario with fixed seed.");
    println!("Seed variant: {}", seed_variant);
    println!("Network profile: {}", network_profile);
    println!("Neighborhood width: {}", neighborhood_width);
    println!("Vote targets per request: {}", vote_target_count);
    println!(
        "Batching: {}, vote replies: {}",
        if enable_batching { "on" } else { "off" },
        if batch_vote_replies { "batched" } else { "standalone" }
    );
    println!(
        "Existing-token workload target: {:.0}%",
        existing_token_fraction * 100.0
    );
    if adaptive_far_width > 0 {
        println!(
            "Adaptive far width: {} beyond {} hops",
            adaptive_far_width, adaptive_hop_threshold
        );
    }

    let mut config = IntegratedSimConfig::default();
    config.seed = Some(fixed_seed(seed_variant));
    config.rounds = rounds;
    config.initial_state = InitialNetworkState {
        num_peers: initial_peers,
        initial_topology: TopologyMode::RandomIdentified { peers_per_node: 6 },
        bootstrap_rounds: 0,
    };
    config.token_distribution = TokenDistributionConfig {
        total_tokens: 0,
        neighbor_overlap: 8,
        coverage_fraction: 0.90,
        genesis_config: Some(GenesisConfig {
            block_count: genesis_blocks,
            seed_string: "Integrated long-run genesis".to_string(),
        }),
        genesis_storage_fraction: 0.25,
    };
    config.peer_config.neighborhood_width = neighborhood_width;
    config.peer_config.vote_target_count = vote_target_count;
    config.peer_config.enable_request_batching = enable_batching;
    config.peer_config.batch_vote_replies = batch_vote_replies;
    config.peer_config.adaptive_neighborhood = if adaptive_far_width > 0 {
        Some(AdaptiveNeighborhoodConfig {
            far_width: adaptive_far_width,
            far_hop_threshold: adaptive_hop_threshold,
        })
    } else {
        None
    };
    config.network = match network_profile.as_str() {
        "perfect" => NetworkConfig::perfect(),
        "same_dc" => NetworkConfig::same_dc(),
        "cross_dc_stressed" | "stressed" => NetworkConfig::cross_dc_stressed(),
        _ => NetworkConfig::cross_dc_normal(),
    };
    config.transactions = TransactionFlowConfig {
        blocks_per_round: 3,
        block_size_range: (1, 3),
        source_policy: TransactionSourcePolicy::ConnectedOnly,
        existing_token_fraction,
    };

    let report_a = rounds / 6;
    let join_round = rounds / 5;
    let report_b = rounds / 3;
    let crash_round = rounds / 2;
    let return_round = crash_round + rounds / 12;
    let second_join_round = (rounds * 7) / 10;
    let second_crash_round = (rounds * 5) / 6;
    let final_report_round = rounds.saturating_sub(rounds / 10);

    config.events.events = vec![
        ScheduledEvent {
            round: report_a,
            event: NetworkEvent::ReportStats {
                label: Some("early-baseline".to_string()),
            },
        },
        ScheduledEvent {
            round: join_round,
            event: NetworkEvent::PeerJoin {
                count: join_count,
                coverage_fraction: 0.90,
                bootstrap_method: BootstrapMethod::Random(4),
                group_name: Some("growth-wave-1".to_string()),
            },
        },
        ScheduledEvent {
            round: report_b,
            event: NetworkEvent::ReportStats {
                label: Some("post-growth-wave-1".to_string()),
            },
        },
        ScheduledEvent {
            round: crash_round,
            event: NetworkEvent::PeerCrash {
                selection: PeerSelection::Random { count: crash_count },
            },
        },
        ScheduledEvent {
            round: return_round,
            event: NetworkEvent::PeerReturn {
                selection: PeerSelection::Random { count: return_count },
                bootstrap_method: BootstrapMethod::Random(4),
            },
        },
        ScheduledEvent {
            round: second_join_round,
            event: NetworkEvent::PeerJoin {
                count: second_join_count,
                coverage_fraction: 0.88,
                bootstrap_method: BootstrapMethod::Random(4),
                group_name: Some("growth-wave-2".to_string()),
            },
        },
        ScheduledEvent {
            round: second_crash_round,
            event: NetworkEvent::PeerCrash {
                selection: PeerSelection::Random {
                    count: second_crash_count,
                },
            },
        },
        ScheduledEvent {
            round: final_report_round,
            event: NetworkEvent::ReportStats {
                label: Some("late-stage".to_string()),
            },
        },
    ];

    let runner = IntegratedRunner::new(config);
    let result = runner.run();
    result.print_summary();
}
