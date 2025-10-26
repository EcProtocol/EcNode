//! # ecRust Simulator
//!
//! This module provides a simulation framework for testing the ecRust consensus protocol.
//! It allows configurable network topologies, network conditions, and transaction patterns
//! for comprehensive testing and analysis.
//!
//! This is a standalone testing tool that uses the core `ec_rust` library.
//!
//! ## Example
//!
//! See `examples/basic_simulation.rs` for a complete example.

mod config;
mod runner;
mod stats;

pub use config::{NetworkConfig, SimConfig, TopologyConfig, TopologyMode, TransactionConfig};
pub use runner::SimRunner;
pub use stats::{MessageCounts, PeerStats, SimResult, SimStatistics};
