//! Commit Chain Module
//!
//! Implements the commit chain system for bootstrapping nodes and maintaining
//! synchronization across the network. Each node builds its own commit chain
//! tracking which transaction blocks were committed.

use crate::ec_interface::{
    BlockId, CommitBlock, CommitBlockId, EcBlocks, EcCommitChainBackend, EcTime, EcTokens,
    MessageTicket, ParentBlockRequest, PeerId, TokenId, GENESIS_BLOCK_ID,
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

    /// Minimum number of confirmations required to commit a shadow mapping
    /// Higher values = more confidence but slower commits
    pub confirmation_threshold: usize,
}

impl Default for CommitChainConfig {
    fn default() -> Self {
        Self {
            max_sync_age: 30 * 24 * 3600,       // 30 days
            fraud_log_retention: 7 * 24 * 3600, // 7 days
            shadow_commit_age: 1000,            // ~1000 ticks to allow peer convergence
            confirmation_threshold: 2,          // Need at least 2 confirmations
        }
    }
}

use std::collections::{HashMap, HashSet};

// ============================================================================
// Actions / Intents
// ============================================================================

/// Actions that commit chain requests the node layer to perform
///
/// Similar to PeerAction, these represent message intents that will be
/// converted to MessageEnvelope by the node layer (ec_node.rs).
#[derive(Debug, Clone)]
pub enum CommitChainAction {
    /// Request a regular transaction block from a peer
    QueryBlock {
        block_id: BlockId,
        ticket: MessageTicket,
    },

    /// Request a commit block from a peer
    QueryCommitBlock {
        receiver: PeerId,
        block_id: CommitBlockId,
        ticket: MessageTicket,
    },
}

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
    /// Number of confirmations this mapping has received
    /// Used as confidence measure for commit decisions
    confirmation_count: usize,
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
    /// Number of times this log has successfully contributed to shadow mappings
    /// Used to assess peer quality and drop poor sources
    shadow_contributions: usize,
}

