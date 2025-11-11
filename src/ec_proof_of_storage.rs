// Signature-based proof of storage implementation
//
// This module contains the signature generation and search logic that works
// with any TokenStorageBackend implementation.
//
// Additionally, this module implements the peer election system for discovering
// and connecting with highly-aligned peers through challenge-response mechanisms.

use crate::ec_interface::{
    BlockId, BlockTime, EcTime, MessageTicket, PeerId, TokenId, TokenMapping, TokenSignature,
    TOKENS_SIGNATURE_SIZE,
};
use std::collections::HashMap;
use std::sync::OnceLock;

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

/// Result of consensus cluster analysis
#[derive(Debug, Clone, PartialEq)]
pub struct ConsensusCluster {
    /// Signatures in the consensus cluster (indices into original list)
    pub members: Vec<usize>,
    /// Minimum number of common mappings between any pair in the cluster
    pub min_agreement: usize,
    /// Average number of common mappings across all pairs in the cluster
    pub avg_agreement: f64,
}

// ============================================================================
// Peer Election System: Ring Distance & Ticket Generation
// ============================================================================

/// Global secret for election ticket generation (injected at startup)
static ELECTION_SECRET: OnceLock<[u8; 32]> = OnceLock::new();

/// Initialize the global election secret (must be called once at startup)
///
/// # Arguments
/// * `secret` - 32-byte secret used for ticket generation
///
/// # Returns
/// * `Ok(())` - Secret successfully initialized
/// * `Err(msg)` - Secret was already initialized
///
/// # Example
/// ```
/// use ec_rust::ec_proof_of_storage::initialize_election_secret;
///
/// let secret = [42u8; 32];
/// initialize_election_secret(secret).expect("Failed to initialize secret");
/// ```
pub fn initialize_election_secret(secret: [u8; 32]) -> Result<(), String> {
    ELECTION_SECRET
        .set(secret)
        .map_err(|_| "Election secret already initialized".to_string())
}

/// Calculate ring distance between two IDs in circular space
///
/// In a ring topology, distance is the minimum of clockwise and counter-clockwise
/// distances. This is used for selecting the winner peer closest to the challenge token.
///
/// # Arguments
/// * `a` - First ID on the ring
/// * `b` - Second ID on the ring
///
/// # Returns
/// Minimum distance between the two IDs (wrapping around ring)
///
/// # Note
/// Currently works with u64 types. When migrating to 256-bit types, this function
/// will need to be updated to handle U256 arithmetic with wrapping.
///
/// # Example
/// ```
/// use ec_rust::ec_proof_of_storage::ring_distance;
///
/// // Normal case
/// assert_eq!(ring_distance(100, 150), 50);
///
/// // Wrapping case (going backwards is shorter)
/// assert_eq!(ring_distance(10, u64::MAX - 5), 16);
/// ```
pub fn ring_distance(a: u64, b: u64) -> u64 {
    let forward = b.wrapping_sub(a);
    let backward = a.wrapping_sub(b);
    forward.min(backward)
}

/// Generate a secure ticket for an election channel
///
/// Tickets uniquely identify challenge channels and prevent cross-channel attacks.
/// The ticket is generated as: Blake3(challenge_token || first_hop_peer || SECRET)
///
/// # Arguments
/// * `challenge_token` - The token being challenged in this election
/// * `first_hop_peer` - The first peer on this channel's route
///
/// # Returns
/// A u64 ticket (first 8 bytes of Blake3 hash)
///
/// # Security
/// - Deterministic: same inputs → same ticket
/// - Unpredictable: secret prevents forgery
/// - Unique per channel: different first-hop → different ticket
/// - Cannot be forged without knowing the SECRET
///
/// # Panics
/// Panics if `initialize_election_secret()` was not called first
///
/// # Example
/// ```
/// use ec_rust::ec_proof_of_storage::{initialize_election_secret, generate_ticket};
///
/// initialize_election_secret([42u8; 32]).unwrap();
///
/// let token = 1000;
/// let peer = 500;
/// let ticket = generate_ticket(token, peer);
///
/// // Same inputs produce same ticket
/// assert_eq!(ticket, generate_ticket(token, peer));
///
/// // Different peer produces different ticket
/// assert_ne!(ticket, generate_ticket(token, 501));
/// ```
pub fn generate_ticket(challenge_token: TokenId, first_hop_peer: PeerId) -> MessageTicket {
    let secret = ELECTION_SECRET
        .get()
        .expect("Election secret not initialized - call initialize_election_secret() first");

    let mut hasher = blake3::Hasher::new();
    hasher.update(&challenge_token.to_le_bytes());
    hasher.update(&first_hop_peer.to_le_bytes());
    hasher.update(secret);

    // Take first 8 bytes of hash as u64 ticket
    let hash = hasher.finalize();
    u64::from_le_bytes(hash.as_bytes()[0..8].try_into().unwrap())
}

/// Count common mappings between two signatures
///
/// Helper function for consensus clustering in elections.
fn count_common_mappings_for_election(sig1: &TokenSignature, sig2: &TokenSignature) -> usize {
    let mut count = 0;
    for mapping1 in &sig1.signature {
        for mapping2 in &sig2.signature {
            if mapping1.id == mapping2.id && mapping1.block == mapping2.block {
                count += 1;
                break;
            }
        }
    }
    count
}

/// Find ALL valid consensus clusters from signatures
///
/// Returns all maximal clusters where all pairs agree on at least `min_threshold` mappings
/// and the cluster has at least `min_size` members. Filters out clusters that are subsets
/// of larger clusters.
///
/// This is used for split-brain detection - if multiple large clusters exist, it indicates
/// competing views of network state.
fn find_all_consensus_clusters(
    signatures: &[TokenSignature],
    min_threshold: usize,
    min_size: usize,
) -> Vec<ConsensusCluster> {
    let n = signatures.len();

    if n == 0 {
        return vec![];
    }

    if n == 1 {
        if min_size <= 1 {
            return vec![ConsensusCluster {
                members: vec![0],
                min_agreement: SIGNATURE_CHUNKS,
                avg_agreement: SIGNATURE_CHUNKS as f64,
            }];
        } else {
            return vec![];
        }
    }

    // Build pairwise agreement matrix
    let mut agreement = vec![vec![0usize; n]; n];
    for i in 0..n {
        agreement[i][i] = SIGNATURE_CHUNKS;
        for j in (i + 1)..n {
            let common = count_common_mappings_for_election(&signatures[i], &signatures[j]);
            agreement[i][j] = common;
            agreement[j][i] = common;
        }
    }

    let mut all_clusters = Vec::new();

    // Check all possible subsets (2^n combinations)
    for mask in 1..(1 << n) {
        let members: Vec<usize> = (0..n).filter(|&i| (mask & (1 << i)) != 0).collect();

        if members.len() < min_size {
            continue; // Skip clusters below minimum size
        }

        // Check if this is a valid cluster
        let mut min_agreement = SIGNATURE_CHUNKS;
        let mut total_agreement = 0usize;
        let mut pair_count = 0;
        let mut valid = true;

        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                let agree = agreement[members[i]][members[j]];
                if agree < min_threshold {
                    valid = false;
                    break;
                }
                min_agreement = min_agreement.min(agree);
                total_agreement += agree;
                pair_count += 1;
            }
            if !valid {
                break;
            }
        }

        if !valid {
            continue;
        }

        let avg_agreement = if pair_count > 0 {
            total_agreement as f64 / pair_count as f64
        } else {
            SIGNATURE_CHUNKS as f64
        };

        all_clusters.push(ConsensusCluster {
            members,
            min_agreement,
            avg_agreement,
        });
    }

    // Filter out clusters that are strict subsets of larger clusters
    remove_subset_clusters(all_clusters)
}

