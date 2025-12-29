// Simple HashMap-based token storage for consensus simulator
//
// This is a minimal backend for the consensus simulator that only needs
// lookup/set operations. It doesn't implement signature search since the
// consensus simulator doesn't use proof-of-storage.

use std::collections::HashMap;

use crate::ec_interface::{BlockId, BlockTime, EcTime, TokenId};
use crate::ec_proof_of_storage::{SignatureSearchResult, TokenStorageBackend};

/// Simple HashMap-based token storage
///
/// This backend is for the consensus simulator which doesn't use
/// proof-of-storage signature searches. It only needs basic lookup/set.
///
/// # Performance
/// - Lookup: O(1) average
/// - Set: O(1) average
/// - Memory: ~32 bytes per token (HashMap overhead)
pub struct HashMapTokens {
    tokens: HashMap<TokenId, BlockTime>,
}

impl HashMapTokens {
    /// Create a new empty token storage
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
        }
    }
}

impl Default for HashMapTokens {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenStorageBackend for HashMapTokens {
    fn lookup(&self, token: &TokenId) -> Option<BlockTime> {
        self.tokens.get(token).copied()
    }

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
        self.tokens
            .entry(*token)
            .and_modify(|m| {
                if m.time < time {
                    m.time = time;
                    m.block = *block;
                }
            })
            .or_insert_with(|| BlockTime::new(*block, time));
    }

    fn search_signature(
        &self,
        _lookup_token: &TokenId,
        _signature_chunks: &[u16; crate::ec_proof_of_storage::SIGNATURE_CHUNKS],
    ) -> SignatureSearchResult {
        panic!("HashMapTokens does not support signature search - use MemTokens or SimulatorTokenStorage instead");
    }

    fn len(&self) -> usize {
        self.tokens.len()
    }
}
