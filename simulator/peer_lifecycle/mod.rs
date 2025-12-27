// Peer Lifecycle Simulator Module

pub mod config;
pub mod stats;
pub mod token_dist;
pub mod runner;

// Re-export commonly used types
pub use config::{
    PeerLifecycleConfig,
    InitialNetworkState,
    TokenDistributionConfig,
    EventSchedule,
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
