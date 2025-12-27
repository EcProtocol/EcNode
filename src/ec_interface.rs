// for now - to be a SHA of the public-key - so 256 bit
pub type PublicKeyReference = u64;
pub type Signature = u64;

// all the same numeric type of some size to allow casting/interop
pub type PeerId = u64;
pub type TokenId = PeerId;
pub type BlockId = PeerId;

pub type EcTime = u64;
pub type MessageTicket = u64;

// ============================================================================
// FUTURE REFACTOR: TokenId vs TokenHash Indirection
// ============================================================================
//
// ## Current Implementation (Simulation-Friendly)
//
// Currently, `TokenId` is used directly for:
// - Storage keys (HashMap/RocksDB lookups)
// - Query messages
// - Block contents (TokenBlock.token)
// - Answer messages
//
// This works for testing/simulation but lacks two critical security properties:
// 1. Privacy: Token ownership is revealed by queries
// 2. Proof-of-knowledge: No cryptographic proof that responder stores the token
//
// ## Future Implementation (Production-Ready)
//
// Storage will be indexed by `Blake3(TokenId)` instead of `TokenId` directly:
//
// ```rust
// pub type TokenHash = [u8; 32];  // Blake3(TokenId)
//
// // Storage layer uses TokenHash as keys
// impl EcTokens {
//     fn lookup(&self, token_hash: &TokenHash) -> Option<&BlockTime>;
//     fn set(&mut self, token_hash: &TokenHash, ...);
// }
// ```
//
// **Query messages** will use `TokenHash` (challenger doesn't know real token):
// ```rust
// QueryToken {
//     token_hash: TokenHash,  // Blake3(TokenId) - what to look up
//     ...
// }
// ```
//
// **Blocks and Answers** will contain real `TokenId` (proves preimage knowledge):
// ```rust
// TokenBlock {
//     token: TokenId,  // Real token ID - proves ownership
//     ...
// }
//
// Answer {
//     answer: TokenMapping {
//         id: TokenId,  // Real token ID, not hash
//         ...
//     }
// }
// ```
//
// ## Security Benefits
//
// 1. **Privacy**: Queries use Blake3(TokenId), so observers can't determine token ownership
// 2. **Proof-of-Knowledge**: Answering with real TokenId proves you know the preimage
// 3. **Storage Security**: Attacker can't predict storage locations without knowing tokens
//
// ## Refactor Scope & Impact
//
// **HIGH IMPACT** - Touches core abstractions:
//
// ### Interfaces (6 files)
// - `EcTokens` trait - all methods take TokenHash instead of TokenId
// - `BlockTime` - might need both TokenId and TokenHash
// - `Message::QueryToken` - uses TokenHash
// - ProofOfStorage - signature generation needs both
//
// ### Backends (3 files)
// - `MemoryBackend` - HashMap<TokenHash, BlockTime>
// - `RocksDB` - keys change from TokenId to TokenHash
// - `TestBackend` - tests need to hash token IDs
//
// ### Core Logic (5 files)
// - `EcNode::handle_message` - QueryToken handling
// - `EcMemPool` - token lookups need hashing
// - `ProofOfStorage` - generate_signature needs to hash for lookups
// - `EcPeers` - election token queries need hashing
//
// ### Simulation & Tests (ALL - ~103 tests)
// - Every test that creates tokens needs to hash them for storage
// - Simulators need to track both TokenId and TokenHash
// - Token generation logic needs to compute hashes
// - All QueryToken constructions need hashing
//
// **Estimated effort**: 2-3 days of focused work + extensive testing
//
// ## Migration Strategy - When to Refactor
//
// **NOT NOW** - Defer until after these milestones:
//
// 1. ✅ Core consensus is stable and well-tested
// 2. ✅ Proof-of-storage signature mechanism is validated
// 3. ⏸️ Peer election and discovery is working
// 4. ⏸️ All major simulation scenarios are passing
// 5. ⏸️ Performance baselines are established
//
// **THEN** - Refactor in dedicated branch:
//
// 1. Add `TokenHash` type alongside `TokenId`
// 2. Deprecate old `EcTokens` trait, create new `EcTokenStorage`
// 3. Update backends one at a time with compatibility layer
// 4. Migrate tests in batches (consensus, then peer lifecycle, then proof-of-storage)
// 5. Update simulators with helper functions to manage hash/id mapping
// 6. Remove compatibility layer
//
// **Why defer?**
// - Current abstraction works for testing consensus logic
// - Premature optimization would slow current development velocity
// - Need stable test suite before breaking changes
// - TokenHash is a storage/security concern, not a consensus concern
//
// ============================================================================

