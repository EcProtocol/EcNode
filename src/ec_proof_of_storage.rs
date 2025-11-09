// Signature-based proof of storage implementation
//
// This module contains the signature generation and search logic that works
// with any TokenStorageBackend implementation.

use crate::ec_interface::{
    BlockId, BlockTime, EcTime, PeerId, TokenId, TokenMapping, TokenSignature,
    TOKENS_SIGNATURE_SIZE,
};

/// Number of signature chunks (10-bit each = 100 bits total)
/// This must match TOKENS_SIGNATURE_SIZE from ec_interface
pub const SIGNATURE_CHUNKS: usize = TOKENS_SIGNATURE_SIZE;

/// Bits per signature chunk
const CHUNK_BITS: usize = 10;

/// Mask for extracting last 10 bits (0x3FF = 1023)
const CHUNK_MASK: u64 = 0x3FF;

/// Result of a signature-based token search
#[derive(Debug, Clone)]
pub struct SignatureSearchResult {
    /// Tokens found matching the signature (up to 10)
    pub tokens: Vec<TokenId>,
    /// Number of search steps taken
    pub steps: usize,
    /// Whether all signature chunks were matched
    pub complete: bool,
}

/// Backend abstraction for token storage operations
///
/// This trait defines the minimal interface needed for proof-of-storage
/// signature generation. Implementations can be in-memory (BTreeMap),
/// persistent (RocksDB), or any other ordered key-value store.
///
/// # Note on Owned vs Borrowed Data
///
/// The `lookup` method returns owned `BlockTime` rather than a reference.
/// This allows database backends (like RocksDB) to decode values from storage
/// without lifetime complications. In-memory backends can cheaply copy the
/// small BlockTime struct (16 bytes for 64-bit IDs, 40 bytes for 256-bit IDs).
pub trait TokenStorageBackend {
    /// Look up a token's block mapping
    ///
    /// Returns owned `BlockTime` to accommodate database backends that must
    /// decode values from storage. The struct is small enough (16-40 bytes)
    /// that copying is negligible compared to storage access costs.
    fn lookup(&self, token: &TokenId) -> Option<BlockTime>;

