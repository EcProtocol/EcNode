#[allow(dead_code, unused_imports)]
mod integrated;
#[allow(dead_code, unused_imports)]
mod peer_lifecycle;

use std::env;

use ec_rust::ec_peers::AdaptiveNeighborhoodConfig;

use integrated::{
    ConflictWorkloadConfig, IntegratedRunner, IntegratedSimConfig, NetworkConfig,
    TransactionFlowConfig, TransactionSourcePolicy,
};
use peer_lifecycle::{
    InitialNetworkState, NetworkEvent, ScheduledEvent, TokenDistributionConfig, TopologyMode,
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
        0x45, 0x63, 0x68, 0x6f, 0x2d, 0x53, 0x74, 0x65, 0x61, 0x64, 0x79, 0x2d, 0x53, 0x74, 0x61,
        0x74, 0x65, 0x2d, 0x30, 0x31, 0x2d, 0x42, 0x61, 0x73, 0x65, 0x6c, 0x69, 0x6e, 0x65, 0x2d,
        0x58, 0x59,
    ];

    for (idx, byte) in variant.to_le_bytes().iter().enumerate() {
        seed[24 + idx] ^= *byte;
    }

    seed
}

fn main() {
    let seed_variant = env_u64("EC_STEADY_STATE_SEED_VARIANT", 0);
    let rounds = env_usize("EC_STEADY_STATE_ROUNDS", 800);
    let initial_peers = env_usize("EC_STEADY_STATE_INITIAL_PEERS", 192);
    let total_tokens = env_usize("EC_STEADY_STATE_TOTAL_TOKENS", 250_000);
    let network_profile = env_string("EC_STEADY_STATE_NETWORK_PROFILE", "cross_dc_normal");
    let topology = env_string("EC_STEADY_STATE_TOPOLOGY", "ring");
    let ring_neighbors = env_usize("EC_STEADY_STATE_RING_NEIGHBORS", 8);
    let ring_tail_peers_per_side = env_usize("EC_STEADY_STATE_RING_TAIL_PEERS_PER_SIDE", 4);
    let ring_linear_center_prob = env_f64("EC_STEADY_STATE_LINEAR_CENTER_PROB", 1.0);
    let ring_linear_far_prob = env_f64("EC_STEADY_STATE_LINEAR_FAR_PROB", 0.2);
    let ring_linear_guaranteed_neighbors =
        env_usize("EC_STEADY_STATE_LINEAR_GUARANTEED_NEIGHBORS", 0);
    let neighborhood_width = env_usize("EC_STEADY_STATE_NEIGHBORHOOD_WIDTH", 6);
    let vote_target_count = env_usize("EC_STEADY_STATE_VOTE_TARGETS", 2);
    let vote_active_rounds =
        env_usize("EC_STEADY_STATE_VOTE_ACTIVE_ROUNDS", 4).min(u8::MAX as usize) as u8;
    let vote_pairs_per_tick =
        env_usize("EC_STEADY_STATE_VOTE_PAIRS_PER_TICK", 1).min(u8::MAX as usize) as u8;
    let adaptive_far_width = env_usize("EC_STEADY_STATE_ADAPTIVE_FAR_WIDTH", 0);
    let adaptive_hop_threshold = env_usize("EC_STEADY_STATE_ADAPTIVE_HOP_THRESHOLD", 0);
    let blocks_per_round = env_usize("EC_STEADY_STATE_BLOCKS_PER_ROUND", 3);
    let existing_token_fraction = env_f64("EC_STEADY_STATE_EXISTING_TOKEN_FRACTION", 0.5);
    let conflict_family_fraction = env_f64("EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION", 0.0);
    let conflict_contenders = env_usize("EC_STEADY_STATE_CONFLICT_CONTENDERS", 2);
    let enable_batching = env_bool("EC_STEADY_STATE_BATCHING", true);
    let batch_vote_replies = env_bool("EC_STEADY_STATE_BATCH_VOTE_REPLIES", false);
    let vote_balance_threshold = env_string("EC_STEADY_STATE_VOTE_BALANCE_THRESHOLD", "2")
        .parse::<i64>()
        .unwrap_or(2);
    let elections_per_tick = env_usize("EC_STEADY_STATE_ELECTIONS_PER_TICK", 0);
    let prune_protection_time = env_u64("EC_STEADY_STATE_PRUNE_PROTECTION_TIME", 600);
    let connected_target = env_usize("EC_STEADY_STATE_CONNECTED_TARGET", 0);
    let connected_hysteresis = env_usize("EC_STEADY_STATE_CONNECTED_HYSTERESIS", 0);
    let elections_when_over_target =
        env_usize("EC_STEADY_STATE_ELECTIONS_WHEN_OVER_TARGET", usize::MAX);
    let connection_timeout = env_u64("EC_STEADY_STATE_CONNECTION_TIMEOUT", rounds as u64 + 10_000);

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  Integrated Steady-State Simulator                    ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!("Runs a fixed connected population without joins or churn.");
    println!("Seed variant: {}", seed_variant);
    println!("Initial peers: {}", initial_peers);
    println!("Network profile: {}", network_profile);
    println!("Topology: {}", topology);
    if topology == "ring" {
        println!("Guaranteed ring neighbors on each side: {}", ring_neighbors);
        println!("Ring tail: linear fade to zero by ±{}", ring_neighbors * 2);
    } else if topology == "ring_core_tail" {
        println!("Guaranteed ring neighbors on each side: {}", ring_neighbors);
        println!(
            "Ring fade tail: linear fade to zero by ±{}",
            ring_neighbors * 2
        );
        println!(
            "Long-range tail peers per side: {} (evenly spaced beyond the fade band)",
            ring_tail_peers_per_side
        );
    } else if topology == "ring_probabilistic" {
        println!("Ring topology: pairwise probabilistic closeness on the 64-bit ring");
    } else if topology == "ring_linear_probability" {
        println!(
            "Ring topology: full-ring linear probability, center {:.2}, far {:.2}, guaranteed ±{}",
            ring_linear_center_prob, ring_linear_far_prob, ring_linear_guaranteed_neighbors
        );
    }
    println!("Neighborhood width: {}", neighborhood_width);
    println!("Vote targets per request: {}", vote_target_count);
    println!(
        "Vote request pattern: deterministic outward pairs, {} pair slots/tick, {} active slots with one pause between each",
        vote_pairs_per_tick,
        vote_active_rounds
    );
    println!("Vote balance threshold: {}", vote_balance_threshold);
    println!("Elections per tick: {}", elections_per_tick);
    println!("Prune protection time: {}", prune_protection_time);
    println!("Connection timeout: {}", connection_timeout);
    println!(
        "Batching: {}, vote replies: {}",
        if enable_batching { "on" } else { "off" },
        if batch_vote_replies {
            "batched"
        } else {
            "standalone"
        }
    );
    println!(
        "Existing-token workload target: {:.0}%",
        existing_token_fraction * 100.0
    );
    println!(
        "Conflict families: {:.0}% of slots, {} contenders",
        conflict_family_fraction * 100.0,
        conflict_contenders,
    );
    if adaptive_far_width > 0 {
        println!(
            "Adaptive far width: {} beyond {} hops",
            adaptive_far_width, adaptive_hop_threshold
        );
    }
    if connected_target > 0 {
        let elections_label = if elections_when_over_target == usize::MAX {
            "default".to_string()
        } else {
            elections_when_over_target.to_string()
        };
        println!(
            "Connected target band: {} ± {}, elections above high band: {}",
            connected_target, connected_hysteresis, elections_label
        );
    }

    let mut config = IntegratedSimConfig::default();
    config.seed = Some(fixed_seed(seed_variant));
    config.rounds = rounds;
    config.initial_state = InitialNetworkState {
        num_peers: initial_peers,
        initial_topology: match topology.as_str() {
            "fully_known" => TopologyMode::FullyKnown {
                connected_fraction: 1.0,
            },
            "ring_probabilistic" => TopologyMode::RingProbabilistic,
            "ring_linear_probability" => TopologyMode::RingLinearProbability {
                center_prob: ring_linear_center_prob,
                far_prob: ring_linear_far_prob,
                guaranteed_neighbors: ring_linear_guaranteed_neighbors,
            },
            "ring_core_tail" => TopologyMode::RingCoreTail {
                neighbors: ring_neighbors,
                tail_peers_per_side: ring_tail_peers_per_side,
            },
            _ => TopologyMode::Ring {
                neighbors: ring_neighbors,
            },
        },
        bootstrap_rounds: 0,
    };
    config.token_distribution = TokenDistributionConfig {
        total_tokens,
        neighbor_overlap: 8,
        coverage_fraction: 0.90,
        genesis_config: None,
        genesis_storage_fraction: 0.25,
    };
    config.peer_config.neighborhood_width = neighborhood_width;
    config.peer_config.vote_target_count = vote_target_count;
    config.peer_config.vote_request_active_rounds = vote_active_rounds;
    config.peer_config.vote_request_pairs_per_tick = vote_pairs_per_tick;
    config.peer_config.vote_balance_threshold = vote_balance_threshold;
    config.peer_config.elections_per_tick = elections_per_tick;
    config.peer_config.prune_protection_time = prune_protection_time;
    config.peer_config.connection_timeout = connection_timeout;
    config.peer_config.enable_request_batching = enable_batching;
    config.peer_config.batch_vote_replies = batch_vote_replies;
    if connected_target > 0 {
        config.peer_config.connected_target = Some(connected_target);
        config.peer_config.connected_target_hysteresis = connected_hysteresis;
        if elections_when_over_target != usize::MAX {
            config.peer_config.elections_per_tick_above_target = Some(elections_when_over_target);
        }
    }
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
        blocks_per_round,
        block_size_range: (1, 3),
        source_policy: TransactionSourcePolicy::ConnectedOnly,
        existing_token_fraction,
        conflicts: ConflictWorkloadConfig {
            family_fraction: conflict_family_fraction,
            contenders: conflict_contenders,
        },
    };

    let report_a = rounds / 4;
    let report_b = rounds / 2;
    let report_c = rounds.saturating_sub(rounds / 5);
    config.events.events = vec![
        ScheduledEvent {
            round: report_a,
            event: NetworkEvent::ReportStats {
                label: Some("early-steady-state".to_string()),
            },
        },
        ScheduledEvent {
            round: report_b,
            event: NetworkEvent::ReportStats {
                label: Some("mid-steady-state".to_string()),
            },
        },
        ScheduledEvent {
            round: report_c,
            event: NetworkEvent::ReportStats {
                label: Some("late-steady-state".to_string()),
            },
        },
    ];

    let runner = IntegratedRunner::new(config);
    let result = runner.run();
    result.print_summary();
}
