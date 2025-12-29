// Simulator Token Storage - Optimized Read-Only Backend
//
// This is a sorted Vec-based storage backend optimized for proof-of-storage
// signature searches in the lifecycle simulator. It's designed for static
// token mappings that don't change during simulation.

use ec_rust::ec_interface::{BlockId, BlockTime, EcTime, TokenId};
use ec_rust::ec_proof_of_storage::{SignatureSearchResult, TokenStorageBackend, SIGNATURE_CHUNKS};

/// Read-only token storage backed by sorted Vec for fast iteration
///
/// This backend is optimized for the lifecycle simulator where:
/// - Token mappings are established at initialization and never change
/// - Search performance is critical (75% of runtime in profiling)
/// - Sorted Vec provides ~10x faster iteration than BTreeMap due to cache locality
///
/// # Performance
/// - Lookup: O(log n) via binary search
/// - Search signature: O(k) linear scan from lookup point (cache-friendly)
/// - Memory: ~24 bytes per token (compact, contiguous)
pub struct SimulatorTokenStorage {
    /// Token mappings sorted by TokenId for binary search and range scans
    mappings: Vec<(TokenId, BlockId, EcTime)>,
}

impl SimulatorTokenStorage {
    /// Create from unsorted mappings (will be sorted internally)
    pub fn new(mut mappings: Vec<(TokenId, BlockId, EcTime)>) -> Self {
        mappings.sort_by_key(|(token, _, _)| *token);
        Self { mappings }
    }

    /// Create from pre-sorted mappings (faster, no sorting needed)
    ///
    /// # Panics
    /// Panics in debug builds if mappings are not sorted
    pub fn from_sorted(mappings: Vec<(TokenId, BlockId, EcTime)>) -> Self {
        debug_assert!(
            mappings.windows(2).all(|w| w[0].0 < w[1].0),
            "Mappings must be sorted by TokenId"
        );
        Self { mappings }
    }
}

impl TokenStorageBackend for SimulatorTokenStorage {
    fn lookup(&self, token: &TokenId) -> Option<BlockTime> {
        self.mappings
            .binary_search_by_key(token, |(t, _, _)| *t)
            .ok()
            .map(|idx| {
                let (_, block, time) = self.mappings[idx];
                BlockTime::new(block, time)
            })
    }

    fn set(&mut self, _token: &TokenId, _block: &BlockId, _time: EcTime) {
        panic!("SimulatorTokenStorage is read-only - set() not supported");
    }

    fn search_signature(
        &self,
        lookup_token: &TokenId,
        signature_chunks: &[u16; SIGNATURE_CHUNKS],
    ) -> SignatureSearchResult {
        let mut found_tokens = Vec::with_capacity(SIGNATURE_CHUNKS);
        let mut steps = 0;
        let mut chunk_idx = 0;

        // Helper to match signature chunk
        #[inline]
        fn matches_chunk(token: &TokenId, chunk_value: u16) -> bool {
            (token & 0x3FF) as u16 == chunk_value
        }

        // Find starting position for forward search using binary search
        let start_idx = match self.mappings.binary_search_by_key(lookup_token, |(t, _, _)| *t) {
            Ok(idx) => idx + 1,  // Found exact match, start after it
            Err(idx) => idx,     // Not found, idx is insertion point (first token > lookup_token)
        };

        // Search forward (above) for first 5 chunks
        for i in start_idx..self.mappings.len() {
            steps += 1;
            let (token, _, _) = self.mappings[i];
            if matches_chunk(&token, signature_chunks[chunk_idx]) {
                found_tokens.push(token);
                chunk_idx += 1;
                if chunk_idx >= 5 {
                    break;
                }
            }
        }

        // Ring wrap: from beginning to lookup_token
        if chunk_idx < 5 {
            for i in 0..start_idx.saturating_sub(1) {
                steps += 1;
                let (token, _, _) = self.mappings[i];
                if matches_chunk(&token, signature_chunks[chunk_idx]) {
                    found_tokens.push(token);
                    chunk_idx += 1;
                    if chunk_idx >= 5 {
                        break;
                    }
                }
            }
        }

        // Find starting position for backward search
        let end_idx = match self.mappings.binary_search_by_key(lookup_token, |(t, _, _)| *t) {
            Ok(idx) => idx.saturating_sub(1),  // Found exact match, start before it
            Err(idx) => idx.saturating_sub(1), // Not found, start at position before insertion point
        };

        // Search backward (below) for last 5 chunks
        if end_idx < self.mappings.len() {
            for i in (0..=end_idx).rev() {
                steps += 1;
                let (token, _, _) = self.mappings[i];
                if matches_chunk(&token, signature_chunks[chunk_idx]) {
                    found_tokens.push(token);
                    chunk_idx += 1;
                    if chunk_idx >= SIGNATURE_CHUNKS {
                        break;
                    }
                }
            }
        }

        // Ring wrap: from end backwards to lookup_token
        if chunk_idx < SIGNATURE_CHUNKS && end_idx < self.mappings.len() {
            for i in (end_idx + 1..self.mappings.len()).rev() {
                steps += 1;
                let (token, _, _) = self.mappings[i];
                if matches_chunk(&token, signature_chunks[chunk_idx]) {
                    found_tokens.push(token);
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
        self.mappings.len()
    }
}
