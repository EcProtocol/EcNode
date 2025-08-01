# Technical Design Document - ecRust (echo-consent)

## System Architecture Overview

ecRust implements a distributed consensus protocol for token-based transactions using a peer-to-peer network simulation. The system follows a modular architecture with clear separation of concerns.

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Simulation    │    │   Node Layer    │    │  Storage Layer  │
│                 │    │                 │    │                 │
│  main.rs        │───▶│  ec_node.rs     │───▶│  ec_blocks.rs   │
│  - Network sim  │    │  - Consensus    │    │  ec_tokens.rs   │
│  - Peer mgmt    │    │  - Message proc │    │  - In-memory    │
│  - Statistics   │    │  - Vote logic   │    │    stores       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                      │
         │              ┌─────────────────┐             │
         │              │  Network Layer  │             │
         │              │                 │             │
         └──────────────│  ec_peers.rs    │─────────────┘
                        │  - Peer routing │
                        │  - Message      │
                        │    delivery     │
                        └─────────────────┘
                                 │
                        ┌─────────────────┐
                        │ Protocol Layer  │
                        │                 │
                        │ ec_interface.rs │
                        │ ec_mempool.rs   │
                        │ - Data structs  │
                        │ - State mgmt    │
                        └─────────────────┘
```

## Core Components

### 1. EcNode (`src/ec_node.rs`)
**Responsibility**: Main consensus node implementation

**Key Features**:
- Message processing and routing
- Consensus voting logic
- Block validation and commitment
- Peer communication interface

**Design Patterns**:
- State machine for consensus phases
- Event-driven message processing
- Asynchronous message handling simulation

### 2. EcMemPool (`src/ec_mempool.rs`)
**Responsibility**: Transaction and block state management

**Key Features**:
- Block lifecycle management (Pending → Commit/Blocked)
- Vote tracking and threshold calculation
- Transaction pool management
- State consistency validation

**Data Structures**:
```rust
enum BlockStatus {
    Pending,    // Awaiting votes
    Commit,     // Reached vote threshold
    Blocked,    // Failed to reach consensus
}
```

### 3. EcPeers (`src/ec_peers.rs`)
**Responsibility**: Peer network management and message routing

**Key Features**:
- Peer discovery and connectivity simulation
- Message delivery with network conditions (delay, loss)
- Routing table management
- Network partition simulation

**Network Model**:
- Configurable peer connectivity (default: 90%)
- Message delay simulation (random delays)
- Packet loss simulation (configurable loss rate)

### 4. Protocol Interface (`src/ec_interface.rs`)
**Responsibility**: Core data structures and protocol definitions

**Key Types**:
```rust
struct Block {
    transactions: Vec<TokenTransaction>,  // Up to TOKENS_PER_BLOCK
    block_id: BlockId,
    creator: PeerId,
}

enum MessageType {
    Vote(BlockId, bool),           // Positive/negative vote
    Query(BlockId),                // Request block info
    Block(Block),                  // Block propagation
    Answer(TokenMapping),          // Query response
}
```

### 5. Storage Layer (`src/ec_blocks.rs`, `src/ec_tokens.rs`)
**Responsibility**: Data persistence and retrieval

**Implementation**:
- In-memory hash-based storage
- Block history tracking
- Token ownership validation
- Thread-safe access patterns (simulation context)

## Consensus Protocol Design

### Voting Mechanism
```
1. Block Creation
   ├─ Node creates block with up to 6 token transactions
   ├─ Block broadcast to connected peers
   └─ Block enters Pending state in mempool

2. Voting Phase
   ├─ Peers validate block contents
   ├─ Send Vote messages (positive/negative)
   ├─ Vote aggregation at each node
   └─ Threshold evaluation (configurable VOTE_THRESHOLD)

3. Commitment Phase
   ├─ Block reaches vote threshold → Commit
   ├─ Block fails threshold → Blocked
   └─ State propagation to network
```

### Message Flow Architecture
```
Query Request:
Node A ──Query(BlockId)──▶ Node B
Node A ◀──Answer(TokenMap)── Node B

