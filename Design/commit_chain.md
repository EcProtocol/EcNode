# Commit Chain Design

## Overview

The **Commit Chain** is a lightweight blockchain that tracks which transaction blocks each peer has committed to their local database. This enables fast peer synchronization and Byzantine fault tolerance through multi-peer confirmation.

## Core Principles

1. **Top-Down Sync**: Sync from newest to oldest (HEAD → past) to be useful quickly
2. **Shadow System**: Unconfirmed mappings with multi-peer confirmation (no age tracking)
3. **Minimal Tracking**: Only track 4 closest active peers (2 above, 2 below on ring)
4. **Batched Commits**: Atomic storage operations for consistency
5. **Block Routing**: Blocks arrive via network routing, not directly from tracked peers

## Data Structures

### CommitBlock

Each `CommitBlock` contains:
- `id`: Blake3 hash of (previous, time, committed_blocks)
- `previous`: Link to previous CommitBlock
- `time`: When these blocks were committed
- `committed_blocks`: Vec of transaction BlockIds committed in this batch

Forms a blockchain per peer tracking their commit history.

### PeerChainLog

Tracks synchronization state for one peer:
- `peer_id`: Which peer we're tracking
- `known_head`: Latest CommitBlockId we know about
- `current_trace`: Active synchronization state machine
- `first_commit_time`: Time of oldest CommitBlock in current trace

### TraceState (State Machine)

Two states per peer:

```
None ──────> WaitingForCommit ──────> FetchingBlocks
              ↑                          │
              └──────────────────────────┘
```

**WaitingForCommit**:
- Requested a CommitBlock from peer
- `requested_id`: Which CommitBlock we asked for
- `ticks_waiting`: Counter for retry logic (retry every 10 ticks)

**FetchingBlocks**:
- Have the CommitBlock, fetching transaction blocks
- `commit_block`: The CommitBlock we're processing
- `waiting_for`: Set of BlockIds we still need

### ShadowTokenMapping

Unconfirmed token mappings awaiting multi-peer confirmation:
- `block`: Latest BlockId for this token
- `parent`: Parent BlockId (for chain validation)
- `time`: Block timestamp
- `confirmations`: Set of PeerIds that confirmed this mapping

No age tracking - only confirmation count matters.

## Synchronization Flow

### Phase 1: Peer Tracking

**Dynamic Peer Management** (every tick):
1. Drop inactive peers (not Pending or Connected)
2. If below 4 peers: find closest active peers and add them
3. Update commit chain heads for all tracked peers

**Selection Strategy**:
- Query `EcPeers` for closest active peers to our peer_id
- Active = Pending or Connected state
- Maximum 4 peers tracked concurrently

### Phase 2: Top-Down Trace

For each tracked peer, run state machine:

**State: None**
- Start trace at peer's known HEAD
- Send `QueryCommitBlock(head)`
- Transition to `WaitingForCommit`

**State: WaitingForCommit**
- Increment tick counter
- Retry every 10 ticks: send `QueryCommitBlock` again
- On receive CommitBlock → transition to FetchingBlocks

**State: FetchingBlocks**
- Send `QueryBlock` for each block in `committed_blocks` we don't have
- On receive Block → apply to shadow system
- When all blocks received:
  - If `commit_block.time < watermark` or `previous == GENESIS`: Trace complete
  - Else: request previous CommitBlock (going backwards in time)

### Phase 3: Block Processing

**Blocks arrive via routing** (not from tracked peers directly):
- `handle_block(block, ticket)` → store in `received_blocks` pool
- Multiple peer logs may need the same block

**Per-tick processing** (`process_peer_logs`):
1. Collect work from all peer logs
2. Apply received blocks to shadow system with peer confirmation
3. Advance trace states, update watermark when traces complete

### Phase 4: Shadow Management

**Applying blocks to shadows** (`apply_block_to_shadow`):

```
For each token in block:
    if token in shadow:
        if block.id == shadow.block:
            → Add peer to confirmations (same mapping)
        else if block.parent == shadow.parent:
            → Higher block.id wins, clear confirmations, add peer
        else:
            → Higher time wins, clear confirmations, add peer
    else:
        Check DB:
        if block.time > db.time:
            → Create shadow with peer confirmation
        else:
            → Ignore (DB wins even if conflict)
```

**Key invariant**: DB is always trusted over shadows.

### Phase 5: Batched Promotion

**Every tick** (`promote_shadows`):

