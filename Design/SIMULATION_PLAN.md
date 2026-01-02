# Simulation Framework Plan

## Goal
Transform the monolithic `main.rs` simulation driver into a flexible, configurable test framework that supports:
- Incremental complexity testing
- Repeatable scenarios with configuration files
- Queryable statistics and metrics
- Automated test suites for consensus validation

## Architecture: Hybrid Rust + Python

### Rust Core (Performance-Critical)
- Consensus logic remains in Rust
- Simulation primitives extracted to library
- Exposed via public API

### Python Orchestration (Flexibility)
- Test scenario configuration (YAML/TOML)
- Parametric test generation
- Statistics collection and querying
- Visualization and analysis

---

## Phase 1: Rust Library Refactoring

### Step 1.1: Extract Simulation Library ✅ COMPLETED
**Goal**: Move simulation logic from `main.rs` to separate module with clean API

**Status**: Complete - Simulation code moved to `src/simulator/` module

**Implementation**:
- ✅ Created `src/simulator/` module with separate concerns:
  - `config.rs` - Configuration structures
  - `runner.rs` - Simulation execution engine
  - `stats.rs` - Results and statistics
  - `mod.rs` - Public API
  - `README.md` - Usage documentation
- ✅ Updated `src/lib.rs` to export only core consensus components
- ✅ Core library (`lib.rs`) is now network-agnostic for production use
- ✅ Simulator is a separate module: `use ec_rust::simulator::*`
- ✅ Updated `main.rs` to use `ec_rust::simulator::*`
- ✅ Verified simulation runs correctly

**Current Module Structure**:
```
src/
├── lib.rs                    # Core consensus library (network-agnostic)
│                             # Exports: EcNode, Block, Message, etc.
└── [core modules]            # ec_node, ec_mempool, ec_peers, ec_tokens, etc.

examples/
├── simulator/                # Simulation framework (standalone)
│   ├── mod.rs               # Public API exports
│   ├── config.rs            # SimConfig, NetworkConfig, TopologyConfig, etc.
│   ├── runner.rs            # SimRunner implementation
│   ├── stats.rs             # SimResult, SimStatistics
│   └── README.md            # Simulator usage guide
└── basic_simulation.rs       # Example simulation driver
```

**Public API**:
```rust
// Production usage (core consensus only):
use ec_rust::{EcNode, Block, Message, ec_blocks::MemBlocks, ec_tokens::MemTokens};
use std::rc::Rc;
use std::cell::RefCell;

// Create node and integrate with your network layer
let peer_id = 12345u64;
let tokens = Rc::new(RefCell::new(MemTokens::new()));
let blocks = Rc::new(RefCell::new(MemBlocks::new()));
let mut node = EcNode::new(tokens, blocks, peer_id, 0);

// Simulation/testing usage:
// examples/my_test.rs
mod simulator;
use simulator::{SimConfig, SimRunner, TopologyMode};

let config = SimConfig {
    rounds: 1000,
    num_peers: 100,
    ..Default::default()
};

let mut runner = SimRunner::new(config);
let result = runner.run();
```

**Documentation**:
- `src/lib.rs` - Core library documentation
- `examples/simulator/README.md` - Complete simulator guide

### Step 1.2: Pluggable Network Models
**Goal**: Make network behavior configurable

**Status**: Partially complete - Basic network model implemented in `runner.rs`

**Current Implementation**:
- ✅ `NetworkConfig` with `delay_fraction` and `loss_fraction`
- ✅ Integrated into `SimRunner::step()`

**TODO** (future enhancement):
- Create `NetworkModel` trait for pluggable behavior
- Implement models: ConstantDelay, VariableDelay, NetworkPartition, Byzantine
- Allow custom network models via trait

### Step 1.3: Pluggable Topology Builders ✅ COMPLETED
**Goal**: Support different peer topology configurations

**Status**: Complete - Implemented in `runner.rs`

**Implementation**:
- ✅ `TopologyMode` enum with three modes:
  - `Random { connectivity }` - Random peer selection
  - `RingGradient { min_prob, max_prob }` - Linear probability decay on ring
  - `RingGaussian { sigma }` - Gaussian distribution on ring
- ✅ `SimRunner::apply_topology()` configures peer connections
- ✅ Fully configurable via `TopologyConfig` in `SimConfig`

**TODO** (future enhancement):
- Create `TopologyBuilder` trait for custom topologies
- Add more modes: Star, Grid, SmallWorld, ScaleFree

