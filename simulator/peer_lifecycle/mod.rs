// Peer Lifecycle Simulator Module

pub mod config;
pub mod stats;
pub mod token_allocation;
pub mod sim_token_storage;
pub mod runner;
pub mod scenarios;

// Re-export commonly used types
pub use config::{
    PeerLifecycleConfig,
    InitialNetworkState,
    TokenDistributionConfig,
    EventSchedule,
    ScheduledEvent,
    NetworkEvent,
    PeerSelection,
    TopologyMode,
    BootstrapMethod,
};

pub use stats::{
    SimulationResult,
    RoundMetrics,
    ElectionStats,
    NetworkHealth,
};

pub use token_allocation::GlobalTokenMapping;
pub use runner::PeerLifecycleRunner;
pub use scenarios::ScenarioBuilder;
