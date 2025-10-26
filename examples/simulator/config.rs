// Simulation Configuration

/// Main simulation configuration
#[derive(Debug, Clone)]
pub struct SimConfig {
    pub rounds: usize,
    pub num_peers: usize,
    pub seed: Option<[u8; 32]>,
    pub network: NetworkConfig,
    pub topology: TopologyConfig,
    pub transactions: TransactionConfig,
    pub enable_event_logging: bool,
    pub csv_output_path: Option<String>,
}

/// Network behavior configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub delay_fraction: f64, // fraction of messages delayed to next round
    pub loss_fraction: f64,  // fraction of messages dropped
}

/// Peer topology configuration
#[derive(Debug, Clone)]
pub struct TopologyConfig {
    pub mode: TopologyMode,
}

/// Topology modes for peer selection
#[derive(Debug, Clone)]
pub enum TopologyMode {
    /// Random selection with specified connectivity (0.0 to 1.0)
    Random { connectivity: f64 },
    /// Ring-based gradient with linear probability decay
    RingGradient { min_prob: f64, max_prob: f64 },
    /// Ring-based Gaussian distribution
    RingGaussian { sigma: f64 },
}

/// Transaction generation configuration
#[derive(Debug, Clone)]
pub struct TransactionConfig {
    pub initial_tokens: usize,
    pub block_size_range: (usize, usize),
}

// ============================================================================
// Default Configurations
// ============================================================================

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            rounds: 1000,
            num_peers: 100,
            seed: None,
            network: NetworkConfig::default(),
            topology: TopologyConfig::default(),
            transactions: TransactionConfig::default(),
            enable_event_logging: false,
            csv_output_path: None,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            delay_fraction: 0.5,
            loss_fraction: 0.02,
        }
    }
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            mode: TopologyMode::RingGradient {
                min_prob: 0.1,
                max_prob: 0.7,
            },
        }
    }
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            initial_tokens: 1,
            block_size_range: (1, 3),
        }
    }
}
