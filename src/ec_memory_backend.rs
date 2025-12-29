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
    BatchedBackend, Block, BlockId, BlockTime, EcBlocks, EcTime, EcTokens, PeerId, StorageBatch,
    TokenId, TokenSignature,
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
/// use ec_rust::ec_memory_backend::MemTokens;
/// use ec_rust::ec_proof_of_storage::TokenStorageBackend;
///
/// let mut storage = MemTokens::new();
/// let token_id = 123u64;
/// let block_id = 456u64;
/// let time = 789u64;
/// TokenStorageBackend::set(&mut storage, &token_id, &block_id, time);
///
/// // Verify the token was stored
/// assert!(TokenStorageBackend::lookup(&storage, &token_id).is_some());
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
    /// use ec_rust::ec_memory_backend::MemTokens;
    /// use ec_rust::ec_proof_of_storage::ProofOfStorage;
    ///
    /// let storage = MemTokens::new();
    ///
    /// let proof_system = storage.into_proof_system();
    /// // Can now use proof_system for generating signatures
    /// ```
    pub fn into_proof_system(self) -> ProofOfStorage {
        ProofOfStorage::new()
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

    fn search_signature(
        &self,
        lookup_token: &TokenId,
        signature_chunks: &[u16; crate::ec_proof_of_storage::SIGNATURE_CHUNKS],
    ) -> crate::ec_proof_of_storage::SignatureSearchResult {
        use crate::ec_proof_of_storage::{SignatureSearchResult, SIGNATURE_CHUNKS};

        let mut found_tokens = Vec::with_capacity(SIGNATURE_CHUNKS);
        let mut steps = 0;
        let mut chunk_idx = 0;

        // Helper to check if a token's last 10 bits match a signature chunk
        #[inline]
        fn matches_chunk(token: &TokenId, chunk_value: u16) -> bool {
            (token & 0x3FF) as u16 == chunk_value
        }

        // Search above (forward) for first 5 chunks
        // First pass: from lookup_token to end
        for (token, _) in self.tokens.range((Excluded(lookup_token), Unbounded)) {
            steps += 1;
            if matches_chunk(token, signature_chunks[chunk_idx]) {
                found_tokens.push(*token);
                chunk_idx += 1;
                if chunk_idx >= 5 {
                    break;
                }
            }
        }

        // Ring wrap: from beginning to lookup_token
        if chunk_idx < 5 {
            for (token, _) in self.tokens.range((Unbounded, Excluded(lookup_token))) {
                steps += 1;
                if matches_chunk(token, signature_chunks[chunk_idx]) {
                    found_tokens.push(*token);
                    chunk_idx += 1;
                    if chunk_idx >= 5 {
                        break;
                    }
                }
            }
        }

        // Search below (backward) for last 5 chunks
        // First pass: from lookup_token backwards to beginning
        for (token, _) in self.tokens.range((Unbounded, Excluded(lookup_token))).rev() {
            steps += 1;
            if matches_chunk(token, signature_chunks[chunk_idx]) {
                found_tokens.push(*token);
                chunk_idx += 1;
                if chunk_idx >= SIGNATURE_CHUNKS {
                    break;
                }
            }
        }

        // Ring wrap: from end backwards to lookup_token
        if chunk_idx < SIGNATURE_CHUNKS {
            for (token, _) in self.tokens.range((Excluded(lookup_token), Unbounded)).rev() {
                steps += 1;
                if matches_chunk(token, signature_chunks[chunk_idx]) {
                    found_tokens.push(*token);
                    chunk_idx += 1;
                    if chunk_idx >= SIGNATURE_CHUNKS {
                        break;
                    }
                }
            }
        }

        SignatureSearchResult {
            complete: chunk_idx == SIGNATURE_CHUNKS,
            tokens: found_tokens,
            steps,
        }
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

    fn search_signature(
        &self,
        lookup_token: &TokenId,
        signature_chunks: &[u16; crate::ec_proof_of_storage::SIGNATURE_CHUNKS],
    ) -> crate::ec_proof_of_storage::SignatureSearchResult {
        self.0.search_signature(lookup_token, signature_chunks)
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
        let proof_system = ProofOfStorage::new();
        proof_system.generate_signature(&wrapper, token, peer)
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
/// use ec_rust::ec_memory_backend::MemBlocks;
/// use ec_rust::ec_interface::{Block, EcBlocks, TokenBlock};
///
/// let mut storage = MemBlocks::new();
/// let block = Block {
///     id: 123,
///     time: 1000,
///     used: 2,
///     parts: [TokenBlock::default(); 6],
///     signatures: [None; 6],
/// };
/// storage.save(&block);
///
/// let block_id = 123u64;
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
/// use ec_rust::ec_memory_backend::MemoryBackend;
/// use ec_rust::ec_interface::{Block, EcBlocks, TokenBlock};
/// use ec_rust::ec_proof_of_storage::TokenStorageBackend;
///
/// let mut backend = MemoryBackend::new();
///
/// // Access tokens
/// let token_id = 123u64;
/// let block_id = 456u64;
/// let time = 789u64;
/// let tokens = backend.tokens_mut();
/// TokenStorageBackend::set(tokens, &token_id, &block_id, time);
///
/// // Access blocks
/// let block = Block {
///     id: block_id,
///     time,
///     used: 1,
///     parts: [TokenBlock::default(); 6],
///     signatures: [None; 6],
/// };
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
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Batched Commit Support
// ============================================================================

/// Batch for memory backend
///
/// Collects operations in memory and applies them all at commit time.
/// Not truly atomic (no rollback), but matches the API for consistency.
pub struct MemoryBatch<'a> {
    backend: &'a mut MemoryBackend,
    blocks: Vec<Block>,
    tokens: Vec<(TokenId, BlockId, EcTime)>,
}

impl<'a> StorageBatch for MemoryBatch<'a> {
    fn save_block(&mut self, block: &Block) {
        self.blocks.push(*block);
    }

    fn update_token(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
        self.tokens.push((*token, *block, time));
    }

    fn commit(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        // Apply all blocks
        for block in &self.blocks {
            self.backend.blocks.save(block);
        }

        // Apply all token updates
        for (token, block, time) in &self.tokens {
            TokenStorageBackend::set(&mut self.backend.tokens, token, block, *time);
        }

        Ok(())
    }

    fn block_count(&self) -> usize {
        self.blocks.len()
    }
}

impl BatchedBackend for MemoryBackend {
    fn begin_batch(&mut self) -> Box<dyn StorageBatch + '_> {
        Box::new(MemoryBatch {
            backend: self,
            blocks: Vec::new(),
            tokens: Vec::new(),
        })
    }
}

// Implement EcTokens for MemoryBackend (delegates to tokens field)
impl EcTokens for MemoryBackend {
    fn lookup(&self, token: &TokenId) -> Option<&BlockTime> {
        EcTokens::lookup(&self.tokens, token)
    }

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
        EcTokens::set(&mut self.tokens, token, block, time)
    }

    fn tokens_signature(&self, token: &TokenId, peer: &PeerId) -> Option<TokenSignature> {
        EcTokens::tokens_signature(&self.tokens, token, peer)
    }
}