/// Remove clusters that are strict subsets of other clusters
///
/// Returns only maximal clusters - those that are not proper subsets of any other cluster.
fn remove_subset_clusters(clusters: Vec<ConsensusCluster>) -> Vec<ConsensusCluster> {
    let mut maximal_clusters = Vec::new();

    for candidate in &clusters {
        let is_subset = clusters.iter().any(|other| {
            if candidate.members.len() >= other.members.len() {
                return false;
            }
            // Check if candidate is strict subset of other
            candidate
                .members
                .iter()
                .all(|m| other.members.contains(m))
        });

        if !is_subset {
            maximal_clusters.push(candidate.clone());
        }
    }

    maximal_clusters
}

// ============================================================================
// Peer Election System: Channel Structures
// ============================================================================

/// State of an election channel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// Waiting for response
    Pending,

    /// Valid response received
    Responded,

    /// Multiple responses received - channel blocked (RED FLAG)
    Blocked,
}

/// Response received from a channel
#[derive(Debug, Clone)]
pub struct ChannelResponse {
    /// Proof-of-storage signature from responder
    pub signature: TokenSignature,

    /// Peer that generated this signature
    pub responder: PeerId,

    /// Time when response was received
    pub received_at: EcTime,
}

/// A single challenge channel in an election
///
/// Each channel represents an independent route through the network,
/// starting from a specific first-hop peer. Channels can receive at most
/// one response - duplicate responses trigger blocking as an anti-gaming mechanism.
#[derive(Debug, Clone)]
pub struct ElectionChannel {
    /// Ticket uniquely identifying this channel
    pub ticket: MessageTicket,

    /// First-hop peer this challenge was sent to
    pub first_hop_peer: PeerId,

    /// Time when challenge was sent
    pub sent_at: EcTime,

    /// Current channel state
    pub state: ChannelState,

    /// Response if received (None if still pending)
    pub response: Option<ChannelResponse>,
}

impl ElectionChannel {
    /// Create a new pending channel
    pub fn new(ticket: MessageTicket, first_hop_peer: PeerId, sent_at: EcTime) -> Self {
        Self {
            ticket,
            first_hop_peer,
            sent_at,
            state: ChannelState::Pending,
            response: None,
        }
    }
}

// ============================================================================
// Peer Election System: Configuration and Results
// ============================================================================

/// Configuration for peer elections
#[derive(Debug, Clone)]
pub struct ElectionConfig {
    /// Minimum agreement required for consensus (default: 8/10 mappings)
    pub consensus_threshold: usize,

    /// Minimum cluster size (default: 2 peers)
    pub min_cluster_size: usize,

    /// Maximum channels to spawn (default: 10)
    pub max_channels: usize,

    /// Minimum collection time before checking consensus (ms, default: 2000)
    pub min_collection_time: u64,

    /// Total election timeout (ms, default: 5000)
    pub ttl_ms: u64,

    /// Majority threshold for decisive win (default: 0.6 = 60%)
    /// Winning cluster must have this fraction of valid responses to be decisive
    pub majority_threshold: f64,
}

impl Default for ElectionConfig {
    fn default() -> Self {
        Self {
            consensus_threshold: 8,
            min_cluster_size: 2,
            max_channels: 10,
            min_collection_time: 2000,
            ttl_ms: 5000,
            majority_threshold: 0.6,
        }
    }
}

/// Result of a successful election
#[derive(Debug, Clone, PartialEq)]
pub struct ElectionResult {
    /// The elected winner (peer closest to challenge_token)
    pub winner: PeerId,

    /// The consensus cluster
    pub cluster: ConsensusCluster,

    /// Signatures from cluster members
    pub cluster_signatures: Vec<(PeerId, TokenSignature)>,

    /// Competing clusters if any (split-brain detection)
    pub competing_clusters: Vec<ConsensusCluster>,

    /// Is this a split-brain scenario?
    pub is_split_brain: bool,
}

/// Result of an election attempt
#[derive(Debug, Clone, PartialEq)]
pub enum ElectionAttempt {
    /// Clear winner found (decisive majority)
    Winner(ElectionResult),

    /// Split-brain detected, need more channels to resolve
    SplitBrain {
        /// Current clusters found
        current_clusters: Vec<ConsensusCluster>,
        /// Suggested number of additional channels to spawn
        suggested_channels: usize,
    },

    /// No consensus found yet
    NoConsensus,
}

/// Errors that can occur during election
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElectionError {
    /// Ticket not found in this election
    UnknownTicket,

    /// Duplicate response on channel (anti-gaming triggered)
    DuplicateResponse,

    /// Election already resolved or failed
    ElectionClosed,

    /// Maximum channels limit reached
    MaxChannelsReached,
}

/// State of an election
#[derive(Debug, Clone, PartialEq)]
pub enum ElectionState {
    /// Election is active and collecting responses
    Active,

    /// Election completed successfully
    Resolved(ElectionResult),

    /// Election failed (timeout without consensus)
    Failed,
}

// ============================================================================
// Peer Election System: Core Election Logic
// ============================================================================

/// Peer election for discovering aligned peers
///
/// Manages a single election for a challenge token. Creates multiple independent
/// channels, collects responses, finds consensus clusters, and elects a winner.
///
/// # Example
/// ```
/// use ec_rust::ec_proof_of_storage::{PeerElection, ElectionConfig, initialize_election_secret};
///
/// // Initialize secret first
/// initialize_election_secret([42u8; 32]).unwrap();
///
/// let mut election = PeerElection::new(1000, 0, ElectionConfig::default());
///
/// // Create channels
/// let ticket1 = election.create_channel(100, 0).unwrap();
/// let ticket2 = election.create_channel(200, 0).unwrap();
///
/// // Submit responses (signatures would be real TokenSignatures)
/// // election.submit_response(ticket1, signature, responder, time).unwrap();
/// ```
pub struct PeerElection {
    /// Token being challenged
    challenge_token: TokenId,

    /// All channels indexed by ticket
    channels: HashMap<MessageTicket, ElectionChannel>,

    /// Election start time
    started_at: EcTime,

    /// Current state
    state: ElectionState,

    /// Configuration
    config: ElectionConfig,
}

impl PeerElection {
    /// Create a new election for a challenge token
    ///
    /// # Arguments
    /// * `challenge_token` - Token to challenge
    /// * `started_at` - Start time
    /// * `config` - Election configuration
    pub fn new(challenge_token: TokenId, started_at: EcTime, config: ElectionConfig) -> Self {
        Self {
            challenge_token,
            channels: HashMap::new(),
            started_at,
            state: ElectionState::Active,
            config,
        }
    }

    /// Create a new channel to a first-hop peer
    ///
    /// Generates a ticket and stores the channel as Pending.
    ///
    /// # Returns
    /// * `Ok(ticket)` - Ticket to include in challenge Query message
    /// * `Err(MaxChannelsReached)` - Cannot create more channels
    pub fn create_channel(
        &mut self,
        first_hop: PeerId,
        sent_at: EcTime,
    ) -> Result<MessageTicket, ElectionError> {
        if self.channels.len() >= self.config.max_channels {
            return Err(ElectionError::MaxChannelsReached);
        }

        let ticket = generate_ticket(self.challenge_token, first_hop);
        let channel = ElectionChannel::new(ticket, first_hop, sent_at);
        self.channels.insert(ticket, channel);

        Ok(ticket)
    }