    /// Set or update a token's block mapping
    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime);

    /// Get an iterator over tokens in ascending order starting after a given token
    fn range_after(&self, start: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_>;

    /// Get an iterator over tokens in descending order starting before a given token
    fn range_before(&self, end: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_>;

    /// Get total number of tokens stored
    fn len(&self) -> usize;

    /// Check if storage is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Proof-of-storage signature generator
///
/// This struct wraps a TokenStorageBackend and provides signature generation
/// functionality. It contains no storage itself - all data is in the backend.
///
/// # Type Parameter
/// - `B`: Any type implementing `TokenStorageBackend`
///
/// # Example
/// ```rust
/// let storage = MemTokens::new();
/// let proof_system = ProofOfStorage::new(storage);
///
/// // Generate signature for a token
/// if let Some(sig) = proof_system.generate_signature(&token, &peer) {
///     // Use signature...
/// }
/// ```
pub struct ProofOfStorage<B: TokenStorageBackend> {
    backend: B,
}

impl<B: TokenStorageBackend> ProofOfStorage<B> {
    /// Create a new proof-of-storage system with the given backend
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Get a reference to the underlying storage backend
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Get a mutable reference to the underlying storage backend
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Extract the last N bits from a token for signature matching
    ///
    /// Works for both u64 (current testing) and future 256-bit types (production).
    #[inline]
    fn token_last_bits(token: &TokenId, bits: usize) -> u64 {
        (token & ((1u64 << bits) - 1)) as u64
    }

    /// Check if a token's last 10 bits match a signature chunk
    #[inline]
    fn matches_signature_chunk(token: &TokenId, chunk_value: u16) -> bool {
        Self::token_last_bits(token, CHUNK_BITS) == chunk_value as u64
    }

    /// Generate a 100-bit signature from token, block, and peer
    ///
    /// Returns 10 chunks of 10 bits each.
    ///
    /// # Current Implementation (u64 types for testing/simulation)
    ///
    /// Uses `DefaultHasher` for fast simulation with 64-bit IDs.
    /// Note: With only 64 bits of hash output, we can only get 6 independent 10-bit chunks,
    /// so chunks 7-9 reuse bits. This is acceptable for testing but reduces entropy.
    ///
    /// # Future Implementation (256-bit types for production)
    ///
    /// When migrating to 256-bit IDs, replace this with Blake3-based hashing.
    /// See `extract_signature_chunks_from_256bit_hash` for the production algorithm.
    fn signature_for(token: &TokenId, block: &BlockId, peer: &PeerId) -> [u16; SIGNATURE_CHUNKS] {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Create a deterministic hash from the three inputs
        // TODO: Replace with Blake3 when migrating to 256-bit types
        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        block.hash(&mut hasher);
        peer.hash(&mut hasher);
        let hash = hasher.finish(); // 64 bits

        // Split into 10 chunks of 10 bits each
        let mut chunks = [0u16; SIGNATURE_CHUNKS];
        for i in 0..SIGNATURE_CHUNKS {
            let bit_offset = (i * CHUNK_BITS) % 64;
            chunks[i] = ((hash >> bit_offset) & CHUNK_MASK) as u16;
        }

        chunks
    }

    /// Perform signature-based token search
    ///
    /// This implements the bidirectional search algorithm:
    /// - Search above the lookup token for chunks 0-4 (first 5 signature chunks)
    /// - Search below the lookup token for chunks 5-9 (last 5 signature chunks)
    ///
    /// Returns tokens matching the signature criteria along with search statistics.
    pub fn search_by_signature(
        &self,
        lookup_token: &TokenId,
        signature_chunks: &[u16; SIGNATURE_CHUNKS],
    ) -> SignatureSearchResult {
        let mut found_tokens = Vec::with_capacity(SIGNATURE_CHUNKS);
        let mut steps = 0;
        let mut chunk_idx = 0;

        // Search above (forward) for first 5 chunks
        let mut after_iter = self.backend.range_after(lookup_token);
        while chunk_idx < 5 {
            if let Some((token, _)) = after_iter.next() {
                steps += 1;
                if Self::matches_signature_chunk(&token, signature_chunks[chunk_idx]) {
                    found_tokens.push(token);
                    chunk_idx += 1;
                }
            } else {
                // Reached end of token space
                break;
            }
        }

        // Search below (backward) for last 5 chunks
        let mut before_iter = self.backend.range_before(lookup_token);
        while chunk_idx < SIGNATURE_CHUNKS {
            if let Some((token, _)) = before_iter.next() {
                steps += 1;
                if Self::matches_signature_chunk(&token, signature_chunks[chunk_idx]) {
                    found_tokens.push(token);
                    chunk_idx += 1;
                }
            } else {
                // Reached beginning of token space
                break;
            }
        }

        SignatureSearchResult {
            complete: chunk_idx == SIGNATURE_CHUNKS,
            tokens: found_tokens,
            steps,
        }
    }

    /// Generate a complete proof-of-storage signature for a token
    ///
    /// This is the main entry point for generating signatures. It:
    /// 1. Looks up the token's block mapping
    /// 2. Generates a signature from (token, block, peer)
    /// 3. Performs bidirectional search to find matching tokens
    /// 4. Returns a complete TokenSignature if successful
    ///
    /// # Arguments
    /// - `token`: The token being queried
    /// - `peer`: The peer requesting the signature (affects signature generation)
    ///
    /// # Returns
    /// - `Some(TokenSignature)`: If the token exists and all 10 signature tokens were found
    /// - `None`: If the token doesn't exist or the signature search was incomplete
    ///
    /// # Example
    /// ```rust
    /// let proof_system = ProofOfStorage::new(storage);
    ///
    /// if let Some(signature) = proof_system.generate_signature(&token, &peer) {
    ///     // Wrap in Message::Answer and send to peer
    ///     let msg = Message::Answer {
    ///         answer: signature.answer,
    ///         signature: signature.signature,
    ///     };
    /// }
    /// ```
    pub fn generate_signature(
        &self,
        token: &TokenId,
        peer: &PeerId,
    ) -> Option<TokenSignature> {
        // Get the block mapping for this token
        let block_time = self.backend.lookup(token)?;

        // Generate signature from token, block, and peer
        let signature_chunks = Self::signature_for(token, &block_time.block, peer);

        // Perform signature-based search
        let search_result = self.search_by_signature(token, &signature_chunks);

        // Only return a signature if we found all 10 tokens
        if search_result.complete {
            // Build the signature array from found tokens
            let mut signature = [TokenMapping {
                id: 0,
                block: 0,
            }; TOKENS_SIGNATURE_SIZE];

            for (i, &token_id) in search_result.tokens.iter().enumerate() {
                if let Some(block_time) = self.backend.lookup(&token_id) {
                    signature[i] = TokenMapping {
                        id: token_id,
                        block: block_time.block,
                    };
                }
            }

            Some(TokenSignature {
                answer: TokenMapping {
                    id: *token,
                    block: block_time.block,
                },
                signature,
            })
        } else {
            // Incomplete signature - cannot provide proof of storage
            None
        }
    }
}

// ============================================================================
// Helper Functions for 256-bit Production Deployment
// ============================================================================

/// Extract 10-bit signature chunks from a 256-bit hash
///
/// This helper function shows how to properly extract signature chunks from
/// Blake3 output when using 256-bit IDs in production.
///
/// # Arguments
/// * `hash_bytes` - 32-byte (256-bit) hash output from Blake3
///
/// # Returns
/// Array of 10 chunks, each containing 10 bits (range 0-1023)
#[allow(dead_code)]
pub fn extract_signature_chunks_from_256bit_hash(hash_bytes: &[u8; 32]) -> [u16; SIGNATURE_CHUNKS] {
    let mut chunks = [0u16; SIGNATURE_CHUNKS];

    for i in 0..SIGNATURE_CHUNKS {
        let bit_offset = i * CHUNK_BITS; // 0, 10, 20, 30, ..., 90
        let byte_offset = bit_offset / 8; // 0, 1, 2, 3, ..., 11
        let bit_in_byte = bit_offset % 8; // 0, 2, 4, 6, ..., 2

        // Each 10-bit chunk may span across two bytes
        // Read two consecutive bytes in little-endian order
        let byte1 = hash_bytes[byte_offset] as u16;
        let byte2 = hash_bytes[byte_offset + 1] as u16;

        // Combine the two bytes and extract 10 bits starting at bit_in_byte
        let combined = (byte2 << 8) | byte1;
        chunks[i] = (combined >> bit_in_byte) & (CHUNK_MASK as u16);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // Simple in-memory backend for testing
    struct TestBackend {
        tokens: BTreeMap<TokenId, BlockTime>,
    }

    impl TestBackend {
        fn new() -> Self {
            Self {
                tokens: BTreeMap::new(),
            }
        }
    }

    impl TokenStorageBackend for TestBackend {
        fn lookup(&self, token: &TokenId) -> Option<BlockTime> {
            self.tokens.get(token).copied()
        }

        fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
            self.tokens.insert(*token, BlockTime { block: *block, time });
        }

        fn range_after(&self, start: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_> {
            use std::ops::Bound::{Excluded, Unbounded};
            Box::new(self.tokens.range((Excluded(start), Unbounded)).map(|(k, v)| (*k, *v)))
        }

        fn range_before(&self, end: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_> {
            use std::ops::Bound::{Excluded, Unbounded};
            Box::new(self.tokens.range((Unbounded, Excluded(end))).rev().map(|(k, v)| (*k, *v)))
        }

        fn len(&self) -> usize {
            self.tokens.len()
        }
    }

    #[test]
    fn test_proof_of_storage_with_backend() {
        let mut backend = TestBackend::new();
        backend.set(&100, &1, 10);

        let proof = ProofOfStorage::new(backend);

        assert_eq!(proof.backend().len(), 1);
        assert!(proof.backend().lookup(&100).is_some());
    }

    #[test]
    fn test_signature_generation_nonexistent_token() {
        let backend = TestBackend::new();
        let proof = ProofOfStorage::new(backend);

        let result = proof.generate_signature(&12345, &99999);
        assert!(result.is_none(), "Should return None for nonexistent token");
    }

    #[test]
    fn test_signature_search_empty_storage() {
        let backend = TestBackend::new();
        let proof = ProofOfStorage::new(backend);

        let signature = [0u16; SIGNATURE_CHUNKS];
        let result = proof.search_by_signature(&1000, &signature);

        assert_eq!(result.tokens.len(), 0);
        assert!(!result.complete);
        assert_eq!(result.steps, 0);
    }

    #[test]
    fn test_256bit_chunk_extraction() {
        let hash: [u8; 32] = [0x42; 32];
        let chunks = extract_signature_chunks_from_256bit_hash(&hash);

        assert_eq!(chunks.len(), SIGNATURE_CHUNKS);
        for &chunk in &chunks {
            assert!(chunk <= 0x3FF, "Chunk exceeds 10-bit range");
        }
    }
}
