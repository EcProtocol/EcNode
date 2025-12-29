# Commit Chain Design

## Overview

The Commit Chain is a blockchain-like structure that tracks the sequence of committed blocks (transactions) in the ecRust consensus system. Each node maintains its own commit chain and syncs with neighboring peers to build a consistent view of token history.

## Core Concept

**Before committing blocks in `batch.commit()`, we create a "commit block"** containing:
- Block IDs of all committing transactions
- Hash of previous commit block (chain linkage)
- Timestamp (EcTime of commit)
- Hash of entire structure (Blake3)

This creates a verifiable chain of commits that nodes can sync and validate.

## Known Limitations (Simulation Only)

This design document describes the **conceptual architecture** that works for both simulation and production. Current implementation uses simulation simplifications:

1. **BlockId = u64** (Production: Blake3 hash of block contents)
   - Simulation: Sequential assignment, honest participants assumed
   - Production: Content-addressed blocks, prevents grinding attacks
   - Impact on commit chain: "Highest BlockId wins" is secure in production

2. **TokenId = u64** (Production: 256-bit with Hash(TokenId) storage)
   - See [src/ec_interface.rs:14-100](../src/ec_interface.rs#L14-L100) for migration plan
   - Minimal impact on CommitBlock structure

3. **Genesis Blocks: parent = 0**
   - First transaction for any token has `last = 0` (sentinel value)
   - All subsequent transactions point to their actual parent BlockId
   - Constant: `pub const GENESIS_BLOCK_ID: BlockId = 0;`

The commit chain design is architecturally compatible with production requirements - no fundamental changes needed when migrating from simulation types.

## Goals

1. **Bootstrap new nodes**: Allow new nodes to sync token state from neighbors
2. **Continuous validation**: Keep nodes synchronized even when temporarily outside consensus
3. **Fraud detection**: Detect inconsistent token updates across peers
4. **Network resilience**: Provide alternative sync mechanism when elections fail

## Key Design Principles

### Slow Sync is a Feature
- Bootstrap may take **days** of continuous syncing
- This acts as **Sybil resistance** (like PoW address computation)
- Makes each node valuable to its operator
- Prevents mass node creation for network attacks

### Range-Based Storage
- Only store token mappings **in our range** (like mempool line 396)
- Validate entire chain but only persist relevant tokens
- Reduces storage requirements per node

### Continuous Background Operation
- Even when Connected and participating in consensus
- Keep syncing from neighbors to detect missed commits
- Handle churn gracefully (nodes evicted from Connected state)

---

# Data Structures

## CommitBlock

```rust
pub type CommitBlockId = [u8; 32];  // Blake3 hash

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitBlock {
    /// Blake3 hash of (previous + time + committed_blocks)
    pub id: CommitBlockId,

    /// Hash of previous commit block (chain linkage)
    pub previous: CommitBlockId,

    /// Time when these blocks were committed
    pub time: EcTime,

    /// Block IDs (transaction IDs) committed in this commit
    pub committed_blocks: Vec<BlockId>,
}

impl CommitBlock {
    /// Calculate Blake3 hash of this commit block
    pub fn calculate_hash(&self) -> CommitBlockId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.previous);
        hasher.update(&self.time.to_le_bytes());
        for block_id in &self.committed_blocks {
            hasher.update(&block_id.to_le_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    /// Create a new commit block
    pub fn new(previous: CommitBlockId, time: EcTime, committed_blocks: Vec<BlockId>) -> Self {
        let mut block = Self {
            id: [0; 32],
            previous,
            time,
            committed_blocks,
        };
        block.id = block.calculate_hash();
        block
    }
}
```

## Enhanced BlockTime (Token Storage)

**Current:**
```rust
pub struct BlockTime {
    pub block: BlockId,
    pub time: EcTime,
}
```

**Enhanced (with parent tracking for conflict resolution):**
```rust
pub struct BlockTime {
    pub block: BlockId,      // Current head
    pub parent: BlockId,     // Parent of current head (NEW)
    pub time: EcTime,
}
```

This allows conflict resolution without re-fetching blocks.

## Message Types

Add to `ec_interface.rs`:

```rust
pub enum Message {
    // ... existing messages

    /// Query for a commit block by its hash
    QueryCommitBlock {
        block_id: CommitBlockId,
        ticket: MessageTicket,
    },

    /// Response containing a commit block
    CommitBlock {
        block: CommitBlock,
    },

    /// Enhanced Answer with head-of-chain
    Answer {
        answer: TokenMapping,
        signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
        head_of_chain: CommitBlockId,  // NEW FIELD
    },
}
```

**Backward compatibility:** For existing tests, `head_of_chain` can be `[0; 32]` (zero hash).

---

# Backend Traits

## EcCommitChainBackend

```rust
pub trait EcCommitChainBackend {
    /// Lookup a commit block by ID
    fn lookup(&self, id: &CommitBlockId) -> Option<CommitBlock>;

    /// Save a commit block
    fn save(&mut self, block: &CommitBlock);

    /// Get the current head of our chain
    fn get_head(&self) -> Option<CommitBlockId>;

    /// Set the current head of our chain
    fn set_head(&mut self, id: CommitBlockId);

    /// Append a commit block to a peer's sync file (bootstrap only)
    fn append_to_peer_file(&mut self, peer: PeerId, block: &CommitBlock) -> Result<(), std::io::Error>;

    /// Read commit blocks from a peer's file in reverse (oldest first)
    /// Returns up to max_blocks, reading backwards from end of file
    fn read_peer_file_reverse(&self, peer: PeerId, max_blocks: usize) -> Result<Vec<CommitBlock>, std::io::Error>;
}
```

### File Format (Bootstrap Only)

Files stored in `/tmp/ecrust_sync/` (or in-memory for simulation):

```
File: /tmp/ecrust_sync/{peer_id}.chain
Format: [MARKER][SIZE][CommitBlock][MARKER][SIZE][CommitBlock]...

MARKER: 0xEC (1 byte) - identifies commit block boundaries
SIZE: u32 (4 bytes LE) - size of following CommitBlock
CommitBlock: bincode-serialized CommitBlock structure
```

Reading backwards:
1. Seek to end of file
2. Read backwards to find MARKER
3. Read SIZE
4. Read CommitBlock
5. Repeat

**Note:** Files are temporary, discarded after bootstrap. Production uses RocksDB.

---

# Module Structure

## EcCommitChain (ec_commit_chain.rs)

```rust
pub struct EcCommitChain {
    peer_id: PeerId,
    my_range: PeerRange,

    /// Sync state machine
    state: SyncState,

    /// Peers we're currently syncing from
    tracked_peers: TrackedPeerSet,

    /// Fraud evidence log
    fraud_log: Vec<(EcTime, FraudEvidence)>,

    /// Configuration
    config: CommitChainConfig,
}

#[derive(Debug, Clone)]
pub struct CommitChainConfig {
    /// Maximum age to sync (e.g., 30 days = 30 * 24 * 60 * 60 ticks)
    pub max_sync_age: EcTime,

    /// Number of peers to track on each side of ring
    pub peers_per_side: usize,  // default: 2

    /// How often to refresh tracked peers (ticks)
    pub peer_refresh_interval: EcTime,  // default: 3600 (1 hour)

    /// Blocks to fetch per tick during bootstrap
    pub commit_blocks_per_tick: usize,  // default: 10

    /// Transaction blocks to fetch per tick
    pub tx_blocks_per_tick: usize,  // default: 50

    /// How long to retain fraud evidence (e.g., 7 days in ticks)
    pub fraud_log_retention: EcTime,  // default: 7 * 24 * 3600

    /// Bootstrap stall timeout (if no progress, refresh peers)
    pub sync_stall_timeout: EcTime,  // default: 300 (5 minutes)
}

#[derive(Debug)]
pub enum SyncState {
    /// Discovering sync sources (waiting for Answers with heads)
    Discovering {
        since: EcTime,
    },

    /// Downloading commit chain history
    Downloading {
        target_depth: EcTime,
        fetched_count: usize,
    },

    /// Fetching and applying transaction blocks
    Applying {
        commit_blocks: Vec<CommitBlock>,  // Sorted oldest → newest
        next_index: usize,
        blocks_fetched: HashSet<BlockId>,
    },

    /// Caught up - continuous sync mode
    Active {
        last_sync: EcTime,
    },
}

#[derive(Debug)]
pub struct TrackedPeerSet {
    /// Peers ahead of us on ring (clockwise)
    clockwise: Vec<(PeerId, CommitBlockId)>,

    /// Peers behind us on ring (counter-clockwise)
    counter_clockwise: Vec<(PeerId, CommitBlockId)>,

    /// Last time we refreshed this set
    last_refresh: EcTime,
}

#[derive(Debug, Clone)]
pub enum FraudEvidence {
    /// Peer provided inconsistent token update
    InconsistentTokenUpdate {
        peer: PeerId,
        token: TokenId,
        our_block: BlockId,
        their_block: BlockId,
        commit_block: CommitBlockId,
    },

    /// Peer's chain has invalid linkage
    InvalidChainLink {
        peer: PeerId,
        block: CommitBlockId,
        claimed_previous: CommitBlockId,
        reason: String,
    },
}
```

### Key Methods

```rust
impl EcCommitChain {
    /// Main tick function - returns messages to send
    pub fn tick(
        &mut self,
        backend: &mut dyn EcCommitChainBackend,
        token_storage: &mut dyn TokenStorageBackend,
        time: EcTime,
    ) -> Vec<MessageEnvelope>;

    /// Handle incoming CommitBlock message
    pub fn handle_commit_block(
        &mut self,
        block: CommitBlock,
        sender: PeerId,
    );

    /// Handle QueryCommitBlock - return block if we have it
    pub fn handle_query_commit_block(
        &self,
        backend: &dyn EcCommitChainBackend,
        block_id: CommitBlockId,
    ) -> Option<CommitBlock>;

    /// Create a new commit block for our commits
    pub fn create_commit_block(
        &mut self,
        backend: &mut dyn EcCommitChainBackend,
        committed_blocks: Vec<BlockId>,
        time: EcTime,
    ) -> CommitBlock;

    /// Update tracked peer's head (from Answer messages)
    pub fn update_peer_head(
        &mut self,
        peer: PeerId,
        head: CommitBlockId,
    );

    /// Select peers to track based on ring position
    fn refresh_tracked_peers(
        &mut self,
        peers: &EcPeers,
        time: EcTime,
    );
}
```

---

# Integration with Existing Code

## ec_mempool.rs - Batch Commit

**Current behavior (line 392):**
```rust
batch.save_block(block);
batch.update_token(&block.parts[i].token, &block.id, block.time);
```

**Enhanced with CommitChain:**
```rust
// Collect all committed block IDs
let mut committed_blocks = Vec::new();

for evaluation in evaluations {
    // ... existing logic ...
    if block_state.remaining == 0 && block_state.validate == 0 {
        batch.save_block(block);
        committed_blocks.push(block.id);  // NEW

        for i in 0..block.used as usize {
            if my_range.in_range(&block.parts[i].token) {
                batch.update_token(
                    &block.parts[i].token,
                    &block.id,
                    block.parts[i].last,  // NEW: parent
                    block.time
                );
            }
        }
        block_state.state = BlockState::Commit;
    }
}

// After all commits, create commit block
if !committed_blocks.is_empty() {
    let commit_block = self.commit_chain.create_commit_block(
        &mut self.commit_chain_backend,
        committed_blocks,
        time,
    );
    batch.save_commit_block(&commit_block);  // NEW
}
```

## ec_interface.rs - StorageBatch Enhancement

```rust
pub trait StorageBatch {
    fn save_block(&mut self, block: &Block);

    fn update_token(
        &mut self,
        token: &TokenId,
        block: &BlockId,
        parent: &BlockId,  // NEW
        time: EcTime
    );

    fn save_commit_block(&mut self, commit_block: &CommitBlock);  // NEW
}
```

## ec_node.rs - Message Handlers

```rust
// In handle_answer
Message::Answer { answer, signature, head_of_chain } => {
    // ... existing logic ...

    // Update commit chain tracker
    self.commit_chain.update_peer_head(sender, head_of_chain);
}

// NEW handlers
Message::QueryCommitBlock { block_id, ticket } => {
    if let Some(block) = self.commit_chain.handle_query_commit_block(
        &self.commit_chain_backend,
        block_id,
    ) {
        self.send_message(sender, Message::CommitBlock { block });
    }
}

Message::CommitBlock { block } => {
    self.commit_chain.handle_commit_block(block, sender);
}
```

## ec_peers.rs - Metadata Storage

Add to `MemPeer` struct (line 134):

```rust
struct MemPeer {
    state: PeerState,
    head_of_chain: Option<CommitBlockId>,  // NEW
    last_head_update: Option<EcTime>,      // NEW
}
```

---

# Bootstrap Process

## Detailed Flow

### Phase 1: Peer Discovery
```
1. Start elections via EcPeers
2. Send token queries
3. Receive Answers with head_of_chain fields
4. Collect (peer_id, head_of_chain) tuples
```

### Phase 2: Select Sync Sources
```
1. Filter peers by state: at least Pending (won elections)
2. Find 2-4 peers closest to us on ring:
   - 1-2 clockwise (higher peer IDs wrapping around)
   - 1-2 counter-clockwise (lower peer IDs)
3. Store in TrackedPeerSet
```

### Phase 3: Download Commit Chains
```
For each tracked peer:
    1. Query their head_of_chain
    2. Follow previous links backwards
    3. Append to peer's sync file: /tmp/ecrust_sync/{peer_id}.chain
    4. Stop when: block.time < (now - max_sync_age)

Throttling: max commit_blocks_per_tick requests
```

### Phase 4: Fetch Transaction Blocks
```
1. Read all peer files in reverse (oldest → newest)
2. Merge by timestamp, maintaining time order
3. For each CommitBlock:
   - For each block_id in committed_blocks:
     - QueryBlock to get full Block structure
     - Store in temporary buffer

Throttling: max tx_blocks_per_tick requests
```

### Phase 5: Validate and Apply
```
1. Process blocks in strict time order
2. For each Block:
   - For each token in block.parts:
     - If token in our range:
       a. Check if we have parent mapping
       b. Validate: parent matches block.parts[i].last
       c. If conflict (two blocks → same parent):
          → Apply "highest block_id wins" rule
       d. If inconsistent:
          → Log FraudEvidence
       e. Update our token_storage (with parent)

3. Build our own CommitChain as we apply
```

### Phase 6: Transition to Active
```
Once we've applied enough history:
    - Can answer token queries in our range
    - Can win elections
    - Transition to SyncState::Active
    - Continue background syncing
```

---

# Continuous Sync (Active Mode)

Even when Connected and participating in consensus:

```rust
// Every peer_refresh_interval ticks:
1. Re-select tracked peers (handle churn)
2. Query their current heads
3. If new commits detected:
   - Fetch new CommitBlocks
   - Fetch new transaction Blocks
   - Validate and apply
4. Detect conflicts/fraud

// On each Answer received:
1. Extract head_of_chain
2. Compare to tracked peer's known head
3. If newer: trigger sync
```

This keeps nodes synchronized even when:
- Temporarily evicted from Connected state
- Missing consensus votes due to network issues
- Recovering from downtime

---

# Conflict Resolution

## The Problem

Two blocks claim the same parent:

```
Token T's chain:

Peer A claims: Block 0x1000 → Block 0x2000 (parent: 0x1000)
Peer B claims: Block 0x1000 → Block 0x3000 (parent: 0x1000)

Which one is correct?
```

## The Rule: Highest Block ID Wins

```rust
fn apply_block_update(
    &mut self,
    token: TokenId,
    new_block: BlockId,
    parent_block: BlockId,
    time: EcTime,
    source_peer: PeerId,
) -> Result<(), FraudEvidence> {
    // Only process tokens in our range
    if !self.my_range.in_range(&token) {
        return Ok(());
    }

    let current = self.token_storage.lookup(&token);

    match current {
        None => {
            // First time seeing this token - accept as root
            self.token_storage.set(&token, &new_block, &parent_block, time);
            Ok(())
        }

        Some(current) if current.block == parent_block => {
            // Normal case: extends our known chain
            self.token_storage.set(&token, &new_block, &parent_block, time);
            Ok(())
        }

        Some(current) if current.parent == parent_block => {
            // CONFLICT: Two blocks point to same parent
            // Rule: Highest block_id (lexical) wins

            if new_block > current.block {
                // New block wins - replace
                self.token_storage.set(&token, &new_block, &parent_block, time);

                self.fraud_log.push((
                    time,
                    FraudEvidence::InconsistentTokenUpdate {
                        peer: source_peer,
                        token,
                        our_block: current.block,
                        their_block: new_block,
                        commit_block: [0; 32], // Fill in actual commit block
                    }
                ));
            }
            // Else: our block wins, ignore update

            Ok(())
        }

        Some(current) => {
            // Inconsistent: new block doesn't extend our chain
            // Either we're behind, or peer is fraudulent

            Err(FraudEvidence::InconsistentTokenUpdate {
                peer: source_peer,
                token,
                our_block: current.block,
                their_block: new_block,
                commit_block: [0; 32],
            })
        }
    }
}
```

## Why Highest ID Wins?

1. **Deterministic**: All nodes reach same conclusion
2. **No trusted timestamps**: Can't trust peer-provided times
3. **Simple**: No complex voting or weighted schemes
4. **Good enough**: Over time, surviving nodes align through elections

## Fraud Detection

We log conflicts but **do not immediately blacklist**:

```rust
// Just log for now
self.fraud_log.push((time, evidence));

// Future enhancements:
// - Count fraud instances per peer
// - Reduce peer quality score
// - Eventually evict persistent offenders
// - Propagate fraud evidence to network
```

---

# Tick Behavior

## Bootstrap Mode
```rust
match &mut self.state {
    SyncState::Discovering { since } => {
        // Wait for Answer messages to populate tracked_peers
        if tracked_peers.has_enough() || time - since > DISCOVERY_TIMEOUT {
            transition_to(SyncState::Downloading { ... });
        }
    }

    SyncState::Downloading { target_depth, fetched_count } => {
        // Fetch next batch of CommitBlocks
        let requests = Vec::new();
        for peer in &tracked_peers {
            if can_request_more() {
                requests.push(QueryCommitBlock { ... });
            }
        }

        if all_peers_synced_to_depth() {
            transition_to(SyncState::Applying { ... });
        }
    }

    SyncState::Applying { commit_blocks, next_index, .. } => {
        // Process next batch of blocks
        let mut applied = 0;
        while applied < tx_blocks_per_tick && next_index < commit_blocks.len() {
            let commit_block = &commit_blocks[next_index];
            for block_id in &commit_block.committed_blocks {
                if let Some(block) = fetch_block(block_id) {
                    apply_block_to_storage(block);
                    applied += 1;
                }
            }
            next_index += 1;
        }

        if next_index >= commit_blocks.len() {
            transition_to(SyncState::Active { last_sync: time });
        }
    }

    SyncState::Active { last_sync } => {
        // Continuous background sync
        if time - last_sync > peer_refresh_interval {
            refresh_tracked_peers();
            query_peer_heads();
        }

        // Prune old fraud evidence periodically (every hour)
        if time % 3600 == 0 {
            prune_old_fraud_evidence(time);
        }

        // If new heads detected, fetch and apply incrementally
    }
}

fn prune_old_fraud_evidence(&mut self, current_time: EcTime) {
    let cutoff = current_time.saturating_sub(self.config.fraud_log_retention);
    self.fraud_log.retain(|(time, _)| *time >= cutoff);
}
```

## Performance Considerations

### Bootstrap Duration

**Simulation (Fast Ticks):**
- Tick duration: ~1ms
- History depth: 30 days (configurable via `max_sync_age`)
- Estimated commits: ~2.6M commit blocks (assumes 1 commit/second)
- Transaction blocks: ~260M (assumes 100 blocks/commit)
- Throttling: 10 CommitBlocks + 50 Blocks per tick
- **Bootstrap time: 1-2 hours**

**Production (Network Latency):**
- Tick duration: ~100ms (network round-trip time)
- History depth: Configurable (30 days to years)
- **Bootstrap time: Hours to days** depending on configured depth
- Combined with PoW address computation: First day is bootstrapping

**Design Philosophy:**
The slow sync is **intentional** (Sybil resistance):
- Fast enough: New nodes can join in reasonable time
- Slow enough: Prevents script-kiddie attacks (can't spawn 1000 nodes quickly)
- Receiver overhead minimal: Simple key/value lookup + UDP packet send

### Active Mode
- **Minimal overhead**: Most ticks send no commit chain messages
- Periodic head checks: Every ~hour (configurable)
- Incremental sync: Only fetch new commits since last check
- Fraud log pruning: Every hour, retain 7 days (configurable)

---

# Testing Strategy

## Unit Tests

1. **CommitBlock hashing**
   - Verify hash calculation
   - Verify chain linkage

2. **Conflict resolution**
   - Same parent, different blocks
   - Highest ID wins
   - Fraud logging

3. **File I/O**
   - Write commit blocks
   - Read backwards
   - Marker detection

## Integration Tests

1. **Bootstrap simulation**
   - New node syncing from 2-4 peers
   - Applying blocks in order
   - Building own chain

2. **Conflict scenarios**
   - Two peers with conflicting chains
   - Resolution converges

3. **Churn handling**
   - Peer eviction and re-tracking
   - Continuous sync while outside Connected

## Simulation Tests

1. **Network-scale bootstrap**
   - 1000 nodes, 100 new nodes join
   - Measure sync time
   - Verify convergence

2. **Fraud injection**
   - Some peers provide bad chains
   - Honest nodes detect and log
   - System converges to consistent state

---

# Open Questions / Future Work

1. **Jump references**: For efficient traversal, add skip pointers?
   ```rust
   pub jump_1min: Option<CommitBlockId>,
   pub jump_1hour: Option<CommitBlockId>,
   ```

2. **Merkle roots**: Add merkle_root for committed_blocks to enable proofs?

3. **Signatures**: Should CommitBlocks be signed by creator for accountability?

4. **Fraud propagation**: Share FraudEvidence with other nodes?

5. **Bootstrap optimization**: Can we parallelize sync from multiple peers?

6. **State snapshots**: Periodically publish full state snapshots for faster bootstrap?

7. **Pruning**: How far back do we keep our own commit chain?

---

# Mathematical Properties

## Convergence Theorem (Informal)

**Claim**: If honest nodes consistently apply "highest block ID wins" rule, the network converges to a consistent token state.

**Intuition**:
1. For any token T with competing blocks B₁, B₂ pointing to same parent P:
   - All nodes deterministically pick max(B₁, B₂)
2. Nodes sync from multiple sources → see all competing blocks
3. Over time, losing blocks are discarded
4. Surviving blocks form consistent chain

**Open**: Formalize this with Byzantine fault tolerance assumptions.

## Time Complexity

Bootstrap sync:
- Download: O(depth × peers) commit blocks
- Apply: O(depth × blocks_per_commit × tokens_per_block)
- Storage: O(tokens_in_range × depth)

Where:
- depth = max_sync_age (e.g., 30 days)
- peers = 2-4 tracked peers
- blocks_per_commit ≈ varies with network activity

---

# Conclusion

The Commit Chain design provides:

✅ **Bootstrap mechanism** for new nodes
✅ **Continuous sync** for churned nodes
✅ **Fraud detection** through multi-peer validation
✅ **Sybil resistance** through slow sync requirement
✅ **Network resilience** through redundant sync sources

The system accepts that synchronization is slow (days) as a **feature that increases network security**.

By tracking only tokens in our range and using deterministic conflict resolution, nodes converge to a consistent state over time, even in the presence of some fraudulent peers.

---

**Next Step**: Review this design, discuss any concerns, then begin Phase 1 implementation.