    /// Submit a response for a channel
    ///
    /// # Arguments
    /// * `ticket` - Channel ticket
    /// * `signature` - Proof-of-storage signature
    /// * `responder` - Peer that generated the signature
    /// * `received_at` - Response time
    ///
    /// # Returns
    /// * `Ok(())` - Response stored successfully
    /// * `Err(UnknownTicket)` - Ticket not found
    /// * `Err(DuplicateResponse)` - Channel already has response (now blocked)
    /// * `Err(ElectionClosed)` - Election already resolved or failed
    pub fn submit_response(
        &mut self,
        ticket: MessageTicket,
        signature: TokenSignature,
        responder: PeerId,
        received_at: EcTime,
    ) -> Result<(), ElectionError> {
        // Check election is still active
        if !matches!(self.state, ElectionState::Active) {
            return Err(ElectionError::ElectionClosed);
        }

        // Get channel
        let channel = self
            .channels
            .get_mut(&ticket)
            .ok_or(ElectionError::UnknownTicket)?;

        // Detect duplicate (anti-gaming mechanism)
        if channel.response.is_some() {
            channel.state = ChannelState::Blocked;
            return Err(ElectionError::DuplicateResponse);
        }

        // Store response
        channel.response = Some(ChannelResponse {
            signature,
            responder,
            received_at,
        });
        channel.state = ChannelState::Responded;

        Ok(())
    }

    /// Check if we should start checking for consensus
    ///
    /// Returns true if minimum collection time has passed.
    pub fn should_check_consensus(&self, current_time: EcTime) -> bool {
        let elapsed = current_time.saturating_sub(self.started_at);
        elapsed >= self.config.min_collection_time
    }

    /// Try to elect a winner with split-brain detection
    ///
    /// Checks for consensus clusters and determines if:
    /// - There's a decisive winner (majority)
    /// - Split-brain detected (competing clusters)
    /// - No consensus yet
    ///
    /// # Returns
    /// * `ElectionAttempt::Winner` - Clear winner with decisive majority
    /// * `ElectionAttempt::SplitBrain` - Competing clusters detected, more channels suggested
    /// * `ElectionAttempt::NoConsensus` - Not enough agreement yet
    pub fn try_elect_winner(&mut self) -> ElectionAttempt {
        if !matches!(self.state, ElectionState::Active) {
            return ElectionAttempt::NoConsensus;
        }

        // Get valid responses (non-blocked)
        let valid_responses: Vec<_> = self
            .channels
            .values()
            .filter(|ch| ch.state == ChannelState::Responded)
            .filter_map(|ch| ch.response.as_ref().map(|r| (ch.ticket, r.clone())))
            .collect();

        if valid_responses.len() < self.config.min_cluster_size {
            return ElectionAttempt::NoConsensus;
        }

        // Extract signatures for clustering
        let signatures: Vec<_> = valid_responses
            .iter()
            .map(|(_, resp)| resp.signature.clone())
            .collect();

        // Find ALL consensus clusters
        let mut all_clusters = find_all_consensus_clusters(
            &signatures,
            self.config.consensus_threshold,
            self.config.min_cluster_size,
        );

        if all_clusters.is_empty() {
            return ElectionAttempt::NoConsensus;
        }

        // Sort clusters by strength (size, then avg_agreement)
        all_clusters.sort_by(|a, b| {
            match b.members.len().cmp(&a.members.len()) {
                std::cmp::Ordering::Equal => b
                    .avg_agreement
                    .partial_cmp(&a.avg_agreement)
                    .unwrap_or(std::cmp::Ordering::Equal),
                other => other,
            }
        });

        let strongest = &all_clusters[0];
        let total_valid = valid_responses.len();

        // Check if strongest cluster has decisive majority
        let cluster_fraction = strongest.members.len() as f64 / total_valid as f64;
        let has_decisive_majority = cluster_fraction >= self.config.majority_threshold;

        // Check for competing clusters (split-brain)
        let has_competing_clusters = all_clusters.len() > 1
            && all_clusters[1].members.len() as f64 / total_valid as f64 > 0.2; // Second cluster has >20% support

        if !has_decisive_majority && has_competing_clusters {
            // Split-brain scenario - suggest spawning more channels
            let suggested = Self::calculate_channels_needed(&all_clusters, total_valid);

            return ElectionAttempt::SplitBrain {
                current_clusters: all_clusters,
                suggested_channels: suggested,
            };
        }

        // Either has decisive majority or no significant competition
        // Select winner from strongest cluster
        let (winner, cluster_sigs) =
            Self::select_winner(self.challenge_token, strongest, &valid_responses);

        // Build competing clusters list (if any)
        let competing_clusters = if all_clusters.len() > 1 {
            all_clusters[1..].to_vec()
        } else {
            vec![]
        };

        let result = ElectionResult {
            winner,
            cluster: strongest.clone(),
            cluster_signatures: cluster_sigs,
            competing_clusters,
            is_split_brain: !has_decisive_majority && has_competing_clusters,
        };

        self.state = ElectionState::Resolved(result.clone());
        ElectionAttempt::Winner(result)
    }

    /// Calculate how many additional channels are needed to break a split-brain
    ///
    /// Suggests spawning enough channels to push one cluster above majority threshold
    fn calculate_channels_needed(clusters: &[ConsensusCluster], current_total: usize) -> usize {
        if clusters.len() < 2 {
            return 0;
        }

        let largest = clusters[0].members.len();
        let second_largest = clusters[1].members.len();

        // Calculate how many responses needed for largest cluster to reach 60% majority
        // current_total + additional = new_total
        // (largest + some_fraction_of_additional) / new_total >= 0.6
        // We assume best case: all new responses favor the largest cluster
        // largest / (current_total + additional) = 0.6
        // largest = 0.6 * (current_total + additional)
        // largest = 0.6 * current_total + 0.6 * additional
        // largest - 0.6 * current_total = 0.6 * additional
        // additional = (largest - 0.6 * current_total) / 0.6

        let needed = ((largest as f64) / 0.6 - current_total as f64).ceil() as isize;

        // Suggest at least the difference between top two clusters
        let min_suggested = (second_largest as isize - largest as isize).abs() + 2;

        needed.max(min_suggested).max(1) as usize
    }

    /// Select winner from consensus cluster (peer closest to challenge_token)
    fn select_winner(
        challenge_token: TokenId,
        cluster: &ConsensusCluster,
        responses: &[(MessageTicket, ChannelResponse)],
    ) -> (PeerId, Vec<(PeerId, TokenSignature)>) {
        // Extract cluster members' responses
        let cluster_responses: Vec<_> = cluster
            .members
            .iter()
            .map(|&idx| {
                let (_, resp) = &responses[idx];
                (resp.responder, resp.signature.clone())
            })
            .collect();

        // Find peer with minimum ring distance to challenge_token
        let winner = cluster_responses
            .iter()
            .map(|(peer_id, _)| peer_id)
            .min_by_key(|&&peer_id| ring_distance(peer_id, challenge_token))
            .copied()
            .expect("Cluster has members");

        (winner, cluster_responses)
    }

    /// Check if election has expired
    pub fn is_expired(&self, current_time: EcTime) -> bool {
        let elapsed = current_time.saturating_sub(self.started_at);
        elapsed >= self.config.ttl_ms
    }

    /// Get number of valid (non-blocked) responses
    pub fn valid_response_count(&self) -> usize {
        self.channels
            .values()
            .filter(|ch| ch.state == ChannelState::Responded)
            .count()
    }

    /// Check if we can create more channels
    pub fn can_create_channel(&self) -> bool {
        self.channels.len() < self.config.max_channels
    }

    /// Get current election state
    pub fn state(&self) -> &ElectionState {
        &self.state
    }

    /// Get challenge token
    pub fn challenge_token(&self) -> TokenId {
        self.challenge_token
    }
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

