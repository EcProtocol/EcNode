//! Commit Chain Module
//!
//! Implements the commit chain system for bootstrapping nodes and maintaining
//! synchronization across the network. Each node builds its own commit chain
//! tracking which transaction blocks were committed.

use crate::ec_interface::{
    BlockId, CommitBlock, CommitBlockId, EcBlocks, EcCommitChainBackend, EcTime, EcTokens,
    Message, MessageEnvelope, MessageTicket, ParentBlockRequest, PeerId, TokenId, GENESIS_BLOCK_ID,
};
use crate::ec_peers::EcPeers;

/// Internal secret for generating message tickets
/// In production this would be cryptographically secure random bytes
type TicketSecret = u64;

/// Configuration for commit chain behavior
#[derive(Debug, Clone)]
pub struct CommitChainConfig {
    /// Maximum age to sync (e.g., 30 days in ticks)
    pub max_sync_age: EcTime,

    /// How long to retain fraud evidence (e.g., 7 days in ticks)
    pub fraud_log_retention: EcTime,

    /// How long to hold shadow mappings before batch commit (e.g., 1000 ticks)
    /// This allows time for multiple peer chains to confirm or reveal conflicts
    pub shadow_commit_age: EcTime,
}

impl Default for CommitChainConfig {
    fn default() -> Self {
        Self {
            max_sync_age: 30 * 24 * 3600,       // 30 days
            fraud_log_retention: 7 * 24 * 3600, // 7 days
            shadow_commit_age: 1000,            // ~1000 ticks to allow peer convergence
        }
    }
}

use std::collections::{HashMap, HashSet};

/// Shadow token mapping - temporarily holds token state changes
/// before committing to our backend. Allows us to wait for consensus
/// across multiple peer chains and detect conflicts.
#[derive(Debug, Clone)]
struct ShadowTokenMapping {
    /// Token ID (redundant with HashMap key but kept for debugging/clarity)
    #[allow(dead_code)]
    token: TokenId,
    /// New block ID for this token
    block: BlockId,
    /// Parent block ID (previous state of this token)
    parent: BlockId,
    /// Time of this mapping
    time: EcTime,
    /// When we first saw this mapping
    first_seen: EcTime,
    /// Which validated blocks contain this mapping (to detect consensus)
    confirmed_by_blocks: HashSet<BlockId>,
}

/// Tracks a single peer's commit chain for sync purposes
#[derive(Debug, Clone)]
struct PeerChainLog {
    /// Their last known head block ID
    last_known_head: CommitBlockId,
    /// Blocks we've collected from this peer (in order)
    /// Last element is the latest block we have
    blocks: Vec<CommitBlock>,
    /// Out-of-order blocks waiting for their parents
    /// Maps parent_id -> block
    orphaned_blocks: HashMap<CommitBlockId, CommitBlock>,
    /// Earliest timestamp we're interested in syncing
    /// Starts at current_time - max_sync_age when we start tracking
    /// Gradually moves forward as we consume blocks
    sync_from_time: EcTime,
}

impl PeerChainLog {
    fn new(head: CommitBlockId, sync_from_time: EcTime) -> Self {
        Self {
            last_known_head: head,
            blocks: Vec::new(),
            orphaned_blocks: HashMap::new(),
            sync_from_time,
        }
    }

    /// Get the ID of the latest block we have in this chain
    fn get_chain_tip(&self) -> Option<CommitBlockId> {
        self.blocks.last().map(|b| b.id)
    }

    /// Try to add a block to the chain
    ///
    /// If it connects to the tip, adds it and checks for orphans that can now connect.
    /// If it doesn't connect, stores it in orphaned_blocks.
    /// Returns true if block was successfully added to the chain.
    fn try_add_block(&mut self, block: CommitBlock) -> bool {
        let chain_tip = self.get_chain_tip();

        // Check if this block connects to our current tip
        if chain_tip == Some(block.previous) || (chain_tip.is_none() && block.previous == GENESIS_BLOCK_ID) {
            // Block connects! Add it to the chain
            self.blocks.push(block.clone());

            // Check if any orphaned blocks can now connect
            let mut connected_something = true;
            while connected_something {
                connected_something = false;
                let current_tip = self.get_chain_tip().unwrap();

                // Look for an orphan that points to current tip
                if let Some(orphan) = self.orphaned_blocks.remove(&current_tip) {
                    self.blocks.push(orphan);
                    connected_something = true;
                }
            }

            true
        } else {
            // Block doesn't connect - store as orphan
            self.orphaned_blocks.insert(block.previous, block);
            false
        }
    }
}

