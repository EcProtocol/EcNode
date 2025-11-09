// In-memory storage backend for tokens and blocks
//
// This module provides simple, fast in-memory storage for both tokens and blocks
// using standard Rust collections (BTreeMap for tokens, HashMap for blocks).
// Ideal for testing, simulation, and development.
//
// For persistent storage, see ec_rocksdb_backend.rs

use std::collections::btree_map::BTreeMap;
use std::collections::HashMap;
use std::ops::Bound::{Excluded, Unbounded};

use crate::ec_interface::{
    Block, BlockId, BlockTime, EcBlocks, EcTime, EcTokens, PeerId, TokenId, TokenSignature,
};
use crate::ec_proof_of_storage::{ProofOfStorage, TokenStorageBackend};

// ============================================================================
// In-Memory Token Storage
// ============================================================================

/// In-memory token storage using BTreeMap for sorted access
///
/// This is the simplest and fastest storage backend for testing and simulation.
/// For production deployments with millions of tokens, consider RocksDB or other
/// persistent storage backends.
///
/// # Performance Characteristics
/// - Lookup: O(log n)
/// - Set: O(log n)
/// - Range iteration: O(log n) seek + O(k) iteration
/// - Memory: ~24 bytes per token (64-bit IDs), ~72 bytes (256-bit IDs)
///
/// # Example
/// ```rust
/// let mut storage = MemTokens::new();
/// storage.set(&token_id, &block_id, time);
///
/// if let Some(block_time) = storage.lookup(&token_id) {
///     println!("Token {} maps to block {}", token_id, block_time.block);
/// }
/// ```
pub struct MemTokens {
    tokens: BTreeMap<TokenId, BlockTime>,
}

impl MemTokens {
    /// Create a new empty in-memory token storage
    pub fn new() -> Self {
        Self {
            tokens: BTreeMap::new(),
        }
    }

    /// Create a ProofOfStorage system using this storage backend
    ///
    /// This is a convenience method for wrapping this storage in a
    /// ProofOfStorage instance for signature generation.
    ///
    /// # Example
    /// ```rust
    /// let mut storage = MemTokens::new();
    /// // ... populate storage ...
    ///
    /// let proof_system = storage.into_proof_system();
    /// if let Some(sig) = proof_system.generate_signature(&token, &peer) {
    ///     // Use signature...
    /// }
    /// ```
    pub fn into_proof_system(self) -> ProofOfStorage<Self> {
        ProofOfStorage::new(self)
    }
}

impl Default for MemTokens {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TokenStorageBackend Implementation
// ============================================================================

impl TokenStorageBackend for MemTokens {
    fn lookup(&self, token: &TokenId) -> Option<BlockTime> {
        self.tokens.get(token).copied()
    }

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
        self.tokens
            .entry(*token)
            // Only update if existing mapping is older than the proposed update
            .and_modify(|m| {
                if m.time < time {
                    m.time = time;
                    m.block = *block;
                }
            })
            .or_insert_with(|| BlockTime {
                block: *block,
                time,
            });
    }

    fn range_after(&self, start: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_> {
        Box::new(self.tokens.range((Excluded(start), Unbounded)).map(|(k, v)| (*k, *v)))
    }

    fn range_before(&self, end: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_> {
        Box::new(self.tokens.range((Unbounded, Excluded(end))).rev().map(|(k, v)| (*k, *v)))
    }

    fn len(&self) -> usize {
        self.tokens.len()
    }
}

// ============================================================================
// Helper wrapper for borrowing MemTokens in ProofOfStorage
// ============================================================================

/// Wrapper that allows using &MemTokens with ProofOfStorage
struct MemTokensRef<'a>(&'a MemTokens);

impl<'a> TokenStorageBackend for MemTokensRef<'a> {
    fn lookup(&self, token: &TokenId) -> Option<BlockTime> {
        self.0.tokens.get(token).copied()
    }

    fn set(&mut self, _token: &TokenId, _block: &BlockId, _time: EcTime) {
        panic!("Cannot mutate through MemTokensRef");
    }

