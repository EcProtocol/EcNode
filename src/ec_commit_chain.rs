//! Commit Chain Module - Two-Slot Sync
//!
//! Implements continuous background synchronization by tracking 4 peer chains
//! (2 above, 2 below on ring) and syncing from newest to oldest.
//!
//! Key principles:
//! - Global watermark (how far back we've synced)
//! - Two-slot persistent storage (current + pending)
//! - Top-down sync (latest → oldest)
//! - Simple 2-state machine per peer
//! - Local protection via mempool delegation
//! - Highest transaction ID wins (deterministic conflict resolution)

use crate::ec_interface::{
    Block, BlockId, CommitBlock, CommitBlockId, EcBlocks, EcCommitChainBackend, EcTime, EcTokensV2,
    MessageTicket, PeerId, StorageBatch, TokenId, GENESIS_BLOCK_ID,
};
use crate::ec_mempool::EcMemPool;
use crate::ec_peers::PeerRange;
use std::collections::{HashMap, HashSet};

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone)]
pub struct CommitChainConfig {
    /// Initial sync target (e.g., 30 days back)
    pub sync_target: EcTime,
}

impl Default for CommitChainConfig {
    fn default() -> Self {
        Self {
            sync_target: 30 * 24 * 3600, // 30 days
        }
    }
}

// ============================================================================
// Data Structures
// ============================================================================

/// Trace state machine (per peer)
#[derive(Debug, Clone)]
enum TraceState {
    /// Waiting for CommitBlock response
    WaitingForCommit {
        requested_id: CommitBlockId,
        /// Tick counter for retries (retry every 10 ticks)
        ticks_waiting: u32,
    },

    /// Fetching blocks from a CommitBlock
    FetchingBlocks {
        commit_block: CommitBlock,
        /// Blocks we're still waiting for
        waiting_for: HashSet<BlockId>,
    },
}

/// Tracks a single peer's commit chain
#[derive(Debug, Clone)]
struct PeerChainLog {
    peer_id: PeerId,
    /// Current head we know about
    known_head: Option<CommitBlockId>,
    /// Active trace (if any)
    current_trace: Option<TraceState>,
    /// Time of first (oldest) CommitBlock in current trace
    /// Used to update global watermark when trace completes
    first_commit_time: Option<EcTime>,
}

impl PeerChainLog {
    fn new(peer_id: PeerId, head: CommitBlockId) -> Self {
        Self {
            peer_id,
            known_head: Some(head),
            current_trace: None,
            first_commit_time: None,
        }
    }

    fn head_changed(&self, new_head: CommitBlockId) -> bool {
        self.known_head.map_or(true, |h| h != new_head)
    }
}

// ============================================================================
// Main Structure
// ============================================================================

pub struct EcCommitChain {
    peer_id: PeerId,
    my_range: PeerRange,
    config: CommitChainConfig,

    /// Track 4 peers (2 above, 2 below on ring)
    peer_logs: HashMap<PeerId, PeerChainLog>,

    /// Blocks to store (block-id in range, but no tokens in range)
    blocks_to_store: HashMap<BlockId, Block>,

    /// Received blocks (shared across all peer logs)
    /// Blocks arrive via routing, not necessarily from tracking peers
    received_blocks: HashMap<BlockId, Block>,

    /// Global watermark: how far back we've synced
    /// Starts at sync_target, moves forward (deeper) as traces complete
    watermark: EcTime,

    /// Secret for generating tickets
    ticket_secret: u64,
}

// ============================================================================
// Sync Operation Types
// ============================================================================

/// Represents a sync update to apply during batch commit
enum SyncOperation {
    /// Update token via sync (two-slot state machine)
    UpdateTokenSync {
        token: TokenId,
        block: BlockId,
        parent: BlockId,
        time: EcTime,
        source_peer: PeerId,
    },
    /// Save block (no tokens in our range, but block-id in range)
    SaveBlock(Block),
    /// Delegate to mempool (Local protection)
    DelegateToMempool(Block),
}

impl EcCommitChain {
    pub fn new(peer_id: PeerId, my_range: PeerRange, config: CommitChainConfig) -> Self {
        // Simple ticket secret (in production: crypto random)
        let ticket_secret = peer_id.wrapping_mul(0x9e3779b97f4a7c15);

        Self {
            peer_id,
            my_range,
            watermark: 0,
            config,
            peer_logs: HashMap::new(),
            blocks_to_store: HashMap::new(),
            received_blocks: HashMap::new(),
            ticket_secret,
        }
    }

