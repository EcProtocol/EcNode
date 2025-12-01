# ecRust Simulators

This directory contains simulation tools for testing and analyzing the ecRust consensus protocol.

## Available Simulators

### 1. Consensus Simulator (`basic_simulation`)

A comprehensive simulator for testing the core consensus mechanism with transaction blocks.

**Location:** `simulator/basic_simulation.rs`
**Modules:** `simulator/consensus/` (config, runner, event sinks, stats)

#### Features
- Full consensus protocol simulation with voting and block commits
- Configurable network conditions (delay, packet loss)
- Multiple topology modes (Random, RingGradient, RingGaussian)
- Event logging and CSV export for analysis
- Statistics tracking (messages, commits, peer counts)

#### Usage

```bash
# Run with default settings (2000 peers, 2000 rounds)
cargo run --example basic_simulation

# View logs during simulation
RUST_LOG=info cargo run --example basic_simulation
```

#### Configuration

Edit `simulator/basic_simulation.rs` to customize:
- `rounds`: Number of simulation rounds (default: 2000)
- `num_peers`: Number of peers in the network (default: 2000)
- `delay_fraction`: Fraction of messages delayed to next round (default: 0.5)
- `loss_fraction`: Fraction of messages lost (default: 0.02)
- `topology.mode`: Network topology pattern
- `csv_output_path`: Export events to CSV file for analysis

#### Topology Modes

**RingGradient**: Peers connect with probability decreasing by ring distance
- `min_prob`: Minimum connection probability for distant peers
- `max_prob`: Maximum connection probability for nearby peers

**RingGaussian**: Gaussian distribution around peers on the ring
- `sigma`: Standard deviation (as fraction of ring size)

**Random**: Uniform random connections
- `connectivity`: Fraction of peers to connect to (0.0-1.0)

#### Output

```
Peers: max: 45 min: 39 avg: 42.1
Messages: 184523. Commits: 156, avg: 12.8 rounds/commit, 1182 messages/commit
Message distribution: Query: 123456, Vote: 45678, Block: 12345, Answer: 3044
```

---

### 2. Peer Lifecycle Simulator (`peer_lifecycle_sim`)

A specialized simulator focused on peer management, election-based discovery, and network growth.

**Location:** `simulator/peer_lifecycle_sim.rs`
**Modules:** `simulator/peer_lifecycle/` (config, runner, token distribution, stats)

#### Features
- Peer state machine: Identified → Pending → Connected
- Election-based peer discovery with challenge-response
- Token ownership distribution strategies
- Ring-based distance classes and budget allocation
- Invitation handshake and mutual promotion
- Election consensus clustering with signature verification

#### Usage

```bash
# Run with default settings (20 peers, 2000 rounds)
cargo run --example peer_lifecycle_sim

# Adjust configuration in simulator/peer_lifecycle_sim.rs:
# - peers: 20
# - rounds: 2000
# - topology: Ring with 3 neighbors
# - election_config: consensus thresholds, timeouts
```

#### Key Metrics

The simulator tracks:
- **Peer States**: Number of peers in each state (Identified, Pending, Connected)
- **Election Performance**: Started, completed, timed out, split-brain
- **Network Health**: Min/max/avg connected peers, ring coverage %
- **Message Overhead**: Query, Answer, Referral counts

#### Configuration Parameters

**Peer Manager Config** (`simulator/peer_lifecycle/config.rs`):
- `total_budget`: Maximum connected peers (default: 50)
- `election_interval`: Ticks between elections (default: 60)
- `election_timeout`: Max ticks to wait for election result (default: 8)
- `pending_timeout`: Ticks before demoting Pending→Identified (default: 10)
- `connection_timeout`: Ticks without keepalive before disconnect (default: 300)

**Election Config**:
- `channel_candidate_count`: Peers to query per election (default: 8)
- `consensus_threshold`: Matching mappings required (default: 8/10)
- `majority_threshold`: Peer agreement required (default: 60%)

**Token Distribution**:
- `Clustered`: Tokens grouped near peer ID with configurable radius
- `Random`: Uniform random distribution across ID space

#### Output

```
═══ Final State ═══
  Peers: 20 total, 20 active
  States: 0 Identified, 0 Pending, 6 Connected

═══ Election Performance ═══
  Total Started: 0
  Completed: 0
  Timed Out: 0
  Split-Brain: 0

═══ Network Health ═══
  Connected Peers: min=6, max=6, avg=6.0
  Ring Coverage: 0.0%

═══ Message Overhead ═══
  Total Messages: 135075
  Queries: 68679
  Answers: 703
  Referrals: 65693
  Per Peer/Round: 3.38
```

---

## Other Test Examples

Additional focused test examples in this directory:

- **`csv_export_test.rs`**: CSV event export functionality
- **`event_analysis.rs`**: Event log analysis tools
- **`event_logging_test.rs`**: Event sink testing
- **`fixed_seed_test.rs`**: Deterministic simulation with fixed seed

Run any example with:
```bash
cargo run --example <example_name>
```

---

## Development

### Adding a New Simulator

1. Create the main example file in `simulator/`
2. Create a module directory in `simulator/` for shared code (optional)
3. Add entry to `Cargo.toml`:
   ```toml
   [[example]]
   name = "my_simulator"
   path = "simulator/my_simulator.rs"
   ```

### Module Structure

**consensus/** - Core consensus simulation infrastructure
- `config.rs`: Simulation configuration structures
- `runner.rs`: Main simulation loop and message handling
- `event_sinks.rs`: Event logging (console, CSV)
- `stats.rs`: Statistics collection and result types

**peer_lifecycle/** - Peer management simulation infrastructure
- `config.rs`: Peer lifecycle configuration
- `runner.rs`: Peer network simulation with elections
- `token_dist.rs`: Token ownership distribution strategies
- `stats.rs`: Peer-focused statistics tracking

---

## Analysis

### CSV Export

Enable CSV export in either simulator:
```rust
csv_output_path: Some("sim_events.csv".to_string())
```

Events include:
- `BlockReceived`: Block arrives at peer
- `BlockCommitted`: Block committed to storage
- `VoteReceived`: Vote message received
- `BlockStateChange`: State transitions (pending→commit/blocked)
- `Reorg`: Chain reorganization detected

Analyze with tools like pandas, Excel, or R.

### Performance Profiling

Run with release mode for accurate performance metrics:
```bash
cargo run --release --example basic_simulation
```

### Debugging

Enable detailed logging:
```bash
RUST_LOG=debug cargo run --example peer_lifecycle_sim
```

Or enable event logging in config:
```rust
enable_event_logging: true
```

---

## Architecture Notes

### Consensus Simulator
- Uses `EcNode` for each peer
- Full transaction block processing
- Realistic network simulation (delays, losses)
- Measures consensus convergence and throughput

### Peer Lifecycle Simulator
- Uses `EcPeers` directly (peer management layer)
- Focuses on peer discovery and election mechanics
- Simplified token ownership (no full transactions)
- Measures network growth and election success

---

## Troubleshooting

**Elections not completing in peer_lifecycle_sim:**
- Check `election_timeout` and `min_collection_time` settings
- Lower `consensus_threshold` and `majority_threshold` for testing
- Verify token distribution gives sufficient Answer responses

**Low commit rate in basic_simulation:**
- Reduce network loss rate
- Decrease delay fraction
- Increase peer connectivity
- Check vote threshold settings

**High message overhead:**
- Tune topology to reduce redundant connections
- Adjust query/referral strategy
- Enable message batching (future feature)

---

For more details on the consensus protocol, see the main [README](../README.md) and design documents in `Design/`.