/// Number of peer chains to track concurrently
const MAX_TRACKED_PEERS: usize = 4;

/// Main commit chain coordinator
///
/// Tracks multiple peer chains for sync and creates our own commit blocks.
pub struct EcCommitChain {
    peer_id: PeerId,
    config: CommitChainConfig,

    /// Next commit block ID to assign (sequential in simulation)
    next_commit_block_id: CommitBlockId,

    /// Tracks peer chains we're following for sync (up to 4)
    /// Maps PeerId -> PeerChainLog
    tracked_peers: HashMap<PeerId, PeerChainLog>,

    /// Pending blocks we've requested but not yet validated/committed
    /// Maps BlockId -> Block
    /// These are checked during commit block consumption alongside our backend
    pending_blocks: HashMap<BlockId, crate::ec_interface::Block>,

    /// Validated blocks that have been checked against our token store
    /// Maps BlockId -> Block
    /// These blocks have valid token mappings and can be considered "known"
    validated_blocks: HashMap<BlockId, crate::ec_interface::Block>,

    /// Shadow token mappings - temporarily holds token state changes
    /// Maps TokenId -> ShadowTokenMapping
    /// Allows us to wait for consensus across multiple peer chains before committing
    shadow_token_mappings: HashMap<TokenId, ShadowTokenMapping>,

    /// Secret for generating message tickets
    /// Used to verify that commit blocks and blocks are responses to our requests
    ticket_secret: TicketSecret,
}

impl EcCommitChain {
    /// Create a new commit chain coordinator
    pub fn new(peer_id: PeerId, config: CommitChainConfig) -> Self {
        // Generate a random secret for ticket generation
        // In simulation: use peer_id as seed for determinism
        // In production: use cryptographically secure random
        let ticket_secret = peer_id.wrapping_mul(0x9e3779b97f4a7c15); // Simple hash

        Self {
            peer_id,
            config,
            next_commit_block_id: 1, // Start from 1 (0 is reserved for genesis)
            tracked_peers: HashMap::new(),
            pending_blocks: HashMap::new(),
            validated_blocks: HashMap::new(),
            shadow_token_mappings: HashMap::new(),
            ticket_secret,
        }
    }

    /// Generate a ticket for requesting a specific commit block
    ///
    /// Ticket = Hash(block_id XOR secret)
    /// This allows us to verify that incoming blocks are responses to our requests.
    fn generate_ticket(&self, block_id: CommitBlockId) -> MessageTicket {
        // Simple hash function for simulation
        // In production, use proper cryptographic hash
        let combined = block_id.wrapping_add(self.ticket_secret);
        combined.wrapping_mul(0x9e3779b97f4a7c15)
    }

    /// Verify that a ticket is valid for a given block ID
    ///
    /// Returns true if the ticket matches what we would generate for this block.
    fn verify_ticket(&self, block_id: CommitBlockId, ticket: MessageTicket) -> bool {
        self.generate_ticket(block_id) == ticket
    }

    /// Create a new commit block for committed transaction blocks
    ///
    /// This is called from mempool after committing a batch of blocks.
    /// Returns the created CommitBlock.
    pub fn create_commit_block(
        &mut self,
        backend: &mut dyn EcCommitChainBackend,
        committed_blocks: Vec<BlockId>,
        time: EcTime,
    ) -> CommitBlock {
        // Get previous commit block (head of chain)
        let previous = backend.get_head().unwrap_or(GENESIS_BLOCK_ID);

        // Create new commit block
        let commit_block = CommitBlock::new(
            self.next_commit_block_id,
            previous,
            time,
            committed_blocks,
        );

        // Increment ID for next time
        self.next_commit_block_id += 1;

        // Save to backend
        backend.save(&commit_block);
        backend.set_head(&commit_block.id);

        commit_block
    }

    /// Handle a query for a commit block
    ///
    /// Returns Some(CommitBlock) if we have it, None otherwise.
    pub fn handle_query_commit_block(
        &self,
        backend: &dyn EcCommitChainBackend,
        block_id: CommitBlockId,
    ) -> Option<CommitBlock> {
        backend.lookup(&block_id)
    }