    // ========================================================================
    // Ticket Generation & Verification
    // ========================================================================
    //
    // Note: These tickets are for QueryCommitBlock/CommitBlock messages only.
    // Regular Block messages use the unified TicketManager in ec_node.rs.

    fn generate_ticket(&self, id: u64) -> MessageTicket {
        let combined = id.wrapping_add(self.ticket_secret);
        combined.wrapping_mul(0x9e3779b97f4a7c15)
    }

    fn verify_ticket(&self, id: u64, ticket: MessageTicket) -> bool {
        self.generate_ticket(id) == ticket
    }

    // ========================================================================
    // Peer Tracking
    // ========================================================================

    /// Update tracked peers: drop inactive, add new if below 4
    ///
    /// Strategy:
    /// 1. Check current tracked peers - drop if not active (Pending or Connected)
    /// 2. If below 4 peers: find closest active peers and add them
    /// 3. Update commit chain heads for tracked peers
    fn update_tracked_peers(&mut self, peers: &crate::ec_peers::EcPeers) {
        // Step 1: Drop inactive peers
        let to_drop: Vec<PeerId> = self
            .peer_logs
            .keys()
            .filter(|peer_id| !peers.is_active(peer_id))
            .copied()
            .collect();

        for peer_id in to_drop {
            self.peer_logs.remove(&peer_id);
        }

        // Step 2: Add new peers if below 4
        while self.peer_logs.len() < 4 {
            // Find closest active peers to our peer_id
            let candidates = peers.find_closest_active_peers(self.peer_id, 10);

            // Filter out already tracked peers and not-active peers
            let new_candidates: Vec<_> = candidates
                .iter()
                .filter(|pid| {
                    !self.peer_logs.contains_key(*pid)
                        && **pid != self.peer_id
                        && peers.is_active(pid)
                })
                .copied()
                .collect();

            if new_candidates.is_empty() {
                break; // No more candidates available
            }

            // Add closest candidate
            if let Some((candidate, head)) = new_candidates.into_iter().find_map(|candidate| {
                peers
                    .get_commit_chain_head(&candidate)
                    .map(|head| (candidate, head))
            }) {
                self.peer_logs
                    .insert(candidate, PeerChainLog::new(candidate, head));
            } else {
                break;
            }
        }

        // Step 3: Update heads for tracked peers
        for peer_id in self.peer_logs.keys().copied().collect::<Vec<_>>() {
            if let Some(new_head) = peers.get_commit_chain_head(&peer_id) {
                if let Some(log) = self.peer_logs.get_mut(&peer_id) {
                    if log.head_changed(new_head) {
                        log.known_head = Some(new_head);
                    }
                }
            }
        }
    }

    // ========================================================================
    // Message Handlers
    // ========================================================================

    /// Handle incoming CommitBlock
    ///
    /// Tracks which peer committed which blocks
    pub fn handle_commit_block(
        &mut self,
        block: CommitBlock,
        sender: PeerId,
        ticket: MessageTicket,
        block_storage: &dyn EcBlocks,
    ) -> bool {
        // Verify ticket
        if !self.verify_ticket(block.id, ticket) {
            return false;
        }

        // Only process if (still) tracking this peer
        let log = match self.peer_logs.get_mut(&sender) {
            Some(l) => l,
            None => return false,
        };

        // Must be waiting for this CommitBlock
        let matches = match &log.current_trace {
            Some(TraceState::WaitingForCommit { requested_id, .. }) => *requested_id == block.id,
            _ => false,
        };

        if !matches {
            return false;
        }

        // Track the oldest commit time seen in this trace as we walk backwards.
        log.first_commit_time = Some(block.time);

        // Filter out blocks already committed locally
        let mut waiting_for = HashSet::new();
        for block_id in &block.committed_blocks {
            if block_storage.lookup(block_id).is_none() {
                waiting_for.insert(*block_id);
            }
        }

        // Transition to FetchingBlocks
        log.current_trace = Some(TraceState::FetchingBlocks {
            commit_block: block,
            waiting_for,
        });

        true
    }

