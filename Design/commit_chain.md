# Commit Chain Design

## Overview

The **Commit Chain** is a lightweight blockchain that tracks which transaction blocks each peer has committed to their local database. This enables fast peer synchronization and Byzantine fault tolerance through multi-peer confirmation.

## Core Principles

1. **Top-Down Sync**: Sync from newest to oldest (HEAD → past) to be useful quickly
2. **Two-Slot Storage**: Persistent current/pending state per token (no in-memory shadows)
3. **Minimal Tracking**: Only track 4 closest active peers (2 above, 2 below on ring)
4. **Batched Commits**: Atomic storage operations for consistency
5. **Local Protection**: Updates past Local state delegate to mempool for full validation
6. **Highest ID Wins**: Deterministic conflict resolution across network

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

### Token State (Two-Slot Model)

Persistent token state in storage:

```rust
struct TokenState {
    /// Trusted state: Confirmed or Local
    current: Option<TrustedMapping>,

    /// Pending state: Newer than current, awaiting confirmation
    pending: Option<PendingMapping>,
}

struct TrustedMapping {
    block: BlockId,
    parent: BlockId,
    time: EcTime,
    source: TrustSource,  // Confirmed | Local
}

struct PendingMapping {
    block: BlockId,
    parent: BlockId,
    time: EcTime,
    source_peer: PeerId,
}
```

### Trust Hierarchy

```
Local > Confirmed > Pending > None

Local:     Our mempool committed - we KNOW this is correct
Confirmed: Two peers agree - high confidence
Pending:   One peer reported - unverified, never served
```

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
- On receive Block → apply to storage
- When all blocks received:
  - If `commit_block.time < watermark` or `previous == GENESIS`: Trace complete
  - Else: request previous CommitBlock (going backwards in time)

### Phase 3: Block Processing

**Blocks arrive via routing** (not from tracked peers directly):
- `handle_block(block, ticket)` → store in `received_blocks` pool
- Multiple peer logs may need the same block

**Per-tick processing** (`process_peer_logs`):
1. Collect work from all peer logs
2. For each block, check storage state and route appropriately
3. Advance trace states, update watermark when traces complete

### Phase 4: Token Update Routing

**For each token in received block**:

```
┌─────────────────────────────────────────┐
│  Check storage.lookup(token)            │
└─────────────────────────────────────────┘
                    │
        ┌───────────┴───────────┐
        │                       │
        ▼                       ▼
┌───────────────┐       ┌───────────────┐
│ current.is_   │       │ Otherwise     │
│ local() AND   │       │               │
│ block.id >    │       │               │
│ current.block │       │               │
└───────────────┘       └───────────────┘
        │                       │
        ▼                       ▼
┌───────────────┐       ┌───────────────┐
│ DELEGATE TO   │       │ Normal sync:  │
│ MEMPOOL       │       │ update_token_ │
│               │       │ sync(...)     │
└───────────────┘       └───────────────┘
```

**Why delegate to mempool for Local?**
- Local means WE committed this transaction
- Someone claiming a newer block must prove the chain
- Mempool already handles chain validation, voting, consensus
- If mempool commits → becomes new Local (we validated it)

### Phase 5: Storage State Machine

**In `update_token_sync`** (called by commit chain):

```
┌─────────────────────────────────────────────────────────────┐
│ Current State │ Pending │ Event              │ Action       │
├───────────────┼─────────┼────────────────────┼──────────────┤
│ None          │ None    │ Peer A reports     │ pending=(A)  │
│ None          │ P(A)    │ Peer B, same block │ confirm→curr │
│ None          │ P(A)    │ Peer B, higher ID  │ pending=(B)  │
│ None          │ P(A)    │ Peer B, lower ID   │ no change    │
│ Confirmed     │ None    │ Peer, higher ID    │ pending=(P)  │
│ Confirmed     │ P(A)    │ Peer B, same block │ confirm→curr │
│ Confirmed     │ P(A)    │ Peer, higher ID    │ pending=(P)  │
│ Local         │ *       │ Peer, higher ID    │ → MEMPOOL    │
└─────────────────────────────────────────────────────────────┘
```

