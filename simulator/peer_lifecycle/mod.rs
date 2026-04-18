// Peer Lifecycle Simulator Module

pub mod config;
pub mod runner;
pub mod scenarios;
pub mod stats;
pub mod token_allocation;
pub mod topology;

// Re-export commonly used types for public API
#[allow(unused_imports)] // Re-exports for external consumers
pub use config::{
    BootstrapMethod, EventSchedule, InitialNetworkState, NetworkEvent, PeerLifecycleConfig,
    PeerSelection, ScheduledEvent, TokenDistributionConfig, TopologyMode,
};

#[allow(unused_imports)] // Re-exports for external consumers
pub use stats::{ElectionStats, NetworkHealth, RoundMetrics, SimulationResult};

pub use runner::PeerLifecycleRunner;
pub use scenarios::ScenarioBuilder;
#[allow(unused_imports)] // Re-exports for external consumers
pub use token_allocation::GlobalTokenMapping;