    /// Handle incoming Block
    ///
    /// Blocks arrive via routing from any peer (not necessarily tracking peers).
    /// Multiple logs may need the same block, so we store in shared pool.
    pub fn handle_block(&mut self, block: Block, _ticket: MessageTicket) -> bool {
        // Note: Ticket validation is now handled by TicketManager in ec_node.rs
        // This method is only called after ticket has been validated

        // Just store in shared pool
        // Will be applied to storage in process_peer_logs
        self.received_blocks.insert(block.id, block);

        true
    }

    /// Collect sync operations from received blocks (read phase)
    ///
    /// Reads storage state to determine what operations are needed.
    /// Returns list of operations to apply and work items for trace updates.
    fn collect_sync_operations<S>(
        &self,
        storage: &S,
    ) -> (Vec<SyncOperation>, Vec<(PeerId, CommitBlock)>)
    where
        S: EcTokensV2,
    {
        let mut operations = Vec::new();
        let mut work: Vec<(PeerId, CommitBlock)> = Vec::new();

        // Collect work from peer logs
        for (peer_id, log) in &self.peer_logs {
            if let Some(TraceState::FetchingBlocks {
                commit_block,
                waiting_for,
            }) = &log.current_trace
            {
                // Check if any blocks have arrived
                let has_new_blocks = waiting_for
                    .iter()
                    .any(|id| self.received_blocks.contains_key(id));
                if has_new_blocks || waiting_for.is_empty() {
                    work.push((*peer_id, commit_block.clone()));
                }
            }
        }

        // Collect operations for each block
        for (peer_id, commit_block) in &work {
            for block_id in &commit_block.committed_blocks {
                if let Some(block) = self.received_blocks.get(block_id) {
                    // Check tokens in our range
                    let tokens_in_range: Vec<_> = (0..block.used as usize)
                        .filter(|&i| self.my_range.in_range(&block.parts[i].token))
                        .collect();

                    if tokens_in_range.is_empty() {
                        // No tokens in our range - check if block-id in range
                        if self.my_range.in_range(&block.id) {
                            operations.push(SyncOperation::SaveBlock(*block));
                        }
                        continue;
                    }

                    // Check for Local protection
                    let mut delegate_to_mempool = false;
                    for &i in &tokens_in_range {
                        let token = block.parts[i].token;
                        if storage.is_local(&token) {
                            if let Some(current_block) =
                                storage.lookup_state(&token).and_then(|s| s.current_block())
                            {
                                if block.id > current_block {
                                    // Local exists and new block is higher
                                    delegate_to_mempool = true;
                                    break;
                                }
                            }
                        }
                    }

                    if delegate_to_mempool {
                        operations.push(SyncOperation::DelegateToMempool(*block));
                    } else {
                        // Normal sync updates
                        for &i in &tokens_in_range {
                            let token = block.parts[i].token;
                            let parent = block.parts[i].last;

                            // Skip if Local (already handled above, or block.id <= current)
                            if !storage.is_local(&token) {
                                operations.push(SyncOperation::UpdateTokenSync {
                                    token,
                                    block: block.id,
                                    parent,
                                    time: block.time,
                                    source_peer: *peer_id,
                                });
                            }
                        }
                        operations.push(SyncOperation::SaveBlock(*block));
                    }
                }
            }
        }

        (operations, work)
    }

    /// Apply sync operations to batch and mempool (write phase)
    fn apply_sync_operations(
        operations: &[SyncOperation],
        batch: &mut dyn StorageBatch,
        mempool: &mut EcMemPool,
        time: EcTime,
    ) {
        for op in operations {
            match op {
                SyncOperation::UpdateTokenSync {
                    token,
                    block,
                    parent,
                    time: block_time,
                    source_peer,
                } => {
                    batch.update_token_sync(token, block, parent, *block_time, *source_peer);
                }
                SyncOperation::SaveBlock(block) => {
                    batch.save_block(block);
                }
                SyncOperation::DelegateToMempool(block) => {
                    mempool.block(block, time);
                }
            }
        }
    }