**In `update_token`** (called by mempool):
- Always sets `current = Local(block)`
- Always clears `pending = None`
- Mempool commits are always trusted

### Phase 6: Query Serving

**Only serve current slot**:

```rust
fn answer_query(token: TokenId) -> QueryResponse {
    match storage.lookup_current(token) {
        Some(mapping) => QueryResponse::Found(mapping),
        None => QueryResponse::NotFound,  // pending exists but not served
    }
}
```

Pending mappings are never served - only current (Confirmed or Local).

## Conflict Resolution

**Single rule: Highest transaction ID wins**

```rust
if new_block.id > existing_block.id {
    // New block wins
} else {
    // Keep existing
}
```

This is:
- **Deterministic**: All nodes make same choice
- **Simple**: One comparison
- **Convergent**: Network agrees on highest ID

## Block Storage Strategy

Three categories of blocks:

1. **Blocks with tokens in our range**:
   - Applied via storage state machine
   - Stored when mapping is confirmed

2. **Blocks with block-id in range but no tokens**:
   - Added to `blocks_to_store`
   - Saved during batch commit
   - We must store these to serve as witnesses

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

Updates require confirmation from two independent peers:
- Default threshold: 2 peers (source + 1 confirmer)
- Peers are distributed on ring (2 above, 2 below)
- Block routing provides cross-network validation

### Local Protection

When `current = Local`:
- We committed this via our mempool
- Any update must go through mempool consensus
- Full network validation required
- Prevents attackers from overwriting our commits

### Attack Resistance

**Sybil Attack**: Limited by peer selection (closest on ring)
**Eclipse Attack**: Multiple independent sync paths (4 peers)
**History Rewrite**: Multi-peer confirmation + Local protection
**Spam**: Ticket system prevents replay attacks

### Byzantine Higher ID Attack

Attacker sends fabricated block with very high ID:
1. Block wins on ID (stored as pending)
2. No honest peer confirms it
3. Pending stays unconfirmed forever
4. We never serve pending data
5. Eventually honest blocks with even higher IDs arrive

Result: Attack delays but doesn't corrupt.

## Performance Characteristics

### Memory Overhead

Per peer:
- `PeerChainLog`: ~128 bytes
- Active traces: ~200 bytes each
- Received blocks: ~300 bytes per block (temporary)

**No shadow memory**: State is in persistent storage

### Storage Overhead

Per token (worst case - both slots populated):
```
Current slot:  block(8) + parent(8) + time(8) + source(1) = 25 bytes
Pending slot:  block(8) + parent(8) + time(8) + source_peer(8) = 32 bytes
Total:         57 bytes per token (worst case)
Typical:       25 bytes (only current populated)
```

### Network Overhead

- 4 concurrent traces maximum
- Each trace: 1 CommitBlock + N transaction blocks
- Retry logic: 1 message every 10 ticks for stalled traces
- Local protection: triggers mempool consensus (rare)

## Integration Points

### Required Traits

**Storage must implement**:
- `EcTokensV2` - For two-slot lookups
- `BatchedBackend` - For atomic batch commits with sync support

**Mempool access**:
- `mempool.block(block, time)` - Submit for consensus (Local protection)

**Peers must provide**:
- `is_active(peer_id)` - Check if peer is Pending/Connected
- `find_closest_active_peers(target, count)` - Find sync candidates
- `get_commit_chain_head(peer_id)` - Get peer's latest CommitBlock

### Storage Interface

