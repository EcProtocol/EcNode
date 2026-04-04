#[allow(dead_code, unused_imports)]
mod integrated;
#[allow(dead_code, unused_imports)]
mod peer_lifecycle;

use ec_rust::ec_genesis::GenesisConfig;

use integrated::{
    IntegratedRunner, IntegratedSimConfig, NetworkConfig, TransactionFlowConfig,
    TransactionSourcePolicy,
};
use peer_lifecycle::{
    BootstrapMethod, InitialNetworkState, NetworkEvent, ScheduledEvent, TokenDistributionConfig,
    TopologyMode,
};

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  Integrated Genesis Simulator                         ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!("Runs the integrated simulator with genesis-backed initial state.");

    let mut config = IntegratedSimConfig::default();
    config.rounds = 160;
    config.initial_state = InitialNetworkState {
        num_peers: 18,
        initial_topology: TopologyMode::RandomIdentified { peers_per_node: 3 },
        bootstrap_rounds: 0,
    };
    config.token_distribution = TokenDistributionConfig {
        total_tokens: 0,
        neighbor_overlap: 6,
        coverage_fraction: 0.90,
        genesis_config: Some(GenesisConfig {
            block_count: 20_000,
            seed_string: "Integrated simulator genesis".to_string(),
        }),
        genesis_storage_fraction: 0.20,
    };
    config.network = NetworkConfig::cross_dc_normal();
    config.transactions = TransactionFlowConfig {
        blocks_per_round: 2,
        block_size_range: (1, 3),
        source_policy: TransactionSourcePolicy::ConnectedOnly,
    };
    config.events.events = vec![
        ScheduledEvent {
            round: 35,
            event: NetworkEvent::ReportStats {
                label: Some("genesis baseline".to_string()),
            },
        },
        ScheduledEvent {
            round: 55,
            event: NetworkEvent::PeerJoin {
                count: 4,
                coverage_fraction: 0.90,
                bootstrap_method: BootstrapMethod::Random(2),
                group_name: Some("cold-joiners".to_string()),
            },
        },
        ScheduledEvent {
            round: 90,
            event: NetworkEvent::PeerCrash {
                selection: peer_lifecycle::PeerSelection::Random { count: 3 },
            },
        },
        ScheduledEvent {
            round: 110,
            event: NetworkEvent::PeerReturn {
                selection: peer_lifecycle::PeerSelection::Random { count: 2 },
                bootstrap_method: BootstrapMethod::Random(2),
            },
        },
        ScheduledEvent {
            round: 130,
            event: NetworkEvent::ReportStats {
                label: Some("post-return".to_string()),
            },
        },
    ];

    let runner = IntegratedRunner::new(config);
    let result = runner.run();
    result.print_summary();
}
