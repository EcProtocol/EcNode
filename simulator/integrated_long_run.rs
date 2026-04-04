#[allow(dead_code, unused_imports)]
mod integrated;
#[allow(dead_code, unused_imports)]
mod peer_lifecycle;

use std::env;

use ec_rust::ec_genesis::GenesisConfig;

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

fn fixed_seed() -> [u8; 32] {
    [
        0x45, 0x63, 0x68, 0x6f, 0x2d, 0x43, 0x6f, 0x6e, 0x73, 0x65, 0x6e, 0x74, 0x2d, 0x50,
        0x4f, 0x43, 0x2d, 0x4c, 0x6f, 0x6e, 0x67, 0x2d, 0x52, 0x75, 0x6e, 0x2d, 0x30, 0x31,
        0x2d, 0x58, 0x59, 0x5a,
    ]
}

fn main() {
    let rounds = env_usize("EC_LONG_RUN_ROUNDS", 2400);
    let initial_peers = env_usize("EC_LONG_RUN_INITIAL_PEERS", 96);
    let join_count = env_usize("EC_LONG_RUN_JOIN_COUNT", 24);
    let crash_count = env_usize("EC_LONG_RUN_CRASH_COUNT", 12);
    let return_count = env_usize("EC_LONG_RUN_RETURN_COUNT", 8);
    let second_join_count = env_usize("EC_LONG_RUN_SECOND_JOIN_COUNT", 16);
    let second_crash_count = env_usize("EC_LONG_RUN_SECOND_CRASH_COUNT", 10);
    let genesis_blocks = env_usize("EC_LONG_RUN_GENESIS_BLOCKS", 50_000);

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  Integrated Long-Run Simulator                        ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!("Runs a longer genesis-backed lifecycle scenario with fixed seed.");

    let mut config = IntegratedSimConfig::default();
    config.seed = Some(fixed_seed());
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
    config.network = NetworkConfig::cross_dc_normal();
    config.transactions = TransactionFlowConfig {
        blocks_per_round: 3,
        block_size_range: (1, 3),
        source_policy: TransactionSourcePolicy::ConnectedOnly,
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