    /// Count how many TokenMappings two signatures have in common
    ///
    /// Compares the signature arrays (not the answer field) to find matching
    /// (token_id, block_id) pairs. Order doesn't matter - any mapping in sig1
    /// that appears anywhere in sig2 counts as a match.
    ///
    /// # Performance
    /// O(TOKENS_SIGNATURE_SIZE²) = O(100) for 10-element arrays.
    /// For small signature arrays this is faster than building hash sets.
    fn count_common_mappings(sig1: &TokenSignature, sig2: &TokenSignature) -> usize {
        let mut count = 0;
        for mapping1 in &sig1.signature {
            for mapping2 in &sig2.signature {
                if mapping1.id == mapping2.id && mapping1.block == mapping2.block {
                    count += 1;
                    break; // Found a match, move to next mapping1
                }
            }
        }
        count
    }

    /// Find the consensus cluster with highest mutual agreement
    ///
    /// This finds a group of signatures where all members agree with each other
    /// above a minimum threshold. Unlike simple ranking, this ensures the cluster
    /// forms a mutually-agreeing group, not just signatures that individually
    /// overlap with different subsets.
    ///
    /// # Algorithm
    /// For small N (< 10 signatures), we use an exhaustive search:
    /// 1. Build pairwise agreement matrix
    /// 2. Find the largest clique where all pairs agree >= min_threshold
    /// 3. Among cliques of same size, pick the one with highest average agreement
    ///
    /// # Arguments
    /// - `signatures`: List of signatures to analyze
    /// - `min_threshold`: Minimum common mappings required between any pair (0-10)
    ///
    /// # Returns
    /// The best consensus cluster, or None if no valid cluster exists
    ///
    /// # Example
    /// ```rust
    /// let signatures = vec![sig1, sig2, sig3, sig4];
    /// // Find cluster where all pairs agree on at least 7/10 mappings
    /// if let Some(cluster) = ProofOfStorage::find_consensus_cluster(&signatures, 7) {
    ///     println!("Found {} agreeing signatures", cluster.members.len());
    ///     let consensus_sigs: Vec<_> = cluster.members.iter()
    ///         .map(|&i| &signatures[i])
    ///         .collect();
    /// }
    /// ```
    pub fn find_consensus_cluster(
        signatures: &[TokenSignature],
        min_threshold: usize,
    ) -> Option<ConsensusCluster> {
        let n = signatures.len();

        if n == 0 {
            return None;
        }

        if n == 1 {
            return Some(ConsensusCluster {
                members: vec![0],
                min_agreement: SIGNATURE_CHUNKS,
                avg_agreement: SIGNATURE_CHUNKS as f64,
            });
        }

        // Build pairwise agreement matrix
        let mut agreement = vec![vec![0usize; n]; n];
        for i in 0..n {
            agreement[i][i] = SIGNATURE_CHUNKS; // Perfect self-agreement
            for j in (i + 1)..n {
                let common = Self::count_common_mappings(&signatures[i], &signatures[j]);
                agreement[i][j] = common;
                agreement[j][i] = common;
            }
        }

        // For N < 10, we can afford to check all subsets
        // Start with largest possible clusters and work down
        let mut best_cluster: Option<ConsensusCluster> = None;

        // Check all possible subsets (2^n combinations)
        // For n=10, this is 1024 checks, which is fine
        for mask in 1..(1 << n) {
            let members: Vec<usize> = (0..n).filter(|&i| (mask & (1 << i)) != 0).collect();

            if members.len() < 2 {
                continue; // Skip single-member clusters
            }

            // Check if this is a valid cluster (all pairs meet threshold)
            let mut min_agreement = SIGNATURE_CHUNKS;
            let mut total_agreement = 0usize;
            let mut pair_count = 0;
            let mut valid = true;

            for i in 0..members.len() {
                for j in (i + 1)..members.len() {
                    let agree = agreement[members[i]][members[j]];
                    if agree < min_threshold {
                        valid = false;
                        break;
                    }
                    min_agreement = min_agreement.min(agree);
                    total_agreement += agree;
                    pair_count += 1;
                }
                if !valid {
                    break;
                }
            }

            if !valid {
                continue;
            }

            let avg_agreement = if pair_count > 0 {
                total_agreement as f64 / pair_count as f64
            } else {
                0.0
            };

            let cluster = ConsensusCluster {
                members: members.clone(),
                min_agreement,
                avg_agreement,
            };

            // Update best cluster if this is better
            // Prioritize: larger size, then higher average agreement
            best_cluster = Some(match best_cluster {
                None => cluster,
                Some(best) => {
                    if cluster.members.len() > best.members.len() {
                        cluster
                    } else if cluster.members.len() == best.members.len()
                        && cluster.avg_agreement > best.avg_agreement
                    {
                        cluster
                    } else {
                        best
                    }
                }
            });
        }

        best_cluster
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

    #[test]
    fn test_count_common_mappings() {
        use crate::ec_interface::TokenMapping;

        let sig1 = TokenSignature {
            answer: TokenMapping { id: 1, block: 100 },
            signature: [
                TokenMapping { id: 10, block: 1 },
                TokenMapping { id: 20, block: 2 },
                TokenMapping { id: 30, block: 3 },
                TokenMapping { id: 40, block: 4 },
                TokenMapping { id: 50, block: 5 },
                TokenMapping { id: 60, block: 6 },
                TokenMapping { id: 70, block: 7 },
                TokenMapping { id: 80, block: 8 },
                TokenMapping { id: 90, block: 9 },
                TokenMapping { id: 100, block: 10 },
            ],
        };

        let sig2 = TokenSignature {
            answer: TokenMapping { id: 1, block: 100 },
            signature: [
                TokenMapping { id: 10, block: 1 },  // match
                TokenMapping { id: 20, block: 2 },  // match
                TokenMapping { id: 30, block: 3 },  // match
                TokenMapping { id: 999, block: 999 },
                TokenMapping { id: 999, block: 999 },
                TokenMapping { id: 999, block: 999 },
                TokenMapping { id: 999, block: 999 },
                TokenMapping { id: 999, block: 999 },
                TokenMapping { id: 999, block: 999 },
                TokenMapping { id: 999, block: 999 },
            ],
        };

        let count = ProofOfStorage::<TestBackend>::count_common_mappings(&sig1, &sig2);
        assert_eq!(count, 3, "Should find 3 common mappings");
    }

    #[test]
    fn test_find_consensus_cluster_perfect_agreement() {
        use crate::ec_interface::TokenMapping;

        // Create 3 signatures that all agree perfectly
        let perfect_mappings = [
            TokenMapping { id: 1, block: 1 },
            TokenMapping { id: 2, block: 2 },
            TokenMapping { id: 3, block: 3 },
            TokenMapping { id: 4, block: 4 },
            TokenMapping { id: 5, block: 5 },
            TokenMapping { id: 6, block: 6 },
            TokenMapping { id: 7, block: 7 },
            TokenMapping { id: 8, block: 8 },
            TokenMapping { id: 9, block: 9 },
            TokenMapping { id: 10, block: 10 },
        ];

        let signatures = vec![
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: perfect_mappings,
            },
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: perfect_mappings,
            },
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: perfect_mappings,
            },
        ];

        let cluster = ProofOfStorage::<TestBackend>::find_consensus_cluster(&signatures, 10);

        assert!(cluster.is_some());
        let cluster = cluster.unwrap();
        assert_eq!(cluster.members.len(), 3, "All 3 should be in cluster");
        assert_eq!(cluster.min_agreement, 10, "Perfect agreement");
        assert_eq!(cluster.avg_agreement, 10.0, "Perfect average");
    }

    #[test]
    fn test_find_consensus_cluster_with_outlier() {
        use crate::ec_interface::TokenMapping;

        // Create 3 signatures: sig1 and sig2 agree well, sig3 is outlier
        let common = [
            TokenMapping { id: 1, block: 1 },
            TokenMapping { id: 2, block: 2 },
            TokenMapping { id: 3, block: 3 },
            TokenMapping { id: 4, block: 4 },
            TokenMapping { id: 5, block: 5 },
            TokenMapping { id: 6, block: 6 },
            TokenMapping { id: 7, block: 7 },
            TokenMapping { id: 8, block: 8 },
            TokenMapping { id: 9, block: 9 },
            TokenMapping { id: 10, block: 10 },
        ];

        let outlier = [
            TokenMapping { id: 100, block: 100 },
            TokenMapping { id: 101, block: 101 },
            TokenMapping { id: 102, block: 102 },
            TokenMapping { id: 103, block: 103 },
            TokenMapping { id: 104, block: 104 },
            TokenMapping { id: 105, block: 105 },
            TokenMapping { id: 106, block: 106 },
            TokenMapping { id: 107, block: 107 },
            TokenMapping { id: 108, block: 108 },
            TokenMapping { id: 109, block: 109 },
        ];

        let signatures = vec![
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: common,
            },
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: common,
            },
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: outlier,
            },
        ];

        // With threshold of 8, only sig1 and sig2 should form a cluster
        let cluster = ProofOfStorage::<TestBackend>::find_consensus_cluster(&signatures, 8);

        assert!(cluster.is_some());
        let cluster = cluster.unwrap();
        assert_eq!(cluster.members.len(), 2, "Only sig1 and sig2 in cluster");
        assert!(cluster.members.contains(&0) && cluster.members.contains(&1));
        assert_eq!(cluster.min_agreement, 10);
    }

    #[test]
    fn test_find_consensus_cluster_two_groups() {
        use crate::ec_interface::TokenMapping;

        // Create two groups that agree internally but not with each other
        // Group A: sig0, sig1 (agree on 10/10)
        // Group B: sig2, sig3, sig4 (agree on 10/10)
        // Inter-group agreement: 2/10

        let group_a_mappings = [
            TokenMapping { id: 1, block: 1 },
            TokenMapping { id: 2, block: 2 },
            TokenMapping { id: 10, block: 10 },
            TokenMapping { id: 11, block: 11 },
            TokenMapping { id: 12, block: 12 },
            TokenMapping { id: 13, block: 13 },
            TokenMapping { id: 14, block: 14 },
            TokenMapping { id: 15, block: 15 },
            TokenMapping { id: 16, block: 16 },
            TokenMapping { id: 17, block: 17 },
        ];

        let group_b_mappings = [
            TokenMapping { id: 1, block: 1 },
            TokenMapping { id: 2, block: 2 },
            TokenMapping { id: 20, block: 20 },
            TokenMapping { id: 21, block: 21 },
            TokenMapping { id: 22, block: 22 },
            TokenMapping { id: 23, block: 23 },
            TokenMapping { id: 24, block: 24 },
            TokenMapping { id: 25, block: 25 },
            TokenMapping { id: 26, block: 26 },
            TokenMapping { id: 27, block: 27 },
        ];

        let signatures = vec![
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: group_a_mappings,
            },
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: group_a_mappings,
            },
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: group_b_mappings,
            },
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: group_b_mappings,
            },
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: group_b_mappings,
            },
        ];

        // With threshold of 8, should find the larger group (Group B with 3 members)
        let cluster = ProofOfStorage::<TestBackend>::find_consensus_cluster(&signatures, 8);

        assert!(cluster.is_some());
        let cluster = cluster.unwrap();
        assert_eq!(
            cluster.members.len(),
            3,
            "Should find the larger group (Group B)"
        );
        // Members should be indices 2, 3, 4
        assert!(cluster.members.contains(&2));
        assert!(cluster.members.contains(&3));
        assert!(cluster.members.contains(&4));
        assert_eq!(cluster.min_agreement, 10);
        assert_eq!(cluster.avg_agreement, 10.0);
    }

    #[test]
    fn test_find_consensus_cluster_no_threshold_met() {
        use crate::ec_interface::TokenMapping;

        // Create signatures that all disagree significantly
        let signatures = vec![
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: [
                    TokenMapping { id: 1, block: 1 },
                    TokenMapping { id: 2, block: 2 },
                    TokenMapping { id: 10, block: 10 },
                    TokenMapping { id: 11, block: 11 },
                    TokenMapping { id: 12, block: 12 },
                    TokenMapping { id: 13, block: 13 },
                    TokenMapping { id: 14, block: 14 },
                    TokenMapping { id: 15, block: 15 },
                    TokenMapping { id: 16, block: 16 },
                    TokenMapping { id: 17, block: 17 },
                ],
            },
            TokenSignature {
                answer: TokenMapping { id: 999, block: 999 },
                signature: [
                    TokenMapping { id: 1, block: 1 },
                    TokenMapping { id: 2, block: 2 },
                    TokenMapping { id: 20, block: 20 },
                    TokenMapping { id: 21, block: 21 },
                    TokenMapping { id: 22, block: 22 },
                    TokenMapping { id: 23, block: 23 },
                    TokenMapping { id: 24, block: 24 },
                    TokenMapping { id: 25, block: 25 },
                    TokenMapping { id: 26, block: 26 },
                    TokenMapping { id: 27, block: 27 },
                ],
            },
        ];

        // With very high threshold, no cluster should be found
        let cluster = ProofOfStorage::<TestBackend>::find_consensus_cluster(&signatures, 9);

        // Should find nothing above threshold 9 (they only agree on 2/10)
        assert!(cluster.is_none() || cluster.unwrap().members.len() == 1);
    }

    #[test]
    fn test_consensus_cluster_empty_input() {
        let signatures: Vec<TokenSignature> = vec![];
        let cluster = ProofOfStorage::<TestBackend>::find_consensus_cluster(&signatures, 5);
        assert!(cluster.is_none());
    }

    #[test]
    fn test_consensus_cluster_single_signature() {
        use crate::ec_interface::TokenMapping;

        let signatures = vec![TokenSignature {
            answer: TokenMapping { id: 999, block: 999 },
            signature: [TokenMapping { id: 1, block: 1 }; SIGNATURE_CHUNKS],
        }];

        let cluster = ProofOfStorage::<TestBackend>::find_consensus_cluster(&signatures, 5);
        assert!(cluster.is_some());
        let cluster = cluster.unwrap();
        assert_eq!(cluster.members.len(), 1);
        assert_eq!(cluster.members[0], 0);
    }

    // ========================================================================
    // Peer Election Tests
    // ========================================================================

    #[test]
    fn test_ring_distance_normal() {
        // Normal case: forward distance shorter
        assert_eq!(ring_distance(100, 150), 50);
        assert_eq!(ring_distance(150, 100), 50); // symmetric
    }

    #[test]
    fn test_ring_distance_wrapping() {
        // Wrapping case: backward distance shorter
        let result = ring_distance(10, u64::MAX - 5);
        // Forward: MAX - 5 - 10 = MAX - 15
        // Backward: 10 - (MAX - 5) = 16 (wraps)
        assert_eq!(result, 16);
    }

    #[test]
    fn test_ring_distance_opposite_sides() {
        // Exactly opposite on ring
        let mid = u64::MAX / 2;
        let dist = ring_distance(0, mid);
        // Both directions should be similar
        assert!(dist > mid - 100 && dist < mid + 100);
    }

    #[test]
    fn test_ring_distance_self() {
        assert_eq!(ring_distance(42, 42), 0);
        assert_eq!(ring_distance(0, 0), 0);
        assert_eq!(ring_distance(u64::MAX, u64::MAX), 0);
    }

    #[test]
    fn test_ticket_generation_deterministic() {
        // Initialize secret
        let _ = initialize_election_secret([42u8; 32]);

        let token = 1000;
        let peer = 500;

        let ticket1 = generate_ticket(token, peer);
        let ticket2 = generate_ticket(token, peer);

        assert_eq!(ticket1, ticket2, "Same inputs should produce same ticket");
    }

    #[test]
    fn test_ticket_generation_unique_per_channel() {
        let _ = initialize_election_secret([42u8; 32]);

        let token = 1000;
        let peer1 = 500;
        let peer2 = 501;

        let ticket1 = generate_ticket(token, peer1);
        let ticket2 = generate_ticket(token, peer2);

        assert_ne!(ticket1, ticket2, "Different peers should produce different tickets");
    }

    #[test]
    fn test_ticket_generation_unique_per_token() {
        let _ = initialize_election_secret([42u8; 32]);

        let token1 = 1000;
        let token2 = 1001;
        let peer = 500;

        let ticket1 = generate_ticket(token1, peer);
        let ticket2 = generate_ticket(token2, peer);

        assert_ne!(ticket1, ticket2, "Different tokens should produce different tickets");
    }

    // Helper function to create test signatures
    fn create_test_signature(mappings: [(TokenId, BlockId); SIGNATURE_CHUNKS]) -> TokenSignature {
        use crate::ec_interface::TokenMapping;

        let mut signature = [TokenMapping { id: 0, block: 0 }; SIGNATURE_CHUNKS];
        for (i, (id, block)) in mappings.iter().enumerate() {
            signature[i] = TokenMapping { id: *id, block: *block };
        }

        TokenSignature {
            answer: TokenMapping { id: 999, block: 999 },
            signature,
        }
    }

    #[test]
    fn test_election_create_channel() {
        let _ = initialize_election_secret([42u8; 32]);

        let mut election = PeerElection::new(1000, 0, ElectionConfig::default());

        let ticket = election.create_channel(100, 0).unwrap();
        assert!(ticket > 0, "Ticket should be non-zero");

        assert_eq!(election.channels.len(), 1);
        assert!(election.channels.contains_key(&ticket));
    }

    #[test]
    fn test_election_max_channels_limit() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            max_channels: 3,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        // Create 3 channels (max)
        assert!(election.create_channel(100, 0).is_ok());
        assert!(election.create_channel(200, 0).is_ok());
        assert!(election.create_channel(300, 0).is_ok());

        // 4th should fail
        assert_eq!(
            election.create_channel(400, 0),
            Err(ElectionError::MaxChannelsReached)
        );
    }

    #[test]
    fn test_election_submit_response() {
        let _ = initialize_election_secret([42u8; 32]);

        let mut election = PeerElection::new(1000, 0, ElectionConfig::default());
        let ticket = election.create_channel(100, 0).unwrap();

        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);

        assert!(election.submit_response(ticket, sig, 101, 10).is_ok());

        // Verify channel updated
        let channel = election.channels.get(&ticket).unwrap();
        assert_eq!(channel.state, ChannelState::Responded);
        assert!(channel.response.is_some());
        assert_eq!(channel.response.as_ref().unwrap().responder, 101);
    }

    #[test]
    fn test_election_duplicate_response_blocked() {
        let _ = initialize_election_secret([42u8; 32]);

        let mut election = PeerElection::new(1000, 0, ElectionConfig::default());
        let ticket = election.create_channel(100, 0).unwrap();

        let sig1 = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        let sig2 = create_test_signature([(2, 20); SIGNATURE_CHUNKS]);

        // First response succeeds
        assert!(election.submit_response(ticket, sig1, 101, 10).is_ok());

        // Second response should fail and block channel
        assert_eq!(
            election.submit_response(ticket, sig2, 102, 20),
            Err(ElectionError::DuplicateResponse)
        );

        // Channel should be blocked
        let channel = election.channels.get(&ticket).unwrap();
        assert_eq!(channel.state, ChannelState::Blocked);
    }

    #[test]
    fn test_election_unknown_ticket() {
        let _ = initialize_election_secret([42u8; 32]);

        let mut election = PeerElection::new(1000, 0, ElectionConfig::default());
        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);

        let result = election.submit_response(999999, sig, 101, 10);
        assert_eq!(result, Err(ElectionError::UnknownTicket));
    }

    #[test]
    fn test_election_closed() {
        let _ = initialize_election_secret([42u8; 32]);

        let mut election = PeerElection::new(1000, 0, ElectionConfig::default());
        let ticket = election.create_channel(100, 0).unwrap();

        // Manually close election
        election.state = ElectionState::Failed;

        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        let result = election.submit_response(ticket, sig, 101, 10);

        assert_eq!(result, Err(ElectionError::ElectionClosed));
    }

    #[test]
    fn test_election_should_check_consensus() {
        let config = ElectionConfig {
            min_collection_time: 2000,
            ..Default::default()
        };
        let election = PeerElection::new(1000, 0, config);

        assert!(!election.should_check_consensus(1999), "Too early");
        assert!(election.should_check_consensus(2000), "Exact time");
        assert!(election.should_check_consensus(3000), "After time");
    }

    #[test]
    fn test_election_is_expired() {
        let config = ElectionConfig {
            ttl_ms: 5000,
            ..Default::default()
        };
        let election = PeerElection::new(1000, 0, config);

        assert!(!election.is_expired(4999), "Not expired yet");
        assert!(election.is_expired(5000), "Exact expiry");
        assert!(election.is_expired(6000), "After expiry");
    }

    #[test]
    fn test_election_valid_response_count() {
        let _ = initialize_election_secret([42u8; 32]);

        let mut election = PeerElection::new(1000, 0, ElectionConfig::default());

        let t1 = election.create_channel(100, 0).unwrap();
        let t2 = election.create_channel(200, 0).unwrap();
        let t3 = election.create_channel(300, 0).unwrap();

        assert_eq!(election.valid_response_count(), 0);

        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        election.submit_response(t1, sig.clone(), 101, 10).unwrap();
        assert_eq!(election.valid_response_count(), 1);

        election.submit_response(t2, sig.clone(), 201, 20).unwrap();
        assert_eq!(election.valid_response_count(), 2);

        // Block t3 by sending duplicate
        election.submit_response(t3, sig.clone(), 301, 30).unwrap();
        let _ = election.submit_response(t3, sig, 302, 31); // Blocks

        // Should still be 2 (blocked channel not counted)
        assert_eq!(election.valid_response_count(), 2);
    }

    #[test]
    fn test_election_consensus_with_two_agreeing() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 8,
            min_cluster_size: 2,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        // Create 2 channels
        let t1 = election.create_channel(100, 0).unwrap();
        let t2 = election.create_channel(200, 0).unwrap();

        // Create signatures that agree on 8/10 mappings
        let sig1 = create_test_signature([
            (1, 10), (2, 20), (3, 30), (4, 40), (5, 50),
            (6, 60), (7, 70), (8, 80), (91, 910), (92, 920)
        ]);
        let sig2 = create_test_signature([
            (1, 10), (2, 20), (3, 30), (4, 40), (5, 50),
            (6, 60), (7, 70), (8, 80), (93, 930), (94, 940)
        ]);

        election.submit_response(t1, sig1, 101, 10).unwrap();
        election.submit_response(t2, sig2, 201, 20).unwrap();

        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::Winner(result) => {
                assert_eq!(result.cluster.members.len(), 2);
                assert!(result.cluster.min_agreement >= 8);
            }
            _ => panic!("Should find consensus with 2 agreeing peers"),
        }
    }

    #[test]
    fn test_election_no_consensus_below_threshold() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 8,
            min_cluster_size: 2,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        let t1 = election.create_channel(100, 0).unwrap();
        let t2 = election.create_channel(200, 0).unwrap();

        // Create signatures that agree on only 5/10 mappings (below threshold)
        let sig1 = create_test_signature([
            (1, 10), (2, 20), (3, 30), (4, 40), (5, 50),
            (10, 100), (11, 110), (12, 120), (13, 130), (14, 140)
        ]);
        let sig2 = create_test_signature([
            (1, 10), (2, 20), (3, 30), (4, 40), (5, 50),
            (20, 200), (21, 210), (22, 220), (23, 230), (24, 240)
        ]);

        election.submit_response(t1, sig1, 101, 10).unwrap();
        election.submit_response(t2, sig2, 201, 20).unwrap();

        let attempt = election.try_elect_winner();
        assert!(
            matches!(attempt, ElectionAttempt::NoConsensus),
            "Should not find consensus below threshold"
        );
    }

    #[test]
    fn test_election_winner_selected_by_ring_distance() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 10, // Perfect agreement
            min_cluster_size: 3,
            ..Default::default()
        };
        let challenge_token = 1000;
        let mut election = PeerElection::new(challenge_token, 0, config);

        // Create 3 channels
        let t1 = election.create_channel(100, 0).unwrap();
        let t2 = election.create_channel(200, 0).unwrap();
        let t3 = election.create_channel(300, 0).unwrap();

        // All signatures identical (perfect agreement)
        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);

        // Responders at different distances from challenge_token (1000)
        election.submit_response(t1, sig.clone(), 1500, 10).unwrap(); // distance 500
        election.submit_response(t2, sig.clone(), 950, 20).unwrap();  // distance 50
        election.submit_response(t3, sig, 2000, 30).unwrap();         // distance 1000

        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::Winner(result) => {
                // Winner should be peer 950 (closest to 1000)
                assert_eq!(result.winner, 950, "Closest peer should win");
            }
            _ => panic!("Should elect winner"),
        }
    }

    #[test]
    fn test_election_full_workflow() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig::default();
        let mut election = PeerElection::new(1000, 0, config);

        // Phase 1: Create channels
        let t1 = election.create_channel(100, 0).unwrap();
        let t2 = election.create_channel(200, 10).unwrap();
        let t3 = election.create_channel(300, 20).unwrap();

        assert_eq!(election.state(), &ElectionState::Active);
        assert!(election.can_create_channel());

        // Phase 2: Collect responses
        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        election.submit_response(t1, sig.clone(), 101, 100).unwrap();
        election.submit_response(t2, sig.clone(), 201, 200).unwrap();
        election.submit_response(t3, sig, 301, 300).unwrap();

        assert_eq!(election.valid_response_count(), 3);

        // Phase 3: Check consensus (after min_collection_time)
        assert!(election.should_check_consensus(2000));

        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::Winner(result) => {
                assert!(matches!(election.state(), ElectionState::Resolved(_)));
                assert_eq!(result.cluster.members.len(), 3);
                assert_eq!(result.cluster_signatures.len(), 3);

                // Verify winner is one of the responders
                assert!(
                    result.winner == 101 || result.winner == 201 || result.winner == 301,
                    "Winner should be one of the responders"
                );
            }
            _ => panic!("Should elect winner with 3 agreeing peers"),
        }
    }

    #[test]
    fn test_election_ignores_blocked_channels() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 10,
            min_cluster_size: 2,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        let t1 = election.create_channel(100, 0).unwrap();
        let t2 = election.create_channel(200, 0).unwrap();
        let t3 = election.create_channel(300, 0).unwrap();

        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        let different_sig = create_test_signature([(2, 20); SIGNATURE_CHUNKS]);

        // t1 and t2: agreeing responses
        election.submit_response(t1, sig.clone(), 101, 10).unwrap();
        election.submit_response(t2, sig.clone(), 201, 20).unwrap();

        // t3: block it with duplicate
        election.submit_response(t3, sig.clone(), 301, 30).unwrap();
        let _ = election.submit_response(t3, different_sig, 302, 31);

        // Should find consensus with just t1 and t2 (t3 blocked)
        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::Winner(result) => {
                assert_eq!(result.cluster.members.len(), 2, "Blocked channel should not be included");
            }
            _ => panic!("Should find consensus with 2 valid channels"),
        }
    }

    // ========================================================================
    // Split-Brain Detection Tests (Option C Extensions)
    // ========================================================================

    #[test]
    fn test_split_brain_detected_equal_clusters() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 10,
            min_cluster_size: 2,
            majority_threshold: 0.6,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        // Create 6 channels - will form 2 equal clusters of 3 each
        let t1 = election.create_channel(100, 0).unwrap();
        let t2 = election.create_channel(200, 0).unwrap();
        let t3 = election.create_channel(300, 0).unwrap();
        let t4 = election.create_channel(400, 0).unwrap();
        let t5 = election.create_channel(500, 0).unwrap();
        let t6 = election.create_channel(600, 0).unwrap();

        // Group A: perfect agreement
        let sig_a = create_test_signature([
            (1, 10), (2, 20), (3, 30), (4, 40), (5, 50),
            (6, 60), (7, 70), (8, 80), (9, 90), (10, 100)
        ]);

        // Group B: perfect agreement (different from A)
        let sig_b = create_test_signature([
            (11, 110), (12, 120), (13, 130), (14, 140), (15, 150),
            (16, 160), (17, 170), (18, 180), (19, 190), (20, 200)
        ]);

        // 3 responses from Group A
        election.submit_response(t1, sig_a.clone(), 101, 10).unwrap();
        election.submit_response(t2, sig_a.clone(), 102, 20).unwrap();
        election.submit_response(t3, sig_a, 103, 30).unwrap();

        // 3 responses from Group B
        election.submit_response(t4, sig_b.clone(), 201, 40).unwrap();
        election.submit_response(t5, sig_b.clone(), 202, 50).unwrap();
        election.submit_response(t6, sig_b, 203, 60).unwrap();

        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::SplitBrain { current_clusters, suggested_channels } => {
                assert_eq!(current_clusters.len(), 2, "Should detect 2 competing clusters");
                assert_eq!(current_clusters[0].members.len(), 3, "First cluster should have 3 members");
                assert_eq!(current_clusters[1].members.len(), 3, "Second cluster should have 3 members");
                assert!(suggested_channels > 0, "Should suggest additional channels");
            }
            _ => panic!("Should detect split-brain with 3v3 equal clusters"),
        }
    }

    #[test]
    fn test_decisive_majority_wins() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 10,
            min_cluster_size: 2,
            majority_threshold: 0.6, // 60%
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        // Create 6 channels: 4 will agree (majority), 2 will differ (minority)
        let t1 = election.create_channel(100, 0).unwrap();
        let t2 = election.create_channel(200, 0).unwrap();
        let t3 = election.create_channel(300, 0).unwrap();
        let t4 = election.create_channel(400, 0).unwrap();
        let t5 = election.create_channel(500, 0).unwrap();
        let t6 = election.create_channel(600, 0).unwrap();

        // Majority signature
        let sig_majority = create_test_signature([
            (1, 10), (2, 20), (3, 30), (4, 40), (5, 50),
            (6, 60), (7, 70), (8, 80), (9, 90), (10, 100)
        ]);

        // Minority signature
        let sig_minority = create_test_signature([
            (11, 110), (12, 120), (13, 130), (14, 140), (15, 150),
            (16, 160), (17, 170), (18, 180), (19, 190), (20, 200)
        ]);

        // 4 responses with majority (67% = decisive)
        election.submit_response(t1, sig_majority.clone(), 101, 10).unwrap();
        election.submit_response(t2, sig_majority.clone(), 102, 20).unwrap();
        election.submit_response(t3, sig_majority.clone(), 103, 30).unwrap();
        election.submit_response(t4, sig_majority, 104, 40).unwrap();

        // 2 responses with minority (33%)
        election.submit_response(t5, sig_minority.clone(), 201, 50).unwrap();
        election.submit_response(t6, sig_minority, 202, 60).unwrap();

        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::Winner(result) => {
                assert_eq!(result.cluster.members.len(), 4, "Winner cluster should have 4 members (67%)");
                assert!(!result.is_split_brain, "Should not be split-brain with decisive majority");
                assert_eq!(result.competing_clusters.len(), 1, "Should have 1 competing cluster");
            }
            _ => panic!("Should elect winner with 67% majority"),
        }
    }

    #[test]
    fn test_no_split_brain_if_second_cluster_too_small() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 10,
            min_cluster_size: 2,
            majority_threshold: 0.6,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        // Create 10 channels: 8 agree, 2 form small cluster
        let mut tickets = Vec::new();
        for i in 0..10 {
            tickets.push(election.create_channel(100 + i * 10, 0).unwrap());
        }

        let sig_major = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        let sig_minor = create_test_signature([(2, 20); SIGNATURE_CHUNKS]);

        // 8 responses with major signature
        for i in 0..8 {
            election.submit_response(tickets[i], sig_major.clone(), 100 + i as u64, i as u64 * 10).unwrap();
        }

        // 2 responses with minor signature (only 20% support - below competition threshold)
        election.submit_response(tickets[8], sig_minor.clone(), 800, 80).unwrap();
        election.submit_response(tickets[9], sig_minor, 900, 90).unwrap();

        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::Winner(result) => {
                assert_eq!(result.cluster.members.len(), 8, "Winner cluster should have 8 members");
                assert!(!result.is_split_brain, "Should not be split-brain - second cluster too small (<20%)");
            }
            _ => panic!("Should elect winner - second cluster below competition threshold"),
        }
    }

    #[test]
    fn test_split_brain_4v3_close_competition() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 10,
            min_cluster_size: 2,
            majority_threshold: 0.6,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        let mut tickets = Vec::new();
        for i in 0..7 {
            tickets.push(election.create_channel(100 + i * 10, 0).unwrap());
        }

        let sig_a = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        let sig_b = create_test_signature([(2, 20); SIGNATURE_CHUNKS]);

        // 4 responses cluster A (57% - below 60% threshold)
        for i in 0..4 {
            election.submit_response(tickets[i], sig_a.clone(), 100 + i as u64, i as u64 * 10).unwrap();
        }

        // 3 responses cluster B (43% - above 20% competition threshold)
        for i in 4..7 {
            election.submit_response(tickets[i], sig_b.clone(), 400 + i as u64, i as u64 * 10).unwrap();
        }

        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::SplitBrain { current_clusters, suggested_channels } => {
                assert_eq!(current_clusters.len(), 2, "Should detect 2 competing clusters");
                assert_eq!(current_clusters[0].members.len(), 4, "Largest cluster should have 4");
                assert_eq!(current_clusters[1].members.len(), 3, "Second cluster should have 3");
                assert!(suggested_channels > 0, "Should suggest more channels");
            }
            _ => panic!("Should detect split-brain with 4v3 (57% vs 43%)"),
        }
    }

    #[test]
    fn test_suggested_channels_calculation() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 10,
            min_cluster_size: 2,
            majority_threshold: 0.6,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        // 3v3 split
        let mut tickets = Vec::new();
        for i in 0..6 {
            tickets.push(election.create_channel(100 + i * 10, 0).unwrap());
        }

        let sig_a = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        let sig_b = create_test_signature([(2, 20); SIGNATURE_CHUNKS]);

        for i in 0..3 {
            election.submit_response(tickets[i], sig_a.clone(), 100 + i as u64, i as u64 * 10).unwrap();
        }
        for i in 3..6 {
            election.submit_response(tickets[i], sig_b.clone(), 300 + i as u64, i as u64 * 10).unwrap();
        }

        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::SplitBrain { suggested_channels, .. } => {
                // With 6 total (3v3), need to reach 60% majority
                // 3 / (6 + n) >= 0.6 -> need n >= 0 to break tie minimally
                // But algorithm suggests enough to decisively break tie
                assert!(suggested_channels >= 2, "Should suggest at least 2 additional channels");
            }
            _ => panic!("Expected split-brain"),
        }
    }

    #[test]
    fn test_no_consensus_with_insufficient_responses() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 8,
            min_cluster_size: 2,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        // Only 1 response (below min_cluster_size)
        let t1 = election.create_channel(100, 0).unwrap();
        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        election.submit_response(t1, sig, 101, 10).unwrap();

        let attempt = election.try_elect_winner();
        assert!(
            matches!(attempt, ElectionAttempt::NoConsensus),
            "Should return NoConsensus with only 1 response"
        );
    }

    #[test]
    fn test_all_clusters_found() {
        let _ = initialize_election_secret([42u8; 32]);

        // Test that find_all_consensus_clusters finds all maximal clusters
        let sig_a = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        let sig_b = create_test_signature([(2, 20); SIGNATURE_CHUNKS]);
        let sig_c = create_test_signature([(3, 30); SIGNATURE_CHUNKS]);

        let signatures = vec![
            sig_a.clone(), // 0
            sig_a.clone(), // 1
            sig_a,         // 2
            sig_b.clone(), // 3
            sig_b,         // 4
            sig_c,         // 5 - outlier
        ];

        let clusters = find_all_consensus_clusters(&signatures, 10, 2);

        // Should find 2 clusters: [0,1,2] and [3,4]
        assert_eq!(clusters.len(), 2, "Should find 2 consensus clusters");

        let cluster_sizes: Vec<_> = clusters.iter().map(|c| c.members.len()).collect();
        assert!(cluster_sizes.contains(&3), "Should have cluster of size 3");
        assert!(cluster_sizes.contains(&2), "Should have cluster of size 2");
    }

    #[test]
    fn test_competing_clusters_populated() {
        let _ = initialize_election_secret([42u8; 32]);

        let config = ElectionConfig {
            consensus_threshold: 10,
            min_cluster_size: 2,
            majority_threshold: 0.6,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 0, config);

        let mut tickets = Vec::new();
        for i in 0..5 {
            tickets.push(election.create_channel(100 + i * 10, 0).unwrap());
        }

        let sig_a = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);
        let sig_b = create_test_signature([(2, 20); SIGNATURE_CHUNKS]);

        // 3 with sig_a (60%)
        for i in 0..3 {
            election.submit_response(tickets[i], sig_a.clone(), 100 + i as u64, i as u64 * 10).unwrap();
        }

        // 2 with sig_b (40%)
        for i in 3..5 {
            election.submit_response(tickets[i], sig_b.clone(), 300 + i as u64, i as u64 * 10).unwrap();
        }

        let attempt = election.try_elect_winner();
        match attempt {
            ElectionAttempt::Winner(result) => {
                // 60% majority - should win
                assert_eq!(result.cluster.members.len(), 3);
                assert_eq!(result.competing_clusters.len(), 1, "Should track competing cluster");
                assert_eq!(result.competing_clusters[0].members.len(), 2);
            }
            _ => panic!("Should elect winner with 60% majority"),
        }
    }
}
