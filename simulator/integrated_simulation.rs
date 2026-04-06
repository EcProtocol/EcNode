#[allow(dead_code, unused_imports)]
mod integrated;
#[allow(dead_code, unused_imports)]
mod peer_lifecycle;

use integrated::{
    ConflictWorkloadConfig, IntegratedRunner, IntegratedSimConfig, NetworkConfig, TransactionFlowConfig,
    TransactionSourcePolicy,
};
use peer_lifecycle::{
    BootstrapMethod, InitialNetworkState, NetworkEvent, ScheduledEvent, TokenDistributionConfig,
    TopologyMode,
};

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  Integrated Simulator                                 ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!("Runs full nodes with peer discovery, transaction flow, and lifecycle events.");

    let mut config = IntegratedSimConfig::default();
    config.rounds = 180;
    config.initial_state = InitialNetworkState {
        num_peers: 24,
        initial_topology: TopologyMode::RandomIdentified {
            peers_per_node: 4,
        },
        bootstrap_rounds: 0,
    };
    config.token_distribution = TokenDistributionConfig {
        total_tokens: 100_000,
        neighbor_overlap: 8,
        coverage_fraction: 0.90,
        genesis_config: None,
        genesis_storage_fraction: 0.25,
    };
    config.network = NetworkConfig::cross_dc_normal();
    config.transactions = TransactionFlowConfig {
        blocks_per_round: 2,
        block_size_range: (1, 3),
        source_policy: TransactionSourcePolicy::ConnectedOnly,
        existing_token_fraction: 0.0,
        conflicts: ConflictWorkloadConfig::default(),
    };
    config.events.events = vec![
        ScheduledEvent {
            round: 40,
            event: NetworkEvent::ReportStats {
                label: Some("baseline".to_string()),
            },
        },
        ScheduledEvent {
            round: 60,
            event: NetworkEvent::PeerJoin {
                count: 6,
                coverage_fraction: 0.85,
                bootstrap_method: BootstrapMethod::Random(3),
                group_name: Some("late-joiners".to_string()),
            },
        },
        ScheduledEvent {
            round: 90,
            event: NetworkEvent::ReportStats {
                label: Some("after join".to_string()),
            },
        },
        ScheduledEvent {
            round: 110,
            event: NetworkEvent::PeerCrash {
                selection: peer_lifecycle::PeerSelection::Random { count: 5 },
            },
        },
        ScheduledEvent {
            round: 130,
            event: NetworkEvent::NetworkCondition {
                delay_fraction: Some(0.45),
                loss_fraction: Some(0.03),
            },
        },
        ScheduledEvent {
            round: 145,
            event: NetworkEvent::PeerReturn {
                selection: peer_lifecycle::PeerSelection::Random { count: 3 },
                bootstrap_method: BootstrapMethod::Random(2),
            },
        },
        ScheduledEvent {
            round: 150,
            event: NetworkEvent::ReportStats {
                label: Some("after churn".to_string()),
            },
        },
    ];

    let runner = IntegratedRunner::new(config);
    let result = runner.run();
    result.print_summary();
}
