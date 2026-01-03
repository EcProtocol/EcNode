//! Commit Chain Module - Simplified Top-Down Sync
//!
//! Implements continuous background synchronization by tracking 4 peer chains
//! (2 above, 2 below on ring) and syncing from newest to oldest.
//!
//! Key principles:
//! - Global watermark (how far back we've synced)
//! - Shadow mappings with multi-peer confirmation
//! - Top-down sync (latest → oldest)
//! - Simple 2-state machine per peer
//! - Check Shadow first (fast), then DB (expensive)

use crate::ec_interface::{
    Block, BlockId, CommitBlock, CommitBlockId, EcBlocks, EcCommitChainBackend, EcTime, EcTokens,
    MessageTicket, PeerId, TokenId, GENESIS_BLOCK_ID,
};
use crate::ec_peers::PeerRange;
use std::collections::{HashMap, HashSet};

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone)]
pub struct CommitChainConfig {
    /// Initial sync target (e.g., 30 days back)
    pub sync_target: EcTime,

    /// Minimum confirmations to promote shadow to DB
    pub confirmation_threshold: usize,

    /// Maximum requests per tick (removed - not needed with only 4 peers)
    /// Kept for backward compatibility, not used
    pub max_requests_per_tick: usize,
}

impl Default for CommitChainConfig {
    fn default() -> Self {
        Self {
            sync_target: 30 * 24 * 3600, // 30 days
            confirmation_threshold: 2,   // 2 peers must agree
            max_requests_per_tick: 20,   // Not used anymore, kept for compatibility
        }
    }
}

// ============================================================================
// Data Structures
// ============================================================================

/// Shadow token mapping - tracks most recent update with confirmations
#[derive(Debug, Clone)]
struct ShadowTokenMapping {
    block: BlockId,
    parent: BlockId,
    time: EcTime,
    /// Peers that confirmed this exact (block, parent, time) tuple
    confirmations: HashSet<PeerId>,
}

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

    /// Shadow mappings (unconfirmed tokens)
    shadows: HashMap<TokenId, ShadowTokenMapping>,

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

