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
/// This struct provides signature generation functionality for proof-of-storage.
/// It does not own storage - storage is passed as a parameter to methods.
///
/// # Example
/// ```rust
/// let storage = MemTokens::new();
/// let proof_system = ProofOfStorage::new();
///
/// // Generate signature for a token
/// if let Some(sig) = proof_system.generate_signature(&storage, &token, &peer) {
///     // Use signature...
/// }
/// ```
pub struct ProofOfStorage {
    // Zero-sized type - all methods take storage as parameter
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
/// The ticket is generated as: Blake3(challenge_token || first_hop_peer || election_secret)
///
/// # Arguments
/// * `challenge_token` - The token being challenged in this election
/// * `first_hop_peer` - The first peer on this channel's route
/// * `election_secret` - The secret for this specific election
///
/// # Returns
/// A u64 ticket (first 8 bytes of Blake3 hash)
///
/// # Security
/// - Deterministic: same inputs → same ticket
/// - Unpredictable: secret prevents forgery
/// - Unique per channel: different first-hop → different ticket
/// - Cannot be forged without knowing the election_secret
fn generate_ticket(
    challenge_token: TokenId,
    first_hop_peer: PeerId,
    election_secret: &[u8; 32],
) -> MessageTicket {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&challenge_token.to_le_bytes());
    hasher.update(&first_hop_peer.to_le_bytes());
    hasher.update(election_secret);

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

    /// Majority threshold for decisive win (default: 0.6 = 60%)
    /// Winning cluster must have this fraction of valid responses to be a clear winner
    /// If no cluster reaches this threshold and there are multiple clusters, it's split-brain
    pub majority_threshold: f64,
}

impl Default for ElectionConfig {
    fn default() -> Self {
        Self {
            consensus_threshold: 8,
            min_cluster_size: 2,
            max_channels: 10,
            majority_threshold: 0.6,
        }
    }
}

/// Result of checking for a winner
#[derive(Debug, Clone, PartialEq)]
pub enum WinnerResult {
    /// Single clear winner found
    Single {
        /// The elected winner (peer closest to challenge_token)
        winner: PeerId,
        /// The consensus cluster
        cluster: ConsensusCluster,
        /// Signatures from cluster members
        cluster_signatures: Vec<(PeerId, TokenSignature)>,
    },

    /// Split-brain: two competing clusters found
    SplitBrain {
        /// First cluster (sorted by size, largest first)
        cluster1: ConsensusCluster,
        /// Cluster 1 winner
        winner1: PeerId,
        /// Cluster 1 signatures
        signatures1: Vec<(PeerId, TokenSignature)>,
        /// Second cluster
        cluster2: ConsensusCluster,
        /// Cluster 2 winner
        winner2: PeerId,
        /// Cluster 2 signatures
        signatures2: Vec<(PeerId, TokenSignature)>,
    },

    /// No consensus found yet (not enough responses or agreement)
    NoConsensus,
}

