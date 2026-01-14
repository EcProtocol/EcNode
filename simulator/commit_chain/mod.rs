//! Commit chain simulator module
//!
//! This module provides a simulator for testing the commit chain synchronization
//! system in isolation. It focuses on:
//! - Peers tracking their closest neighbors
//! - Top-down synchronization (newest to oldest)
//! - Shadow token mappings with multi-peer confirmation
//! - Bootstrap scenarios for new peers

pub mod config;
pub mod runner;
pub mod stats;

pub use config::{BlockInjectionConfig, CommitChainSimConfig, NetworkConfig};
pub use runner::CommitChainRunner;
pub use stats::{CommitStats, MessageStats, SimResult, SyncStats};
