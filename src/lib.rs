//! # ecRust - Echo Consent Distributed Consensus
//!
//! A Rust implementation of a distributed consensus protocol for token-based transactions.
//! The system allows peers in a distributed network to vote on and commit transaction blocks
//! containing token transfers.
//!
//! ## Core Components
//!
//! - **EcNode**: Main node implementation handling consensus logic and message processing
//! - **EcMemPool**: Transaction pool tracking block states and voting
//! - **EcPeers**: Peer management and routing
//! - **Block/Token System**: Core data structures for blocks, tokens, and messages
//!
//! ## Usage with Network Layer
//!
//! This library provides network-agnostic consensus components. You need to:
//! 1. Implement your network transport layer
//! 2. Create EcNode instances for each peer
//! 3. Route MessageEnvelope between nodes via your network
//! 4. Call `node.tick()` and `node.handle_message()` as messages arrive
//!
//! ```no_run
//! use ec_rust::{EcNode, ec_memory_backend::{MemBlocks, MemTokens}};
//! use std::rc::Rc;
//! use std::cell::RefCell;
//!
//! // Create storage backends
//! let tokens = Rc::new(RefCell::new(MemTokens::new()));
//! let blocks = Rc::new(RefCell::new(MemBlocks::new()));
//!
//! // Create a consensus node
//! let peer_id = 12345u64;
//! let mut node = EcNode::new(tokens, blocks, peer_id, 0);
//!
//! // In your network event loop:
//! // - Call node.tick(&mut outgoing_messages) periodically
//! // - Call node.handle_message(&incoming_msg, &mut outgoing_messages) for each message
//! // - Send outgoing_messages via your network layer
//! ```
//!
//! ## Testing and Simulation
//!
//! For testing the consensus protocol without a real network, see the separate
//! `simulator` crate in `src/simulator/`. It provides a configurable simulation
//! framework for protocol validation and performance analysis.

// Core consensus modules
pub mod ec_interface;
pub mod ec_mempool;
pub mod ec_node;
pub mod ec_peers;
pub mod ec_proof_of_storage;

// Storage backends
pub mod ec_memory_backend;

#[cfg(feature = "rocksdb-backend")]
pub mod ec_rocksdb_backend;

// Re-export commonly used types
pub use ec_interface::{
    Block, BlockId, EcBlocks, EcTime, EcTokens, Event, EventSink, Message, MessageEnvelope,
    NoOpSink, PeerId, TokenId,
};
pub use ec_node::EcNode;
// Public API for peer elections (used by clients to evaluate and discover peers)
pub use ec_proof_of_storage::{
    initialize_election_secret, ring_distance, ConsensusCluster, ElectionAttempt, ElectionConfig,
    ElectionError, ElectionResult, PeerElection,
};

// Internal utilities (used by ec_peers.rs and ec_node.rs, not exposed to external clients)
pub(crate) use ec_proof_of_storage::{generate_ticket, ChannelState, ElectionState};