impl EcCommitChain {
    pub fn new(peer_id: PeerId, my_range: PeerRange, config: CommitChainConfig) -> Self {
        // Simple ticket secret (in production: crypto random)
        let ticket_secret = peer_id.wrapping_mul(0x9e3779b97f4a7c15);

        Self {
            peer_id,
            my_range,
            watermark: config.sync_target, // Initial watermark
            config,
            peer_logs: HashMap::new(),
            shadows: HashMap::new(),
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
    // Shadow Logic
    // ========================================================================

    /// Apply a block to shadow system
    ///
    /// peer_id is added as a confirmation when creating/updating shadow.
    ///
    /// Strategy:
    /// 1. Check if any tokens in our range
    /// 2. If no tokens but block-id in range: store block
    /// 3. For each token in range:
    ///    - Check Shadow first (fast HashMap lookup)
    ///    - If in Shadow: apply conflict resolution
    ///      - Same block-id → count confirmation
    ///      - Same parent → highest block-id wins
    ///      - Different parent → highest time wins
    ///    - If not in Shadow: check DB (expensive)
    ///      - If DB has token and block is newer → create shadow
    ///      - If DB has token and is newer/same → ignore (DB wins)
    ///      - If DB doesn't have token → create shadow
    fn apply_block_to_shadow(&mut self, block: &Block, storage: &dyn EcTokens, peer_id: PeerId) {
        // Check if any tokens in our range
        let tokens_in_range = block.parts[0..block.used as usize]
            .iter()
            .any(|tb| self.my_range.in_range(&tb.token));

        if !tokens_in_range {
            // No tokens in our range - check if block-id in range
            if self.my_range.in_range(&block.id) {
                // Must store for serving to others
                self.blocks_to_store.insert(block.id, block.clone());
            }
            return;
        }

        // Process tokens in our range
        for i in 0..block.used as usize {
            let token = block.parts[i].token;
            if !self.my_range.in_range(&token) {
                continue;
            }

            let parent = block.parts[i].last;

            // Check Shadow first (fast HashMap lookup)
            if let Some(shadow) = self.shadows.get_mut(&token) {
                // Same block-id → count confirmation
                if block.id == shadow.block {
                    shadow.confirmations.insert(peer_id);
                    continue;
                }

                // Same parent → conflict: highest block-id wins
                if parent == shadow.parent {
                    if block.id > shadow.block {
                        shadow.block = block.id;
                        shadow.time = block.time;
                        shadow.confirmations.clear();
                        shadow.confirmations.insert(peer_id);
                    }
                    // Else: shadow has higher block-id, ignore
                } else {
                    // Different parent → check if newer time
                    if block.time > shadow.time {
                        shadow.block = block.id;
                        shadow.parent = parent;
                        shadow.time = block.time;
                        shadow.confirmations.clear();
                        shadow.confirmations.insert(peer_id);
                    }
                    // Else: shadow is newer, ignore
                }
            } else {
                // Not in shadow - check DB (expensive)
                if let Some(current) = storage.lookup(&token) {
                    // DB has this token - only create shadow if block is newer
                    if block.time > current.time() {
                        let mut confirmations = HashSet::new();
                        confirmations.insert(peer_id);
                        self.shadows.insert(
                            token,
                            ShadowTokenMapping {
                                block: block.id,
                                parent,
                                time: block.time,
                                confirmations,
                            },
                        );
                    }
                    // Else: DB is newer or same time, ignore (DB wins even if conflict)
                } else {
                    // First time seeing this token - create shadow
                    let mut confirmations = HashSet::new();
                    confirmations.insert(peer_id);
                    self.shadows.insert(
                        token,
                        ShadowTokenMapping {
                            block: block.id,
                            parent,
                            time: block.time,
                            confirmations,
                        },
                    );
                }
            }
        }
    }

    /// Promote shadows that have enough confirmations
    ///
    /// Creates a batch with:
    /// - Token updates for promoted shadows
    /// - Block saves for all dependent blocks (from received_blocks)
    /// - Block saves for blocks in our range (from blocks_to_store)
    fn promote_shadows(&mut self, storage: &mut dyn crate::ec_interface::BatchedBackend) {
        // Collect shadows to promote
        let mut to_promote = Vec::new();
        for (token, shadow) in &self.shadows {
            if shadow.confirmations.len() >= self.config.confirmation_threshold {
                to_promote.push(*token);
            }
        }

        // If nothing to promote and no blocks to store, skip
        if to_promote.is_empty() && self.blocks_to_store.is_empty() {
            return;
        }

        // Start batch
        let mut batch = storage.begin_batch();

        // Promote shadows: update tokens and save dependent blocks
        for token in to_promote {
            if let Some(shadow) = self.shadows.remove(&token) {
                // Update token mapping
                batch.update_token(&token, &shadow.block, &shadow.parent, shadow.time);

                // Save the block if we have it
                if let Some(block) = self.received_blocks.remove(&shadow.block) {
                    batch.save_block(&block);
                }
            }
        }

        // Save all blocks in our block-id range (but no tokens in range)
        for block in self.blocks_to_store.values() {
            batch.save_block(block);
        }

        // Commit batch
        if let Err(e) = batch.commit() {
            // Log error but continue - shadows were removed so we won't retry
            eprintln!("Error committing batch: {:?}", e);
        } else {
            // Clear blocks_to_store on successful commit
            self.blocks_to_store.clear();
        }
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
            if let Some(&candidate) = new_candidates.first() {
                if let Some(head) = peers.get_commit_chain_head(&candidate) {
                    self.peer_logs
                        .insert(candidate, PeerChainLog::new(candidate, head));
                } else {
                    // No commit chain head available yet, skip
                    break;
                }
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
    /// Tracks which peer committed which blocks (for confirmations later)
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

        // Track first (latest) commit time
        if log.first_commit_time.is_none() {
            log.first_commit_time = Some(block.time);
        }

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
    ///
    /// Note: apply_block_to_shadow happens in process_peer_logs, not here.
    /// This allows the peer-chain to be counted as a confirmation.
    pub fn handle_block(&mut self, block: Block, _ticket: MessageTicket) -> bool {
        // Note: Ticket validation is now handled by TicketManager in ec_node.rs
        // This method is only called after ticket has been validated

        // Just store in shared pool
        // Will be applied to shadow in process_peer_logs
        self.received_blocks.insert(block.id, block);

        true
    }

    /// Process received blocks for all peer logs
    ///
    /// Called during tick to advance traces based on received blocks
    fn process_peer_logs(&mut self, token_storage: &dyn EcTokens) {
        // Step 1: Collect blocks to process from peer logs (without mutating)
        let mut work: Vec<(PeerId, CommitBlock)> = Vec::new();

        for (peer_id, log) in &self.peer_logs {
            if let Some(TraceState::FetchingBlocks {
                commit_block,
                waiting_for,
            }) = &log.current_trace
            {
                // Check if all blocks have arrived
                let all_received = waiting_for
                    .iter()
                    .all(|id| self.received_blocks.contains_key(id));
                if all_received || !waiting_for.is_empty() {
                    work.push((*peer_id, commit_block.clone()));
                }
            }
        }

        // Step 2: Apply blocks to shadows (can mutate shadows safely)
        for (peer_id, commit_block) in &work {
            for block_id in &commit_block.committed_blocks {
                if let Some(block) = self.received_blocks.get(block_id).cloned() {
                    // Apply to shadow with peer confirmation
                    // This will add the peer to confirmations when creating/updating shadow
                    self.apply_block_to_shadow(&block, token_storage, *peer_id);
                }
            }
        }

        // Step 3: Update peer logs (advance traces, update watermark)
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
                    if commit_block.time < self.watermark
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
        _time: EcTime,
    ) -> Vec<(PeerId, TickMessage)>
    where
        S: crate::ec_interface::EcTokens + crate::ec_interface::BatchedBackend,
    {
        let mut messages = Vec::new();

        // Update tracked peers (drop inactive, add new if below 4)
        self.update_tracked_peers(peers);

        // Process received blocks (advance traces, add confirmations)
        // Use immutable reference for read-only token lookups
        self.process_peer_logs(storage as &dyn crate::ec_interface::EcTokens);

        // Generate requests for each peer's trace
        // No budget - only 4 peers max with limited blocks each

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

        // Promote shadows (batched write operation)
        self.promote_shadows(storage);

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
    use std::collections::HashMap;

    struct MockTokenStorage {
        tokens: HashMap<TokenId, crate::ec_interface::BlockTime>,
    }

    impl MockTokenStorage {
        fn new() -> Self {
            Self {
                tokens: HashMap::new(),
            }
        }
    }

    impl EcTokens for MockTokenStorage {
        fn lookup(&self, token: &TokenId) -> Option<&crate::ec_interface::BlockTime> {
            self.tokens.get(token)
        }

        fn set(&mut self, token: &TokenId, block: &BlockId, parent: &BlockId, time: EcTime) {
            self.tokens.insert(
                *token,
                crate::ec_interface::BlockTime::new(*block, *parent, time),
            );
        }

        fn tokens_signature(
            &self,
            _token: &TokenId,
            _peer: &PeerId,
        ) -> Option<crate::ec_interface::TokenSignature> {
            None // Not needed for commit chain tests
        }
    }

    // Simple batch implementation for tests
    struct MockBatch<'a> {
        storage: &'a mut MockTokenStorage,
        blocks: Vec<Block>,
        tokens: Vec<(TokenId, BlockId, BlockId, EcTime)>,
    }

    impl<'a> crate::ec_interface::StorageBatch for MockBatch<'a> {
        fn save_block(&mut self, block: &Block) {
            self.blocks.push(*block);
        }

        fn update_token(&mut self, token: &TokenId, block: &BlockId, parent: &BlockId, time: EcTime) {
            self.tokens.push((*token, *block, *parent, time));
        }

        fn commit(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
            for (token, block, parent, time) in &self.tokens {
                self.storage.set(token, block, parent, *time);
            }
            Ok(())
        }

        fn block_count(&self) -> usize {
            self.blocks.len()
        }
    }

    impl crate::ec_interface::BatchedBackend for MockTokenStorage {
        fn begin_batch(&mut self) -> Box<dyn crate::ec_interface::StorageBatch + '_> {
            Box::new(MockBatch {
                storage: self,
                blocks: Vec::new(),
                tokens: Vec::new(),
            })
        }
    }

    #[test]
    fn test_shadow_confirmation() {
        use crate::ec_interface::{TokenBlock, TOKENS_PER_BLOCK};

        let my_range = PeerRange::new(0, 1000);
        let mut chain = EcCommitChain::new(500, my_range, CommitChainConfig::default());
        let mut storage = MockTokenStorage::new();

        // Create a block
        let mut block = Block {
            id: 100,
            time: 1000,
            used: 1,
            parts: [TokenBlock::default(); TOKENS_PER_BLOCK],
            signatures: [None; TOKENS_PER_BLOCK],
        };
        block.parts[0].token = 50; // In range
        block.parts[0].last = GENESIS_BLOCK_ID;

        // Apply from peer 1
        chain.apply_block_to_shadow(&block, &storage, 1);
        assert_eq!(chain.shadows.len(), 1);
        assert_eq!(chain.shadows[&50].confirmations.len(), 1);

        // Apply same block from peer 2 (confirmation)
        chain.apply_block_to_shadow(&block, &storage, 2);
        assert_eq!(chain.shadows.len(), 1);
        assert_eq!(chain.shadows[&50].confirmations.len(), 2);

        // Promote (threshold = 2)
        chain.promote_shadows(&mut storage);
        assert_eq!(chain.shadows.len(), 0);
        assert!(storage.lookup(&50).is_some());
        assert_eq!(storage.lookup(&50).unwrap().block(), 100);
    }

    #[test]
    fn test_conflict_resolution() {
        use crate::ec_interface::{TokenBlock, TOKENS_PER_BLOCK};

        let my_range = PeerRange::new(0, 1000);
        let mut chain = EcCommitChain::new(500, my_range, CommitChainConfig::default());
        let mut storage = MockTokenStorage::new();

        // First block
        let mut block1 = Block {
            id: 100,
            time: 1000,
            used: 1,
            parts: [TokenBlock::default(); TOKENS_PER_BLOCK],
            signatures: [None; TOKENS_PER_BLOCK],
        };
        block1.parts[0].token = 50;
        block1.parts[0].last = GENESIS_BLOCK_ID;

        chain.apply_block_to_shadow(&block1, &storage, 1);

        // Conflicting block (higher ID wins)
        let mut block2 = Block {
            id: 200,
            time: 1000,
            used: 1,
            parts: [TokenBlock::default(); TOKENS_PER_BLOCK],
            signatures: [None; TOKENS_PER_BLOCK],
        };
        block2.parts[0].token = 50;
        block2.parts[0].last = GENESIS_BLOCK_ID;

        chain.apply_block_to_shadow(&block2, &storage, 2);

        // Should have block2 (200 > 100)
        assert_eq!(chain.shadows[&50].block, 200);
        assert_eq!(chain.shadows[&50].confirmations.len(), 1); // Peer 2 confirmed the winner
    }

    #[test]
    fn test_block_not_in_token_range_but_in_block_range() {
        use crate::ec_interface::{TokenBlock, TOKENS_PER_BLOCK};

        let my_range = PeerRange::new(0, 1000);
        let mut chain = EcCommitChain::new(500, my_range, CommitChainConfig::default());
        let storage = MockTokenStorage::new();

        // Block with token outside range, but block-id in range
        let mut block = Block {
            id: 50,
            time: 1000,
            used: 1,
            parts: [TokenBlock::default(); TOKENS_PER_BLOCK],
            signatures: [None; TOKENS_PER_BLOCK],
        };
        block.parts[0].token = 2000; // Token outside range

        chain.apply_block_to_shadow(&block, &storage, 999);

        // Should store block for serving
        assert_eq!(chain.blocks_to_store.len(), 1);
        assert!(chain.blocks_to_store.contains_key(&50));
        assert_eq!(chain.shadows.len(), 0);
    }
}
