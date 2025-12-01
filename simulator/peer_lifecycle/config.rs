// Peer Lifecycle Simulator Configuration

use ec_rust::ec_peers::PeerManagerConfig;
use ec_rust::ec_interface::{PeerId, TokenId};
use std::collections::HashMap;

// ============================================================================
// Main Configuration
// ============================================================================

/// Main configuration for peer lifecycle simulation
#[derive(Debug, Clone)]
pub struct PeerLifecycleConfig {
    /// Total number of simulation rounds
    pub rounds: usize,

    /// Simulated time per tick (milliseconds)
    pub tick_duration_ms: u64,

    /// Random seed for reproducibility
    pub seed: Option<[u8; 32]>,

    /// Initial network state
    pub initial_state: InitialNetworkState,

    /// Token distribution strategy
    pub token_distribution: TokenDistribution,

    /// Scheduled network events
    pub events: EventSchedule,

    /// Peer manager configuration (from ec_peers)
    pub peer_config: PeerManagerConfig,

    /// Network simulation parameters
    pub network: NetworkConfig,

    /// Metrics tracking configuration
    pub metrics: MetricsConfig,

    /// Output configuration
    pub output: OutputConfig,
}

// ============================================================================
// Initial Network State
// ============================================================================

/// Configuration for initial network topology
#[derive(Debug, Clone)]
pub struct InitialNetworkState {
    /// Number of peers to create initially
    pub num_peers: usize,

    /// How peers initially know each other
    pub initial_topology: TopologyMode,

    /// Number of rounds to stabilize before events start
    pub bootstrap_rounds: usize,
}

/// Topology modes for initial peer discovery
#[derive(Debug, Clone)]
pub enum TopologyMode {
    /// All peers know all other peers
    FullyConnected,

    /// Random connections with specified connectivity (0.0 to 1.0)
    Random { connectivity: f64 },

    /// Ring topology with N neighbors on each side
    Ring { neighbors: usize },

    /// No initial connections (peers must discover via elections)
    Isolated,
}

// ============================================================================
// Token Distribution
// ============================================================================

/// Token distribution strategies
#[derive(Debug, Clone)]
pub enum TokenDistribution {
    /// Each peer owns N tokens uniformly distributed on ring
    Uniform {
        tokens_per_peer: usize,
    },

    /// Tokens clustered by ring proximity to peer IDs
    Clustered {
        tokens_per_peer: usize,
        cluster_radius: u64,
    },

    /// Completely random distribution
    Random {
        total_tokens: usize,
        min_per_peer: usize,
        max_per_peer: usize,
    },

    /// Weighted distribution (some peers have more tokens)
    Weighted {
        total_tokens: usize,
        distribution: WeightDistribution,
    },

    /// Custom: exact token assignments
    Custom(HashMap<PeerId, Vec<TokenId>>),
}

/// Weight distribution functions
#[derive(Debug, Clone)]
pub enum WeightDistribution {
    /// Power law distribution (scale-free network)
    PowerLaw { alpha: f64 },

    /// Exponential distribution
    Exponential { lambda: f64 },

    /// Normal (Gaussian) distribution
    Normal { mean: f64, stddev: f64 },
}

// ============================================================================
// Event Scheduling
// ============================================================================

/// Schedule of network events
#[derive(Debug, Clone)]
pub struct EventSchedule {
    pub events: Vec<ScheduledEvent>,
}

/// A single scheduled event
#[derive(Debug, Clone)]
pub struct ScheduledEvent {
    /// Round number when event triggers
    pub round: usize,

    /// The event to trigger
    pub event: NetworkEvent,
}

/// Types of network events
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// Add new peers to the network
    PeerJoin {
        count: usize,
        tokens: TokenDistribution,
        initial_knowledge: Vec<PeerId>, // Bootstrap peers they know
    },

    /// Gracefully remove peers (they send goodbye messages)
    PeerLeave {
        selection: PeerSelection,
    },

    /// Suddenly remove peers (no cleanup, simulates crashes)
    PeerCrash {
        selection: PeerSelection,
    },

    /// Change network conditions
    NetworkCondition {
        delay_fraction: Option<f64>,
        loss_fraction: Option<f64>,
    },

    /// Pause elections for N rounds (test recovery)
    PauseElections {
        duration: usize,
    },
}

/// Methods for selecting which peers to affect
#[derive(Debug, Clone)]
pub enum PeerSelection {
    /// Random selection
    Random { count: usize },

    /// Specific peer IDs
    Specific { peer_ids: Vec<PeerId> },

    /// By quality score (worst or best)
    ByQuality { count: usize, worst: bool },

    /// By token count (richest or poorest)
    ByTokenCount { count: usize, most: bool },
}

// ============================================================================
// Network Configuration
// ============================================================================

/// Network behavior simulation
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Fraction of messages delayed to next round (0.0 to 1.0)
    pub delay_fraction: f64,

    /// Fraction of messages dropped (0.0 to 1.0)
    pub loss_fraction: f64,
}

// ============================================================================
// Metrics Configuration
// ============================================================================

/// Configuration for metrics tracking
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// Track peer state distribution
    pub track_peer_states: bool,

    /// Track election statistics
    pub track_elections: bool,

    /// Track quality scores
    pub track_quality_scores: bool,

    /// Track ring coverage
    pub track_ring_coverage: bool,

    /// Track convergence time
    pub track_convergence_time: bool,

    /// How often to sample metrics (every N rounds)
    pub sample_interval: usize,

    /// Track individual peer snapshots
    pub peer_snapshots: bool,
}

// ============================================================================
// Output Configuration
// ============================================================================

/// Configuration for output and logging
#[derive(Debug, Clone)]
pub struct OutputConfig {
    /// Enable console event logging
    pub enable_console: bool,

    /// CSV output file path
    pub csv_path: Option<String>,

    /// Verbose logging
    pub verbose: bool,
}

// ============================================================================
// Default Implementations
// ============================================================================

impl Default for PeerLifecycleConfig {
    fn default() -> Self {
        Self {
            rounds: 500,
            tick_duration_ms: 100,
            seed: None,
            initial_state: InitialNetworkState::default(),
            token_distribution: TokenDistribution::default(),
            events: EventSchedule::default(),
            peer_config: PeerManagerConfig::default(),
            network: NetworkConfig::default(),
            metrics: MetricsConfig::default(),
            output: OutputConfig::default(),
        }
    }
}

impl Default for InitialNetworkState {
    fn default() -> Self {
        Self {
            num_peers: 50,
            initial_topology: TopologyMode::Random { connectivity: 0.3 },
            bootstrap_rounds: 50,
        }
    }
}

impl Default for TokenDistribution {
    fn default() -> Self {
        Self::Uniform { tokens_per_peer: 10 }
    }
}

impl Default for EventSchedule {
    fn default() -> Self {
        Self { events: Vec::new() }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            delay_fraction: 0.3,
            loss_fraction: 0.01,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            track_peer_states: true,
            track_elections: true,
            track_quality_scores: true,
            track_ring_coverage: true,
            track_convergence_time: true,
            sample_interval: 10,
            peer_snapshots: false,
        }
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            enable_console: false,
            csv_path: None,
            verbose: false,
        }
    }
}
