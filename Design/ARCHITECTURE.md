# ecRust Architecture

## Overview

The ecRust project is split into two independent components:

1. **Core Consensus Library** (`src/`) - Production-ready, network-agnostic consensus implementation
2. **Simulator** (`examples/simulator/`) - Standalone testing framework

## Directory Structure

```
ecRust/
├── src/                          # Core consensus library
│   ├── lib.rs                    # Public API and documentation
│   ├── ec_node.rs                # Main consensus node
│   ├── ec_mempool.rs             # Transaction pool and voting
│   ├── ec_peers.rs               # Peer management
│   ├── ec_interface.rs           # Core data structures
│   ├── ec_blocks.rs              # Block storage backend
│   └── ec_tokens.rs              # Token storage backend
│
├── examples/                     # Testing and examples
│   ├── basic_simulation.rs       # Example simulation runner
│   └── simulator/                # Simulation framework
│       ├── mod.rs                # Simulator module
│       ├── config.rs             # Configuration structures
│       ├── runner.rs             # Simulation engine
│       ├── stats.rs              # Results and statistics
│       └── README.md             # Simulator documentation
│
├── Cargo.toml                    # Package configuration
├── CLAUDE.md                     # Claude Code guidelines
├── SIMULATION_PLAN.md            # Development roadmap
└── ARCHITECTURE.md               # This file
```

## Core Library (`src/`)

### Purpose

Network-agnostic consensus protocol implementation that can be integrated with any network layer.

### Key Components

- **EcNode**: Main consensus node handling message processing and state management
- **EcMemPool**: Transaction pool tracking block states (Pending, Commit, Blocked)
- **EcPeers**: Peer management and routing
- **EcBlocks/EcTokens**: Storage abstractions with in-memory implementations

### Usage

```rust
use ec_rust::{EcNode, ec_blocks::MemBlocks, ec_tokens::MemTokens};
use std::rc::Rc;
use std::cell::RefCell;

// Create storage backends
let tokens = Rc::new(RefCell::new(MemTokens::new()));
let blocks = Rc::new(RefCell::new(MemBlocks::new()));

// Create consensus node
let peer_id = 12345u64;
let mut node = EcNode::new(tokens, blocks, peer_id, 0);

// In your network event loop:
// 1. Call node.tick(&mut outgoing) periodically
// 2. Call node.handle_message(&msg, &mut outgoing) for each incoming message
// 3. Send outgoing messages via your network layer
```

### Design Principles

1. **Network Agnostic**: Core library has no networking code
2. **Storage Agnostic**: Block and token storage are traits
3. **Zero Dependencies on Simulator**: Can be used independently
4. **Message-Driven**: All communication via `MessageEnvelope`

## Simulator (`examples/simulator/`)

### Purpose

Standalone testing framework for protocol validation, performance analysis, and regression testing.

### Key Features

- **Configurable Network**: Delay, packet loss, message shuffling
- **Multiple Topologies**: Random, RingGradient, RingGaussian
- **Transaction Patterns**: Configurable token counts and block sizes
- **Statistics Collection**: Message counts, commit rates, peer connectivity

### Usage

```bash
# Run default simulation
cargo run --example basic_simulation --release

# Create custom simulation
# examples/my_test.rs
mod simulator;
use simulator::{SimConfig, SimRunner};

fn main() {
    let mut runner = SimRunner::new(SimConfig::default());
    let result = runner.run();
    println!("Commits: {}", result.committed_blocks);
}
```

### Design Principles

1. **Independent**: Uses core library via public API only
2. **Standalone**: Located in `examples/` to emphasize separation
3. **Module-based**: Other examples can `mod simulator;` to reuse
4. **Well-documented**: Complete README in `examples/simulator/`

## Dependency Flow

```
┌─────────────────────────┐
│  Production Network     │
│  (Your Implementation)  │
└───────────┬─────────────┘
            │ uses
            ▼
┌─────────────────────────┐
│   Core Library (src/)   │
│   - EcNode              │
│   - EcMemPool           │
│   - EcPeers             │
└─────────────────────────┘
            ▲
            │ uses
            │
┌─────────────────────────┐
│  Simulator (examples/)  │
│  - SimRunner            │
│  - Network Simulation   │
└─────────────────────────┘
```

## Integration Patterns

### Pattern 1: Direct Integration (Production)

1. Create `EcNode` instances for each peer
2. Implement your network transport layer
3. Route `MessageEnvelope` between nodes
4. Call `node.tick()` and `node.handle_message()` in your event loop

### Pattern 2: Simulation (Testing)

1. Use simulator module in `examples/`
2. Configure network conditions and topology
3. Run simulation and collect statistics
4. Validate consensus behavior

## Future Extensions

### Core Library

- Pluggable storage backends (database, persistent storage)
- Enhanced cryptographic signatures
- Dynamic peer discovery protocols
- Byzantine fault tolerance enhancements

### Simulator

- Python bindings via PyO3 (Phase 2)
- YAML configuration files
- Database persistence for results
- Visualization dashboard
- Parametric test generation

See `SIMULATION_PLAN.md` for detailed roadmap.

## Build and Test

```bash
# Build core library only
cargo build --lib

# Build with examples
cargo build --examples

# Run simulation
cargo run --example basic_simulation --release

# Generate documentation
cargo doc --open
```

## Documentation

- **Core Library**: See `src/lib.rs` and `cargo doc`
- **Simulator**: See `examples/simulator/README.md`
- **Development Plan**: See `SIMULATION_PLAN.md`
- **Contributing**: See `CLAUDE.md`