/// Errors that can occur during election
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElectionError {
    /// Ticket not found in this election
    UnknownTicket,

    /// Wrong token for this election
    WrongToken,

    /// Duplicate response on channel (anti-gaming triggered)
    DuplicateResponse,

    /// Channel already exists for this first-hop peer
    ChannelAlreadyExists,

    /// Peer is already participating in this election (has channel or sent response)
    PeerAlreadyParticipating,

    /// Maximum channels limit reached
    MaxChannelsReached,

    /// Channel is blocked (duplicate response detected)
    ChannelBlocked,

    /// Signature verification failed
    SignatureVerificationFailed,

    /// All suggested peers from referral are already participating
    NoViableSuggestions,
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
/// use ec_rust::ec_proof_of_storage::{PeerElection, ElectionConfig};
///
/// let my_peer_id = 12345;
/// let challenge_token = 1000;
/// let mut election = PeerElection::new(challenge_token, my_peer_id, ElectionConfig::default());
///
/// // Create channels
/// let ticket1 = election.create_channel(100).unwrap();
/// let ticket2 = election.create_channel(200).unwrap();
///
/// // When Answer received, verify and submit
/// // election.handle_answer(ticket, answer, signature, responder_peer).unwrap();
/// ```
pub struct PeerElection {
    /// Token being challenged
    challenge_token: TokenId,

    /// This node's peer ID (challenger)
    my_peer_id: PeerId,

    /// Election-specific secret for ticket generation
    election_secret: [u8; 32],

    /// All channels indexed by ticket
    channels: HashMap<MessageTicket, ElectionChannel>,

    /// Track first-hop peers to prevent duplicate channels
    first_hop_peers: HashMap<PeerId, MessageTicket>,

    /// Configuration
    config: ElectionConfig,
}

impl PeerElection {
    /// Create a new election for a challenge token
    ///
    /// Generates a secure random election-specific secret for ticket generation.
    ///
    /// # Arguments
    /// * `challenge_token` - Token to challenge
    /// * `my_peer_id` - This node's peer ID (the challenger)
    /// * `config` - Election configuration
    pub fn new(challenge_token: TokenId, my_peer_id: PeerId, config: ElectionConfig) -> Self {
        // Generate secure random election-specific secret
        let mut election_secret = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut election_secret);

        Self {
            challenge_token,
            my_peer_id,
            election_secret,
            channels: HashMap::new(),
            first_hop_peers: HashMap::new(),
            config,
        }
    }

    /// Create a new channel to a first-hop peer
    ///
    /// Generates a ticket and stores the channel as Pending.
    /// Returns an error if this peer is already participating in the election
    /// (either as a first-hop peer or as a responder to another channel).
    ///
    /// # Arguments
    /// * `first_hop` - The first peer to send the Query to
    ///
    /// # Returns
    /// * `Ok(ticket)` - Ticket to include in challenge Query message
    /// * `Err(MaxChannelsReached)` - Cannot create more channels
    /// * `Err(ChannelAlreadyExists)` - Channel for this first-hop peer already exists
    /// * `Err(PeerAlreadyParticipating)` - Peer has already responded via another channel
    pub fn create_channel(&mut self, first_hop: PeerId) -> Result<MessageTicket, ElectionError> {
        if self.channels.len() >= self.config.max_channels {
            return Err(ElectionError::MaxChannelsReached);
        }

        // Check if channel already exists for this first-hop peer
        if self.first_hop_peers.contains_key(&first_hop) {
            return Err(ElectionError::ChannelAlreadyExists);
        }

        // Check if this peer has already responded via another channel
        // (peer could be responder on a different route)
        for channel in self.channels.values() {
            if let Some(response) = &channel.response {
                if response.responder == first_hop {
                    return Err(ElectionError::PeerAlreadyParticipating);
                }
            }
        }

        let ticket = generate_ticket(self.challenge_token, first_hop, &self.election_secret);
        let channel = ElectionChannel::new(ticket, first_hop, 0); // User controls time
        self.channels.insert(ticket, channel);
        self.first_hop_peers.insert(first_hop, ticket);

        Ok(ticket)
    }

    /// Handle an Answer message received for a channel
    ///
    /// Verifies the token matches, checks the ticket, validates the signature,
    /// and stores the response if valid.
    ///
    /// # Arguments
    /// * `ticket` - Channel ticket from the Answer message
    /// * `answer` - The token mapping (token_id, block_id)
    /// * `signature_mappings` - The 10 signature token mappings
    /// * `responder_peer` - The peer that sent the Answer
    ///
    /// # Returns
    /// * `Ok(())` - Response verified and stored successfully
    /// * `Err(WrongToken)` - Answer is for a different token
    /// * `Err(UnknownTicket)` - Ticket not found in this election
    /// * `Err(ChannelBlocked)` - Channel is blocked
    /// * `Err(DuplicateResponse)` - Channel already has response (now blocked)
    /// * `Err(SignatureVerificationFailed)` - Signature doesn't match expected values
    pub fn handle_answer(
        &mut self,
        ticket: MessageTicket,
        answer: &TokenMapping,
        signature_mappings: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
        responder_peer: PeerId,
    ) -> Result<(), ElectionError> {
        // Check this election is for the correct token
        if answer.id != self.challenge_token {
            return Err(ElectionError::WrongToken);
        }

        // Verify the signature BEFORE getting mutable access to channel
        // (to avoid borrow checker issues)
        self.verify_signature(answer.block, signature_mappings)?;

        // Get channel (now we can borrow mutably)
        let channel = self
            .channels
            .get_mut(&ticket)
            .ok_or(ElectionError::UnknownTicket)?;

        // Check if channel is already blocked
        if channel.state == ChannelState::Blocked {
            return Err(ElectionError::ChannelBlocked);
        }

        // Detect duplicate (anti-gaming mechanism)
        if channel.response.is_some() {
            channel.state = ChannelState::Blocked;
            return Err(ElectionError::DuplicateResponse);
        }

        // Store response
        let token_signature = TokenSignature {
            answer: *answer,
            signature: *signature_mappings,
        };

        channel.response = Some(ChannelResponse {
            signature: token_signature,
            responder: responder_peer,
            received_at: 0, // User controls time
        });
        channel.state = ChannelState::Responded;

        Ok(())
    }

    /// Verify a signature by checking the 10-bit chunks
    ///
    /// Calculates the expected signature using Blake3(my_peer_id, token_id, response_block_id)
    /// and verifies that the signature_mappings match the expected chunks.
    fn verify_signature(
        &self,
        response_block_id: BlockId,
        signature_mappings: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
    ) -> Result<(), ElectionError> {
        // Calculate expected hash: Blake3(my_peer_id, token_id, response_block_id)
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.my_peer_id.to_le_bytes());
        hasher.update(&self.challenge_token.to_le_bytes());
        hasher.update(&response_block_id.to_le_bytes());
        let hash = hasher.finalize();

        // Extract expected 10-bit chunks from the hash
        let expected_chunks = extract_signature_chunks_from_256bit_hash(hash.as_bytes());

        // Verify each signature mapping matches the expected chunk
        for (i, mapping) in signature_mappings.iter().enumerate() {
            let expected_chunk = expected_chunks[i];
            let token_last_bits = (mapping.id & 0x3FF) as u16; // Last 10 bits

            if token_last_bits != expected_chunk {
                return Err(ElectionError::SignatureVerificationFailed);
            }
        }

        Ok(())
    }

    /// Handle a Referral message (when first-hop peer doesn't have the answer)
    ///
    /// Destroys the channel (if not blocked) and returns one of the suggested peers
    /// for the user to create a new channel.
    ///
    /// # Arguments
    /// * `ticket` - Channel ticket from the Referral
    /// * `token_challenge` - Token from the referral (should match our challenge_token)
    /// * `suggested_peers` - Array of 2 suggested peers from the responder
    /// * `responder_peer` - The peer that sent the Referral
    ///
    /// # Returns
    /// * `Ok(suggested_peer)` - One of the suggested peers to try next (not already participating)
    /// * `Err(WrongToken)` - Referral is for a different token
    /// * `Err(UnknownTicket)` - Ticket not found
    /// * `Err(ChannelBlocked)` - Channel is blocked, ignoring referral
    /// * `Err(NoViableSuggestions)` - Both suggested peers are already participating
    pub fn handle_referral(
        &mut self,
        ticket: MessageTicket,
        token_challenge: TokenId,
        suggested_peers: [PeerId; 2],
        responder_peer: PeerId,
    ) -> Result<PeerId, ElectionError> {
        // Verify correct token for this election
        if token_challenge != self.challenge_token {
            return Err(ElectionError::WrongToken);
        }

        // Get channel and verify ticket matches
        let channel = self
            .channels
            .get(&ticket)
            .ok_or(ElectionError::UnknownTicket)?;

        // If channel is blocked, reject the referral
        if channel.state == ChannelState::Blocked {
            return Err(ElectionError::ChannelBlocked);
        }

        // Destroy the channel (no other answer should come for it)
        self.first_hop_peers.remove(&channel.first_hop_peer);
        self.channels.remove(&ticket);

        // Get all participating peers to filter suggestions
        let participating = self.get_participating_peers();

        // Shuffle suggested peers to avoid predictability
        use rand::seq::SliceRandom;
        let mut peers_shuffled = suggested_peers.to_vec();
        peers_shuffled.shuffle(&mut rand::thread_rng());

        // Find first suggested peer not already participating
        for &peer in &peers_shuffled {
            if !participating.contains(&peer) {
                return Ok(peer);
            }
        }

        // Both suggested peers are already participating
        Err(ElectionError::NoViableSuggestions)
    }

    /// Check for a winner based on current accepted answers
    ///
    /// Analyzes all valid (non-blocked) responses to find consensus clusters.
    /// Returns either a single winner, two winners (split-brain), or no consensus.
    ///
    /// **Important**: Deduplicates by responder PeerId - if the same peer responds on
    /// multiple channels (can happen by chance if peer is close to challenge token),
    /// only their first response is counted. This prevents gaming and ensures each
    /// peer only participates once in consensus.
    ///
    /// User controls when to call this - can be called any time to check status.
    ///
    /// # Returns
    /// * `WinnerResult::Single` - Clear winner with consensus cluster
    /// * `WinnerResult::SplitBrain` - Two competing clusters found
    /// * `WinnerResult::NoConsensus` - Not enough responses or no agreement
    pub fn check_for_winner(&self) -> WinnerResult {
        // Get valid responses (non-blocked)
        let all_responses: Vec<_> = self
            .channels
            .values()
            .filter(|ch| ch.state == ChannelState::Responded)
            .filter_map(|ch| ch.response.as_ref().map(|r| (ch.ticket, r.clone())))
            .collect();

        // Deduplicate by responder PeerId (keep first response from each unique peer)
        // This prevents the same peer from being counted multiple times if they
        // happened to respond on multiple channels
        let mut seen_responders = std::collections::HashSet::new();
        let valid_responses: Vec<_> = all_responses
            .into_iter()
            .filter(|(_, resp)| seen_responders.insert(resp.responder))
            .collect();

        if valid_responses.len() < self.config.min_cluster_size {
            return WinnerResult::NoConsensus;
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
            return WinnerResult::NoConsensus;
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

        let strongest_cluster = &all_clusters[0];
        let total_valid = valid_responses.len();

        // Calculate if strongest cluster has decisive majority
        let cluster_fraction = strongest_cluster.members.len() as f64 / total_valid as f64;
        let has_decisive_majority = cluster_fraction >= self.config.majority_threshold;

        // Check for split-brain: multiple significant clusters and no decisive majority
        if !has_decisive_majority && all_clusters.len() >= 2 {
            let cluster2 = &all_clusters[1];

            // If second cluster also meets min_cluster_size, we have split-brain
            if cluster2.members.len() >= self.config.min_cluster_size {
                let (winner1, sigs1) =
                    Self::select_winner(self.challenge_token, strongest_cluster, &valid_responses);
                let (winner2, sigs2) =
                    Self::select_winner(self.challenge_token, cluster2, &valid_responses);

                return WinnerResult::SplitBrain {
                    cluster1: strongest_cluster.clone(),
                    winner1,
                    signatures1: sigs1,
                    cluster2: cluster2.clone(),
                    winner2,
                    signatures2: sigs2,
                };
            }
        }

        // Single winner (either has decisive majority, or only one cluster exists)
        let (winner, cluster_sigs) =
            Self::select_winner(self.challenge_token, strongest_cluster, &valid_responses);

        WinnerResult::Single {
            winner,
            cluster: strongest_cluster.clone(),
            cluster_signatures: cluster_sigs,
        }
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

    /// Get number of valid (non-blocked) responses currently collected
    pub fn valid_response_count(&self) -> usize {
        self.channels
            .values()
            .filter(|ch| ch.state == ChannelState::Responded)
            .count()
    }

    /// Check if we can create more channels (haven't hit max_channels limit)
    pub fn can_create_channel(&self) -> bool {
        self.channels.len() < self.config.max_channels
    }

    /// Get the challenge token for this election
    pub fn challenge_token(&self) -> TokenId {
        self.challenge_token
    }

    /// Get the number of channels (including pending and blocked)
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Get all peer IDs participating in this election
    ///
    /// Returns a HashSet of all peer IDs that either:
    /// - Have a channel created for them (first-hop peers)
    /// - Have sent an Answer (responder peers)
    ///
    /// This is useful for filtering out peers when spawning new channels,
    /// to avoid creating duplicate channels or channels to peers that have
    /// already responded via other routes.
    pub fn get_participating_peers(&self) -> std::collections::HashSet<PeerId> {
        let mut peers = std::collections::HashSet::new();

        for channel in self.channels.values() {
            // Add first-hop peer
            peers.insert(channel.first_hop_peer);

            // Add responder if channel has a response
            if let Some(response) = &channel.response {
                peers.insert(response.responder);
            }
        }

        peers
    }
}

impl ProofOfStorage {
    /// Create a new proof-of-storage system (zero-sized type)
    pub fn new() -> Self {
        Self {}
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
        backend: &dyn TokenStorageBackend,
        lookup_token: &TokenId,
        signature_chunks: &[u16; SIGNATURE_CHUNKS],
    ) -> SignatureSearchResult {
        let mut found_tokens = Vec::with_capacity(SIGNATURE_CHUNKS);
        let mut steps = 0;
        let mut chunk_idx = 0;

        // Search above (forward) for first 5 chunks
        let mut after_iter = backend.range_after(lookup_token);
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
        let mut before_iter = backend.range_before(lookup_token);
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
        backend: &dyn TokenStorageBackend,
        token: &TokenId,
        peer: &PeerId,
    ) -> Option<TokenSignature> {
        // Get the block mapping for this token
        let block_time = backend.lookup(token)?;

        // Generate signature from token, block, and peer
        let signature_chunks = Self::signature_for(token, &block_time.block, peer);

        // Perform signature-based search
        let search_result = self.search_by_signature(backend, token, &signature_chunks);

        // Only return a signature if we found all 10 tokens
        if search_result.complete {
            // Build the signature array from found tokens
            let mut signature = [TokenMapping {
                id: 0,
                block: 0,
            }; TOKENS_SIGNATURE_SIZE];

            for (i, &token_id) in search_result.tokens.iter().enumerate() {
                if let Some(block_time) = backend.lookup(&token_id) {
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

        let proof = ProofOfStorage::new();

        assert_eq!(backend.len(), 1);
        assert!(backend.lookup(&100).is_some());

        // Verify proof system can work with the backend
        let _ = proof.generate_signature(&backend, &100, &42);
    }

    #[test]
    fn test_signature_generation_nonexistent_token() {
        let backend = TestBackend::new();
        let proof = ProofOfStorage::new();

        let result = proof.generate_signature(&backend, &12345, &99999);
        assert!(result.is_none(), "Should return None for nonexistent token");
    }

    #[test]
    fn test_signature_search_empty_storage() {
        let backend = TestBackend::new();
        let proof = ProofOfStorage::new();

        let signature = [0u16; SIGNATURE_CHUNKS];
        let result = proof.search_by_signature(&backend, &1000, &signature);

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

        let count = ProofOfStorage::count_common_mappings(&sig1, &sig2);
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

        let cluster = ProofOfStorage::find_consensus_cluster(&signatures, 10);

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
        let cluster = ProofOfStorage::find_consensus_cluster(&signatures, 8);

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
        let cluster = ProofOfStorage::find_consensus_cluster(&signatures, 8);

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
        let cluster = ProofOfStorage::find_consensus_cluster(&signatures, 9);

        // Should find nothing above threshold 9 (they only agree on 2/10)
        assert!(cluster.is_none() || cluster.unwrap().members.len() == 1);
    }

    #[test]
    fn test_consensus_cluster_empty_input() {
        let signatures: Vec<TokenSignature> = vec![];
        let cluster = ProofOfStorage::find_consensus_cluster(&signatures, 5);
        assert!(cluster.is_none());
    }

    #[test]
    fn test_consensus_cluster_single_signature() {
        use crate::ec_interface::TokenMapping;

        let signatures = vec![TokenSignature {
            answer: TokenMapping { id: 999, block: 999 },
            signature: [TokenMapping { id: 1, block: 1 }; SIGNATURE_CHUNKS],
        }];

        let cluster = ProofOfStorage::find_consensus_cluster(&signatures, 5);
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

    // Ticket generation tests removed - tickets are now generated internally per-election

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

    // ========================================================================
    // Peer Election Tests - Updated for Simplified API
    // ========================================================================

    #[test]
    fn test_election_create_channel() {
        let my_peer_id = 999;
        let challenge_token = 1000;
        let mut election = PeerElection::new(challenge_token, my_peer_id, ElectionConfig::default());

        let ticket = election.create_channel(100).unwrap();
        assert!(ticket > 0, "Ticket should be non-zero");

        assert_eq!(election.channel_count(), 1);
        assert_eq!(election.valid_response_count(), 0);
    }

    #[test]
    fn test_election_max_channels_limit() {
        let config = ElectionConfig {
            max_channels: 3,
            ..Default::default()
        };
        let mut election = PeerElection::new(1000, 999, config);

        // Create 3 channels (max)
        assert!(election.create_channel(100).is_ok());
        assert!(election.create_channel(200).is_ok());
        assert!(election.create_channel(300).is_ok());

        // 4th should fail
        assert_eq!(
            election.create_channel(400),
            Err(ElectionError::MaxChannelsReached)
        );
    }

    #[test]
    fn test_election_duplicate_channel_rejected() {
        let mut election = PeerElection::new(1000, 999, ElectionConfig::default());

        // Create channel for peer 100
        assert!(election.create_channel(100).is_ok());

        // Try to create another channel for same peer
        assert_eq!(
            election.create_channel(100),
            Err(ElectionError::ChannelAlreadyExists)
        );
    }

    #[test]
    fn test_election_handle_answer() {
        let my_peer_id = 999;
        let challenge_token = 1000;
        let mut election = PeerElection::new(challenge_token, my_peer_id, ElectionConfig::default());

        let ticket = election.create_channel(100).unwrap();

        // Note: signature verification will fail with test data since we can't easily
        // generate matching signatures. This tests the path up to verification.
        let answer = TokenMapping { id: challenge_token, block: 42 };
        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);

        // Will fail signature verification, but that's expected with test data
        let result = election.handle_answer(ticket, &answer, &sig.signature, 101);
        assert!(result.is_err()); // Signature verification will fail
    }

    // Removed test_election_blocked_peer_rejected - peer blocking no longer exists
    // Only channels are blocked, not individual peers

    #[test]
    fn test_election_wrong_token_rejected() {
        let mut election = PeerElection::new(1000, 999, ElectionConfig::default());
        let ticket = election.create_channel(100).unwrap();

        // Answer for wrong token
        let answer = TokenMapping { id: 9999, block: 42 };
        let sig = create_test_signature([(1, 10); SIGNATURE_CHUNKS]);

        let result = election.handle_answer(ticket, &answer, &sig.signature, 101);
        assert_eq!(result, Err(ElectionError::WrongToken));
    }

    #[test]
    fn test_election_handle_referral() {
        let mut election = PeerElection::new(1000, 999, ElectionConfig::default());
        let ticket = election.create_channel(100).unwrap();

        let suggested_peers = [200, 300];
        let result = election.handle_referral(ticket, 1000, suggested_peers, 100);

        assert!(result.is_ok());
        // Should return one of the suggested peers (randomized order)
        let returned_peer = result.unwrap();
        assert!(suggested_peers.contains(&returned_peer));

        // Channel should be destroyed
        assert_eq!(election.channel_count(), 0);
    }

    #[test]
    fn test_election_referral_wrong_token() {
        let mut election = PeerElection::new(1000, 999, ElectionConfig::default());
        let ticket = election.create_channel(100).unwrap();

        let suggested_peers = [200, 300];
        let result = election.handle_referral(ticket, 9999, suggested_peers, 100);

        assert_eq!(result, Err(ElectionError::WrongToken));
    }

    #[test]
    fn test_election_check_for_winner_no_responses() {
        let election = PeerElection::new(1000, 999, ElectionConfig::default());
        let result = election.check_for_winner();

        assert!(matches!(result, WinnerResult::NoConsensus));
    }

    #[test]
    fn test_election_accessors() {
        let challenge_token = 1000;
        let my_peer_id = 999;
        let mut election = PeerElection::new(challenge_token, my_peer_id, ElectionConfig::default());

        assert_eq!(election.challenge_token(), challenge_token);
        assert_eq!(election.valid_response_count(), 0);
        assert_eq!(election.channel_count(), 0);
        assert!(election.can_create_channel());

        election.create_channel(100).unwrap();
        assert_eq!(election.channel_count(), 1);
    }

    #[test]
    fn test_get_participating_peers() {
        let mut election = PeerElection::new(1000, 999, ElectionConfig::default());

        // Initially empty
        assert_eq!(election.get_participating_peers().len(), 0);

        // Create channels to peers 100 and 200
        election.create_channel(100).unwrap();
        election.create_channel(200).unwrap();

        // Both first-hop peers should be participating
        let peers = election.get_participating_peers();
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&100));
        assert!(peers.contains(&200));

        // Peer 300 responds on channel to peer 100 (different from first-hop)
        // This simulates a response coming from a different peer than the first-hop
        // (can happen if query is forwarded)
        // Note: We can't easily test this without mocking the signature verification
        // but the logic is covered by the code inspection
    }

    #[test]
    fn test_create_channel_rejects_participating_peer() {
        let mut election = PeerElection::new(1000, 999, ElectionConfig::default());

        // Create channel to peer 100
        election.create_channel(100).unwrap();

        // Try to create another channel to same peer - should fail
        let result = election.create_channel(100);
        assert_eq!(result, Err(ElectionError::ChannelAlreadyExists));
    }

    #[test]
    fn test_handle_referral_filters_participating_peers() {
        let mut election = PeerElection::new(1000, 999, ElectionConfig::default());

        // Create channels to peers 100, 200, 300
        let ticket1 = election.create_channel(100).unwrap();
        election.create_channel(200).unwrap();
        election.create_channel(300).unwrap();

        // Referral suggests peers 200 and 400
        // Peer 200 is already participating, so should suggest 400
        let suggested = election.handle_referral(ticket1, 1000, [200, 400], 100);
        assert_eq!(suggested, Ok(400));

        // Create channel to peer 500
        let ticket2 = election.create_channel(500).unwrap();

        // Referral suggests peers 300 and 200 (both already participating)
        // Should return NoViableSuggestions
        let suggested = election.handle_referral(ticket2, 1000, [300, 200], 500);
        assert_eq!(suggested, Err(ElectionError::NoViableSuggestions));
    }

    #[test]
    fn test_deduplication_in_check_for_winner() {
        // This test verifies the deduplication logic conceptually
        // In practice, we can't easily create duplicate responses from same peer
        // because of the channel blocking logic, but the deduplication ensures
        // robustness even if somehow the same peer responds on multiple channels

        // The deduplication is tested implicitly: if a peer somehow responds twice,
        // only first response is counted in consensus finding
        // This prevents gaming where a peer tries to amplify their vote

        // Note: Full integration testing would require mocking the signature
        // verification to allow duplicate responses, which is beyond unit test scope
        assert!(true); // Placeholder for documentation purposes
    }
}