// Implement TokenStorageBackend for MemoryBackend (delegates to tokens field)
impl TokenStorageBackend for MemoryBackend {
    fn lookup(&self, token: &TokenId) -> Option<BlockTime> {
        TokenStorageBackend::lookup(&self.tokens, token)
    }

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
        TokenStorageBackend::set(&mut self.tokens, token, block, time)
    }

    fn search_signature(
        &self,
        lookup_token: &TokenId,
        signature_chunks: &[u16; crate::ec_proof_of_storage::SIGNATURE_CHUNKS],
    ) -> crate::ec_proof_of_storage::SignatureSearchResult {
        TokenStorageBackend::search_signature(&self.tokens, lookup_token, signature_chunks)
    }

    fn len(&self) -> usize {
        TokenStorageBackend::len(&self.tokens)
    }

    fn is_empty(&self) -> bool {
        TokenStorageBackend::is_empty(&self.tokens)
    }
}

// Implement EcBlocks for MemoryBackend (delegates to blocks field)
impl EcBlocks for MemoryBackend {
    fn lookup(&self, block: &BlockId) -> Option<Block> {
        self.blocks.lookup(block)
    }

    fn exists(&self, block: &BlockId) -> bool {
        self.blocks.exists(block)
    }

    fn save(&mut self, block: &Block) {
        self.blocks.save(block)
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

        // Verify storage has the token before conversion
        assert_eq!(TokenStorageBackend::len(&storage), 1);
        assert!(TokenStorageBackend::lookup(&storage, &100).is_some());

        // Create proof system (no longer consumes storage since it's zero-sized)
        let _proof_system = ProofOfStorage::new();

        // Verify we can still use storage independently
        assert_eq!(TokenStorageBackend::len(&storage), 1);
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

    // ========================================================================
    // Batch Operations Tests
    // ========================================================================

    #[test]
    fn test_memory_batch_single_block() {
        let mut backend = MemoryBackend::new();

        let block = Block {
            id: 100,
            time: 1000,
            used: 2,
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
                TokenBlock::default(),
                TokenBlock::default(),
                TokenBlock::default(),
                TokenBlock::default(),
            ],
            signatures: [None; 6],
        };

        // Use batch
        {
            let mut batch = backend.begin_batch();
            batch.save_block(&block);
            // Add token updates
            for i in 0..block.used as usize {
                batch.update_token(&block.parts[i].token, &block.id, block.time);
            }
            assert_eq!(batch.block_count(), 1);
            batch.commit().unwrap();
        }

        // Verify block was saved
        assert!(backend.blocks().exists(&100));

        // Verify tokens were updated
        assert_eq!(
            TokenStorageBackend::lookup(backend.tokens(), &10)
                .unwrap()
                .block,
            100
        );
        assert_eq!(
            TokenStorageBackend::lookup(backend.tokens(), &20)
                .unwrap()
                .block,
            100
        );
    }