    /// Update peer logs after processing (advance traces, update watermark)
    fn update_peer_logs_after_sync(&mut self, work: Vec<(PeerId, CommitBlock)>, time: EcTime) {
        let cutoff = time.saturating_sub(self.config.sync_target);

        for (peer_id, commit_block) in work {
            let log = match self.peer_logs.get_mut(&peer_id) {
                Some(l) => l,
                None => continue,
            };

            if let Some(TraceState::FetchingBlocks { waiting_for, .. }) = &mut log.current_trace {
                // Mark received blocks as done
                waiting_for.retain(|block_id| !self.received_blocks.contains_key(block_id));

                // Check if trace complete
                if waiting_for.is_empty() {
                    if commit_block.time <= self.watermark.max(cutoff)
                        || commit_block.previous == GENESIS_BLOCK_ID
                    {
                        // Trace complete! Update global watermark
                        if let Some(first_time) = log.first_commit_time {
                            self.watermark = self.watermark.max(first_time);
                        }
                        log.current_trace = None;
                        log.first_commit_time = None;
                    } else {
                        // Request previous CommitBlock (going backwards)
                        log.current_trace = Some(TraceState::WaitingForCommit {
                            requested_id: commit_block.previous,
                            ticks_waiting: 0,
                        });
                    }
                }
            }
        }
    }

    /// Create a new commit block for our commits
    pub fn create_commit_block(
        &self,
        backend: &dyn EcCommitChainBackend,
        committed_blocks: Vec<BlockId>,
        time: EcTime,
    ) -> CommitBlock {
        let previous = backend.get_head().unwrap_or(GENESIS_BLOCK_ID);

        // Generate ID (in production: Blake3 hash)
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hash, Hasher};
        let random_state = RandomState::new();
        let mut hasher = random_state.build_hasher();
        self.peer_id.hash(&mut hasher);
        time.hash(&mut hasher);
        previous.hash(&mut hasher);
        let id = hasher.finish();

        CommitBlock::new(id, previous, time, committed_blocks)
    }

    // ========================================================================
    // Tick Function
    // ========================================================================

    /// Main tick function
    ///
    /// Returns list of (receiver, message_type, data) for node to send
    pub fn tick<S>(
        &mut self,
        peers: &crate::ec_peers::EcPeers,
        storage: &mut S,
        mempool: &mut EcMemPool,
        time: EcTime,
    ) -> Vec<(PeerId, TickMessage)>
    where
        S: EcTokensV2 + crate::ec_interface::BatchedBackend,
    {
        let mut messages = Vec::new();

        // Update tracked peers (drop inactive, add new if below 4)
        self.update_tracked_peers(peers);

        // Phase 1: Collect operations (reads storage, no mutations)
        let (operations, work) = self.collect_sync_operations(storage);

        // Phase 2: Create batch and apply operations
        let mut batch = storage.begin_batch();

        // Apply collected sync operations
        Self::apply_sync_operations(&operations, &mut *batch, mempool, time);

        // Save blocks in our block-id range (no tokens in range)
        for block in self.blocks_to_store.values() {
            batch.save_block(block);
        }

        // Commit batch
        if let Err(e) = batch.commit() {
            eprintln!("Error committing batch: {:?}", e);
        } else {
            // Clear blocks_to_store on successful commit
            self.blocks_to_store.clear();
        }

        // Phase 3: Update peer logs (advance traces, update watermark)
        self.update_peer_logs_after_sync(work, time);

        // Generate requests for each peer's trace

        // Collect work to do (without holding mutable borrows)
        let mut start_traces = Vec::new();
        let mut retry_commits = Vec::new();
        let mut query_blocks = Vec::new();

        for (peer_id, log) in &self.peer_logs {
            match &log.current_trace {
                None => {
                    if let Some(head) = log.known_head {
                        start_traces.push((*peer_id, head));
                    }
                }
                Some(TraceState::WaitingForCommit {
                    requested_id,
                    ticks_waiting,
                }) => {
                    if *ticks_waiting % 10 == 0 {
                        retry_commits.push((*peer_id, *requested_id));
                    }
                }
                Some(TraceState::FetchingBlocks { waiting_for, .. }) => {
                    let blocks: Vec<BlockId> = waiting_for
                        .iter()
                        .filter(|id| !self.received_blocks.contains_key(id))
                        .copied()
                        .collect();
                    if !blocks.is_empty() {
                        query_blocks.push((*peer_id, blocks));
                    }
                }
            }
        }

        // Execute work (now safe to call generate_ticket and mutate)
        for (peer_id, head) in start_traces {
            let ticket = self.generate_ticket(head);
            messages.push((
                peer_id,
                TickMessage::QueryCommitBlock {
                    block_id: head,
                    ticket,
                },
            ));
            if let Some(log) = self.peer_logs.get_mut(&peer_id) {
                log.current_trace = Some(TraceState::WaitingForCommit {
                    requested_id: head,
                    ticks_waiting: 0,
                });
            }
        }

        for (peer_id, block_id) in retry_commits {
            let ticket = self.generate_ticket(block_id);
            messages.push((peer_id, TickMessage::QueryCommitBlock { block_id, ticket }));
        }

        for (peer_id, blocks) in query_blocks {
            for block_id in blocks {
                let ticket = self.generate_ticket(block_id);
                messages.push((peer_id, TickMessage::QueryBlock { block_id, ticket }));
            }
        }

        // Update tick counters
        for log in self.peer_logs.values_mut() {
            if let Some(TraceState::WaitingForCommit { ticks_waiting, .. }) = &mut log.current_trace
            {
                *ticks_waiting += 1;
            }
        }

        messages
    }

    /// Get blocks that need to be stored on next batch commit
    pub fn take_blocks_to_store(&mut self) -> HashMap<BlockId, Block> {
        std::mem::take(&mut self.blocks_to_store)
    }

    /// Get current watermark (how far back we've synced)
    pub fn watermark(&self) -> EcTime {
        self.watermark
    }

    /// Get number of active traces
    pub fn active_traces(&self) -> usize {
        self.peer_logs
            .values()
            .filter(|log| log.current_trace.is_some())
            .count()
    }
}

