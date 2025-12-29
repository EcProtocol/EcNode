//! Commit Chain Module
//!
//! Implements the commit chain system for bootstrapping nodes and maintaining
//! synchronization across the network. Each node builds its own commit chain
//! tracking which transaction blocks were committed.

use crate::ec_interface::{
    BlockId, CommitBlock, CommitBlockId, EcCommitChainBackend, EcTime, MessageEnvelope,
    MessageTicket, PeerId, GENESIS_BLOCK_ID,
};

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

/// Main commit chain coordinator
///
/// For MVP: Simple implementation that just creates and stores commit blocks.
/// Future: Will add bootstrap sync, peer tracking, fraud detection.
pub struct EcCommitChain {
    peer_id: PeerId,
    config: CommitChainConfig,

    /// Next commit block ID to assign (sequential in simulation)
    next_commit_block_id: CommitBlockId,
}

impl EcCommitChain {
    /// Create a new commit chain coordinator
    pub fn new(peer_id: PeerId, config: CommitChainConfig) -> Self {
        Self {
            peer_id,
            config,
            next_commit_block_id: 1, // Start from 1 (0 is reserved for genesis)
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
    /// For MVP: Just store it if we don't have it.
    /// Future: Validate chain, detect conflicts, track peer heads.
    pub fn handle_commit_block(
        &mut self,
        _backend: &mut dyn EcCommitChainBackend,
        _block: CommitBlock,
        _sender: PeerId,
    ) {
        // TODO: Implement commit block validation and storage
        // For now, we just ignore incoming commit blocks
        // Full implementation will:
        // - Validate chain linkage
        // - Store if valid
        // - Update peer head tracking
        // - Detect fraud
    }

    /// Get the current head of our commit chain
    pub fn get_head(&self, backend: &dyn EcCommitChainBackend) -> Option<CommitBlockId> {
        backend.get_head()
    }

    /// Main tick function for commit chain operations
    ///
    /// For MVP: Does nothing (no active sync yet).
    /// Future: Will handle bootstrap sync, peer tracking, fraud detection.
    pub fn tick(
        &mut self,
        _backend: &mut dyn EcCommitChainBackend,
        _time: EcTime,
    ) -> Vec<MessageEnvelope> {
        // For MVP, commit chain is passive - it only responds to queries
        // Future phases will add:
        // - Bootstrap sync state machine
        // - Peer head tracking and refresh
        // - Fraud evidence pruning
        // - Continuous background sync
        Vec::new()
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