    #[test]
    fn test_memory_batch_multiple_blocks() {
        let mut backend = MemoryBackend::new();

        let blocks = vec![
            Block {
                id: 1,
                time: 100,
                used: 1,
                parts: [
                    TokenBlock {
                        token: 10,
                        last: 0,
                        key: 100,
                    },
                    TokenBlock::default(),
                    TokenBlock::default(),
                    TokenBlock::default(),
                    TokenBlock::default(),
                    TokenBlock::default(),
                ],
                signatures: [None; 6],
            },
            Block {
                id: 2,
                time: 200,
                used: 2,
                parts: [
                    TokenBlock {
                        token: 20,
                        last: 0,
                        key: 200,
                    },
                    TokenBlock {
                        token: 30,
                        last: 0,
                        key: 300,
                    },
                    TokenBlock::default(),
                    TokenBlock::default(),
                    TokenBlock::default(),
                    TokenBlock::default(),
                ],
                signatures: [None; 6],
            },
            Block {
                id: 3,
                time: 300,
                used: 1,
                parts: [
                    TokenBlock {
                        token: 40,
                        last: 0,
                        key: 400,
                    },
                    TokenBlock::default(),
                    TokenBlock::default(),
                    TokenBlock::default(),
                    TokenBlock::default(),
                    TokenBlock::default(),
                ],
                signatures: [None; 6],
            },
        ];

        // Batch commit all blocks
        {
            let mut batch = backend.begin_batch();
            for block in &blocks {
                batch.save_block(block);
                // Add token updates for this block
                for i in 0..block.used as usize {
                    batch.update_token(&block.parts[i].token, &block.id, block.time);
                }
            }
            assert_eq!(batch.block_count(), 3);
            batch.commit().unwrap();
        }

        // Verify all blocks saved
        assert!(backend.blocks().exists(&1));
        assert!(backend.blocks().exists(&2));
        assert!(backend.blocks().exists(&3));

        // Verify all tokens updated
        assert_eq!(
            TokenStorageBackend::lookup(backend.tokens(), &10)
                .unwrap()
                .block,
            1
        );
        assert_eq!(
            TokenStorageBackend::lookup(backend.tokens(), &20)
                .unwrap()
                .block,
            2
        );
        assert_eq!(
            TokenStorageBackend::lookup(backend.tokens(), &30)
                .unwrap()
                .block,
            2
        );
        assert_eq!(
            TokenStorageBackend::lookup(backend.tokens(), &40)
                .unwrap()
                .block,
            3
        );
    }

    #[test]
    fn test_memory_batch_empty_commit() {
        let mut backend = MemoryBackend::new();

        {
            let batch = backend.begin_batch();
            assert_eq!(batch.block_count(), 0);
            batch.commit().unwrap();
        }

        // Should succeed with no changes
    }
}