// ============================================================================
// Message Types (for tick return)
// ============================================================================

#[derive(Debug, Clone)]
pub enum TickMessage {
    QueryCommitBlock {
        block_id: CommitBlockId,
        ticket: MessageTicket,
    },
    QueryBlock {
        block_id: BlockId,
        ticket: MessageTicket,
    },
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ec_interface::{
        BatchedBackend, BlockTime, EcTokens, PendingMapping, TokenId, TokenSignature, TokenState,
        TrustSource, TrustedMapping,
    };
    use crate::ec_peers::EcPeers;
    use std::collections::{HashMap, HashSet};

    struct MockTokenStorage {
        tokens: HashMap<TokenId, TokenState>,
    }

    impl MockTokenStorage {
        fn new() -> Self {
            Self {
                tokens: HashMap::new(),
            }
        }
    }

    impl EcTokens for MockTokenStorage {
        fn lookup(&self, _token: &TokenId) -> Option<&BlockTime> {
            // For compatibility - return None, use lookup_state instead
            None
        }

        fn set(&mut self, token: &TokenId, block: &BlockId, parent: &BlockId, time: EcTime) {
            self.tokens.insert(
                *token,
                TokenState {
                    current: Some(TrustedMapping {
                        block: *block,
                        parent: *parent,
                        time,
                        source: TrustSource::Local,
                    }),
                    pending: None,
                },
            );
        }

        fn tokens_signature(&self, _token: &TokenId, _peer: &PeerId) -> Option<TokenSignature> {
            None
        }
    }

    impl EcTokensV2 for MockTokenStorage {
        fn lookup_state(&self, token: &TokenId) -> Option<TokenState> {
            self.tokens.get(token).cloned()
        }

        fn lookup_current(&self, token: &TokenId) -> Option<TrustedMapping> {
            self.tokens.get(token).and_then(|s| s.current)
        }

        fn is_local(&self, token: &TokenId) -> bool {
            self.tokens.get(token).map_or(false, |s| s.is_local())
        }
    }

