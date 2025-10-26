# ecRust Simulator

A configurable simulation framework for testing the ecRust distributed consensus protocol.

**Location**: `examples/simulator/` - This is a standalone testing tool.

**Core Library**: The simulator uses the core `ec_rust` library, which is completely independent and network-agnostic (see `src/lib.rs`).

## Overview

The simulator allows you to test consensus behavior under various network conditions, topologies, and transaction patterns. It's designed for:

- **Protocol validation**: Verify consensus convergence and correctness
- **Performance testing**: Measure message overhead and commit latency
- **Parameter exploration**: Test different configurations systematically
- **Regression testing**: Maintain baseline performance benchmarks

## Quick Start

### Running the Default Simulation

```bash
cargo run --example basic_simulation --release
```

### Using in Your Own Example

Create a new file in `examples/` that includes the simulator module:

```rust
// examples/my_test.rs
mod simulator;
use simulator::{SimConfig, SimRunner};

fn main() {
    let mut runner = SimRunner::new(SimConfig::default());
    let result = runner.run();

    println!("Committed blocks: {}", result.committed_blocks);
    println!("Messages per commit: {:.0}", result.statistics.messages_per_commit);
}
```

Run with: `cargo run --example my_test --release`

### Custom Configuration

```rust
mod simulator;
use simulator::{
    SimConfig, SimRunner, NetworkConfig, TopologyConfig,
    TopologyMode, TransactionConfig
};

let config = SimConfig {
    rounds: 1000,
    num_peers: 100,
    seed: None, // Auto-generated, or provide [u8; 32]

    network: NetworkConfig {
        delay_fraction: 0.5,  // 50% of messages delayed one round
        loss_fraction: 0.02,  // 2% packet loss
    },

    topology: TopologyConfig {
        mode: TopologyMode::RingGradient {
            min_prob: 0.1,    // Farthest peers: 10% connection probability
            max_prob: 0.7,    // Closest peers: 70% connection probability
        },
    },

    transactions: TransactionConfig {
        initial_tokens: 10,
        block_size_range: (1, 3),  // Blocks contain 1-3 tokens
    },
};

let mut runner = SimRunner::new(config);
let result = runner.run();
```

## Configuration Options

### SimConfig

| Field | Type | Description |
|-------|------|-------------|
| `rounds` | `usize` | Number of simulation rounds to execute |
| `num_peers` | `usize` | Number of peers in the network |
| `seed` | `Option<[u8; 32]>` | RNG seed for reproducibility (None = random) |
| `network` | `NetworkConfig` | Network behavior configuration |
| `topology` | `TopologyConfig` | Peer connectivity topology |
| `transactions` | `TransactionConfig` | Block/token generation settings |

### NetworkConfig

| Field | Type | Description |
|-------|------|-------------|
| `delay_fraction` | `f64` | Fraction of messages delayed to next round (0.0-1.0) |
| `loss_fraction` | `f64` | Fraction of messages dropped (0.0-1.0) |

### TopologyMode

#### Random
```rust
TopologyMode::Random {
    connectivity: 0.3  // Each peer connects to 30% of network
}
```

#### RingGradient
```rust
TopologyMode::RingGradient {
    min_prob: 0.1,  // Probability for farthest peers
    max_prob: 0.7,  // Probability for closest peers
}
```
Linearly decreasing connection probability based on distance on a ring.

#### RingGaussian
```rust
TopologyMode::RingGaussian {
    sigma: 1.0  // Width of Gaussian distribution (relative to ring_size/8)
}
```
Bell curve distribution centered on each peer's ID.

### TransactionConfig

| Field | Type | Description |
|-------|------|-------------|
| `initial_tokens` | `usize` | Number of tokens to start with |
| `block_size_range` | `(usize, usize)` | Min and max tokens per block |

## Results and Statistics

### SimResult

```rust
pub struct SimResult {
    pub statistics: SimStatistics,
    pub committed_blocks: usize,
    pub total_messages: usize,
    pub seed_used: [u8; 32],
}
```

### SimStatistics

```rust
pub struct SimStatistics {
    pub message_counts: MessageCounts,  // Query, Vote, Block, Answer counts
    pub peer_stats: PeerStats,          // Max, min, avg peer connections
    pub rounds_per_commit: f64,
    pub messages_per_commit: f64,
}
```

## Examples

### High Connectivity Network

Test with well-connected peers and perfect network:

```rust
let config = SimConfig {
    rounds: 500,
    num_peers: 50,
    seed: None,
    network: NetworkConfig {
        delay_fraction: 0.0,  // No delays
        loss_fraction: 0.0,   // No packet loss
    },
    topology: TopologyConfig {
        mode: TopologyMode::Random { connectivity: 0.9 },
    },
    transactions: TransactionConfig::default(),
};
```

### Stressed Network

Test with network partitioning and high loss:

```rust
let config = SimConfig {
    rounds: 2000,
    num_peers: 200,
    seed: None,
    network: NetworkConfig {
        delay_fraction: 0.7,  // 70% delayed
        loss_fraction: 0.1,   // 10% loss
    },
    topology: TopologyConfig {
        mode: TopologyMode::RingGradient {
            min_prob: 0.05,
            max_prob: 0.3,
        },
    },
    transactions: TransactionConfig::default(),
};
```

### Reproducible Tests

Use a fixed seed for deterministic results:

```rust
let seed = [42u8; 32];  // Fixed seed

let config = SimConfig {
    seed: Some(seed),
    ..Default::default()
};

let result1 = SimRunner::new(config.clone()).run();
let result2 = SimRunner::new(config.clone()).run();

assert_eq!(result1.committed_blocks, result2.committed_blocks);
```

## Module Structure

```
simulator/
├── mod.rs       # Public API and documentation
├── config.rs    # Configuration structures
├── runner.rs    # Simulation execution engine
├── stats.rs     # Results and statistics
└── README.md    # This file
```

## Future Enhancements

See `SIMULATION_PLAN.md` in the project root for the full roadmap:

- **Phase 1**: Pluggable network models and topology builders
- **Phase 2**: Python integration for scripted testing
- **Phase 3**: Parametric test suites and complexity levels
- **Phase 4**: Analysis tools and visualization dashboard

## Tips

1. **Start small**: Test with 50-100 peers before scaling to thousands
2. **Use fixed seeds**: For debugging and regression testing
3. **Monitor statistics**: `rounds_per_commit` indicates convergence speed
4. **Vary topologies**: Different patterns expose different behaviors
5. **Network stress**: Increase loss/delay to test resilience

## Related

- See `src/main.rs` for a complete example
- See `SIMULATION_PLAN.md` for development roadmap
- Core consensus: `src/ec_node.rs`, `src/ec_mempool.rs`