    fn range_after(&self, start: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_> {
        Box::new(self.0.tokens.range((Excluded(start), Unbounded)).map(|(k, v)| (*k, *v)))
    }

    fn range_before(&self, end: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_> {
        Box::new(self.0.tokens.range((Unbounded, Excluded(end))).rev().map(|(k, v)| (*k, *v)))
    }

    fn len(&self) -> usize {
        self.0.tokens.len()
    }
}

// ============================================================================
// EcTokens Implementation (Wrapper for Backward Compatibility)
// ============================================================================

impl EcTokens for MemTokens {
    fn lookup(&self, token: &TokenId) -> Option<&BlockTime> {
        // EcTokens trait expects a reference, but our backend returns owned
        // We need to keep the old signature for backward compatibility
        // This is a temporary workaround - ideally update EcTokens trait too
        self.tokens.get(token)
    }

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
        TokenStorageBackend::set(self, token, block, time)
    }

    fn tokens_signature(&self, token: &TokenId, peer: &PeerId) -> Option<TokenSignature> {
        // Create a temporary ProofOfStorage system for signature generation
        // We use a wrapper that implements TokenStorageBackend by forwarding to self
        let wrapper = MemTokensRef(self);
        let proof_system = ProofOfStorage::new(wrapper);
        proof_system.generate_signature(token, peer)
    }
}

// ============================================================================
// In-Memory Block Storage
// ============================================================================

/// In-memory block storage using HashMap for fast access
///
/// This is the simplest storage backend for blocks, suitable for testing
/// and simulation. Blocks are stored in memory and not persisted.
///
/// # Performance Characteristics
/// - Lookup: O(1) average
/// - Exists: O(1) average
/// - Save: O(1) average
/// - Memory: ~200-300 bytes per block (depending on TOKENS_PER_BLOCK)
///
/// # Example
/// ```rust
/// let mut storage = MemBlocks::new();
/// storage.save(&block);
///
/// if storage.exists(&block_id) {
///     let retrieved = storage.lookup(&block_id).unwrap();
///     println!("Block {} has {} tokens", retrieved.id, retrieved.used);
/// }
/// ```
pub struct MemBlocks {
    blocks: HashMap<BlockId, Block>,
}

impl MemBlocks {
    /// Create a new empty in-memory block storage
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }
}

impl Default for MemBlocks {
    fn default() -> Self {
        Self::new()
    }
}

impl EcBlocks for MemBlocks {
    fn lookup(&self, block: &BlockId) -> Option<Block> {
        self.blocks.get(block).copied()
    }

    fn exists(&self, block: &BlockId) -> bool {
        self.blocks.contains_key(block)
    }

    fn save(&mut self, block: &Block) {
        self.blocks.insert(block.id, *block);
    }
}

// ============================================================================
// Combined Memory Backend
// ============================================================================

/// Combined in-memory backend for both tokens and blocks
///
/// This provides a convenient single struct that manages both token and block
/// storage in memory, similar to how EcRocksDb manages both in a single database.
///
/// # Example
/// ```rust
/// let mut backend = MemoryBackend::new();
///
/// // Access tokens
/// let tokens = backend.tokens_mut();
/// tokens.set(&token_id, &block_id, time);
///
/// // Access blocks
/// let blocks = backend.blocks_mut();
/// blocks.save(&block);
/// ```
pub struct MemoryBackend {
    tokens: MemTokens,
    blocks: MemBlocks,
}

impl MemoryBackend {
    /// Create a new empty memory backend
    pub fn new() -> Self {
        Self {
            tokens: MemTokens::new(),
            blocks: MemBlocks::new(),
        }
    }

    /// Get immutable reference to token storage
    pub fn tokens(&self) -> &MemTokens {
        &self.tokens
    }

    /// Get mutable reference to token storage
    pub fn tokens_mut(&mut self) -> &mut MemTokens {
        &mut self.tokens
    }