    // Simple batch implementation for tests
    struct MockBatch<'a> {
        storage: &'a mut MockTokenStorage,
        blocks: Vec<Block>,
        sync_updates: Vec<(TokenId, BlockId, BlockId, EcTime, PeerId)>,
        local_updates: Vec<(TokenId, BlockId, BlockId, EcTime)>,
    }

    impl<'a> StorageBatch for MockBatch<'a> {
        fn save_block(&mut self, block: &Block) {
            self.blocks.push(*block);
        }

        fn update_token(
            &mut self,
            token: &TokenId,
            block: &BlockId,
            parent: &BlockId,
            time: EcTime,
        ) {
            self.local_updates.push((*token, *block, *parent, time));
        }

        fn update_token_sync(
            &mut self,
            token: &TokenId,
            block: &BlockId,
            parent: &BlockId,
            time: EcTime,
            source_peer: PeerId,
        ) {
            self.sync_updates
                .push((*token, *block, *parent, time, source_peer));
        }

        fn commit(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
            // Apply local updates (become Local)
            for (token, block, parent, time) in &self.local_updates {
                self.storage.set(token, block, parent, *time);
            }

            // Apply sync updates (two-slot logic)
            for (token, block, parent, time, source_peer) in &self.sync_updates {
                let state = self.storage.tokens.entry(*token).or_default();

                match (&state.current, &state.pending) {
                    (None, None) => {
                        // First seen - create pending
                        state.pending = Some(PendingMapping {
                            block: *block,
                            parent: *parent,
                            time: *time,
                            source_peer: *source_peer,
                        });
                    }
                    (None, Some(p)) => {
                        if *block == p.block && *source_peer != p.source_peer {
                            // Confirmation! Promote to current
                            state.current = Some(TrustedMapping {
                                block: p.block,
                                parent: p.parent,
                                time: p.time,
                                source: TrustSource::Confirmed,
                            });
                            state.pending = None;
                        } else if *block > p.block {
                            // Higher ID replaces pending
                            state.pending = Some(PendingMapping {
                                block: *block,
                                parent: *parent,
                                time: *time,
                                source_peer: *source_peer,
                            });
                        }
                    }
                    (Some(c), pending) => {
                        if *block <= c.block {
                            // Not newer than current, ignore
                            continue;
                        }
                        // Newer block - handle based on pending
                        match pending {
                            None => {
                                state.pending = Some(PendingMapping {
                                    block: *block,
                                    parent: *parent,
                                    time: *time,
                                    source_peer: *source_peer,
                                });
                            }
                            Some(p) if *block == p.block && *source_peer != p.source_peer => {
                                // Confirms pending
                                state.current = Some(TrustedMapping {
                                    block: p.block,
                                    parent: p.parent,
                                    time: p.time,
                                    source: TrustSource::Confirmed,
                                });
                                state.pending = None;
                            }
                            Some(p) if *block > p.block => {
                                state.pending = Some(PendingMapping {
                                    block: *block,
                                    parent: *parent,
                                    time: *time,
                                    source_peer: *source_peer,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(())
        }

        fn block_count(&self) -> usize {
            self.blocks.len()
        }
    }

    impl crate::ec_interface::BatchedBackend for MockTokenStorage {
        fn begin_batch(&mut self) -> Box<dyn StorageBatch + '_> {
            Box::new(MockBatch {
                storage: self,
                blocks: Vec::new(),
                sync_updates: Vec::new(),
                local_updates: Vec::new(),
            })
        }
    }

    #[test]
    fn test_two_slot_confirmation() {
        use crate::ec_interface::{TokenBlock, TOKENS_PER_BLOCK};

        let mut storage = MockTokenStorage::new();

        // Create a block
        let mut block = Block {
            id: 100,
            time: 1000,
            used: 1,
            parts: [TokenBlock::default(); TOKENS_PER_BLOCK],
            signatures: [None; TOKENS_PER_BLOCK],
        };
        block.parts[0].token = 50;
        block.parts[0].last = GENESIS_BLOCK_ID;

        // Apply from peer 1 via batch
        {
            let mut batch = storage.begin_batch();
            batch.update_token_sync(&50, &100, &GENESIS_BLOCK_ID, 1000, 1);
            batch.commit().unwrap();
        }

        // Should be pending
        let state = storage.lookup_state(&50).unwrap();
        assert!(state.current.is_none());
        assert!(state.pending.is_some());
        assert_eq!(state.pending.unwrap().block, 100);

        // Apply same block from peer 2 (confirmation)
        {
            let mut batch = storage.begin_batch();
            batch.update_token_sync(&50, &100, &GENESIS_BLOCK_ID, 1000, 2);
            batch.commit().unwrap();
        }

        // Should be confirmed now
        let state = storage.lookup_state(&50).unwrap();
        assert!(state.current.is_some());
        assert!(state.pending.is_none());
        assert_eq!(state.current.unwrap().block, 100);
        assert_eq!(state.current.unwrap().source, TrustSource::Confirmed);
    }

    #[test]
    fn test_highest_id_wins() {
        let mut storage = MockTokenStorage::new();

        // Apply block 100 from peer 1
        {
            let mut batch = storage.begin_batch();
            batch.update_token_sync(&50, &100, &GENESIS_BLOCK_ID, 1000, 1);
            batch.commit().unwrap();
        }

        // Apply block 200 from peer 2 (higher ID wins)
        {
            let mut batch = storage.begin_batch();
            batch.update_token_sync(&50, &200, &GENESIS_BLOCK_ID, 1000, 2);
            batch.commit().unwrap();
        }

        // Pending should be block 200
        let state = storage.lookup_state(&50).unwrap();
        assert!(state.current.is_none());
        assert_eq!(state.pending.unwrap().block, 200);

        // Apply block 200 from peer 3 (confirmation)
        {
            let mut batch = storage.begin_batch();
            batch.update_token_sync(&50, &200, &GENESIS_BLOCK_ID, 1000, 3);
            batch.commit().unwrap();
        }

        // Should be confirmed with block 200
        let state = storage.lookup_state(&50).unwrap();
        assert_eq!(state.current.unwrap().block, 200);
    }

    #[test]
    fn test_block_in_range_no_tokens() {
        use crate::ec_interface::{TokenBlock, TOKENS_PER_BLOCK};

        let my_range = PeerRange::new(0, 1000);
        let mut chain = EcCommitChain::new(500, my_range, CommitChainConfig::default());

        // Block with token outside range, but block-id in range
        let mut block = Block {
            id: 50,
            time: 1000,
            used: 1,
            parts: [TokenBlock::default(); TOKENS_PER_BLOCK],
            signatures: [None; TOKENS_PER_BLOCK],
        };
        block.parts[0].token = 2000; // Token outside range

        // Store in received_blocks
        chain.received_blocks.insert(block.id, block);

        // Set up a peer log to reference this block
        let commit_block = CommitBlock::new(999, GENESIS_BLOCK_ID, 1000, vec![block.id]);
        chain.peer_logs.insert(
            100,
            PeerChainLog {
                peer_id: 100,
                known_head: Some(999),
                current_trace: Some(TraceState::FetchingBlocks {
                    commit_block,
                    waiting_for: [block.id].into_iter().collect(),
                }),
                first_commit_time: Some(1000),
            },
        );

        // Collect sync operations
        let storage = MockTokenStorage::new();
        let (operations, _work) = chain.collect_sync_operations(&storage);

        // Should have a SaveBlock operation for the block
        let save_count = operations
            .iter()
            .filter(|op| matches!(op, SyncOperation::SaveBlock(b) if b.id == 50))
            .count();
        assert_eq!(
            save_count, 1,
            "Block should be saved since block-id is in range"
        );
    }

    #[test]
    fn test_update_tracked_peers_skips_active_peers_without_heads() {
        let my_range = PeerRange::new(0, 1000);
        let mut chain = EcCommitChain::new(100, my_range, CommitChainConfig::default());
        let mut peers = EcPeers::new(100);

        peers.update_peer(&110, 0);
        peers.update_peer(&120, 0);
        peers.update_peer_commit_chain_head(&120, 999);

        chain.update_tracked_peers(&peers);

        assert!(!chain.peer_logs.contains_key(&110));
        assert!(chain.peer_logs.contains_key(&120));
    }

    #[test]
    fn test_empty_waiting_for_advances_trace_without_new_blocks() {
        let my_range = PeerRange::new(0, 1000);
        let mut chain = EcCommitChain::new(500, my_range, CommitChainConfig::default());

        let commit_block = CommitBlock::new(900, 800, 25, vec![10, 20]);
        chain.peer_logs.insert(
            42,
            PeerChainLog {
                peer_id: 42,
                known_head: Some(commit_block.id),
                current_trace: Some(TraceState::FetchingBlocks {
                    commit_block: commit_block.clone(),
                    waiting_for: HashSet::new(),
                }),
                first_commit_time: Some(commit_block.time),
            },
        );

        chain.update_peer_logs_after_sync(vec![(42, commit_block)], 50);

        let log = chain.peer_logs.get(&42).unwrap();
        match log.current_trace.as_ref() {
            Some(TraceState::WaitingForCommit { requested_id, .. }) => {
                assert_eq!(*requested_id, 800);
            }
            _ => panic!("trace should advance to the previous commit block"),
        }
    }
}