    /// Handle an incoming commit block from a peer
    ///
    /// Processes the block if it's from a tracked peer and ticket is valid:
    /// - Verifies ticket to prevent unsolicited blocks
    /// - Tries to add it to the chain using try_add_block
    /// - If it connects, stores it in the peer log
    /// - If it doesn't connect and we're still tracking this peer, stores as orphan
    /// - Returns optional message to request parent if needed (respecting max_sync_age)
    pub fn handle_commit_block(
        &mut self,
        block: CommitBlock,
        sender: PeerId,
        ticket: MessageTicket,
        _current_time: EcTime,
    ) -> Option<ParentBlockRequest> {
        // Verify ticket first - this proves the block is a response to our request
        if !self.verify_ticket(block.id, ticket) {
            // Invalid ticket - ignore this block (likely spam or attack)
            return None;
        }

        // Only process if we're tracking this peer
        let log = self.tracked_peers.get_mut(&sender)?;

        // Try to add the block
        if log.try_add_block(block.clone()) {
            // Block was added successfully (and maybe some orphans too)
            None
        } else {
            // Block was stored as orphan

            // Check if block is too old to sync (respecting sync_from_time)
            if block.time < log.sync_from_time {
                // Block is too old - don't request parent
                // The orphan is already stored in case we get the parent from another source
                return None;
            }

            // Generate ticket for parent request
            let parent_ticket = self.generate_ticket(block.previous);

            // Return request data - caller will construct the MessageEnvelope
            Some(ParentBlockRequest {
                receiver: sender,
                block_id: block.previous,
                ticket: parent_ticket,
            })
        }
    }

    /// Handle an incoming block from a peer
    ///
    /// Verifies ticket and stores block in pending_blocks for validation.
    /// The block will be validated against token mappings during tick processing.
    pub fn handle_block(
        &mut self,
        block: crate::ec_interface::Block,
        ticket: MessageTicket,
    ) -> bool {
        // Verify ticket first - this proves the block is a response to our request
        if !self.verify_ticket(block.id, ticket) {
            // Invalid ticket - ignore this block (likely spam or attack)
            return false;
        }

        // Add to pending blocks for validation during tick
        self.pending_blocks.insert(block.id, block);
        true
    }

    /// Add or update a token mapping in the shadow
    ///
    /// Creates a new shadow mapping if it doesn't exist, or updates it if this
    /// is a sequential update (newer block). The shadow can track multiple sequential
    /// updates (A -> B -> C), but we only keep the latest. All blocks are tracked
    /// in confirmed_by_blocks for later batch commit.
    ///
    /// Conflict resolution: If we have competing BlockIds at the same time, we pick
    /// the highest lexical BlockId (our consensus rule).
    fn add_to_shadow(
        &mut self,
        token: TokenId,
        block: BlockId,
        parent: BlockId,
        block_time: EcTime,
        current_time: EcTime,
        confirmed_by_block: BlockId,
    ) {
        self.shadow_token_mappings
            .entry(token)
            .and_modify(|shadow| {
                // Shadow exists - check if this is a sequential update, confirmation, or conflict
                if block_time > shadow.time {
                    // This is a newer sequential update (e.g., A->B->C)
                    // Update to the latest mapping and reset first_seen
                    shadow.block = block;
                    shadow.parent = parent;
                    shadow.time = block_time;
                    shadow.first_seen = current_time; // Reset age for new update
                    shadow.confirmed_by_blocks.clear();
                    shadow.confirmed_by_blocks.insert(confirmed_by_block);
                } else if block_time == shadow.time {
                    if block == shadow.block {
                        // Same mapping - just add confirmation
                        shadow.confirmed_by_blocks.insert(confirmed_by_block);
                    } else if block > shadow.block {
                        // CONFLICT: Different block at same time - pick highest lexical BlockId
                        shadow.block = block;
                        shadow.parent = parent;
                        // Keep same time and first_seen
                        shadow.confirmed_by_blocks.insert(confirmed_by_block);
                    }
                    // If block < shadow.block, ignore it (we already have higher BlockId)
                }
                // If block_time < shadow.time, ignore it (we're already ahead)
            })
            .or_insert_with(|| {
                // Create new shadow mapping
                let mut confirmed_by_blocks = HashSet::new();
                confirmed_by_blocks.insert(confirmed_by_block);

                ShadowTokenMapping {
                    token,
                    block,
                    parent,
                    time: block_time,
                    first_seen: current_time,
                    confirmed_by_blocks,
                }
            });
    }