If shadows to promote OR blocks to store:
1. Start batch: `storage.begin_batch()`
2. For each shadow with enough confirmations:
   - `batch.update_token(token, block, parent, time)`
   - `batch.save_block(block)` - if in received_blocks (REMOVE from pool)
3. For each block in `blocks_to_store`:
   - `batch.save_block(block)` - (block-id in range, no tokens in range)
4. `batch.commit()` - Atomic write
5. Clear `blocks_to_store` on success

**Confirmation threshold**: Default 2 peers (configurable)

## Watermark System

**Global watermark**: Single value tracking sync depth across all peers

**Initialization**: Set to `sync_target` (e.g., 30 days back)

**Updates**: When trace completes:
```rust
if commit_block.time < watermark || commit_block.previous == GENESIS_BLOCK_ID {
    watermark = max(watermark, first_commit_time)
    // Trace complete
}
```

**Direction**: Watermark moves forward (deeper into history) as we sync more

## Block Storage Strategy

Three categories of blocks:

1. **Blocks with tokens in our range**:
   - Stored via shadow promotion
   - Removed from `received_blocks` when promoted

2. **Blocks with block-id in range but no tokens**:
   - Added to `blocks_to_store`
   - Saved during batch promotion
   - Cleared after successful commit

3. **Blocks outside our range**:
   - Not stored (routing only)

## Message Types

### Query Messages
- `QueryCommitBlock { block_id, ticket }` - Request a CommitBlock
- `QueryBlock { block_id, ticket }` - Request a transaction Block

### Response Messages
- `CommitBlock { block }` - Deliver a CommitBlock
- `Block { block }` - Deliver a transaction Block (via routing)

## Ticket System

**Purpose**: Prevent replay attacks and spam

**Generation**:
```rust
fn generate_ticket(id: u64) -> MessageTicket {
    let combined = id.wrapping_add(self.ticket_secret);
    combined.wrapping_mul(0x9e3779b97f4a7c15)
}
```

**Verification**: Must match on receive or message is rejected

## Byzantine Fault Tolerance

### Multi-Peer Confirmation

Shadows require confirmations from multiple independent peers:
- Default threshold: 2 peers
- Peers are geographically distributed on ring (2 above, 2 below)
- Block routing provides cross-network validation

### Conflict Resolution

**Same parent** (fork scenario):
- Highest block-id wins deterministically
- Ensures network converges on same choice

**Different parent** (reorganization):
- Highest timestamp wins
- Reflects most recent state

**DB vs Shadow**:
- DB always wins (committed state is trusted)
- Shadow only created if block.time > db.time

### Attack Resistance

**Sybil Attack**: Limited by peer selection (closest on ring)
**Eclipse Attack**: Multiple independent sync paths (4 peers)
**History Rewrite**: Multi-peer confirmation required for commits
**Spam**: Ticket system prevents replay attacks

## Performance Characteristics

### Memory Overhead

Per peer (worst case):
- `PeerChainLog`: ~128 bytes
- Active traces: ~200 bytes each
- Shadow mappings: ~80 bytes per unconfirmed token
- Received blocks: ~300 bytes per block (temporary)

**Total**: ~1KB per tracked peer + shadows + pending blocks

### Network Overhead

- 4 concurrent traces maximum
- Each trace: 1 CommitBlock + N transaction blocks
- Retry logic: 1 message every 10 ticks for stalled traces
- Block routing: shared with normal network traffic

### Storage Overhead

Per committed shadow:
- Token mapping: set() call to DB
- Block: save_block() call to DB
- Batched atomically for consistency

## Integration Points

### Required Traits

**Storage must implement**:
- `EcTokens` - For shadow DB lookups
- `BatchedBackend` - For atomic batch commits

**Peers must provide**:
- `is_active(peer_id)` - Check if peer is Pending/Connected
- `find_closest_active_peers(target, count)` - Find sync candidates
- `get_commit_chain_head(peer_id)` - Get peer's latest CommitBlock

### Message Handling

**In `EcNode`**:
- Route `QueryCommitBlock` messages to owning peer
- Route `QueryBlock` messages (normal routing)
- Route `CommitBlock` responses to requester
- Route `Block` messages (normal routing)

**In `EcMemoryBackend` (or storage layer)**:
- Call `commit_chain.tick()` each round
- Pass messages to `handle_commit_block()` and `handle_block()`
- Convert `TickMessage` to network messages

## Configuration

