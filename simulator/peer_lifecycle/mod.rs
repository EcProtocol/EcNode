// Peer Lifecycle Simulator Module

pub mod config;
pub mod stats;
pub mod token_dist;
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
};

pub use stats::{
    SimulationResult,
    RoundMetrics,
    ElectionStats,
    NetworkHealth,
};

pub use token_dist::GlobalTokenMapping;
pub use runner::PeerLifecycleRunner;
pub use scenarios::{ScenarioBuilder, BootstrapMethod};