pub const TOKENS_PER_BLOCK: usize = 6;
/// Number of tokens in a proof-of-storage signature response
/// This should match SIGNATURE_CHUNKS in ec_tokens.rs (10 chunks)
pub const TOKENS_SIGNATURE_SIZE: usize = 10;

// block can not claim to be further into the future
pub const SOME_STEPS_INTO_THE_FUTURE: EcTime = 100;

pub const VOTE_THRESHOLD: i64 = 2;

// TODO bad name
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct TokenBlock {
    pub token: TokenId,
    pub last: BlockId,
    pub key: PublicKeyReference,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Block {
    pub id: BlockId,
    pub time: EcTime,
    pub used: u8,
    pub parts: [TokenBlock; TOKENS_PER_BLOCK],

    // Not part of the block it-self (that is not part of the id)
    pub signatures: [Option<Signature>; TOKENS_PER_BLOCK],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TokenMapping {
    pub id: TokenId,
    pub block: BlockId,
}

/// Result of a signature-based proof of storage query
/// Contains the queried token's mapping plus signature tokens that prove storage
#[derive(Clone, Debug, PartialEq)]
pub struct TokenSignature {
    /// The main token that was queried
    pub answer: TokenMapping,
    /// Array of signature tokens proving storage (proof of storage)
    pub signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
}

// TODO make group message of Submit, Query and Validate
#[derive(Clone)]
pub enum Message {
    Vote {
        block_id: BlockId,
        vote: u8,
        reply: bool,
    },
    QueryBlock {
        block_id: BlockId,
        target: PeerId, // who wants the result? 0 => me (in network case this allow NAT discovery of peer address)
        ticket: MessageTicket,
    },
    QueryToken {
        token_id: TokenId,
        target: PeerId, // who wants the result? 0 => me
        ticket: MessageTicket,
    },
    Answer {
        answer: TokenMapping,
        signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
    },
    Block {
        block: Block,
    },
    Referral {
        token: TokenId,
        high: PeerId,
        low: PeerId,
    },
}

pub enum RequestMessage {
    Vote {
        block: BlockId,
        status: [bool; TOKENS_PER_BLOCK],
    },
    Query {
        token: TokenId,
        target: PeerId,
    },
    Empty,
}

pub enum Message2 {
    Answer {
        answer: TokenMapping,
        signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
    },
    Block {
        block: Block,
    },
    Requests {
        messages: [RequestMessage; TOKENS_SIGNATURE_SIZE],
    },
}

#[derive(Clone)]
pub struct MessageEnvelope {
    pub sender: PeerId,
    pub receiver: PeerId,
    pub ticket: MessageTicket,
    pub time: EcTime,
    pub message: Message,
}

///
/// Database type
///
///

#[derive(Copy, Clone, Debug)]
pub struct BlockTime {
    pub(crate) block: BlockId,
    pub(crate) time: EcTime,
}

pub trait EcTokens {
    fn lookup(&self, token: &TokenId) -> Option<&BlockTime>;

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime);

    /// Generate a proof-of-storage signature for a token
    ///
    /// Performs signature-based token search to prove that the node stores tokens.
    /// The signature is generated from (token, block, peer) and used to find
    /// matching tokens in storage via bidirectional search.
    ///
    /// # Arguments
    /// * `token` - The token being queried
    /// * `peer` - The peer requesting the signature (affects signature generation)
    ///
    /// # Returns
    /// * `Some(TokenSignature)` - If the token exists and a complete signature was found
    /// * `None` - If the token doesn't exist or signature search was incomplete
    ///
    /// The returned `TokenSignature` can be wrapped in `Message::Answer` for transmission.
    fn tokens_signature(&self, token: &TokenId, peer: &PeerId) -> Option<TokenSignature>;
}

pub trait EcBlocks {
    fn lookup(&self, block: &BlockId) -> Option<Block>;

    fn exists(&self, block: &BlockId) -> bool;

    fn save(&mut self, block: &Block);
}

// ============================================================================
// Batch Commit Abstraction
// ============================================================================

/// Represents a batch of pending storage operations
///
/// This abstraction allows accumulating blocks and token updates during a tick,
/// then committing them all atomically at the end.
///
/// # Design
/// - Mempool adds blocks and individual token updates to the batch
/// - Backends don't need to know about block internal structure
/// - Memory backend: Collects operations, applies on commit
/// - RocksDB backend: Uses WriteBatch for atomic multi-operation commits
///
/// # Error Handling
/// Errors are expected to be rare infrastructure failures (disk full, corruption).
/// On error, the entire batch is discarded and caller should retry.
///
/// # Example
/// ```rust
/// use ec_rust::ec_memory_backend::MemoryBackend;
/// use ec_rust::ec_interface::{Block, BatchedBackend, TokenBlock};
///
/// let mut backend = MemoryBackend::new();
/// let mut batch = backend.begin_batch();
///
/// let block = Block {
///     id: 123,
///     time: 1000,
///     used: 1,
///     parts: [TokenBlock::default(); 6],
///     signatures: [None; 6],
/// };
/// let token_id = 456u64;
/// let block_id = 123u64;
/// let time = 1000u64;
///
/// // Mempool adds blocks and tokens during tick
/// batch.save_block(&block);
/// batch.update_token(&token_id, &block_id, time);
///
/// // End of tick: commit everything atomically
/// batch.commit().unwrap();
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait StorageBatch {
    /// Save a block to the batch
    fn save_block(&mut self, block: &Block);

    /// Update a single token mapping
    ///
    /// Updates the token to point to the specified block at the given time.
    fn update_token(&mut self, token: &TokenId, block: &BlockId, time: EcTime);

    /// Commit all batched operations atomically
    ///
    /// # Errors
    /// Returns error only on infrastructure failures (disk I/O, corruption, etc.)
    /// On error, all operations in the batch are discarded.
    fn commit(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>>;

    /// Get the number of blocks in this batch
    fn block_count(&self) -> usize;
}

/// Backend that supports batched commits
///
/// Backends implementing this trait can create batch objects that accumulate
/// operations and commit them atomically.
pub trait BatchedBackend {
    /// Begin a new batch of operations
    ///
    /// The returned batch accumulates operations until commit() is called.
    /// Returns a boxed trait object to avoid lifetime issues.
    fn begin_batch(&mut self) -> Box<dyn StorageBatch + '_>;
}

// ============================================================================
// Event Logging System
// ============================================================================

/// Events emitted by the consensus system for debugging and analysis
#[derive(Debug, Clone)]
pub enum Event {
    /// Block received from another peer
    BlockReceived {
        block_id: BlockId,
        peer: PeerId,
        size: u8,
    },
    /// Vote cast on a block
    VoteCast {
        block_id: BlockId,
        token: TokenId,
        vote: u8,
        positive: bool,
    },
    /// Block committed to storage
    BlockCommitted {
        block_id: BlockId,
        peer: PeerId,
        votes: usize,
    },
    /// Reorganization detected
    Reorg {
        block_id: BlockId,
        peer: PeerId,
        from: BlockId,
        to: BlockId,
    },
    /// Block not found during query
    BlockNotFound {
        block_id: BlockId,
        peer: PeerId,
        from_peer: PeerId,
    },
    /// Block state change
    BlockStateChange {
        block_id: BlockId,
        from_state: &'static str,
        to_state: &'static str,
    },
    VoteReceived {
        block_id: BlockId,
        from_peer: PeerId,
    },
}

/// Trait for consuming events from the consensus system
pub trait EventSink {
    fn log(&mut self, round: EcTime, peer: PeerId, event: Event);
}

/// No-op event sink for production use (zero overhead)
pub struct NoOpSink;

impl EventSink for NoOpSink {
    #[inline(always)]
    fn log(&mut self, _round: EcTime, _peer: PeerId, _event: Event) {
        // Intentionally empty - compiler should optimize this away
    }
}
