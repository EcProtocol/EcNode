//! Commit Chain Module
//!
//! Implements the commit chain system for bootstrapping nodes and maintaining
//! synchronization across the network. Each node builds its own commit chain
//! tracking which transaction blocks were committed.

use crate::ec_interface::{
    BlockId, CommitBlock, CommitBlockId, EcCommitChainBackend, EcTime, Message, MessageEnvelope,
    MessageTicket, PeerId, GENESIS_BLOCK_ID,
};
use crate::ec_peers::EcPeers;

/// Configuration for commit chain behavior
#[derive(Debug, Clone)]
pub struct CommitChainConfig {
    /// Maximum age to sync (e.g., 30 days in ticks)
    pub max_sync_age: EcTime,

    /// How long to retain fraud evidence (e.g., 7 days in ticks)
    pub fraud_log_retention: EcTime,
}

impl Default for CommitChainConfig {
    fn default() -> Self {
        Self {
            max_sync_age: 30 * 24 * 3600,       // 30 days
            fraud_log_retention: 7 * 24 * 3600, // 7 days
        }
    }
}

use std::collections::HashMap;

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
}

impl PeerChainLog {
    fn new(head: CommitBlockId) -> Self {
        Self {
            last_known_head: head,
            blocks: Vec::new(),
            orphaned_blocks: HashMap::new(),
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
}

impl EcCommitChain {
    /// Create a new commit chain coordinator
    pub fn new(peer_id: PeerId, config: CommitChainConfig) -> Self {
        Self {
            peer_id,
            config,
            next_commit_block_id: 1, // Start from 1 (0 is reserved for genesis)
            tracked_peers: HashMap::new(),
        }
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
    /// Processes the block if it's from a tracked peer:
    /// - Tries to add it to the chain using try_add_block
    /// - If it connects, stores it in the peer log
    /// - If it doesn't connect and we're still tracking this peer, stores as orphan
    /// - Returns optional message to request parent if needed
    pub fn handle_commit_block(
        &mut self,
        block: CommitBlock,
        sender: PeerId,
    ) -> Option<MessageEnvelope> {
        // Only process if we're tracking this peer
        let log = self.tracked_peers.get_mut(&sender)?;

        // Try to add the block
        if log.try_add_block(block.clone()) {
            // Block was added successfully (and maybe some orphans too)
            None
        } else {
            // Block was stored as orphan
            // Request the parent block to try to connect it
            Some(MessageEnvelope {
                sender: self.peer_id,
                receiver: sender,
                ticket: 0,
                time: 0, // Will be filled by caller
                message: Message::QueryCommitBlock {
                    block_id: block.previous,
                    ticket: 0,
                },
            })
        }
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
    /// - Maintain up to 4 peer chain logs (HashMap)
    /// - Fill slots with closest peers that have heads
    /// - Detect when peer heads change and request missing blocks
    pub fn tick(
        &mut self,
        _backend: &dyn EcCommitChainBackend,
        peers: &EcPeers,
        time: EcTime,
    ) -> Vec<MessageEnvelope> {
        let mut messages = Vec::new();

        // Only run sync logic periodically (every 100 ticks)
        if time % 100 != 0 {
            return messages;
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
                    self.tracked_peers.insert(candidate_peer, PeerChainLog::new(head));

                    // Stop if we've reached max capacity
                    if self.tracked_peers.len() >= MAX_TRACKED_PEERS {
                        break;
                    }
                }
            }
        }

        // Step 2: Check each tracked peer for head changes and request missing blocks
        for (peer_id, log) in &mut self.tracked_peers {
            // Get current head from peers
            if let Some(current_head) = peers.get_peer_commit_chain_head(peer_id) {
                // Check if head has changed
                if current_head != log.last_known_head {
                    // Head changed! We need to sync
                    log.last_known_head = current_head;

                    // Request the block at the new head
                    messages.push(MessageEnvelope {
                        sender: self.peer_id,
                        receiver: *peer_id,
                        ticket: 0,
                        time,
                        message: Message::QueryCommitBlock {
                            block_id: current_head,
                            ticket: 0,
                        },
                    });
                }
            }
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