    /// Get the current head of our commit chain
    pub fn get_head(&self, backend: &dyn EcCommitChainBackend) -> Option<CommitBlockId> {
        backend.get_head()
    }

    /// Main tick function for commit chain operations
    ///
    /// Manages peer chain tracking and requests missing blocks.
    ///
    /// Strategy:
    /// - Consume peer logs from oldest end
    /// - Maintain up to 4 peer chain logs (HashMap)
    /// - Fill slots with closest peers that have heads
    /// - Detect when peer heads change and request missing blocks
    pub fn tick(
        &mut self,
        _commit_chain_backend: &mut dyn EcCommitChainBackend,
        block_backend: &dyn EcBlocks,
        token_backend: &dyn EcTokens,
        peers: &EcPeers,
        time: EcTime,
    ) -> Vec<MessageEnvelope> {
        let mut messages = Vec::new();

        // Only run sync logic periodically (every 100 ticks)
        if time % 100 != 0 {
            return messages;
        }

        // Step 0: Consume peer logs from the oldest end
        // Process blocks from beginning of logs (oldest first)
        // Validate pending blocks and check if we have all transaction blocks

        // First collect any missing blocks to request (to avoid borrow conflicts)
        let mut blocks_to_request: Vec<(PeerId, BlockId)> = Vec::new();

        for (peer_id, log) in &mut self.tracked_peers {
            // Process blocks from oldest to newest (from front of Vec)
            while let Some(commit_block) = log.blocks.first() {
                // Check each BlockId in this CommitBlock
                let mut all_blocks_ready = true;
                let mut found_pending = false;

                for block_id in &commit_block.committed_blocks {
                    if block_backend.lookup(block_id).is_some() {
                        // Already committed - good
                        continue;
                    } else if self.validated_blocks.contains_key(block_id) {
                        // Already validated - good
                        continue;
                    } else if self.pending_blocks.contains_key(block_id) {
                        // In pending - need to validate
                        found_pending = true;
                        all_blocks_ready = false;
                        break;
                    } else {
                        // Missing - need to request
                        blocks_to_request.push((*peer_id, *block_id));
                        all_blocks_ready = false;
                        break;
                    }
                }

                if found_pending {
                    // We have a pending block - validate it now
                    // We'll handle validation after this loop to avoid borrow conflicts
                    break;
                } else if all_blocks_ready {
                    // We have all transaction blocks - remove commit block from peer log
                    let removed_block = log.blocks.remove(0);

                    // Advance sync_from_time to at least this block's time
                    // This gradually moves our sync window forward as we consume blocks
                    log.sync_from_time = log.sync_from_time.max(removed_block.time);

                    // Continue to next commit block in this log
                } else {
                    // Missing blocks - already queued for request
                    break;
                }
            }
        }

        // Step 0.5: Validate pending blocks against our token store
        // Collect blocks to validate (blocks in pending_blocks that are referenced in commit blocks)
        let mut blocks_to_validate: Vec<BlockId> = Vec::new();

        for log in self.tracked_peers.values() {
            if let Some(commit_block) = log.blocks.first() {
                for block_id in &commit_block.committed_blocks {
                    if self.pending_blocks.contains_key(block_id) {
                        blocks_to_validate.push(*block_id);
                    }
                }
            }
        }

        // Validate each pending block and add token mappings to shadow
        for block_id in blocks_to_validate {
            if let Some(block) = self.pending_blocks.remove(&block_id) {
                let mut is_valid = true;

                // Validate token mappings in this block
                // Loop over each token mapping (part) in the block
                for i in 0..block.used as usize {
                    let token_block = &block.parts[i];
                    let token_id = token_block.token;

                    // Check if this token is in "our range"
                    // For now, we'll validate all tokens (TODO: add range check based on peer_id)

                    // Look up current state: shadow FIRST (our most current view), then backend
                    let current_state = self.shadow_token_mappings.get(&token_id)
                        .map(|shadow| (shadow.block, shadow.time))
                        .or_else(|| {
                            token_backend.lookup(&token_id)
                                .map(|bt| (bt.block, bt.time))
                        });

                    if let Some((current_block, current_time)) = current_state {
                        // We have this token (either committed or in shadow)
                        if block.time > current_time {
                            // This block is newer - check that it properly extends the chain
                            if token_block.last == current_block {
                                // Valid extension - add to shadow mappings
                                self.add_to_shadow(token_id, block.id, token_block.last, block.time, time, block_id);
                            } else {
                                // Invalid - token_block.last doesn't match current state
                                is_valid = false;
                                break;
                            }
                        } else if block.time == current_time {
                            // Same time - already have this state (duplicate)
                            // Still add to shadow to track confirmation
                            self.add_to_shadow(token_id, block.id, token_block.last, block.time, time, block_id);
                        } else {
                            // We're ahead - this block is outdated for this token
                            // Just ignore this particular token mapping
                        }
                    } else {
                        // First time seeing this token - validate as new token
                        if token_block.last == GENESIS_BLOCK_ID {
                            // Valid new token - add to shadow
                            self.add_to_shadow(token_id, block.id, token_block.last, block.time, time, block_id);
                        } else {
                            // Invalid - new token should have GENESIS_BLOCK_ID as parent
                            is_valid = false;
                            break;
                        }
                    }
                }

                if is_valid {
                    // Block validated successfully - move to validated_blocks
                    self.validated_blocks.insert(block_id, block);
                } else {
                    // Block validation failed - discard it
                    // It won't be in pending_blocks or validated_blocks
                    // Next tick will request it again (if still needed)
                }
            }
        }

        // Generate messages for missing blocks
        for (peer_id, block_id) in blocks_to_request {
            let block_ticket = self.generate_ticket(block_id);

            messages.push(MessageEnvelope {
                sender: self.peer_id,
                receiver: peer_id,
                ticket: block_ticket,
                time,
                message: Message::QueryBlock {
                    block_id,
                    target: self.peer_id, // We want the result
                    ticket: block_ticket,
                },
            });
        }

        // Step 1: Fill empty slots with new peers (up to MAX_TRACKED_PEERS)
        if self.tracked_peers.len() < MAX_TRACKED_PEERS {
            // Find closest peers
            let closest = peers.find_closest_peers(self.peer_id, 10);

            // Look for a peer we're not already tracking that has a known head
            for candidate_peer in closest {
                if candidate_peer == self.peer_id {
                    continue; // Skip ourselves
                }

                // Check if already tracking this peer
                if self.tracked_peers.contains_key(&candidate_peer) {
                    continue;
                }

                // Check if this peer has a known head
                if let Some(head) = peers.get_peer_commit_chain_head(&candidate_peer) {
                    // Start tracking this peer
                    // Set initial sync_from_time to current_time - max_sync_age
                    let sync_from_time = time.saturating_sub(self.config.max_sync_age);
                    self.tracked_peers.insert(candidate_peer, PeerChainLog::new(head, sync_from_time));

                    // Stop if we've reached max capacity
                    if self.tracked_peers.len() >= MAX_TRACKED_PEERS {
                        break;
                    }
                }
            }
        }

        // Step 2: Check each tracked peer for head changes and request missing blocks
        // Collect peer IDs and current heads first to avoid borrow conflicts
        let peer_head_changes: Vec<(PeerId, CommitBlockId)> = self.tracked_peers.iter()
            .filter_map(|(peer_id, log)| {
                peers.get_peer_commit_chain_head(peer_id)
                    .filter(|&current_head| current_head != log.last_known_head)
                    .map(|current_head| (*peer_id, current_head))
            })
            .collect();

        // Now update logs and generate messages
        for (peer_id, current_head) in peer_head_changes {
            if let Some(log) = self.tracked_peers.get_mut(&peer_id) {
                log.last_known_head = current_head;
            }

            // Generate ticket for this request
            let ticket = self.generate_ticket(current_head);

            // Request the block at the new head
            messages.push(MessageEnvelope {
                sender: self.peer_id,
                receiver: peer_id,
                ticket,
                time,
                message: Message::QueryCommitBlock {
                    block_id: current_head,
                    ticket,
                },
            });
        }

        // Step 3: Batch commit mature shadow mappings
        // TODO: This needs careful consideration - we need to ensure peer logs are aligned
        // past the point where we commit shadows. If we commit a shadow but then receive
        // an older conflicting block from a slow peer, we may have issues.
        // For now, we collect shadows that haven't been updated for shadow_commit_age.

        let mut shadows_to_commit: Vec<TokenId> = Vec::new();

        for (token_id, shadow) in &self.shadow_token_mappings {
            let age = time.saturating_sub(shadow.first_seen);
            if age >= self.config.shadow_commit_age {
                shadows_to_commit.push(*token_id);
            }
        }

        // TODO: Actually commit these shadows to the backend
        // This would involve:
        // 1. Committing all BlockIds in shadow.confirmed_by_blocks to block_backend
        // 2. Committing the token mapping (shadow.token -> shadow.block) to token_backend
        // 3. Creating our own CommitBlock from these committed blocks
        // 4. Removing the shadow from shadow_token_mappings
        // 5. Removing validated blocks that have been committed
        //
        // For now, just log that we would commit (placeholder for future implementation)
        if !shadows_to_commit.is_empty() {
            // Placeholder - actual commit logic to be implemented
            // self.batch_commit_shadows(shadows_to_commit, block_backend, token_backend);
        }

        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Simple in-memory backend for testing
    struct TestBackend {
        blocks: HashMap<CommitBlockId, CommitBlock>,
        head: Option<CommitBlockId>,
    }

    impl TestBackend {
        fn new() -> Self {
            Self {
                blocks: HashMap::new(),
                head: None,
            }
        }
    }

    impl EcCommitChainBackend for TestBackend {
        fn lookup(&self, id: &CommitBlockId) -> Option<CommitBlock> {
            self.blocks.get(id).cloned()
        }

        fn save(&mut self, block: &CommitBlock) {
            self.blocks.insert(block.id, block.clone());
        }

        fn get_head(&self) -> Option<CommitBlockId> {
            self.head
        }

        fn set_head(&mut self, id: &CommitBlockId) {
            self.head = Some(*id);
        }
    }

    #[test]
    fn test_create_first_commit_block() {
        let peer_id = 123;
        let mut chain = EcCommitChain::new(peer_id, CommitChainConfig::default());
        let mut backend = TestBackend::new();

        let committed = vec![1, 2, 3];
        let time = 1000;

        let commit_block = chain.create_commit_block(&mut backend, committed.clone(), time);

        assert_eq!(commit_block.id, 1);
        assert_eq!(commit_block.previous, GENESIS_BLOCK_ID);
        assert_eq!(commit_block.time, time);
        assert_eq!(commit_block.committed_blocks, committed);

        // Verify it was saved
        assert!(backend.lookup(&1).is_some());
        assert_eq!(backend.get_head(), Some(1));
    }

    #[test]
    fn test_create_multiple_commit_blocks() {
        let peer_id = 123;
        let mut chain = EcCommitChain::new(peer_id, CommitChainConfig::default());
        let mut backend = TestBackend::new();

        // Create first commit block
        let block1 = chain.create_commit_block(&mut backend, vec![1, 2], 1000);
        assert_eq!(block1.id, 1);
        assert_eq!(block1.previous, GENESIS_BLOCK_ID);

        // Create second commit block
        let block2 = chain.create_commit_block(&mut backend, vec![3, 4], 2000);
        assert_eq!(block2.id, 2);
        assert_eq!(block2.previous, 1); // Points to previous

        // Create third commit block
        let block3 = chain.create_commit_block(&mut backend, vec![5], 3000);
        assert_eq!(block3.id, 3);
        assert_eq!(block3.previous, 2); // Points to previous

        // Verify chain linkage
        assert_eq!(backend.get_head(), Some(3));
        assert!(backend.lookup(&1).is_some());
        assert!(backend.lookup(&2).is_some());
        assert!(backend.lookup(&3).is_some());
    }

    #[test]
    fn test_handle_query_commit_block() {
        let peer_id = 123;
        let mut chain = EcCommitChain::new(peer_id, CommitChainConfig::default());
        let mut backend = TestBackend::new();

        // Create a commit block
        chain.create_commit_block(&mut backend, vec![1, 2], 1000);

        // Query for it
        let result = chain.handle_query_commit_block(&backend, 1);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 1);

        // Query for non-existent block
        let result = chain.handle_query_commit_block(&backend, 999);
        assert!(result.is_none());
    }
}
