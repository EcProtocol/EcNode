// Peer Lifecycle Simulator Module

pub mod config;
pub mod stats;
pub mod token_allocation;
pub mod runner;
pub mod scenarios;

// Re-export commonly used types for public API
#[allow(unused_imports)] // Re-exports for external consumers
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

#[allow(unused_imports)] // Re-exports for external consumers
pub use stats::{
    SimulationResult,
    RoundMetrics,
    ElectionStats,
    NetworkHealth,
};

#[allow(unused_imports)] // Re-exports for external consumers
pub use token_allocation::GlobalTokenMapping;
pub use runner::PeerLifecycleRunner;
pub use scenarios::ScenarioBuilder;