impl PeerChainLog {
    fn new(head: CommitBlockId, sync_from_time: EcTime) -> Self {
        Self {
            last_known_head: head,
            blocks: Vec::new(),
            orphaned_blocks: HashMap::new(),
            sync_from_time,
            shadow_contributions: 0,
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

        // First block logic: accept Genesis OR old timestamp
        if chain_tip.is_none() {
            if block.previous == GENESIS_BLOCK_ID {
                self.blocks.push(block.clone());
                self.last_known_head = block.id;
                return true;
            } else if block.time < self.sync_from_time {
                // Historical bootstrap - accept as first block
                self.blocks.push(block.clone());
                self.last_known_head = block.id;
                return true;
            }
            // Else fall through to store as orphan
        }

        // Check if this block connects to our current tip
        if chain_tip == Some(block.previous) {
            // Block connects! Add it to the chain
            self.blocks.push(block.clone());
            self.last_known_head = block.id;

            // Check if any orphaned blocks can now connect
            let mut connected_something = true;
            while connected_something {
                connected_something = false;
                let current_tip = self.get_chain_tip().unwrap();

                // Look for an orphan that points to current tip
                if let Some(orphan) = self.orphaned_blocks.remove(&current_tip) {
                    self.blocks.push(orphan.clone());
                    self.last_known_head = orphan.id;
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

    /// Tracks peer chains we're following for sync (up to 4)
    /// Maps PeerId -> PeerChainLog
    tracked_peers: HashMap<PeerId, PeerChainLog>,

    /// Pending blocks we've requested but not yet validated/committed
    /// Maps BlockId -> Block
    /// These are checked during commit block consumption alongside our backend
    pending_blocks: HashMap<BlockId, crate::ec_interface::Block>,

    /// Validated blocks that have been checked against our token store
    /// Set of BlockIds we've already validated (to avoid redundant work)
    /// These blocks have valid token mappings and can be considered "known"
    validated_blocks: HashSet<BlockId>,

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
        // TODO In production: use cryptographically secure random
        let ticket_secret = peer_id.wrapping_mul(0x9e3779b97f4a7c15); // Simple hash

        Self {
            peer_id,
            config,
            tracked_peers: HashMap::new(),
            pending_blocks: HashMap::new(),
            validated_blocks: HashSet::new(),
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
        // TODO In production, use proper cryptographic hash
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
        backend: &dyn EcCommitChainBackend,
        committed_blocks: Vec<BlockId>,
        time: EcTime,
    ) -> CommitBlock {
        // Get previous commit block (head of chain)
        let previous = backend.get_head().unwrap_or(GENESIS_BLOCK_ID);

        // Generate random commit block ID
        // In production this would be Blake3 hash of commit block contents
        // For simulation we use random u64
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hash, Hasher};
        let random_state = RandomState::new();
        let mut hasher = random_state.build_hasher();
        self.peer_id.hash(&mut hasher);
        time.hash(&mut hasher);
        previous.hash(&mut hasher);
        let commit_block_id = hasher.finish();

        // Create new commit block (caller handles saving)
        CommitBlock::new(
            commit_block_id,
            previous,
            time,
            committed_blocks,
        )
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

    /// Update an existing shadow mapping based on parent-relationship logic
    ///
    /// IMPORTANT: This function assumes a shadow already exists for the token.
    /// Caller should check shadow existence before calling.
    ///
    /// Parent-relationship logic:
    /// - If parent == shadow.block: Extension (B2 extends B1), replace shadow
    /// - If parent == shadow.parent && block != shadow.block: Conflict, pick max(block, shadow.block)
    /// - If block == shadow.block: Same block, increment confirmation count
    /// - If block_time < shadow.time: Older mapping, ignore (potential fraud)
    /// - Otherwise: Doesn't connect, ignore
    ///
    /// Returns true if the mapping was updated, false if it was rejected.
    fn add_to_shadow(
        &mut self,
        token: TokenId,
        block: BlockId,
        parent: BlockId,
        block_time: EcTime,
        current_time: EcTime,
    ) -> bool {
        // Shadow must exist - caller's responsibility to check
        let shadow = self.shadow_token_mappings.get_mut(&token)
            .expect("add_to_shadow called without existing shadow");

        // Check parent-relationship to determine how to handle this update
        if parent == shadow.block {
            // EXTENSION: This block extends the current shadow (A -> B)
            // Replace shadow with this newer mapping
            shadow.block = block;
            shadow.parent = parent;
            shadow.time = block_time;
            shadow.first_seen = current_time;
            shadow.confirmation_count = 1;
            true
        } else if parent == shadow.parent && block != shadow.block {
            // CONFLICT: Both blocks extend the same parent but are different
            // Pick the higher lexical BlockId (our consensus rule)
            if block > shadow.block {
                shadow.block = block;
                shadow.parent = parent;
                shadow.time = block_time;
                shadow.first_seen = current_time;
                shadow.confirmation_count = 1;
                true
            } else {
                // Keep current shadow
                false
            }
        } else if block == shadow.block {
            // CONFIRMATION: Same block seen again
            shadow.confirmation_count += 1;
            true
        } else if block_time < shadow.time {
            // OLDER: This mapping is older than what we have
            // Could be potential fraud or late arrival, ignore it
            // TODO: Consider fraud detection/logging
            false
        } else {
            // DISCONNECTED: This block doesn't connect to our shadow
            // Ignore it - it's not part of this token's history chain
            false
        }
    }

    /// Create a new shadow mapping, validating against database state
    ///
    /// IMPORTANT: This function assumes NO shadow exists for the token.
    /// Caller should check shadow existence before calling.
    ///
    /// Validation logic:
    /// - If db_state exists: Must extend or confirm database state
    /// - If no db_state: Must be new token with genesis parent
    ///
    /// Returns true if shadow was created, false if validation failed.
    fn create_shadow(
        &mut self,
        token: TokenId,
        block: BlockId,
        parent: BlockId,
        block_time: EcTime,
        current_time: EcTime,
        db_state: Option<(BlockId, EcTime)>,
    ) -> bool {
        if let Some((db_block, db_time)) = db_state {
            // We have database state - validate chain extension
            if block_time > db_time && parent == db_block {
                // Valid extension of database state
                self.shadow_token_mappings.insert(token, ShadowTokenMapping {
                    token,
                    block,
                    parent,
                    time: block_time,
                    first_seen: current_time,
                    confirmation_count: 1,
                });
                true
            } else if block_time == db_time && block == db_block {
                // Confirmation of database state
                self.shadow_token_mappings.insert(token, ShadowTokenMapping {
                    token,
                    block,
                    parent,
                    time: block_time,
                    first_seen: current_time,
                    confirmation_count: 1,
                });
                true
            } else {
                // Invalid - doesn't extend database state correctly
                false
            }
        } else {
            // No database state - must be new token with genesis parent
            if parent == GENESIS_BLOCK_ID {
                self.shadow_token_mappings.insert(token, ShadowTokenMapping {
                    token,
                    block,
                    parent,
                    time: block_time,
                    first_seen: current_time,
                    confirmation_count: 1,
                });
                true
            } else {
                // Invalid - new token must have genesis parent
                false
            }
        }
    }

    /// Get the current head of our commit chain
    pub fn get_head(&self, backend: &dyn EcCommitChainBackend) -> Option<CommitBlockId> {
        backend.get_head()
    }

    // ============================================================================
    // Tick Helper Functions
    // ============================================================================

    /// Consume peer logs and collect missing blocks to request
    ///
    /// Processes CommitBlocks from oldest to newest, checking if we have
    /// all transaction BlockIds. Returns list of blocks to request.
    fn consume_peer_logs(
        &mut self,
        block_backend: &dyn EcBlocks,
    ) -> (Vec<(PeerId, BlockId)>, Vec<BlockId>) {
        let mut blocks_to_request = Vec::new();
        let mut blocks_to_validate = Vec::new();

        for (peer_id, log) in &mut self.tracked_peers {
            while let Some(commit_block) = log.blocks.first() {
                let mut all_blocks_ready = true;
                let mut found_pending = false;

                for block_id in &commit_block.committed_blocks {
                    // Check validated_blocks first (small HashSet, fast)
                    if self.validated_blocks.contains(block_id) {
                        // Already validated - good
                        continue;
                    } else if self.pending_blocks.contains_key(block_id) {
                        // In pending - need to validate
                        found_pending = true;
                        blocks_to_validate.push(*block_id);
                        all_blocks_ready = false;
                        break;
                    } else if block_backend.lookup(block_id).is_some() {
                        // Already committed - good
                        continue;
                    } else {
                        // Missing - need to request
                        blocks_to_request.push((*peer_id, *block_id));
                        all_blocks_ready = false;
                        break;
                    }
                }

                if found_pending {
                    // Have pending blocks - will validate them
                    break;
                } else if all_blocks_ready {
                    // Have all blocks - consume this CommitBlock
                    let removed_block = log.blocks.remove(0);
                    log.sync_from_time = log.sync_from_time.max(removed_block.time);
                } else {
                    // Missing blocks - stop processing this log
                    break;
                }
            }
        }

        (blocks_to_request, blocks_to_validate)
    }

    /// Validate a single block's token mappings against our token store
    ///
    /// Returns true if all token mappings are valid, false otherwise.
    /// Adds valid mappings to shadow_token_mappings.
    ///
    /// Strategy:
    /// 1. Check if shadow exists for token
    /// 2. If yes: Update shadow (no db lookup needed)
    /// 3. If no: Fetch from db and create shadow with validation
    fn validate_block_token_mappings(
        &mut self,
        block: &crate::ec_interface::Block,
        _block_id: BlockId,
        token_backend: &dyn EcTokens,
        time: EcTime,
    ) -> bool {
        for i in 0..block.used as usize {
            let token_block = &block.parts[i];
            let token_id = token_block.token;

            // Check if we already have a shadow for this token
            let has_shadow = self.shadow_token_mappings.contains_key(&token_id);

            let valid = if has_shadow {
                // Shadow exists - update it (no db lookup needed)
                self.add_to_shadow(
                    token_id,
                    block.id,
                    token_block.last,
                    block.time,
                    time,
                )
            } else {
                // No shadow - fetch from db and create shadow with validation
                let db_state = token_backend.lookup(&token_id)
                    .map(|bt| (bt.block, bt.time));

                self.create_shadow(
                    token_id,
                    block.id,
                    token_block.last,
                    block.time,
                    time,
                    db_state,
                )
            };

            if !valid {
                // Invalid mapping - block fails validation
                return false;
            }
        }

        true
    }

    /// Validate pending blocks and add valid ones to validated_blocks set
    fn validate_pending_blocks(
        &mut self,
        blocks_to_validate: Vec<BlockId>,
        token_backend: &dyn EcTokens,
        time: EcTime,
    ) {
        for block_id in blocks_to_validate {
            if let Some(block) = self.pending_blocks.remove(&block_id) {
                if self.validate_block_token_mappings(&block, block_id, token_backend, time) {
                    // Valid - add to validated_blocks set
                    self.validated_blocks.insert(block_id);
                    // Note: We don't need to keep the block itself anymore
                    // The shadow already has the token mappings
                }
                // else: invalid - discard (will be re-requested if needed)
            }
        }
    }

    /// Generate QueryBlock actions for missing blocks
    fn request_missing_blocks(
        &self,
        blocks_to_request: Vec<(PeerId, BlockId)>,
    ) -> Vec<CommitChainAction> {
        blocks_to_request.into_iter()
            .map(|(block_id, ..)| {
                let ticket = self.generate_ticket(block_id);
                CommitChainAction::QueryBlock {
                    block_id,
                    ticket,
                }
            })
            .collect()
    }

    /// Fill tracking slots with new peers (up to MAX_TRACKED_PEERS)
    fn fill_tracking_slots(&mut self, peers: &EcPeers, time: EcTime) {
        if self.tracked_peers.len() >= MAX_TRACKED_PEERS {
            return;
        }

        let closest = peers.find_closest_peers(self.peer_id, 10);

        for candidate_peer in closest {
            if candidate_peer == self.peer_id {
                continue; // Skip ourselves
            }

            if self.tracked_peers.contains_key(&candidate_peer) {
                continue; // Already tracking
            }

            // Only track Connected or Pending peers
            if !peers.is_peer_connected_or_pending(&candidate_peer) {
                continue;
            }

            if let Some(head) = peers.get_peer_commit_chain_head(&candidate_peer) {
                let sync_from_time = time.saturating_sub(self.config.max_sync_age);
                self.tracked_peers.insert(candidate_peer, PeerChainLog::new(head, sync_from_time));

                if self.tracked_peers.len() >= MAX_TRACKED_PEERS {
                    break;
                }
            }
        }
    }

    /// Check for peer head changes and generate QueryCommitBlock actions
    fn request_peer_head_updates(&mut self, peers: &EcPeers) -> Vec<CommitChainAction> {
        // Collect peer head changes
        let peer_head_changes: Vec<(PeerId, CommitBlockId)> = self.tracked_peers.iter()
            .filter_map(|(peer_id, log)| {
                peers.get_peer_commit_chain_head(peer_id)
                    .filter(|&current_head| current_head != log.last_known_head)
                    .map(|current_head| (*peer_id, current_head))
            })
            .collect();

        // Update logs and generate actions
        peer_head_changes.into_iter()
            .filter_map(|(peer_id, current_head)| {
                if let Some(log) = self.tracked_peers.get_mut(&peer_id) {
                    log.last_known_head = current_head;
                }

                let ticket = self.generate_ticket(current_head);
                Some(CommitChainAction::QueryCommitBlock {
                    receiver: peer_id,
                    block_id: current_head,
                    ticket,
                })
            })
            .collect()
    }

    /// Collect shadows that are mature enough for batch commit
    ///
    /// Returns list of TokenIds ready for commit.
    fn collect_mature_shadows(&self, time: EcTime) -> Vec<TokenId> {
        self.shadow_token_mappings.iter()
            .filter_map(|(token_id, shadow)| {
                let age = time.saturating_sub(shadow.first_seen);
                // Shadow must be both old enough AND have sufficient confirmations
                if age >= self.config.shadow_commit_age
                    && shadow.confirmation_count >= self.config.confirmation_threshold {
                    Some(*token_id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Prepare shadow commit by collecting blocks and token mappings
    ///
    /// Removes shadows from shadow_token_mappings and collects the data needed for batch commit.
    /// Cleans up validated_blocks to prevent unbounded growth.
    ///
    /// # Arguments
    /// * `tokens_to_commit` - List of TokenIds that have mature shadow mappings
    /// * `block_backend` - Block backend to look up transaction blocks
    ///
    /// # Returns
    /// Tuple of (blocks, token_mappings, block_ids) ready for batch commit
    fn prepare_shadow_commit(
        &mut self,
        tokens_to_commit: &[TokenId],
        block_backend: &dyn EcBlocks,
    ) -> (Vec<crate::ec_interface::Block>, Vec<(TokenId, BlockId, BlockId, EcTime)>, Vec<BlockId>) {
        let mut blocks_to_save: HashSet<BlockId> = HashSet::new();
        let mut token_mappings = Vec::new();

        // Collect token mappings and blocks to save
        for token_id in tokens_to_commit {
            if let Some(shadow) = self.shadow_token_mappings.remove(token_id) {
                token_mappings.push((*token_id, shadow.block, shadow.parent, shadow.time));
                blocks_to_save.insert(shadow.block);
            }
        }

        // Collect blocks
        let mut blocks = Vec::new();
        let mut block_ids = Vec::new();
        for block_id in &blocks_to_save {
            if let Some(block) = block_backend.lookup(block_id) {
                blocks.push(block.clone());
                block_ids.push(*block_id);
            }
        }

        // Clean up validated_blocks - remove BlockIds that were committed
        for block_id in blocks_to_save {
            self.validated_blocks.remove(&block_id);
        }

        (blocks, token_mappings, block_ids)
    }

    // ============================================================================
    // Main Tick Function
    // ============================================================================

    /// Main tick function for commit chain operations
    ///
    /// Manages peer chain tracking and requests missing blocks.
    /// Orchestrates batch commits for mature shadows using the provided batched backend.
    ///
    /// Returns a list of actions for ec_node to convert to messages.
    ///
    /// Strategy:
    /// - Consume peer logs from oldest end
    /// - Maintain up to 4 peer chain logs (HashMap)
    /// - Fill slots with closest peers that have heads
    /// - Detect when peer heads change and request missing blocks
    /// - Batch commit mature shadows atomically
    pub fn tick<B>(
        &mut self,
        commit_chain_backend: &dyn EcCommitChainBackend,
        batched_backend: &mut B,
        peers: &EcPeers,
        time: EcTime,
    ) -> (Vec<CommitChainAction>, Option<CommitBlock>)
    where
        B: crate::ec_interface::BatchedBackend + EcBlocks + EcTokens,
    {
        // Only run sync logic periodically (every 100 ticks)
        if time % 100 != 0 {
            return (Vec::new(), None);
        }

        let mut actions = Vec::new();
        let mut commit_block_to_save = None;

        // Step 1: Consume peer logs and collect missing/pending blocks
        let (blocks_to_request, blocks_to_validate) = self.consume_peer_logs(batched_backend);

        // Step 2: Validate pending blocks against token store
        self.validate_pending_blocks(blocks_to_validate, batched_backend, time);

        // Step 3: Request missing blocks
        actions.extend(self.request_missing_blocks(blocks_to_request));

        // Step 4: Fill tracking slots with new peers
        self.fill_tracking_slots(peers, time);

        // Step 5: Request peer head updates
        actions.extend(self.request_peer_head_updates(peers));

        // Step 6: Collect and commit mature shadows
        let shadows_to_commit = self.collect_mature_shadows(time);
        if !shadows_to_commit.is_empty() {
            // Collect blocks and token mappings before creating batch
            let (blocks_to_save, token_mappings, block_ids) =
                self.prepare_shadow_commit(&shadows_to_commit, batched_backend);

            // Begin batch for shadow commits (like mempool does for block commits)
            let mut batch = batched_backend.begin_batch();

            // Add blocks and tokens to batch
            for block in blocks_to_save {
                batch.save_block(&block);
            }
            for (token, block_id, parent, shadow_time) in token_mappings {
                batch.update_token(&token, &block_id, &parent, shadow_time);
            }

            // Commit the batch atomically first
            match batch.commit() {
                Ok(()) => {
                    // Batch succeeded - create commit block for caller to save
                    if !block_ids.is_empty() {
                        commit_block_to_save = Some(self.create_commit_block(commit_chain_backend, block_ids, time));
                    }
                }
                Err(e) => {
                    eprintln!("Failed to commit shadow batch at time {}: {}", time, e);
                }
            }
        }

        // Return actions and optional commit block to save
        (actions, commit_block_to_save)
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

        fn get_head(&self) -> Option<CommitBlockId> {
            self.head
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

        // ID is now random, just check it's non-zero
        assert_ne!(commit_block.id, 0);
        assert_eq!(commit_block.previous, GENESIS_BLOCK_ID);
        assert_eq!(commit_block.time, time);
        assert_eq!(commit_block.committed_blocks, committed);

        // Verify it was saved
        assert!(backend.lookup(&commit_block.id).is_some());
        assert_eq!(backend.get_head(), Some(commit_block.id));
    }

    #[test]
    fn test_create_multiple_commit_blocks() {
        let peer_id = 123;
        let mut chain = EcCommitChain::new(peer_id, CommitChainConfig::default());
        let mut backend = TestBackend::new();

        // Create first commit block
        let block1 = chain.create_commit_block(&mut backend, vec![1, 2], 1000);
        assert_ne!(block1.id, 0);
        assert_eq!(block1.previous, GENESIS_BLOCK_ID);

        // Create second commit block
        let block2 = chain.create_commit_block(&mut backend, vec![3, 4], 2000);
        assert_ne!(block2.id, 0);
        assert_eq!(block2.previous, block1.id); // Points to previous

        // Create third commit block
        let block3 = chain.create_commit_block(&mut backend, vec![5], 3000);
        assert_ne!(block3.id, 0);
        assert_eq!(block3.previous, block2.id); // Points to previous

        // Verify chain linkage
        assert_eq!(backend.get_head(), Some(block3.id));
        assert!(backend.lookup(&block1.id).is_some());
        assert!(backend.lookup(&block2.id).is_some());
        assert!(backend.lookup(&block3.id).is_some());
    }

    #[test]
    fn test_handle_query_commit_block() {
        let peer_id = 123;
        let mut chain = EcCommitChain::new(peer_id, CommitChainConfig::default());
        let mut backend = TestBackend::new();

        // Create a commit block
        let block = chain.create_commit_block(&mut backend, vec![1, 2], 1000);

        // Query for it
        let result = chain.handle_query_commit_block(&backend, block.id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, block.id);

        // Query for non-existent block
        let result = chain.handle_query_commit_block(&backend, 999);
        assert!(result.is_none());
    }
}
