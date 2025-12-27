// Peer Lifecycle Simulator Configuration

use ec_rust::ec_peers::PeerManagerConfig;
use ec_rust::ec_interface::PeerId;

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

    /// Token distribution configuration
    pub token_distribution: TokenDistributionConfig,

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

/// Topology modes for initial peer discovery and knowledge
#[derive(Debug, Clone)]
pub enum TopologyMode {
    /// All peers know all other peers (100% knowledge)
    /// connected_fraction controls how many become Connected vs just Identified
    FullyKnown { connected_fraction: f64 },

    /// Peers know neighbors within view_width based on neighbor_overlap
    /// peer_knowledge_fraction controls what % of nearby peers they know (0.0-1.0)
    /// connected_fraction of known peers start as Connected
    LocalKnowledge {
        peer_knowledge_fraction: f64,  // What % of nearby peers are known
        connected_fraction: f64,       // What % of known peers are Connected
    },

    /// Ring topology with N neighbors on each side, all Connected
    Ring { neighbors: usize },

    /// No initial connections (peers must discover via elections)
    Isolated,
}

// ============================================================================
// Token Distribution
// ============================================================================

/// Configuration for token distribution
///
/// Uses a global token mapping with per-peer views based on:
/// - neighbor_overlap: How many neighbors on each side should overlap (determines view_width)
/// - coverage_fraction: Quality parameter - fraction of tokens within range that peer knows (0.0-1.0)
#[derive(Debug, Clone)]
pub struct TokenDistributionConfig {
    /// Total number of tokens in the global mapping (excluding peer IDs)
    pub total_tokens: usize,

    /// How many neighbors on each side should peers overlap with (±neighbors)
    /// This determines view_width to ensure sufficient overlap for elections
    pub neighbor_overlap: usize,

    /// Fraction of tokens within view_width that peer knows (0.0-1.0)
    /// This is the "quality" parameter
    pub coverage_fraction: f64,
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
        coverage_fraction: f64, // Quality of new peers (0.0-1.0)
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
            token_distribution: TokenDistributionConfig::default(),
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
            initial_topology: TopologyMode::LocalKnowledge {
                peer_knowledge_fraction: 0.2,  // Know 20% of nearby peers
                connected_fraction: 0.3,        // 30% of known peers are Connected
            },
            bootstrap_rounds: 50,
        }
    }
}

impl Default for TokenDistributionConfig {
    fn default() -> Self {
        Self {
            total_tokens: 10_000,
            neighbor_overlap: 5,  // Overlap with 5 neighbors on each side
            coverage_fraction: 0.8,  // Know 80% of nearby tokens
        }
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