Block Propagation:
Creator ──Block──▶ Peer1, Peer2, ..., PeerN
Peers ──Vote──▶ All Connected Peers
```

### Network Simulation Model

**Connectivity**: 
- Each peer connects to ~90% of network (configurable)
- Random connection topology generation
- Support for network partitions and healing

**Message Delivery**:
- Configurable message delays (simulating network latency)
- Packet loss simulation (random drop rate)
- No message reordering or duplication (simplified model)

**Fault Tolerance**:
- Graceful handling of peer failures
- Message timeout and retry mechanisms
- Consensus recovery after network partitions

## Performance Characteristics

### Current Metrics (from Notes.md analysis)
```
Threshold 2, 30% peer connectivity:
- 28 commits over 1000 rounds
- Average: 35 rounds/commit
- 4551 messages/commit
- Message distribution: (Query: 2674, Vote: 122323, Block: 2445, Answer: 0)

Threshold 1, 90% peer connectivity:
- 55 commits over 1000 rounds  
- Average: 18 rounds/commit
- 464 messages/commit
- Message distribution: (Query: 1249, Vote: 23190, Block: 1135, Answer: 0)
```

### Performance Analysis
- **Vote Threshold Impact**: Lower thresholds dramatically reduce message overhead
- **Connectivity Impact**: Higher connectivity improves commit rates and reduces rounds
- **Message Patterns**: Vote messages dominate traffic (>90% of total messages)

## Security Considerations

### Threat Model
- **Honest Majority**: Assumes >50% of peers are honest
- **Network Adversary**: Can delay, drop, or reorder messages
- **Byzantine Tolerance**: Currently limited (no explicit Byzantine fault handling)

### Attack Vectors
1. **Vote Flooding**: Malicious peers sending excessive vote messages
2. **Block Withholding**: Creators not propagating valid blocks
3. **Network Partitioning**: Splitting network to prevent consensus
4. **Double Spending**: Creating conflicting token transactions

### Mitigation Strategies
- Vote threshold mechanisms prevent minority attacks
- Block validation prevents invalid transactions
- Network simulation includes partition recovery
- Token ownership tracking prevents double spending

## Scalability Design

### Current Limitations
- **Memory Growth**: Linear growth with network size and block history
- **Message Complexity**: Quadratic message growth in worst case
- **Processing Overhead**: All nodes process all messages

### Optimization Opportunities
1. **Message Batching**: Combine multiple votes into single message
2. **Selective Propagation**: Route messages based on relevance
3. **State Pruning**: Remove old consensus state after commitment
4. **Hierarchical Consensus**: Multi-level voting for large networks

## Configuration Parameters

### Network Simulation
```rust
const DEFAULT_ROUNDS: usize = 1000;          // Simulation duration
const DEFAULT_PEERS: usize = 2000;           // Network size
const PEER_CONNECTIVITY: f64 = 0.9;          // Connection ratio
const MESSAGE_LOSS_RATE: f64 = 0.1;         // Packet loss probability
```

### Consensus Protocol
```rust
const TOKENS_PER_BLOCK: usize = 6;           // Block size limit
const VOTE_THRESHOLD: usize = 2;             // Votes needed for commit
const MAX_PENDING_BLOCKS: usize = 1000;      // Mempool size limit
```

### Performance Tuning
```rust
const MESSAGE_DELAY_RANGE: (u64, u64) = (10, 100);  // Latency simulation (ms)
const VOTE_TIMEOUT: u64 = 5000;                      // Consensus timeout (ms)
const CLEANUP_INTERVAL: usize = 100;                 // State cleanup frequency
```

## Testing Strategy

### Simulation-Based Validation
- **Correctness**: Verify consensus properties (safety, liveness)
- **Performance**: Measure message overhead and convergence time
- **Fault Tolerance**: Test under network failures and attacks
- **Scalability**: Profile performance across different network sizes

### Test Scenarios
1. **Baseline Performance**: Optimal network conditions
2. **Network Degradation**: Increasing loss rates and delays
3. **Peer Failures**: Random peer disconnections and recoveries
4. **Byzantine Behavior**: Malicious voting and block creation
5. **Scale Testing**: Networks from 100 to 10,000+ peers

## Future Enhancements

### Protocol Improvements
- **Adaptive Thresholds**: Dynamic vote requirements based on network conditions
- **Byzantine Fault Tolerance**: Explicit handling of malicious peers  
- **Parallel Consensus**: Multiple concurrent block commitment
- **Sharding**: Horizontal scaling through network partitioning

### Implementation Enhancements
- **Persistent Storage**: Replace in-memory stores with disk-based storage
- **Network Stack**: Replace simulation with real network protocols
- **Monitoring**: Comprehensive metrics and observability
- **Configuration**: Runtime parameter adjustment and tuning tools