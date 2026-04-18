//! Integrated simulator module
//!
//! This simulator combines:
//! - full `EcNode` message flow and transaction commits
//! - peer discovery using `MemTokens` proof-of-storage views
//! - scheduled lifecycle events such as joins and crashes
//! - commit-chain sync running in the background through normal node ticks

pub mod config;
pub mod runner;
pub mod stats;

pub use config::{
    ConflictWorkloadConfig, IntegratedSimConfig, NetworkConfig, TransactionFlowConfig,
    TransactionSourcePolicy,
};
pub use runner::IntegratedRunner;
pub use stats::{
    ConflictWorkloadSummary, DistributionSummary, FloatDistributionSummary, MempoolPressureSummary,
    MessageTypeBreakdown, NeighborhoodBucketSummary, NeighborhoodSummary, OnboardingSummary,
    RecoverySummary, RoundMetrics, SimResult, TransactionSpreadSummary, TransactionWorkloadSummary,
    VoteIngressSummary,
};