    /// Get immutable reference to block storage
    pub fn blocks(&self) -> &MemBlocks {
        &self.blocks
    }

    /// Get mutable reference to block storage
    pub fn blocks_mut(&mut self) -> &mut MemBlocks {
        &mut self.blocks
    }

    /// Atomically "commit" a block and update associated token mappings
    ///
    /// Note: This is not truly atomic in the memory backend (no rollback on failure),
    /// but it mimics the API of the RocksDB backend for consistency.
    ///
    /// # Example
    /// ```rust
    /// backend.commit_block(&block);
    /// // Block is saved AND all token mappings are updated
    /// ```
    pub fn commit_block(&mut self, block: &Block) {
        // Save the block
        self.blocks.save(block);

        // Update all token mappings
        for i in 0..block.used as usize {
            let token_block = &block.parts[i];
            TokenStorageBackend::set(&mut self.tokens, &token_block.token, &block.id, block.time);
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ec_interface::TokenBlock;

    // ========================================================================
    // Token Storage Tests
    // ========================================================================

    #[test]
    fn test_mem_tokens_basic_operations() {
        let mut storage = MemTokens::new();
        assert!(TokenStorageBackend::is_empty(&storage));

        let token: TokenId = 100;
        let block: BlockId = 1;
        let time: EcTime = 42;

        TokenStorageBackend::set(&mut storage, &token, &block, time);
        assert_eq!(TokenStorageBackend::len(&storage), 1);

        let result = TokenStorageBackend::lookup(&storage, &token);
        assert!(result.is_some());
        assert_eq!(result.unwrap().block, block);
        assert_eq!(result.unwrap().time, time);
    }

    #[test]
    fn test_mem_tokens_update_only_newer() {
        let mut storage = MemTokens::new();

        let token: TokenId = 100;
        let block1: BlockId = 1;
        let block2: BlockId = 2;

        TokenStorageBackend::set(&mut storage, &token, &block1, 10);
        TokenStorageBackend::set(&mut storage, &token, &block2, 5); // Older time, should not update

        let result = TokenStorageBackend::lookup(&storage, &token).unwrap();
        assert_eq!(result.block, block1, "Should keep newer mapping");

        TokenStorageBackend::set(&mut storage, &token, &block2, 20); // Newer time, should update
        let result = TokenStorageBackend::lookup(&storage, &token).unwrap();
        assert_eq!(result.block, block2, "Should update with newer mapping");
    }

    #[test]
    fn test_mem_tokens_range_iteration() {
        let mut storage = MemTokens::new();

        // Insert tokens at intervals
        for i in 0..10 {
            TokenStorageBackend::set(&mut storage, &(i * 100), &i, i);
        }

        // Test range_after
        let tokens_after: Vec<_> = TokenStorageBackend::range_after(&storage, &250)
            .take(3)
            .map(|(t, _)| t)
            .collect();

        assert_eq!(tokens_after, vec![300, 400, 500]);

        // Test range_before
        let tokens_before: Vec<_> = TokenStorageBackend::range_before(&storage, &250)
            .take(3)
            .map(|(t, _)| t)
            .collect();

        assert_eq!(tokens_before, vec![200, 100, 0]);
    }

    #[test]
    fn test_mem_tokens_with_proof_system() {
        let mut storage = MemTokens::new();

        let token: TokenId = 50000;
        let block: BlockId = 100;
        let peer: PeerId = 777;

        TokenStorageBackend::set(&mut storage, &token, &block, 1);

        // Add many tokens to potentially complete a signature
        for i in 0..2000 {
            let test_token = (token + i * 100) | (i % 1024);
            TokenStorageBackend::set(&mut storage, &test_token, &(block + i), i);
        }

        // Test using EcTokens interface (backward compatible)
        let result = EcTokens::tokens_signature(&storage, &token, &peer);

        if let Some(sig) = result {
            assert_eq!(sig.answer.id, token);
            assert_eq!(sig.signature.len(), 10);
        }
    }

    #[test]
    fn test_into_proof_system() {
        let mut storage = MemTokens::new();
        TokenStorageBackend::set(&mut storage, &100, &1, 10);

        let proof_system = storage.into_proof_system();

        assert_eq!(TokenStorageBackend::len(proof_system.backend()), 1);
        assert!(TokenStorageBackend::lookup(proof_system.backend(), &100).is_some());
    }

    // ========================================================================
    // Block Storage Tests
    // ========================================================================

    #[test]
    fn test_mem_blocks_basic_operations() {
        let mut storage = MemBlocks::new();

        let block = Block {
            id: 123,
            time: 1000,
            used: 2,
            parts: [
                TokenBlock {
                    token: 1,
                    last: 0,
                    key: 100,
                },
                TokenBlock {
                    token: 2,
                    last: 0,
                    key: 200,
                },
                TokenBlock::default(),
                TokenBlock::default(),
                TokenBlock::default(),
                TokenBlock::default(),
            ],
            signatures: [None, None, None, None, None, None],
        };

        storage.save(&block);
        assert!(storage.exists(&123));

        let retrieved = storage.lookup(&123).unwrap();
        assert_eq!(retrieved.id, 123);
        assert_eq!(retrieved.time, 1000);
        assert_eq!(retrieved.used, 2);
        assert_eq!(retrieved.parts[0].token, 1);
        assert_eq!(retrieved.parts[1].token, 2);
    }

    #[test]
    fn test_mem_blocks_overwrite() {
        let mut storage = MemBlocks::new();

        let block1 = Block {
            id: 100,
            time: 1000,
            used: 1,
            parts: [TokenBlock::default(); 6],
            signatures: [None; 6],
        };

        let block2 = Block {
            id: 100,
            time: 2000,
            used: 2,
            parts: [TokenBlock::default(); 6],
            signatures: [None; 6],
        };

        storage.save(&block1);
        storage.save(&block2);

        let retrieved = storage.lookup(&100).unwrap();
        assert_eq!(retrieved.time, 2000);
        assert_eq!(retrieved.used, 2);
    }

    // ========================================================================
    // Combined Backend Tests
    // ========================================================================

    #[test]
    fn test_memory_backend_commit_block() {
        let mut backend = MemoryBackend::new();

        let block = Block {
            id: 999,
            time: 5000,
            used: 3,
            parts: [
                TokenBlock {
                    token: 10,
                    last: 0,
                    key: 1000,
                },
                TokenBlock {
                    token: 20,
                    last: 0,
                    key: 2000,
                },
                TokenBlock {
                    token: 30,
                    last: 0,
                    key: 3000,
                },
                TokenBlock::default(),
                TokenBlock::default(),
                TokenBlock::default(),
            ],
            signatures: [None, None, None, None, None, None],
        };

        // Commit block atomically
        backend.commit_block(&block);

        // Verify block was saved
        assert!(backend.blocks().exists(&999));

        // Verify all tokens were updated
        assert_eq!(
            TokenStorageBackend::lookup(backend.tokens(), &10)
                .unwrap()
                .block,
            999
        );
        assert_eq!(
            TokenStorageBackend::lookup(backend.tokens(), &20)
                .unwrap()
                .block,
            999
        );
        assert_eq!(
            TokenStorageBackend::lookup(backend.tokens(), &30)
                .unwrap()
                .block,
            999
        );
    }

    #[test]
    fn test_memory_backend_separate_access() {
        let mut backend = MemoryBackend::new();

        // Add tokens
        TokenStorageBackend::set(backend.tokens_mut(), &100, &1, 10);
        TokenStorageBackend::set(backend.tokens_mut(), &200, &2, 20);

        // Add blocks
        let block = Block {
            id: 1,
            time: 10,
            used: 1,
            parts: [TokenBlock::default(); 6],
            signatures: [None; 6],
        };
        backend.blocks_mut().save(&block);

        // Verify both are accessible
        assert_eq!(TokenStorageBackend::len(backend.tokens()), 2);
        assert!(backend.blocks().exists(&1));
    }
}
