#[allow(dead_code, unused_imports)]
mod integrated;
#[allow(dead_code, unused_imports)]
mod peer_lifecycle;

use std::env;

use ec_rust::ec_genesis::GenesisConfig;
use ec_rust::ec_peers::AdaptiveNeighborhoodConfig;
use ec_rust::ec_peers::PeerShapeTargetConfig;
use ec_rust::ec_peers::PeerSmallWorldConfig;

use integrated::{
    ConflictWorkloadConfig, IntegratedRunner, IntegratedSimConfig, NetworkConfig,
    PeerIdLocationPatternConfig, TransactionEntryLocationConfig, TransactionFlowConfig,
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

fn env_location_pattern(name: &str) -> Option<Vec<u64>> {
    let value = env::var(name).ok()?;
    let locations = value
        .split(',')
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                None
            } else if let Some(hex) = trimmed.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).ok()
            } else {
                trimmed.parse::<u64>().ok()
            }
        })
        .collect::<Vec<_>>();

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

fn fixed_seed(variant: u64) -> [u8; 32] {
    let mut seed = [
        0x45, 0x63, 0x68, 0x6f, 0x2d, 0x43, 0x6f, 0x6e, 0x73, 0x65, 0x6e, 0x74, 0x2d, 0x50, 0x4f,
        0x43, 0x2d, 0x4c, 0x6f, 0x6e, 0x67, 0x2d, 0x52, 0x75, 0x6e, 0x2d, 0x30, 0x31, 0x2d, 0x58,
        0x59, 0x5a,
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
    let initial_topology = env_string("EC_LONG_RUN_INITIAL_TOPOLOGY", "random_identified");
    let initial_peers_per_node = env_usize("EC_LONG_RUN_INITIAL_PEERS_PER_NODE", 6);
    let peer_id_location_pattern = env_location_pattern("EC_LONG_RUN_PEER_ID_LOCATION_PATTERN");
    let peer_id_location_bits = env_usize("EC_LONG_RUN_PEER_ID_LOCATION_BITS", 16).min(63) as u8;
    let linear_center_prob = env_f64("EC_LONG_RUN_LINEAR_CENTER_PROB", 1.0);
    let linear_far_prob = env_f64("EC_LONG_RUN_LINEAR_FAR_PROB", 0.2);
    let linear_guaranteed_neighbors = env_usize("EC_LONG_RUN_LINEAR_GUARANTEED_NEIGHBORS", 10);
    let location_topology_bits = env_usize("EC_LONG_RUN_LOCATION_TOPOLOGY_BITS", 16).min(63) as u8;
    let location_topology_center_prob = env_f64("EC_LONG_RUN_LOCATION_TOPOLOGY_CENTER_PROB", 1.0);
    let location_topology_far_prob = env_f64("EC_LONG_RUN_LOCATION_TOPOLOGY_FAR_PROB", 0.05);
    let join_count = env_usize("EC_LONG_RUN_JOIN_COUNT", 24);
    let crash_count = env_usize("EC_LONG_RUN_CRASH_COUNT", 12);
    let return_count = env_usize("EC_LONG_RUN_RETURN_COUNT", 8);
    let second_join_count = env_usize("EC_LONG_RUN_SECOND_JOIN_COUNT", 16);
    let second_crash_count = env_usize("EC_LONG_RUN_SECOND_CRASH_COUNT", 10);
    let genesis_blocks = env_usize("EC_LONG_RUN_GENESIS_BLOCKS", 50_000);
    let network_profile = env_string("EC_LONG_RUN_NETWORK_PROFILE", "cross_dc_normal");
    let neighborhood_width = env_usize("EC_LONG_RUN_NEIGHBORHOOD_WIDTH", 4);
    let vote_target_count = env_usize("EC_LONG_RUN_VOTE_TARGETS", 2);
    let elections_per_tick = env_usize("EC_LONG_RUN_ELECTIONS_PER_TICK", 3);
    let election_timeout = env_usize("EC_LONG_RUN_ELECTION_TIMEOUT", 30) as u64;
    let min_collection_time = env_usize("EC_LONG_RUN_MIN_COLLECTION_TIME", 10) as u64;
    let consensus_threshold = env_usize("EC_LONG_RUN_CONSENSUS_THRESHOLD", 8);
    let majority_threshold = env_f64("EC_LONG_RUN_MAJORITY_THRESHOLD", 0.6);
    let random_discovery_elections = env_usize("EC_LONG_RUN_RANDOM_DISCOVERY_ELECTIONS", 0);
    let random_discovery_until = env_u64("EC_LONG_RUN_RANDOM_DISCOVERY_UNTIL", 0);
    let peer_id_election_only = env_bool("EC_LONG_RUN_PEER_ID_ELECTION_ONLY", false);
    let referral_probes_per_tick = env_usize("EC_LONG_RUN_REFERRAL_PROBES_PER_TICK", 0);
    let referral_probe_hops = env_usize("EC_LONG_RUN_REFERRAL_PROBE_HOPS", 5);
    let local_discovery_target = env_usize("EC_LONG_RUN_LOCAL_DISCOVERY_TARGET", 100);
    let adaptive_far_width = env_usize("EC_LONG_RUN_ADAPTIVE_FAR_WIDTH", 0);
    let adaptive_hop_threshold = env_usize("EC_LONG_RUN_ADAPTIVE_HOP_THRESHOLD", 0);
    let transaction_start_round = env_usize("EC_LONG_RUN_TRANSACTION_START_ROUND", 0);
    let entry_locations = env_usize("EC_LONG_RUN_ENTRY_LOCATIONS", 0);
    let entry_location_bits = env_usize("EC_LONG_RUN_ENTRY_LOCATION_BITS", 32) as u8;
    let entry_location_width = env_f64("EC_LONG_RUN_ENTRY_LOCATION_WIDTH", 0.05);
    let blocks_per_round = env_usize("EC_LONG_RUN_BLOCKS_PER_ROUND", 3);
    let block_size_min = env_usize("EC_LONG_RUN_BLOCK_SIZE_MIN", 1);
    let block_size_max = env_usize("EC_LONG_RUN_BLOCK_SIZE_MAX", 3);
    let existing_token_fraction = env_f64("EC_LONG_RUN_EXISTING_TOKEN_FRACTION", 0.0);
    let conflict_family_fraction = env_f64("EC_LONG_RUN_CONFLICT_FAMILY_FRACTION", 0.0);
    let conflict_contenders = env_usize("EC_LONG_RUN_CONFLICT_CONTENDERS", 2);
    let enable_batching = env_bool("EC_LONG_RUN_BATCHING", true);
    let batch_vote_replies = env_bool("EC_LONG_RUN_BATCH_VOTE_REPLIES", false);
    let prune_protection_time = env_u64("EC_LONG_RUN_PRUNE_PROTECTION_TIME", 600);
    let connected_target = env_usize("EC_LONG_RUN_CONNECTED_TARGET", 0);
    let connected_hysteresis = env_usize("EC_LONG_RUN_CONNECTED_HYSTERESIS", 0);
    let elections_when_over_target =
        env_usize("EC_LONG_RUN_ELECTIONS_WHEN_OVER_TARGET", usize::MAX);
    let enable_dense_shape_target = env_bool("EC_LONG_RUN_DENSE_SHAPE_TARGET", false);
    let dense_shape_neighbors = env_usize("EC_LONG_RUN_DENSE_SHAPE_NEIGHBORS", 10);
    let dense_shape_far_prob = env_f64("EC_LONG_RUN_DENSE_SHAPE_FAR_PROB", 0.2);
    let dense_shape_hysteresis = env_usize("EC_LONG_RUN_DENSE_SHAPE_HYSTERESIS", 4);
    let enable_small_world = env_bool("EC_LONG_RUN_SMALL_WORLD", false);
    let small_world_budget = env_usize("EC_LONG_RUN_SMALL_WORLD_BUDGET", 0);
    let small_world_hysteresis = env_usize("EC_LONG_RUN_SMALL_WORLD_HYSTERESIS", 16);
    let small_world_location_bits = env_usize("EC_LONG_RUN_SMALL_WORLD_LOCATION_BITS", 32) as u8;
    let small_world_cell_bits = env_usize("EC_LONG_RUN_SMALL_WORLD_CELL_BITS", 0) as u8;
    let small_world_remote_cell_target = env_usize("EC_LONG_RUN_SMALL_WORLD_REMOTE_CELL_TARGET", 0);
    let small_world_min_local_fraction =
        env_f64("EC_LONG_RUN_SMALL_WORLD_MIN_LOCAL_FRACTION", 0.80);
    let small_world_far_fraction = env_f64("EC_LONG_RUN_SMALL_WORLD_FAR_FRACTION", 0.10);
    let small_world_far_distance = env_f64("EC_LONG_RUN_SMALL_WORLD_FAR_DISTANCE", 0.25);
    let small_world_distance_exponent = env_f64("EC_LONG_RUN_SMALL_WORLD_DISTANCE_EXPONENT", 2.0);
    let focus_first_crash = env_bool("EC_LONG_RUN_FOCUS_FIRST_CRASH", false);
    let focus_report_delta = env_usize("EC_LONG_RUN_FOCUS_REPORT_DELTA", 12);
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  Integrated Long-Run Simulator                        ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!("Runs a longer genesis-backed lifecycle scenario with fixed seed.");
    println!("Seed variant: {}", seed_variant);
    println!("Network profile: {}", network_profile);
    if let Some(locations) = &peer_id_location_pattern {
        let labels = locations
            .iter()
            .map(|location| format!("0x{:x}", location))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "Synthetic peer-id locations: {} low bits, pattern {}",
            peer_id_location_bits, labels
        );
    }
    println!("Initial topology: {}", initial_topology);
    if initial_topology == "random_identified" {
        println!(
            "Initial random identified peers/node: {}",
            initial_peers_per_node
        );
    } else if initial_topology == "ring_linear_probability" {
        println!(
            "Initial ring-linear topology: center {:.2}, far {:.2}, guaranteed ±{}",
            linear_center_prob, linear_far_prob, linear_guaranteed_neighbors
        );
    } else if initial_topology == "location_linear_probability" {
        println!(
            "Initial location-linear topology: {} low bits, center {:.2}, far {:.2}",
            location_topology_bits, location_topology_center_prob, location_topology_far_prob
        );
    }
    println!("Neighborhood width: {}", neighborhood_width);
    println!("Vote targets per request: {}", vote_target_count);
    println!("Peer elections per tick: {}", elections_per_tick);
    println!(
        "Peer election threshold: {} answers @ {:.2} majority, timeout {}, min collection {}",
        consensus_threshold, majority_threshold, election_timeout, min_collection_time
    );
    if random_discovery_elections > 0 {
        let until_label = if random_discovery_until == 0 {
            "always".to_string()
        } else {
            format!("until round {}", random_discovery_until)
        };
        println!(
            "Random discovery elections per tick: {} ({})",
            random_discovery_elections, until_label
        );
    }
    if peer_id_election_only {
        println!(
            "Peer-ID election-only discovery: {} referral probes/tick, {} hops, local target {}",
            referral_probes_per_tick, referral_probe_hops, local_discovery_target
        );
    }
    println!("Vote request pattern: deterministic outward pairs, 4 rounds on / 1 round skip");
    println!("Prune protection time: {}", prune_protection_time);
    if transaction_start_round > 0 {
        println!("Transaction start round: {}", transaction_start_round);
    }
    if entry_locations > 0 {
        println!(
            "Transaction entry locations: {} cells, {} location bits, {:.1}% cell width",
            entry_locations,
            entry_location_bits,
            entry_location_width * 100.0
        );
    }
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
    if conflict_family_fraction > 0.0 {
        println!(
            "Conflict families: {:.0}% of slots, {} contenders",
            conflict_family_fraction * 100.0,
            conflict_contenders
        );
    }
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
    if enable_dense_shape_target {
        println!(
            "Dense shape target: core ±{}, far probability {:.2}, hysteresis {}",
            dense_shape_neighbors, dense_shape_far_prob, dense_shape_hysteresis
        );
    }
    if enable_small_world {
        println!(
            "Small-world target: budget {}, hysteresis {}, location bits {}, cell bits {}, remote target {}, local min {:.0}%, far {:.1}% beyond {:.2}, exponent {:.2}",
            small_world_budget,
            small_world_hysteresis,
            small_world_location_bits,
            small_world_cell_bits,
            small_world_remote_cell_target,
            small_world_min_local_fraction * 100.0,
            small_world_far_fraction * 100.0,
            small_world_far_distance,
            small_world_distance_exponent
        );
    }

    let mut config = IntegratedSimConfig::default();
    config.seed = Some(fixed_seed(seed_variant));
    config.rounds = rounds;
    config.peer_id_location_pattern =
        peer_id_location_pattern.map(|locations| PeerIdLocationPatternConfig {
            location_bits: peer_id_location_bits,
            locations,
        });
    config.initial_state = InitialNetworkState {
        num_peers: initial_peers,
        initial_topology: match initial_topology.as_str() {
            "fully_known" => TopologyMode::FullyKnown {
                connected_fraction: 1.0,
            },
            "ring_linear_probability" => TopologyMode::RingLinearProbability {
                center_prob: linear_center_prob,
                far_prob: linear_far_prob,
                guaranteed_neighbors: linear_guaranteed_neighbors,
            },
            "location_linear_probability" => TopologyMode::LocationLinearProbability {
                location_bits: location_topology_bits,
                center_prob: location_topology_center_prob,
                far_prob: location_topology_far_prob,
            },
            _ => TopologyMode::RandomIdentified {
                peers_per_node: initial_peers_per_node,
            },
        },
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
    config.peer_config.elections_per_tick = elections_per_tick;
    config.peer_config.election_timeout = election_timeout;
    config.peer_config.min_collection_time = min_collection_time;
    config.peer_config.election_config.consensus_threshold = consensus_threshold;
    config.peer_config.election_config.majority_threshold = majority_threshold;
    config.peer_config.random_discovery_elections_per_tick = random_discovery_elections;
    if random_discovery_until > 0 {
        config.peer_config.random_discovery_until = Some(random_discovery_until);
    }
    config.peer_config.peer_id_election_only = peer_id_election_only;
    config.peer_config.referral_probes_per_tick = referral_probes_per_tick;
    config.peer_config.referral_probe_hops = referral_probe_hops;
    config.peer_config.local_discovery_target = local_discovery_target;
    config.peer_config.enable_request_batching = enable_batching;
    config.peer_config.batch_vote_replies = batch_vote_replies;
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
    if enable_small_world {
        config.peer_config.shape_target = None;
        config.peer_config.small_world = Some(PeerSmallWorldConfig {
            peer_budget: if small_world_budget > 0 {
                small_world_budget
            } else {
                initial_peers.saturating_sub(1)
            },
            hysteresis: small_world_hysteresis,
            location_bits: small_world_location_bits,
            cell_bits: small_world_cell_bits,
            remote_cell_target: small_world_remote_cell_target,
            min_local_fraction: small_world_min_local_fraction,
            far_fraction: small_world_far_fraction,
            far_distance_fraction: small_world_far_distance,
            distance_exponent: small_world_distance_exponent,
        });
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
        start_round: transaction_start_round,
        blocks_per_round,
        block_size_range: (block_size_min, block_size_max.max(block_size_min)),
        source_policy: TransactionSourcePolicy::ConnectedOnly,
        entry_locations: TransactionEntryLocationConfig {
            locations: entry_locations,
            location_bits: entry_location_bits,
            cell_width_fraction: entry_location_width,
        },
        existing_token_fraction,
        conflicts: ConflictWorkloadConfig {
            family_fraction: conflict_family_fraction,
            contenders: conflict_contenders,
        },
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
                selection: PeerSelection::Random {
                    count: return_count,
                },
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

    if focus_first_crash {
        let before_crash_round = crash_round.saturating_sub(focus_report_delta);
        let after_crash_round = (crash_round + focus_report_delta).min(rounds.saturating_sub(1));
        let after_return_round = (return_round + focus_report_delta).min(rounds.saturating_sub(1));
        config.events.events.extend([
            ScheduledEvent {
                round: before_crash_round,
                event: NetworkEvent::ReportStats {
                    label: Some("pre-crash-focus".to_string()),
                },
            },
            ScheduledEvent {
                round: after_crash_round,
                event: NetworkEvent::ReportStats {
                    label: Some("post-crash-focus".to_string()),
                },
            },
            ScheduledEvent {
                round: after_return_round,
                event: NetworkEvent::ReportStats {
                    label: Some("post-return-focus".to_string()),
                },
            },
        ]);
        config
            .events
            .events
            .sort_by_key(|scheduled| scheduled.round);
    }

    let runner = IntegratedRunner::new(config);
    let result = runner.run();
    result.print_summary();
}
