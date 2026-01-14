//! Configuration for commit chain simulator

use ec_rust::ec_commit_chain::CommitChainConfig;
use rand::rngs::StdRng;
use rand::SeedableRng;

/// Configuration for commit chain simulation
#[derive(Debug, Clone)]
pub struct CommitChainSimConfig {
    /// Number of simulation rounds
    pub rounds: usize,

    /// Number of peers in the network
    pub num_peers: usize,

    /// Random seed (None = generate random)
    pub seed: Option<[u8; 32]>,

    /// Commit chain configuration
    pub commit_chain: CommitChainConfig,

    /// Block injection configuration
    pub block_injection: BlockInjectionConfig,

    /// Network simulation configuration
    pub network: NetworkConfig,
}

impl Default for CommitChainSimConfig {
    fn default() -> Self {
        Self {
            rounds: 500,
            num_peers: 20,
            seed: None,
            commit_chain: CommitChainConfig::default(),
            block_injection: BlockInjectionConfig::default(),
            network: NetworkConfig::default(),
        }
    }
}

impl CommitChainSimConfig {
    /// Get or generate seed
    pub fn resolve_seed(&self) -> [u8; 32] {
        self.seed.unwrap_or_else(|| {
            let mut temp_rng = StdRng::from_entropy();
            let mut seed = [0u8; 32];
            use rand::RngCore;
            temp_rng.fill_bytes(&mut seed);
            seed
        })
    }
}

/// Configuration for block injection
#[derive(Debug, Clone)]
pub struct BlockInjectionConfig {
    /// Average number of blocks injected per round across all peers
    pub blocks_per_round: f64,

    /// Range of token count per block (min, max)
    pub block_size_range: (usize, usize),

    /// Total token pool size
    pub total_tokens: usize,
}

impl Default for BlockInjectionConfig {
    fn default() -> Self {
        Self {
            blocks_per_round: 2.0,
            block_size_range: (1, 3),
            total_tokens: 10000,
        }
    }
}

/// Network simulation configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Fraction of messages that are delayed by one round
    pub delay_fraction: f64,

    /// Fraction of messages that are lost
    pub loss_fraction: f64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            delay_fraction: 0.3,
            loss_fraction: 0.01,
        }
    }
}