### Step 1.4: Statistics Collection System
**Goal**: Capture detailed per-round metrics

**Status**: Partially complete - Basic statistics implemented

**Current Implementation**:
- ✅ `SimStatistics` with message counts and peer stats
- ✅ `MessageCounts` breakdown (Query, Vote, Block, Answer)
- ✅ `PeerStats` (max, min, avg peer connections)
- ✅ Efficiency metrics (rounds/commit, messages/commit)
- ✅ Seed tracking for reproducibility

**TODO** (future enhancement):
- Per-round statistics tracking
- Block lifecycle events (created, voted, committed)
- Node-level statistics per peer
- Time-series data collection
- Export to structured formats (JSON, CSV)

---

## Phase 2: Python Test Framework

### Step 2.1: PyO3 Bindings
**Goal**: Expose Rust simulation to Python

**Tasks**:
- Add PyO3 dependency
- Create Python module bindings
- Expose `SimConfig` and `SimRunner` to Python
- Build basic Python package

### Step 2.2: YAML Configuration
**Goal**: Define scenarios in YAML files

**Tasks**:
- Design YAML schema for scenarios
- Implement Python loader
- Create example scenarios (basic, stress, partition)
- Validate configuration before running

### Step 2.3: Test Runner
**Goal**: Execute scenarios and collect results

**Tasks**:
- Create `SimulationRunner` Python class
- Implement batch execution
- Add progress reporting
- Handle errors gracefully

### Step 2.4: Statistics Persistence
**Goal**: Store results for analysis

**Tasks**:
- Choose database (SQLite or DuckDB)
- Design schema (runs, rounds, blocks, messages)
- Implement result writer
- Create basic query interface

---

## Phase 3: Advanced Test Scenarios

### Step 3.1: Complexity Levels
**Goal**: Incremental test difficulty

**Scenarios**:
- **L1**: Single block, perfect network (baseline)
- **L2**: Multiple blocks, delayed messages
- **L3**: Network partitions, message loss
- **L4**: Dynamic peer churn
- **L5**: Adversarial (conflicting blocks)

### Step 3.2: Parametric Testing
**Goal**: Systematic parameter exploration

**Tasks**:
- Generate test matrices (peers × loss × topology)
- Run parameter sweeps
- Compare results across configurations

### Step 3.3: Regression Suite
**Goal**: Continuous validation

**Tasks**:
- Define baseline benchmarks
- Create CI integration
- Alert on performance degradation

---

## Phase 4: Analysis & Visualization

### Step 4.1: Query Interface
**Goal**: Easy data exploration

**Tasks**:
- Jupyter notebook templates
- Query helper functions
- Example analysis notebooks

### Step 4.2: Metrics Dashboard
**Goal**: Visual insights

**Tasks**:
- Plot commit rates over time
- Message efficiency trends
- Topology comparison charts
- Latency distributions

### Step 4.3: Report Generation
**Goal**: Automated analysis

**Tasks**:
- Generate markdown reports
- Include plots and statistics
- Compare scenario results
- Export to PDF/HTML

---

## Success Criteria

### Phase 1 Complete
- ✓ `main.rs` uses library API
- ✓ Existing simulation produces same results
- ✓ Configuration is external to code
- ✓ Statistics are structured

### Phase 2 Complete
- Python can run Rust simulations
- YAML scenarios are working
- Results stored in database
- Basic queries functional

### Phase 3 Complete
- 5+ complexity levels defined
- Parametric testing automated
- Regression suite in CI

### Phase 4 Complete
- Jupyter analysis workflow
- Automated reports
- Performance dashboard

---

## Current Status

**Phase 1**: Partially Complete ✅
- ✅ **Step 1.1**: Simulation framework extracted to `src/simulator/` module
- ⚠️ **Step 1.2**: Basic network model implemented, trait-based system pending
- ✅ **Step 1.3**: Topology modes implemented (Random, RingGradient, RingGaussian)
- ⚠️ **Step 1.4**: Basic statistics implemented, detailed metrics pending

**Phase 2**: Not Started
- Python integration pending
- Scenario configuration system pending
- Database persistence pending

**Next Steps**:
1. Enhance statistics collection (per-round, per-block tracking)
2. Implement trait-based network models
3. Begin Phase 2: Python bindings with PyO3
4. Create YAML scenario configuration system