```rust
pub trait EcTokensV2: EcTokens {
    /// Lookup full token state (both slots)
    fn lookup_state(&self, token: &TokenId) -> Option<TokenState>;

    /// Lookup only current (trusted) - for query serving
    fn lookup_current(&self, token: &TokenId) -> Option<TrustedMapping>;

    /// Check if current state is Local
    fn is_local(&self, token: &TokenId) -> bool;
}

pub trait StorageBatch {
    fn save_block(&mut self, block: &Block);

    /// Update from mempool commit - always becomes Local
    fn update_token(&mut self, token: &TokenId, block: &BlockId,
                    parent: &BlockId, time: EcTime);

    /// Update from sync - handles pending/confirmation logic
    fn update_token_sync(&mut self, token: &TokenId, block: &BlockId,
                         parent: &BlockId, time: EcTime, source_peer: PeerId);

    fn commit(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>>;
    fn block_count(&self) -> usize;
}
```

## Configuration

```rust
pub struct CommitChainConfig {
    /// Initial sync target (e.g., 30 days back in seconds)
    pub sync_target: EcTime,  // Default: 30 * 24 * 3600
}
```

Note: `confirmation_threshold` removed - always 2 (source + 1 confirmer).

## Example: Synchronization Scenario

**New node joins network**:

1. **Tick 0**: Discover 4 closest active peers
   - Peer 1000 (above), Peer 2000 (above)
   - Peer 8000 (below), Peer 9000 (below)

2. **Tick 1**: Start traces at all 4 heads
   - Send `QueryCommitBlock` to each peer

3. **Tick 3**: Receive CommitBlocks
   - Peer 1000: CommitBlock #5000 (contains blocks 100, 101, 102)
   - Peer 2000: CommitBlock #5001 (contains blocks 100, 103, 104)
   - Request blocks 100-104

4. **Tick 5**: Receive blocks, apply to storage
   - Block 100 token T: peer 1000 reports → `pending = (100, 1000)`
   - Block 100 token T: peer 2000 confirms → `current = Confirmed(100)`
   - Block 101 token U: peer 1000 only → stays pending
   - Block 103 token V: peer 2000 only → stays pending

5. **Tick 6**: Continue traces backwards
   - Block 101 may get confirmed by peer 8000 or 9000
   - Repeat until watermark reached

**Result**: Node is useful immediately with confirmed tokens.

## Example: Local Protection

**Node has Local(100) for token T, peer sends block 200**:

1. Commit chain receives block 200 for token T
2. Check storage: `current = Local(100)`, block 200 > 100
3. Delegate to mempool: `mempool.block(block_200, time)`
4. Mempool runs normal consensus:
   - Checks chain: does 200's parent eventually reach 100?
   - Collects votes from network
   - If valid and votes pass → commits
5. On commit: storage gets `update_token(T, 200, ...)` → `current = Local(200)`

**Result**: Our commit is protected, update validated by network.

## Comparison to Shadow System (Previous Design)

| Aspect | Shadow System | Two-Slot Storage |
|--------|--------------|------------------|
| Memory | O(unconfirmed tokens) | O(1) - in DB |
| Crash resilience | Lost | Persisted |
| Max state per token | Unbounded (confirmation set) | 2 slots |
| Local protection | None | Mempool delegation |
| Query serving | Could serve shadows | Only serve current |
| Complexity | Shadow + promote logic | Simple state machine |

## Implementation Status

✅ **Completed**:
- Top-down synchronization
- Dynamic peer tracking (drop inactive, add new)
- Ticket-based replay protection
- Received blocks pool (shared across traces)
- Retry logic (every 10 ticks)

🔄 **In Progress**:
- Two-slot storage state machine
- Mempool delegation for Local protection
- Storage trait extensions

⏸️ **Pending**:
- Network testing with packet loss/delay
- Performance benchmarking

---

**Document Version**: 2.0
**Last Updated**: 2026-03-01
**Implementation**: [src/ec_commit_chain.rs](../src/ec_commit_chain.rs)
**Storage Design**: [Design/unconfirmed_storage.md](./unconfirmed_storage.md)