```rust
pub struct CommitChainConfig {
    /// Initial sync target (e.g., 30 days back in seconds)
    pub sync_target: EcTime,  // Default: 30 * 24 * 3600

    /// Minimum confirmations to promote shadow to DB
    pub confirmation_threshold: usize,  // Default: 2
}
```

## Example: Synchronization Scenario

**New node joins network**:

1. **Tick 0**: Discover 4 closest active peers
   - Peer 1000 (above)
   - Peer 2000 (above)
   - Peer 8000 (below)
   - Peer 9000 (below)

2. **Tick 1**: Start traces at all 4 heads
   - Send `QueryCommitBlock` to each peer

3. **Tick 3**: Receive CommitBlocks
   - Peer 1000: CommitBlock #5000 (contains blocks 100, 101, 102)
   - Peer 2000: CommitBlock #5001 (contains blocks 100, 103, 104)
   - Request blocks 100-104

4. **Tick 5**: Receive blocks
   - Block 100: confirmed by both peers → shadow gets 2 confirmations
   - Block 101: confirmed by peer 1000 only → shadow gets 1 confirmation
   - Block 103: confirmed by peer 2000 only → shadow gets 1 confirmation
   - Apply all to shadow system

5. **Tick 5**: Promote shadows
   - Block 100 has 2 confirmations → Batch commit to DB
   - Blocks 101, 103 still in shadow (waiting for more confirmations)

6. **Tick 6**: Continue traces backwards
   - Request previous CommitBlocks from all peers
   - Repeat until watermark reached

**Result**: Node is useful immediately with latest tokens, continues syncing history in background.

## Comparison to Traditional Sync

| Aspect | Traditional (Bottom-Up) | Commit Chain (Top-Down) |
|--------|------------------------|-------------------------|
| Time to useful | Hours (must sync all) | Seconds (get latest first) |
| Memory footprint | Large pending state | Small shadow set |
| Fraud detection | Assumes global order | Multi-peer confirmation |
| Byzantine tolerance | Weak | Strong (independent peers) |
| Network efficiency | Sequential | Parallel (4 peers) |
| Storage writes | Incremental | Batched atomic |

## Mathematical Properties

### Convergence

Given:
- `n` active peers in network
- `f` Byzantine (faulty) peers where `f < n/3`
- Confirmation threshold `t = 2`

**Theorem**: If at least 2 honest peers confirm a mapping, the probability of accepting a fraudulent mapping approaches 0 as the network stabilizes.

**Proof sketch**:
- We track 4 peers (2 above, 2 below)
- Probability all 4 are Byzantine: `(f/n)^4`
- For `f < n/3`: This approaches 0 as `n` increases
- With `t=2`: Need at least 2 Byzantine in our set of 4
- Network-wide: Multiple sync paths provide independent verification

### Worst-Case Shadow Size

Maximum shadows per peer = (tokens in range) × (unconfirmed mapping rate)

For a peer with range covering `R` tokens:
- If all tokens update simultaneously: `R` shadows created
- Promotion rate: 1 shadow per tick per token (at threshold)
- Steady state: `R × (arrival_rate / confirmation_rate)` shadows

**Example**: 10,000 tokens, 100 updates/sec, threshold=2, 4 peers
- Shadow arrival: 100/sec
- Confirmation rate: ~50/sec (2 out of 4 peers)
- Steady state: ~2,000 shadows (~160KB)

## Future Enhancements

1. **Adaptive Peer Selection**: Choose peers based on network regions for better fault tolerance
2. **Parallel Block Fetching**: Request blocks from multiple peers simultaneously
3. **Checkpoint System**: Periodic signed checkpoints for faster bootstrap
4. **Pruning**: Archive old CommitBlocks beyond retention period
5. **Compression**: Delta-encode CommitBlocks for bandwidth efficiency

## Implementation Status

✅ **Completed**:
- Top-down synchronization
- Shadow system with multi-peer confirmation
- Dynamic peer tracking (drop inactive, add new)
- Batched atomic commits
- Ticket-based replay protection
- Conflict resolution (same parent → highest block-id, different parent → highest time)
- DB check before shadow creation
- Received blocks pool (shared across traces)
- Retry logic (every 10 ticks)

⏸️ **Pending Integration**:
- Message routing in EcNode
- Backend trait implementation
- Network testing with packet loss/delay
- Performance benchmarking

---

**Document Version**: 1.0
**Last Updated**: 2026-01-03
**Implementation**: [src/ec_commit_chain.rs](../src/ec_commit_chain.rs)
