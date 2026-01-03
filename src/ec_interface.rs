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

// ============================================================================
// Commit Chain Types
// ============================================================================

/// Commit block ID type - Blake3 hash of commit block contents
/// In simulation: u64 for simplicity
/// In production: [u8; 32] Blake3 hash
pub type CommitBlockId = BlockId;

/// A commit block in the commit chain
///
/// CommitBlocks form a blockchain tracking which transaction blocks were committed.
/// Each node builds its own commit chain and syncs with neighbors for bootstrap/validation.
#[derive(Clone, Debug, PartialEq)]
pub struct CommitBlock {
    /// Blake3 hash of (previous + time + committed_blocks)
    pub id: CommitBlockId,

    /// Hash of previous commit block (chain linkage)
    pub previous: CommitBlockId,

    /// Time when these blocks were committed
    pub time: EcTime,

    /// Block IDs (transaction IDs) committed in this commit
    pub committed_blocks: Vec<BlockId>,
}

impl CommitBlock {
    /// Create a new commit block
    ///
    /// Note: In simulation, the id is just assigned sequentially.
    /// In production, it should be Blake3(previous || time || committed_blocks)
    pub fn new(id: CommitBlockId, previous: CommitBlockId, time: EcTime, committed_blocks: Vec<BlockId>) -> Self {
        Self {
            id,
            previous,
            time,
            committed_blocks,
        }
    }

    /// Calculate Blake3 hash of this commit block (for production use)
    ///
    /// Currently unused in simulation, but provided for future migration
    #[allow(dead_code)]
    pub fn calculate_hash(&self) -> CommitBlockId {
        // In simulation with u64 IDs, just use the assigned ID
        // In production with [u8; 32], this would be:
        // let mut hasher = blake3::Hasher::new();
        // hasher.update(&self.previous.to_le_bytes());
        // hasher.update(&self.time.to_le_bytes());
        // for block_id in &self.committed_blocks {
        //     hasher.update(&block_id.to_le_bytes());
        // }
        // *hasher.finalize().as_bytes()
        self.id
    }
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
        head_of_chain: CommitBlockId,  // Head of sender's commit chain (0 for nodes without commit chain)
    },
    Block {
        block: Block,
    },
    Referral {
        token: TokenId,
        high: PeerId,
        low: PeerId,
    },
    // Commit chain messages
    QueryCommitBlock {
        block_id: CommitBlockId,
        ticket: MessageTicket,
    },
    CommitBlock {
        block: CommitBlock,
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

/// Request for a parent commit block
/// Returned by backend when a commit block can't be connected to the chain
#[derive(Debug, Clone)]
pub struct ParentBlockRequest {
    pub receiver: PeerId,
    pub block_id: BlockId,
    pub ticket: MessageTicket,
}

///
/// Database type
///
///

/// Genesis block ID constant - used for tokens with no parent (newly created tokens)
pub const GENESIS_BLOCK_ID: BlockId = 0;

#[derive(Copy, Clone, Debug)]
pub struct BlockTime {
    pub(crate) block: BlockId,
    pub(crate) parent: BlockId,  // Parent block in token chain (GENESIS_BLOCK_ID for new tokens)
    pub(crate) time: EcTime,
}

impl BlockTime {
    /// Create a new BlockTime
    pub fn new(block: BlockId, parent: BlockId, time: EcTime) -> Self {
        Self { block, parent, time }
    }

    /// Get the block ID
    pub fn block(&self) -> BlockId {
        self.block
    }

    /// Get the parent block ID
    pub fn parent(&self) -> BlockId {
        self.parent
    }

    /// Get the time
    pub fn time(&self) -> EcTime {
        self.time
    }
}

pub trait EcTokens {
    fn lookup(&self, token: &TokenId) -> Option<&BlockTime>;

    fn set(&mut self, token: &TokenId, block: &BlockId, parent: &BlockId, time: EcTime);

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
// Commit Chain Backend
// ============================================================================

/// Backend storage for commit chain blocks
///
/// Provides storage and retrieval of CommitBlocks that form the commit chain.
/// For MVP, this is a simple in-memory implementation.
/// For production, this would use RocksDB or file-based storage.
pub trait EcCommitChainBackend {
    /// Lookup a commit block by its ID
    fn lookup(&self, id: &CommitBlockId) -> Option<CommitBlock>;

    /// Get the current head of our commit chain
    fn get_head(&self) -> Option<CommitBlockId>;
}

/// Trait for backends that support commit chain operations
///
/// This trait provides access to the commit chain functionality embedded
/// within a backend implementation.
pub trait EcCommitChainAccess {
    /// Get the current head of the commit chain
    fn get_commit_chain_head(&self) -> Option<CommitBlockId>;

    /// Query a commit block by ID
    fn query_commit_block(&self, block_id: CommitBlockId) -> Option<CommitBlock>;

    /// Handle an incoming commit block from a peer
    ///
    /// Verifies ticket and stores block in peer log if from tracked peer.
    /// Returns optional request data for parent block if needed (respecting max_sync_age).
    fn handle_commit_block(&mut self, block: CommitBlock, sender: PeerId, ticket: MessageTicket, current_time: EcTime) -> Option<ParentBlockRequest>;

    /// Handle an incoming transaction block from a peer
    ///
    /// Verifies ticket and stores block in pending_blocks for validation.
    /// Returns true if block was accepted, false if ticket was invalid.
    fn handle_block(&mut self, block: Block, ticket: MessageTicket) -> bool;

    /// Tick function for commit chain sync operations
    ///
    /// Orchestrates batch commits for mature shadows and returns actions for messaging.
    /// Requires peers reference to find neighbors for sync.
    ///
    /// # Arguments
    /// * `peers` - Peer manager for finding sync targets
    /// * `time` - Current time
    ///
    /// # Returns
    /// List of (peer_id, message) tuples for ec_node to convert to messages
    fn commit_chain_tick(&mut self, peers: &crate::ec_peers::EcPeers, time: EcTime) -> Vec<(PeerId, crate::ec_commit_chain::TickMessage)>;
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
    /// Updates the token to point to the specified block with the given parent at the given time.
    /// For newly created tokens (genesis transactions), use GENESIS_BLOCK_ID as the parent.
    fn update_token(&mut self, token: &TokenId, block: &BlockId, parent: &BlockId, time: EcTime);

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
    /// Identity-block received from a peer
    IdentityBlockReceived {
        peer_id: TokenId,
        sender: PeerId,
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
