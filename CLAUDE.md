# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is **ecRust** (echo-consent), a Rust implementation of a distributed consensus protocol for token-based transactions. The system simulates a network of peers that vote on and commit transaction blocks containing token transfers.

## Claude Code guidelines

Ignore content under the folder "Scratch" - its outdated.
WHen asked to make design documents DO NOT include any work schedule or rollout plans. DO NOT compare to the current implementation - just focus on the concept and provide analasis and mathematical proofs.

Diagrams should be in "mermaid".

Formulas should use UNICODE formatting.

## Core Architecture

The system consists of several key components:

- **EcNode** (`src/ec_node.rs`): The main node implementation that handles consensus logic, message processing, and peer communication
- **EcMemPool** (`src/ec_mempool.rs`): Transaction pool that tracks block states (Pending, Commit, Blocked) and manages voting
- **EcPeers** (`src/ec_peers.rs`): Peer management and routing for the distributed network
- **Block/Token System** (`src/ec_interface.rs`): Core data structures for blocks, tokens, and messages
- **Storage Backends** (`src/ec_blocks.rs`, `src/ec_tokens.rs`): In-memory storage implementations

## Key Concepts

- **Blocks**: Contain up to 6 token transactions (`TOKENS_PER_BLOCK`)
- **Tokens**: Digital assets with ownership tracked through blockchain-like history
- **Voting**: Nodes vote on block validity; blocks commit when reaching `VOTE_THRESHOLD`
- **Network Simulation**: Includes message delays, packet loss, and peer connectivity simulation

## Common Development Commands

### Build and Run
```bash
# Build the project
cargo build

# Run with logging
cargo run

# Run in release mode
cargo run --release
```

### Development Tools
```bash
# Check code formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy linter
cargo clippy

# Check without building
cargo check
```

## Network Simulation Parameters

The main simulation in `src/main.rs` configures:
- `rounds`: Number of simulation rounds (default: 1000)
- `num_of_peers`: Network size (default: 2000)
- Message delay/loss simulation with configurable rates
- Peer connectivity (90% connectivity by default)

## Message Types

The consensus protocol uses these message types:
- **Vote**: Voting on block validity (positive/negative votes)
- **Query**: Request for block/token information
- **Block**: Block propagation messages
- **Answer**: Response messages with token mappings

## Testing and Validation

The system currently operates as a simulation rather than having traditional unit tests. Validation is done through:
- Consensus convergence metrics
- Message count and round statistics
- Commit success rates across different network configurations

## Performance Notes

- Vote threshold and peer connectivity significantly impact performance
- Lower thresholds reduce message overhead but may affect security
- Network loss/delay parameters simulate real-world conditions
- The system tracks message distribution: (Query, Vote, Block, Answer) counts
