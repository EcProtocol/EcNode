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
mod event_sinks;
mod hashmap_tokens;
mod runner;
mod stats;

#[allow(unused_imports)]
pub use config::{NetworkConfig, SimConfig, TopologyConfig, TopologyMode, TransactionConfig};
#[allow(unused_imports)]
#[allow(unused_imports)]
pub use event_sinks::{
    CollectorEventSink, ConsoleEventSink, CsvEventSink, EventRecord, EventTypeCounts,
    MultiEventSink,
};
#[allow(unused_imports)]
pub use runner::SimRunner;
#[allow(unused_imports)]
pub use stats::{MessageCounts, PeerStats, SimResult, SimStatistics};
