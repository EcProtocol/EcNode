use crate::peer_lifecycle::{EventSchedule, InitialNetworkState, TokenDistributionConfig};

/// Configuration for the first integrated simulator.
///
/// This keeps the setup intentionally small:
/// - reuse peer lifecycle topology + event scheduling
/// - reuse token-view generation for discovery
/// - add a simple transaction workload for full-node flow
#[derive(Debug, Clone)]
pub struct IntegratedSimConfig {
    pub rounds: usize,
    pub seed: Option<[u8; 32]>,
    pub initial_state: InitialNetworkState,
    pub token_distribution: TokenDistributionConfig,
    pub events: EventSchedule,
    pub network: NetworkConfig,
    pub transactions: TransactionFlowConfig,
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Fixed extra rounds every message waits in the network queue.
    ///
    /// Messages already have an implicit minimum of one round because node output
    /// is only scheduled for delivery on the next simulation round.
    pub base_delay_rounds: usize,

    /// Uniform random jitter added on top of `base_delay_rounds`.
    pub jitter_rounds: usize,

    /// Probability that a message incurs one more round of delay.
    ///
    /// This is sampled repeatedly, producing a geometric tail distribution.
    pub delay_fraction: f64,

    /// Probability a scheduled message is dropped permanently.
    pub loss_fraction: f64,
}

impl NetworkConfig {
    pub fn same_dc() -> Self {
        Self {
            base_delay_rounds: 0,
            jitter_rounds: 0,
            delay_fraction: 0.05,
            loss_fraction: 0.0005,
        }
    }

    pub fn cross_dc_normal() -> Self {
        Self {
            base_delay_rounds: 0,
            jitter_rounds: 1,
            delay_fraction: 0.20,
            loss_fraction: 0.002,
        }
    }

    pub fn cross_dc_stressed() -> Self {
        Self {
            base_delay_rounds: 1,
            jitter_rounds: 2,
            delay_fraction: 0.35,
            loss_fraction: 0.01,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransactionFlowConfig {
    pub blocks_per_round: usize,
    pub block_size_range: (usize, usize),
    pub source_policy: TransactionSourcePolicy,
}

#[derive(Debug, Clone, Copy)]
pub enum TransactionSourcePolicy {
    AnyActive,
    ConnectedOnly,
}

impl TransactionSourcePolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::AnyActive => "any-active",
            Self::ConnectedOnly => "connected-only",
        }
    }
}

impl Default for IntegratedSimConfig {
    fn default() -> Self {
        Self {
            rounds: 200,
            seed: None,
            initial_state: InitialNetworkState::default(),
            token_distribution: TokenDistributionConfig::default(),
            events: EventSchedule::default(),
            network: NetworkConfig::default(),
            transactions: TransactionFlowConfig::default(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self::cross_dc_normal()
    }
}

impl Default for TransactionFlowConfig {
    fn default() -> Self {
        Self {
            blocks_per_round: 2,
            block_size_range: (1, 3),
            source_policy: TransactionSourcePolicy::ConnectedOnly,
        }
    }
}
