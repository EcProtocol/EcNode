use crate::ec_interface::{
    EcTime, MessageTicket, PeerId, TokenId, TokenMapping, TOKENS_SIGNATURE_SIZE,
};
use crate::ec_proof_of_storage::{ElectionConfig, PeerElection, ProofOfStorage, TokenStorageBackend};
use std::collections::{BTreeMap, BTreeSet, HashMap};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the peer management system
#[derive(Debug, Clone)]
pub struct PeerManagerConfig {
    /// Total budget of connections across all distance classes (default: 50)
    pub total_budget: usize,

    /// Interval for budget enforcement in ticks (default: 60)
    pub budget_enforcement_interval: u64,

    /// Interval between starting new elections (in ticks, default: 60)
    pub election_interval: u64,

    /// Minimum time to collect election responses before checking for winner (in ticks, default: 2)
    pub min_collection_time: u64,

    /// Maximum time to wait for election before timeout (in ticks, default: 8)
    pub election_timeout: u64,

    /// Timeout for Pending state before demoting to Identified (in ticks, default: 10)
    pub pending_timeout: u64,

    /// Timeout for Connected state without keepalive (in ticks, default: 300 = 5 min)
    pub connection_timeout: u64,

    /// Decay factor for exponential moving average of quality scores (default: 0.3)
    pub quality_decay_alpha: f64,

    /// Random churn rate applied per epoch (default: 0.4 = 40%)
    pub churn_rate: f64,

    /// Interval between churn epochs (in ticks, default: 1800 = 30 min)
    pub churn_interval: u64,

    /// Minimum connection age to be eligible for churn (in ticks, default: 600 = 10 min)
    pub churn_protection_time: u64,

    /// Number of election channels to create (default: 3)
    pub election_channel_count: usize,

    /// Number of closest peers to consider for channel selection (default: 8)
    pub channel_candidate_count: usize,

    /// Maximum discovered peers to keep (default: 1000)
    pub max_discovered_peers: usize,

    /// Election result cache TTL in ticks (default: 300 = 5 minutes)
    pub election_cache_ttl: u64,

    /// Configuration for PeerElection
    pub election_config: ElectionConfig,

    /// Maximum tokens per distance class in sample collection (default: 20)
    /// Total capacity = 64 classes * 20 = 1,280 tokens
    pub max_tokens_per_distance_class: usize,
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self {
            total_budget: 50,
            budget_enforcement_interval: 60,
            election_interval: 60,
            min_collection_time: 2,
            election_timeout: 8,
            pending_timeout: 10,
            connection_timeout: 300,
            quality_decay_alpha: 0.3,
            churn_rate: 0.4,
            churn_interval: 1800,
            churn_protection_time: 600,
            election_channel_count: 3,
            channel_candidate_count: 8,
            max_discovered_peers: 1000,
            election_cache_ttl: 300,
            election_config: ElectionConfig::default(),
            max_tokens_per_distance_class: 20,
        }
    }
}

// ============================================================================
// Peer State Machine
// ============================================================================

/// State of a peer in the lifecycle
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PeerState {
    /// Peer discovered via referral or answer, not yet invited
    Identified {
        discovered_at: EcTime,
    },

    /// Invitation sent, waiting for reciprocal invitation
    Pending {
        invitation_sent_at: EcTime,
        /// Token used in the election that selected this peer
        from_election: TokenId,
    },

    /// Bidirectional connection established
    Connected {
        connected_since: EcTime,
        last_keepalive: EcTime,
        /// Number of elections this peer won
        election_wins: usize,
        /// Total number of elections this peer participated in
        election_attempts: usize,
        /// Current quality score (0.0 - 1.0)
        quality_score: f64,
    },
}

impl PeerState {
    pub fn is_connected(&self) -> bool {
        matches!(self, PeerState::Connected { .. })
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, PeerState::Pending { .. })
    }

    pub fn is_identified(&self) -> bool {
        matches!(self, PeerState::Identified { .. })
    }
}

/// Extended peer information with state and distance class
struct MemPeer {
    id: PeerId,
    state: PeerState,
    /// Cached distance class for efficient lookups
    distance_class: usize,
    // TODO: network address, shared secret
}

// ============================================================================
// Actions
// ============================================================================

/// Actions that EcPeers requests EcNode to perform
#[derive(Debug, Clone)]
pub enum PeerAction {
    /// Send a Query message to a peer (for elections)
    SendQuery {
        receiver: PeerId,
        token: TokenId,
        ticket: MessageTicket,
    },

    /// Send an Answer message with proof-of-storage signature
    SendAnswer {
        answer: TokenMapping,
        signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
        ticket: MessageTicket,
    },

    /// Send a Referral message with suggested peers
    SendReferral {
        token: TokenId,
        ticket: MessageTicket,
        suggested_peers: [PeerId; 2],
    },

    /// Send an Invitation (Answer with ticket=0)
    SendInvitation {
        receiver: PeerId,
        answer: TokenMapping,
        signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
    },
}

// ============================================================================
// Election Tracking
// ============================================================================

/// State of an election for caching
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ElectionState {
    /// Election is actively collecting responses
    Running,
    /// Election completed successfully with a winner
    Completed {
        winner: PeerId,
        completed_at: EcTime,
    },
    /// Election timed out without finding a winner
    TimedOut {
        timed_out_at: EcTime,
    },
}

/// Tracks an ongoing or cached election
struct OngoingElection {
    election: PeerElection,
    started_at: EcTime,
    purpose: ElectionPurpose,
    state: ElectionState,
}

/// Purpose of an election
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ElectionPurpose {
    /// Bootstrap: trying to establish initial connections
    Bootstrap,
    /// Continuous: validating existing connections or finding better candidates
    Continuous,
    /// Responding to an incoming invitation
    InvitationResponse { from_peer: PeerId },
}

// ============================================================================
// Token Sample Collection
// ============================================================================

/// Number of distance classes (one per bit position in u64)
const NUM_DISTANCE_CLASSES: usize = 64;

/// Sampled tokens organized by distance class for even ring coverage
///
/// This structure maintains a bounded collection of tokens distributed evenly
/// across the u64 ID space. Tokens are organized into 64 distance classes
/// (one per bit position), with each class holding a limited number of tokens.
///
/// This enables efficient peer discovery across the entire network by ensuring
/// we have queryable tokens in all regions of the ID space.
struct TokenSampleCollection {
    /// Tokens organized by distance class (log2-based bucketing)
    /// Each BTreeSet holds up to max_per_class tokens
    tokens_by_class: [BTreeSet<TokenId>; NUM_DISTANCE_CLASSES],

    /// Maximum tokens per distance class
    max_per_class: usize,

    /// Our peer ID (for calculating distances)
    peer_id: PeerId,
}

impl TokenSampleCollection {
    /// Create a new empty token sample collection
    fn new(peer_id: PeerId, max_per_class: usize) -> Self {
        // Initialize array of empty BTreeSets
        let tokens_by_class: [BTreeSet<TokenId>; NUM_DISTANCE_CLASSES] =
            std::array::from_fn(|_| BTreeSet::new());

        Self {
            tokens_by_class,
            max_per_class,
            peer_id,
        }
    }

    /// Calculate which distance class a token belongs to
    /// Uses same logic as EcPeers::calculate_distance_class
    fn calculate_distance_class(peer_id: PeerId, token: TokenId) -> usize {
        let dist = {
            let forward = token.wrapping_sub(peer_id);
            let backward = peer_id.wrapping_sub(token);
            forward.min(backward)
        };

        if dist == 0 {
            return 0;
        }

        // Use leading zeros to get log2-based classes
        (64 - dist.leading_zeros()) as usize
    }

    /// Add a token to the collection
    /// Returns true if token was added, false if rejected
    fn add_token(&mut self, token: TokenId) -> bool {
        if token == self.peer_id {
            return false; // Don't store self
        }

        let class = Self::calculate_distance_class(self.peer_id, token);
        let class_set = &mut self.tokens_by_class[class];

        // If already present, nothing to do
        if class_set.contains(&token) {
            return false;
        }

        // If class is full, check if we should replace
        if class_set.len() >= self.max_per_class {
            // Simple strategy: replace a random token
            // Could be improved to replace the token closest to an existing one
            if let Some(&first_token) = class_set.iter().next() {
                class_set.remove(&first_token);
            }
        }

        class_set.insert(token);
        true
    }

    /// Sample tokens from an Answer message
    /// Answer contains: 1 answer token + 10 signature tokens = 11 total
    /// We sample intelligently to fill gaps in our collection
    fn sample_from_answer(
        &mut self,
        answer: &TokenMapping,
        signature: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
    ) {
        // Add the answer token
        self.add_token(answer.id);

        // Sample from signature tokens
        // Strategy: Add tokens from underrepresented distance classes first
        for sig_token in signature {
            if sig_token.id != 0 {
                self.add_token(sig_token.id);
            }
        }
    }

    /// Pick a random token from a specific distance class
    fn pick_from_class(&self, class_index: usize) -> Option<TokenId> {
        if class_index >= NUM_DISTANCE_CLASSES {
            return None;
        }

        let class_set = &self.tokens_by_class[class_index];
        if class_set.is_empty() {
            return None;
        }

        // Pick random token from class
        use rand::seq::IteratorRandom;
        class_set.iter().choose(&mut rand::thread_rng()).copied()
    }

    /// Pick a random token from any class
    fn pick_random(&self) -> Option<TokenId> {
        // Collect all tokens
        let all_tokens: Vec<TokenId> = self.tokens_by_class
            .iter()
            .flat_map(|set| set.iter().copied())
            .collect();

        if all_tokens.is_empty() {
            return None;
        }

        use rand::seq::SliceRandom;
        all_tokens.choose(&mut rand::thread_rng()).copied()
    }

    /// Get total number of sampled tokens
    fn len(&self) -> usize {
        self.tokens_by_class.iter().map(|set| set.len()).sum()
    }

    /// Check if collection is empty
    fn is_empty(&self) -> bool {
        self.tokens_by_class.iter().all(|set| set.is_empty())
    }
}

impl OngoingElection {
    fn new(election: PeerElection, started_at: EcTime, purpose: ElectionPurpose) -> Self {
        Self {
            election,
            started_at,
            purpose,
            state: ElectionState::Running,
        }
    }

    /// Check if this election result can be used from cache
    fn is_cache_valid(&self, current_time: EcTime, cache_ttl: u64) -> bool {
        match self.state {
            ElectionState::Completed { completed_at, .. } => {
                current_time - completed_at < cache_ttl
            }
            _ => false,
        }
    }

    /// Mark election as completed with winner
    fn complete(&mut self, winner: PeerId, time: EcTime) {
        self.state = ElectionState::Completed {
            winner,
            completed_at: time,
        };
    }

    /// Mark election as timed out
    fn timeout(&mut self, time: EcTime) {
        self.state = ElectionState::TimedOut {
            timed_out_at: time,
        };
    }

    /// Get winner if election is completed
    fn get_winner(&self) -> Option<PeerId> {
        match self.state {
            ElectionState::Completed { winner, .. } => Some(winner),
            _ => None,
        }
    }
}

// ============================================================================
// Main Peer Manager
// ============================================================================

pub struct EcPeers {
    pub peer_id: PeerId,

    /// All known peers (BTreeMap = sorted ring topology for walking)
    peers: BTreeMap<PeerId, MemPeer>,

    /// Connected peer IDs only (Vec = fast binary search routing)
    active: Vec<PeerId>,

    /// Target connections per distance class (computed on-demand)
    distance_class_budgets: Vec<usize>,

    /// Ongoing elections indexed by challenge token
    active_elections: HashMap<TokenId, OngoingElection>,

    /// Proof-of-storage system (zero-sized helper, storage passed as parameter)
    proof_system: ProofOfStorage,

    /// Sampled tokens for peer discovery across the ID space
    token_samples: TokenSampleCollection,

    /// Configuration
    config: PeerManagerConfig,

    /// Last time budget enforcement ran
    last_budget_enforcement: EcTime,

    /// Last time churn was applied
    last_churn_time: EcTime,

    /// Next time to trigger a new election
    next_election_time: EcTime,
}

pub struct PeerRange {
    high: PeerId,
    low: PeerId,
}

impl PeerRange {
    pub fn in_range(&self, key: &TokenId) -> bool {
        if self.low < self.high {
            *key >= self.low && *key <= self.high
        } else {
            // wrapped case (or empty) TokenId == 0 means "no token"
            *key <= self.high || *key >= self.low
        }
    }
}

impl EcPeers {
    // ========================================================================
    // Distance and Budget Management
    // ========================================================================

    /// Calculate ring distance between two peer IDs
    fn ring_distance(a: PeerId, b: PeerId) -> u64 {
        let forward = b.wrapping_sub(a);
        let backward = a.wrapping_sub(b);
        forward.min(backward)
    }

    /// Calculate which distance class a peer belongs to
    /// Uses log2-based bucketing: closer peers = lower class index
    fn calculate_distance_class(&self, peer_id: PeerId) -> usize {
        let dist = Self::ring_distance(self.peer_id, peer_id);
        if dist == 0 {
            return 0;
        }
        // Use leading zeros to get log2-based classes
        // Closer peers (smaller distance) have more leading zeros -> lower class
        (64 - dist.leading_zeros()) as usize
    }

    /// Allocate budget across distance classes using exponential gradient
    /// Closer classes get more connections
    fn allocate_distance_budgets(total_budget: usize, num_classes: usize) -> Vec<usize> {
        if num_classes == 0 {
            return vec![];
        }

        // Exponential decay: weight[i] = 2^(-i/4)
        // This gives more weight to closer classes
        let weights: Vec<f64> = (0..num_classes)
            .map(|i| 2.0_f64.powf(-(i as f64) / 4.0))
            .collect();

        let total_weight: f64 = weights.iter().sum();

        // Allocate budget proportionally
        let mut budgets: Vec<usize> = weights
            .iter()
            .map(|&w| ((w / total_weight) * total_budget as f64).round() as usize)
            .collect();

        // Ensure sum equals total_budget (handle rounding errors)
        let sum: usize = budgets.iter().sum();
        if sum < total_budget {
            // Add remainder to closest class
            budgets[0] += total_budget - sum;
        } else if sum > total_budget {
            // Remove excess from furthest class
            let excess = sum - total_budget;
            for i in (0..num_classes).rev() {
                if budgets[i] >= excess {
                    budgets[i] -= excess;
                    break;
                }
            }
        }

        budgets
    }

    /// Initialize distance class budgets
    fn initialize_distance_class_budgets(config: &PeerManagerConfig) -> Vec<usize> {
        // Create 64 distance classes (for u64 address space)
        const NUM_CLASSES: usize = 64;
        Self::allocate_distance_budgets(config.total_budget, NUM_CLASSES)
    }

    /// Distance bounds for a class: [2^i, 2^(i+1))
    fn distance_bounds(class_index: usize) -> (u64, u64) {
        let lower = if class_index == 0 { 1 } else { 1u64 << class_index };
        let upper = 1u64 << (class_index + 1);
        (lower, upper)
    }

    /// Check if a peer is in a specific distance class
    fn is_peer_in_class(&self, peer_id: PeerId, class_index: usize) -> bool {
        let dist = Self::ring_distance(self.peer_id, peer_id);
        let (lower, upper) = Self::distance_bounds(class_index);
        dist >= lower && dist < upper
    }

    /// Count Connected peers in a distance class (bidirectional)
    fn connected_count_in_class(&self, class_index: usize) -> usize {
        let (lower, upper) = Self::distance_bounds(class_index);

        self.active.iter()
            .filter(|&&peer_id| {
                let dist = Self::ring_distance(self.peer_id, peer_id);
                dist >= lower && dist < upper
            })
            .count()
    }

    /// Get Connected peers in a distance class (bidirectional)
    fn connected_peers_in_class(&self, class_index: usize) -> Vec<PeerId> {
        let (lower, upper) = Self::distance_bounds(class_index);

        self.active.iter()
            .filter(|&&peer_id| {
                let dist = Self::ring_distance(self.peer_id, peer_id);
                dist >= lower && dist < upper
            })
            .copied()
            .collect()
    }

    /// Find closest peers to a target token (for election channels)
    /// Walks BTreeMap in both directions from target
    fn find_closest_peers(&self, target: TokenId, count: usize) -> Vec<PeerId> {
        let mut candidates = Vec::new();

        // Walk forward from target (including target)
        let forward: Vec<_> = self.peers.range(target..)
            .take(count)
            .map(|(id, _)| *id)
            .collect();

        // Walk backward from target (excluding target)
        let backward: Vec<_> = self.peers.range(..target)
            .rev()
            .take(count)
            .map(|(id, _)| *id)
            .collect();

        // Collect all candidates
        candidates.extend(forward);
        candidates.extend(backward);

        // Sort by distance from target
        candidates.sort_by_key(|&p| Self::ring_distance(p, target));

        // Return closest N
        candidates.into_iter().take(count).collect()
    }

    /// Pick random target token in distance class
    fn pick_target_in_distance_class(&self, class_index: usize) -> TokenId {
        let (lower, upper) = Self::distance_bounds(class_index);

        use rand::Rng;
        let dist = rand::thread_rng().gen_range(lower..upper);

        // Random direction (bidirectional distance class)
        if rand::random() {
            self.peer_id.wrapping_add(dist)
        } else {
            self.peer_id.wrapping_sub(dist)
        }
    }

    /// Random sample from slice (for eviction)
    fn random_sample(peers: &[PeerId], count: usize) -> Vec<PeerId> {
        use rand::seq::SliceRandom;
        peers.choose_multiple(&mut rand::thread_rng(), count.min(peers.len()))
            .copied()
            .collect()
    }

    // ========================================================================
    // Legacy Helper Methods (backward compatibility)
    // ========================================================================

    fn idx_adj(&self, idx: usize, adj: isize) -> usize {
        let tmp = idx as isize + adj;
        let len = self.active.len() as isize;

        let res = if tmp >= len {
            tmp - len
        } else if tmp < 0 {
            len + tmp
        } else {
            tmp
        };

        if res == len || res < 0 {
            panic!("adj {} {} -> {}", idx, adj, res);
        }

        res as usize
    }

    // ========================================================================
    // Message Handlers (TODO: will be fully implemented in later phases)
    // ========================================================================

    /// Handle an Answer message (election response or invitation)
    pub fn handle_answer(
        &mut self,
        answer: &TokenMapping,
        signature: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
        ticket: MessageTicket,
        peer_id: PeerId,
        time: EcTime,
    ) {
        // handle Invitation
        if ticket == 0 {
            // TODO handle Invitation here
            
        }
        // Sample tokens from Answer for future discovery
        // Answer contains: 1 answer token + 10 signature tokens = 11 valid tokens
        self.token_samples.sample_from_answer(answer, signature);

        // Route answer to the correct ongoing election
        let challenge_token = answer.id;

        if let Some(ongoing) = self.active_elections.get_mut(&challenge_token) {
            // Try to record the answer in the election
            match ongoing.election.handle_answer(ticket, answer, signature, peer_id, time) {
                Ok(()) => {
                    // Answer successfully recorded
                    // Winner will be detected in process_elections()
                }
                Err(_e) => {
                    // Invalid signature or ticket, or channel already blocked
                    // Ignore the answer
                }
            }
        }
        // If no election found for this token, ignore the answer
    }

    /// Handle a Referral message (peer suggestions)
    /// Routes the referral to the appropriate election and creates a new channel to the suggested peer
    pub fn handle_referral(
        &mut self,
        ticket: MessageTicket,
        token: TokenId,
        suggested_peers: [PeerId; 2],
        sender: PeerId,
        time: EcTime,
    ) -> Option<PeerAction> {
        // Find the ongoing election for this token
        if let Some(ongoing) = self.active_elections.get_mut(&token) {
            // Try to handle the referral
            match ongoing.election.handle_referral(ticket, token, suggested_peers, sender) {
                Ok(next_peer) => {
                    // Election returned a suggested peer to try next

                    // Sample peer IDs from Referral (peer IDs are valid tokens)
                    self.token_samples.add_token(suggested_peers[0]);
                    self.token_samples.add_token(suggested_peers[1]);

                    // Note: We DO allow querying non-Connected peers during discovery!
                    // The referral mechanism needs to work across the whole network,
                    // not just among already-Connected peers. This allows DHT-style
                    // routing to find token owners even if they're not directly connected.

                    // Create a new channel to the suggested peer
                    if let Ok(new_ticket) = ongoing.election.create_channel(next_peer, time) {
                        return Some(PeerAction::SendQuery {
                            receiver: next_peer,
                            token,
                            ticket: new_ticket,
                        });
                    }
                }
                Err(_) => {
                    // Referral failed (wrong token, unknown ticket, blocked channel, etc.)
                    // Ignore
                }
            }
        }
        // If no election found for this token, ignore the referral

        None
    }

    /// Handle a Query message - gateway to proof-of-storage system
    ///
    /// This is the main entry point for responding to queries. It:
    /// 1. Checks if we own the requested token using proof-of-storage
    /// 2. If found: generates Answer with signature
    /// 3. If not found: generates Referral with 2 closest Connected Peers
    ///
    /// # Arguments
    /// - `token`: The token being queried
    /// - `ticket`: Message ticket for this query
    /// - `querier`: The peer requesting the token
    ///
    /// # Returns
    /// - `Some(PeerAction::SendAnswer)`: If we own the token
    /// - `Some(PeerAction::SendReferral)`: If we don't own it but have peers to suggest
    /// - `None`: If we don't own the token and have no peers to suggest
    pub fn handle_query(
        &self,
        token_storage: &dyn TokenStorageBackend,
        token: TokenId,
        ticket: MessageTicket,
        querier: PeerId,
    ) -> Option<PeerAction> {
        // Try to generate a signature (checks if we own the token)
        if let Some(signature) = self.proof_system.generate_signature(token_storage, &token, &querier) {
            // We own the token - send Answer
            return Some(PeerAction::SendAnswer {
                answer: signature.answer,
                signature: signature.signature,
                ticket,
            });
        }

        // We don't own the token - find closest Connected Peers to refer
        let closest = self.find_closest_peers(token, 2);

        if closest.len() >= 2 {
            Some(PeerAction::SendReferral {
                token,
                ticket,
                suggested_peers: [closest[0], closest[1]],
            })
        } else {
            // Not enough peers to provide a referral
            None
        }
    }

    // ========================================================================
    // Routing Methods (backward compatibility with existing consensus code)
    // ========================================================================

    /// Get two peers responsible for a token
    pub(crate) fn peers_for(&self, key: &TokenId, time: EcTime) -> [PeerId; 2] {
        if self.active.is_empty() {
            return [0, 0];
        }

        let idx = match self.active.binary_search(key) {
            Ok(i) => (i + 1) % self.active.len(),
            Err(i) => i % self.active.len(),
        };

        let adj = (((key ^ self.peer_id) + time) & 0x3) as isize + 1;

        [
            *self.active.get(self.idx_adj(idx, -adj)).unwrap(),
            *self.active.get(self.idx_adj(idx, adj)).unwrap(),
        ]
    }

    /// Get a single peer responsible for a token
    pub(crate) fn peer_for(&self, key: &TokenId, time: EcTime) -> PeerId {
        if self.active.is_empty() {
            return 0;
        }

        let idx = match self.active.binary_search(key) {
            Ok(i) => (i + 1) % self.active.len(),
            Err(i) => i % self.active.len(),
        };

        let adj = (((key ^ self.peer_id) + time) & 0x7) as isize - 3;

        *self.active.get(self.idx_adj(idx, adj)).unwrap()
    }

    /// Get peer indices for a token (internal use)
    pub(crate) fn peers_idx_for(&self, key: &TokenId, time: EcTime) -> Vec<usize> {
        if self.active.is_empty() {
            return vec![];
        }

        let idx = match self.active.binary_search(key) {
            Ok(i) => (i + 1) % self.active.len(),
            Err(i) => i % self.active.len(),
        };

        let adj = (((key ^ self.peer_id) + time) & 0x3) as isize + 1;

        vec![self.idx_adj(idx, -adj), self.idx_adj(idx, adj)]
    }

    /// Get peer ID by index in active list
    pub fn for_index(&self, idx: usize) -> Option<PeerId> {
        self.active.get(idx).copied()
    }

    // ========================================================================
    // Peer Management
    // ========================================================================

    /// Update or add a peer (for backward compatibility with existing code)
    /// This is used by seed_peer in EcNode
    pub fn update_peer(&mut self, key: &PeerId, time: EcTime) {
        if *key == self.peer_id {
            return; // Never store self
        }

        // Check if peer already exists
        if let Some(peer) = self.peers.get_mut(key) {
            // Update keepalive time if connected
            if let PeerState::Connected { last_keepalive, .. } = &mut peer.state {
                *last_keepalive = time;
            }
        } else {
            // Add new peer in Connected state (for backward compatibility)
            // This is how seed_peer works - directly to Connected
            let distance_class = self.calculate_distance_class(*key);
            self.peers.insert(
                *key,
                MemPeer {
                    id: *key,
                    state: PeerState::Connected {
                        connected_since: time,
                        last_keepalive: time,
                        election_wins: 0,
                        election_attempts: 0,
                        quality_score: 1.0, // Start with max quality
                    },
                    distance_class,
                },
            );

            // Update active list (maintain sorted order)
            match self.active.binary_search(key) {
                Ok(_) => {} // Already in list
                Err(idx) => self.active.insert(idx, *key),
            }

            // Add peer ID to token samples (peer IDs are valid tokens for discovery)
            self.token_samples.add_token(*key);
        }
    }

    // ========================================================================
    // State Transitions
    // ========================================================================

    /// Add newly discovered peer to Identified state (from Referral)
    pub fn add_identified_peer(&mut self, peer_id: PeerId, time: EcTime) -> bool {
        if peer_id == self.peer_id {
            return false; // Never add self
        }

        // Check if peer already exists
        if self.peers.contains_key(&peer_id) {
            return false; // Already known
        }

        // Add to Identified state
        let distance_class = self.calculate_distance_class(peer_id);
        self.peers.insert(
            peer_id,
            MemPeer {
                id: peer_id,
                state: PeerState::Identified {
                    discovered_at: time,
                },
                distance_class,
            },
        );

        // Add peer ID to token samples (peer IDs are valid tokens for discovery)
        self.token_samples.add_token(peer_id);

        true
    }

    /// Promote Identified peer to Pending after election win (we send Invitation)
    pub fn promote_to_pending(&mut self, peer_id: PeerId, election_token: TokenId, time: EcTime) -> bool {
        let peer = match self.peers.get_mut(&peer_id) {
            Some(p) => p,
            None => return false, // Peer not found
        };

        // Can only promote from Identified
        if !peer.state.is_identified() {
            return false;
        }

        peer.state = PeerState::Pending {
            invitation_sent_at: time,
            from_election: election_token,
        };

        true
    }

    /// Promote peer to Connected (mutual Invitation exchange)
    pub fn promote_to_connected(&mut self, peer_id: PeerId, time: EcTime) -> bool {
        let peer = match self.peers.get_mut(&peer_id) {
            Some(p) => p,
            None => return false, // Peer not found
        };

        // Can promote from Identified or Pending
        if !peer.state.is_identified() && !peer.state.is_pending() {
            return false;
        }

        peer.state = PeerState::Connected {
            connected_since: time,
            last_keepalive: time,
            election_wins: 0,
            election_attempts: 0,
            quality_score: 1.0, // Start with perfect quality
        };

        // Add to active list
        if let Err(idx) = self.active.binary_search(&peer_id) {
            self.active.insert(idx, peer_id);
        }

        true
    }

    /// Demote Connected peer to Identified (timeout, churn, budget enforcement)
    pub fn demote_from_connected(&mut self, peer_id: PeerId, time: EcTime) -> bool {
        let peer = match self.peers.get_mut(&peer_id) {
            Some(p) => p,
            None => return false, // Peer not found
        };

        // Can only demote from Connected
        if !peer.state.is_connected() {
            return false;
        }

        peer.state = PeerState::Identified {
            discovered_at: time,
        };

        // Remove from active list
        if let Ok(idx) = self.active.binary_search(&peer_id) {
            self.active.remove(idx);
        }

        true
    }

    /// Demote Pending peer to Identified (timeout)
    pub fn demote_to_identified(&mut self, peer_id: PeerId, time: EcTime) -> bool {
        let peer = match self.peers.get_mut(&peer_id) {
            Some(p) => p,
            None => return false, // Peer not found
        };

        // Can demote from Pending
        if !peer.state.is_pending() {
            return false;
        }

        peer.state = PeerState::Identified {
            discovered_at: time,
        };

        true
    }

    /// Update last_keepalive for Connected peer
    pub fn update_keepalive(&mut self, peer_id: PeerId, time: EcTime) -> bool {
        let peer = match self.peers.get_mut(&peer_id) {
            Some(p) => p,
            None => return false, // Peer not found
        };

        if let PeerState::Connected { last_keepalive, .. } = &mut peer.state {
            *last_keepalive = time;
            true
        } else {
            false
        }
    }

    // ========================================================================
    // Timeout Detection
    // ========================================================================

    /// Detect and handle Pending peer timeouts
    /// Returns list of peers that were demoted
    pub fn detect_pending_timeouts(&mut self, time: EcTime) -> Vec<PeerId> {
        let timeout_threshold = self.config.pending_timeout;
        let mut timed_out = Vec::new();

        for (peer_id, peer) in &self.peers {
            if let PeerState::Pending { invitation_sent_at, .. } = peer.state {
                if time - invitation_sent_at >= timeout_threshold {
                    timed_out.push(*peer_id);
                }
            }
        }

        // Demote all timed out peers
        for peer_id in &timed_out {
            self.demote_to_identified(*peer_id, time);
        }

        timed_out
    }

    /// Detect and handle Connected peer timeouts (no keepalive)
    /// Returns list of peers that were demoted
    pub fn detect_connection_timeouts(&mut self, time: EcTime) -> Vec<PeerId> {
        let timeout_threshold = self.config.connection_timeout;
        let mut timed_out = Vec::new();

        for (peer_id, peer) in &self.peers {
            if let PeerState::Connected { last_keepalive, .. } = peer.state {
                if time - last_keepalive >= timeout_threshold {
                    timed_out.push(*peer_id);
                }
            }
        }

        // Demote all timed out peers
        for peer_id in &timed_out {
            self.demote_from_connected(*peer_id, time);
        }

        timed_out
    }

    /// Clean up expired election cache entries
    /// Returns number of elections cleaned up
    pub fn cleanup_expired_elections(&mut self, time: EcTime) -> usize {
        let cache_ttl = self.config.election_cache_ttl;
        let mut to_remove = Vec::new();

        for (token, election) in &self.active_elections {
            // Remove if completed and TTL expired, or if timed out long ago
            let should_remove = match election.state {
                ElectionState::Completed { completed_at, .. } => {
                    time - completed_at >= cache_ttl
                }
                ElectionState::TimedOut { timed_out_at } => {
                    time - timed_out_at >= cache_ttl
                }
                ElectionState::Running => {
                    // Remove if running too long (expired)
                    time - election.started_at >= self.config.election_timeout
                }
            };

            if should_remove {
                to_remove.push(*token);
            }
        }

        let count = to_remove.len();
        for token in to_remove {
            self.active_elections.remove(&token);
        }

        count
    }

    // ========================================================================
    // Peer Lookup and Range
    // ========================================================================

    /// Get peer range for a key (for Referral messages)
    pub(crate) fn peer_range(&self, key: &PeerId) -> PeerRange {
        if self.active.len() <= 10 {
            return PeerRange {
                low: PeerId::MIN,
                high: PeerId::MAX,
            };
        }

        match self.active.binary_search(key) {
            Ok(idx) | Err(idx) => {
                let idx = idx % self.active.len();
                PeerRange {
                    low: self.active[self.idx_adj(idx, -4)],
                    high: self.active[self.idx_adj(idx, 4)],
                }
            }
        }
    }

    /// Check if a peer is trusted (in active list)
    pub(crate) fn trusted_peer(&self, key: &PeerId) -> Option<usize> {
        self.active.binary_search(key).ok()
    }

    // ========================================================================
    // Construction and Accessors
    // ========================================================================

    /// Create a new peer manager with default configuration
    pub fn new(peer_id: PeerId) -> Self {
        Self::with_config(peer_id, PeerManagerConfig::default())
    }

    /// Create a new peer manager with custom configuration
    pub fn with_config(peer_id: PeerId, config: PeerManagerConfig) -> Self {
        let distance_class_budgets = Self::initialize_distance_class_budgets(&config);
        let proof_system = ProofOfStorage::new();
        let token_samples = TokenSampleCollection::new(peer_id, config.max_tokens_per_distance_class);

        Self {
            peer_id,
            peers: BTreeMap::new(),
            active: Vec::new(),
            distance_class_budgets,
            active_elections: HashMap::new(),
            proof_system,
            token_samples,
            config,
            last_budget_enforcement: 0,
            last_churn_time: 0,
            next_election_time: 0,
        }
    }

    /// Get number of peers (backward compatibility)
    pub fn num_peers(&self) -> usize {
        self.active.len()
    }

    /// Get number of Connected peers
    pub fn num_connected(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.state.is_connected())
            .count()
    }

    // ========================================================================
    // Election Management (Phase 3)
    // ========================================================================

    /// Start a new peer election for a challenge token
    pub fn start_election(&mut self, challenge_token: TokenId, time: EcTime) -> Vec<PeerAction> {
        // Check if we already have an election for this token
        if self.active_elections.contains_key(&challenge_token) {
            return Vec::new(); // Election already running
        }


        // Create new election
        let election = PeerElection::new(
            challenge_token,
            self.peer_id,
            self.config.election_config.clone(),
        );

        let ongoing = OngoingElection::new(
            election,
            time,
            ElectionPurpose::Continuous,
        );

        self.active_elections.insert(challenge_token, ongoing);

        // Spawn initial channels and return Query actions
        self.spawn_election_channels(challenge_token, time)
    }

    /// Spawn N channels for an election
    /// Returns PeerActions to send Query messages for the channels
    fn spawn_election_channels(&mut self, challenge_token: TokenId, time: EcTime) -> Vec<PeerAction> {
        // Check if election exists
        if !self.active_elections.contains_key(&challenge_token) {
            return Vec::new(); // Election not found
        }

        let count = self.config.election_channel_count;
        let mut actions = Vec::new();

        // OPTIMIZATION: Always try querying the challenge token directly first
        // If it's a peer ID, that peer owns their own ID token and will Answer immediately
        // If it's not a peer ID, the query will fail or get Referred to the actual owner
        let mut candidates = vec![challenge_token];

        // Add closest peers as additional candidates (for DHT-style routing)
        let closest = self.find_closest_peers(
            challenge_token,
            self.config.channel_candidate_count,
        );
        candidates.extend(closest);

        // Now get mutable access to election
        let Some(ongoing) = self.active_elections.get_mut(&challenge_token) else {
            return Vec::new();
        };

        for first_hop in candidates.iter().take(count) {
            // Create channel
            match ongoing.election.create_channel(*first_hop, time) {
                Ok(ticket) => {
                    // Create action to send Query message
                    actions.push(PeerAction::SendQuery {
                        receiver: *first_hop,
                        token: challenge_token,
                        ticket,
                    });
                }
                Err(_) => {
                    // Channel already exists or other error, skip
                    continue;
                }
            }
        }

        actions
    }

    /// Pick a challenge token for election
    /// Strategy: Prioritize peer IDs (guaranteed Answers), build ring with density gradient
    fn pick_challenge_token(&self) -> TokenId {
        use rand::Rng;

        // Strategy 0: Prioritize known peer IDs (80% of the time when we have peers)
        // Peer IDs ARE tokens by design, and peers always own their own ID
        // This ensures high Answer rate and successful elections
        if !self.peers.is_empty() && rand::thread_rng().gen_bool(0.8) {
            // Pick a random peer ID from our peer list
            let peer_ids: Vec<PeerId> = self.peers.keys().copied().collect();
            if let Some(peer_id) = peer_ids.get(rand::thread_rng().gen_range(0..peer_ids.len())) {
                return *peer_id;
            }
        }

        // Strategy 1: Check for coverage gaps in distance classes
        // Prioritize closer classes (build gradient around our peer ID)
        // Only use sampled tokens to fill gaps, don't generate new ones (Strategy 4 does that)
        for class_index in 0..NUM_DISTANCE_CLASSES {
            let class_set = &self.token_samples.tokens_by_class[class_index];

            // If this class is underrepresented (< 50% capacity) AND has some tokens, use them
            if class_set.len() > 0 && class_set.len() < self.config.max_tokens_per_distance_class / 2 {
                if let Some(token) = self.token_samples.pick_from_class(class_index) {
                    return token;
                }
            }
        }

        // Strategy 2: All classes have good coverage - use sampled tokens
        // Prioritize closer classes to maintain density gradient
        for class_index in 0..NUM_DISTANCE_CLASSES {
            if let Some(token) = self.token_samples.pick_from_class(class_index) {
                // Weight by distance: closer classes more likely
                let weight = 1.0 / (class_index as f64 + 1.0);
                if rand::thread_rng().gen_bool(weight.min(0.8)) {
                    return token;
                }
            }
        }

        // Strategy 3: Random sampled token as fallback
        if let Some(token) = self.token_samples.pick_random() {
            return token;
        }

        // Strategy 4: Pure random exploration (only when we have NO peers and NO samples)
        rand::thread_rng().gen()
    }

    /// Pick a peer for re-election (peer that hasn't been verified recently)
    fn pick_peer_for_reelection(&self) -> Option<PeerId> {
        use rand::seq::SliceRandom;

        let connected_peers: Vec<_> = self.peers
            .values()
            .filter(|p| p.state.is_connected())
            .collect();

        if connected_peers.is_empty() {
            return None;
        }

        // Pick random connected peer
        connected_peers
            .choose(&mut rand::thread_rng())
            .map(|p| p.id)
    }

    /// Pick a token near a peer on the ring (for targeted re-election)
    fn pick_token_near_peer(&self, peer: PeerId) -> TokenId {
        use rand::Rng;

        // Try to find a sampled token in the same distance class as the peer
        let class = Self::ring_distance(self.peer_id, peer);
        let class_index = if class == 0 {
            0
        } else {
            (64 - class.leading_zeros()) as usize
        };

        // 70% of the time, use a sampled token from the same class
        if rand::thread_rng().gen_bool(0.7) {
            if let Some(token) = self.token_samples.pick_from_class(class_index) {
                return token;
            }
        }

        // Fallback: Pick random token near peer
        peer.wrapping_add(rand::thread_rng().gen_range(0..1000))
    }

    /// Process ongoing elections and check for winners
    fn process_elections(&mut self, token_storage: &dyn TokenStorageBackend, time: EcTime) -> Vec<PeerAction> {
        use crate::ec_proof_of_storage::WinnerResult;
        let mut actions = Vec::new();
        let mut to_resolve: Vec<(TokenId, usize)> = Vec::new();
        let mut winners: Vec<(TokenId, PeerId)> = Vec::new();

        // First pass: collect election results (only read, no mutable calls)
        let tokens: Vec<TokenId> = self.active_elections.keys().copied().collect();

        for token in tokens {
            let Some(ongoing) = self.active_elections.get_mut(&token) else {
                continue;
            };

            // Skip if not Running
            if !matches!(ongoing.state, ElectionState::Running) {
                continue;
            }

            let elapsed = time.saturating_sub(ongoing.started_at);

            // Wait for minimum collection time
            if elapsed < self.config.min_collection_time {
                continue;
            }

            // Check for winner
            match ongoing.election.check_for_winner() {
                WinnerResult::Single { winner, .. } => {
                    // Success! Election complete
                    ongoing.complete(winner, time);
                    winners.push((token, winner));
                }

                WinnerResult::SplitBrain { .. } => {
                    // Split-brain detected
                    if elapsed < self.config.election_timeout && ongoing.election.can_create_channel() {
                        // Try to resolve with more channels
                        let needed = 2;
                        to_resolve.push((token, needed));
                    } else {
                        // Give up
                        ongoing.timeout(time);
                    }
                }

                WinnerResult::NoConsensus => {
                    // Not enough responses yet
                    if elapsed >= self.config.election_timeout {
                        // Timeout
                        ongoing.timeout(time);
                    }
                }
            }
        }

        // Second pass: handle winners (needs mutable self)
        for (token, winner) in winners {
            let new_actions = self.handle_election_success(token_storage, token, winner, time);
            actions.extend(new_actions);
        }

        // Spawn more channels for split-brain elections
        for (token, _count) in to_resolve {
            let _spawned = self.spawn_election_channels(token, time);
            // TODO: Generate SendQuery actions for spawned channels
        }

        // Note: Don't remove completed elections immediately - keep them in cache
        // They will be cleaned up by cleanup_expired_elections()

        actions
    }

    /// Handle successful election - add winner to peer list
    fn handle_election_success(&mut self, token_storage: &dyn TokenStorageBackend, _token: TokenId, winner: PeerId, time: EcTime) -> Vec<PeerAction> {
        let mut actions = Vec::new();

        // Check if winner is self (shouldn't happen, but be safe)
        if winner == self.peer_id {
            return actions;
        }

        // Check if peer already exists
        if let Some(peer) = self.peers.get_mut(&winner) {
            // Update election stats if Connected
            if let PeerState::Connected {
                election_wins,
                election_attempts,
                quality_score,
                ..
            } = &mut peer.state
            {
                *election_wins += 1;
                *election_attempts += 1;
                // Update quality score with exponential moving average
                let alpha = self.config.quality_decay_alpha;
                *quality_score = alpha * 1.0 + (1.0 - alpha) * *quality_score;
            } else {
                // Not connected - promote to Pending
                // This means we should send an Invitation
                if peer.state.is_identified() {
                    self.promote_to_pending(winner, _token, time);
                    // Generate SendInvitation action
                    if let Some(sig) = self.proof_system.generate_signature(token_storage, &self.peer_id, &winner) {
                        actions.push(PeerAction::SendInvitation {
                            receiver: winner,
                            answer: sig.answer,
                            signature: sig.signature,
                        });
                    }
                }
            }
        } else {
            // New peer discovered - add to Identified state
            self.add_identified_peer(winner, time);
            // Optionally promote to Pending and send invitation
            self.promote_to_pending(winner, _token, time);
            // Generate SendInvitation action
            if let Some(sig) = self.proof_system.generate_signature(token_storage, &self.peer_id, &winner) {
                actions.push(PeerAction::SendInvitation {
                    receiver: winner,
                    answer: sig.answer,
                    signature: sig.signature,
                });
            }
        }

        // Check if we need to evict peers (over budget)
        let connected_count = self.num_connected();
        if connected_count > self.config.total_budget {
            self.evict_worst_peer(time);
        }

        actions
    }

    /// Evict the worst performing peer
    fn evict_worst_peer(&mut self, time: EcTime) {
        // Find worst peer by quality score
        let worst_peer = self.peers
            .iter()
            .filter(|(_, p)| p.state.is_connected())
            .min_by(|(_, a), (_, b)| {
                let quality_a = match a.state {
                    PeerState::Connected { quality_score, .. } => quality_score,
                    _ => 1.0,
                };
                let quality_b = match b.state {
                    PeerState::Connected { quality_score, .. } => quality_score,
                    _ => 1.0,
                };
                quality_a.partial_cmp(&quality_b).unwrap()
            })
            .map(|(id, _)| *id);

        if let Some(peer_id) = worst_peer {
            self.demote_from_connected(peer_id, time);
        }
    }

    // ========================================================================
    // Main Tick Method (Phase 3)
    // ========================================================================

    /// Main tick function - returns actions for EcNode to execute
    pub fn tick(&mut self, token_storage: &dyn TokenStorageBackend, time: EcTime) -> Vec<PeerAction> {
        let mut actions = Vec::new();

        // Phase 1: Timeout detection
        self.detect_pending_timeouts(time);
        self.detect_connection_timeouts(time);
        self.cleanup_expired_elections(time);

        // Phase 2: Process ongoing elections
        let election_actions = self.process_elections(token_storage, time);
        actions.extend(election_actions);

        // Phase 3: Start new elections if needed
        if time >= self.next_election_time {
            let new_election_actions = self.trigger_next_election(time);
            actions.extend(new_election_actions);
            self.next_election_time = time + self.config.election_interval;
        }

        // TODO Phase 4: Apply quality updates
        // TODO Phase 5: Apply churn if epoch boundary

        actions
    }

    /// Trigger the next election (continuous re-election)
    /// Returns PeerActions to send Query messages
    fn trigger_next_election(&mut self, time: EcTime) -> Vec<PeerAction> {
        use rand::random;

        // 50% of elections: verify existing peer
        // 50% of elections: discover new peer
        let challenge_token = if random::<bool>() && !self.peers.is_empty() {
            // Re-elect existing peer
            if let Some(peer) = self.pick_peer_for_reelection() {
                self.pick_token_near_peer(peer)
            } else {
                self.pick_challenge_token()
            }
        } else {
            // Discover new peer
            self.pick_challenge_token()
        };

        self.start_election(challenge_token, time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ec_memory_backend::MemTokens;

    // Helper function to create a test EcPeers instance
    fn create_test_peers(peer_id: PeerId) -> EcPeers {
        EcPeers::new(peer_id)
    }

    // Helper function to create a test EcPeers instance with custom config
    fn create_test_peers_with_config(peer_id: PeerId, config: PeerManagerConfig) -> EcPeers {
        EcPeers::with_config(peer_id, config)
    }

    // ========================================================================
    // Distance and Budget Tests
    // ========================================================================

    #[test]
    fn test_ring_distance_normal() {
        // Normal case: forward distance shorter
        assert_eq!(EcPeers::ring_distance(100, 150), 50);
        assert_eq!(EcPeers::ring_distance(150, 100), 50); // Symmetric
    }

    #[test]
    fn test_ring_distance_wrapping() {
        // Wrapping case: backward distance shorter
        let result = EcPeers::ring_distance(10, u64::MAX - 5);
        // Forward: (MAX - 5) - 10 = MAX - 15 (very large)
        // Backward: 10 - (MAX - 5) wraps to 16
        assert_eq!(result, 16);
    }

    #[test]
    fn test_ring_distance_self() {
        assert_eq!(EcPeers::ring_distance(42, 42), 0);
        assert_eq!(EcPeers::ring_distance(0, 0), 0);
        assert_eq!(EcPeers::ring_distance(u64::MAX, u64::MAX), 0);
    }

    #[test]
    fn test_distance_class_calculation() {
        let peers = create_test_peers(1000);

        // Self should be class 0
        assert_eq!(peers.calculate_distance_class(1000), 0);

        // Very close peer (distance 1)
        assert_eq!(peers.calculate_distance_class(1001), 1);

        // Moderate distance
        let dist_class_100 = peers.calculate_distance_class(1100);
        let dist_class_200 = peers.calculate_distance_class(1200);
        // Further peers should be in higher classes
        assert!(dist_class_200 >= dist_class_100);
    }

    #[test]
    fn test_budget_allocation_sums_to_total() {
        let total = 50;
        let budgets = EcPeers::allocate_distance_budgets(total, 64);

        // Sum should equal total budget
        let sum: usize = budgets.iter().sum();
        assert_eq!(sum, total);
    }

    #[test]
    fn test_budget_allocation_gradient() {
        let budgets = EcPeers::allocate_distance_budgets(100, 10);

        // Closer classes should have more budget
        for i in 0..budgets.len() - 1 {
            assert!(
                budgets[i] >= budgets[i + 1],
                "Budget[{}]={} should be >= Budget[{}]={}",
                i,
                budgets[i],
                i + 1,
                budgets[i + 1]
            );
        }
    }

    #[test]
    fn test_budget_allocation_empty() {
        let budgets = EcPeers::allocate_distance_budgets(50, 0);
        assert_eq!(budgets.len(), 0);
    }

    #[test]
    fn test_distance_class_budgets_initialized() {
        let config = PeerManagerConfig::default();
        let budgets = EcPeers::initialize_distance_class_budgets(&config);

        assert_eq!(budgets.len(), 64); // 64 classes for u64 address space

        // Check budgets sum to total
        let total_budget: usize = budgets.iter().sum();
        assert_eq!(total_budget, config.total_budget);
    }

    // ========================================================================
    // Peer State Tests
    // ========================================================================

    #[test]
    fn test_peer_state_predicates() {
        let identified = PeerState::Identified { discovered_at: 0 };
        assert!(identified.is_identified());
        assert!(!identified.is_pending());
        assert!(!identified.is_connected());

        let pending = PeerState::Pending {
            invitation_sent_at: 10,
            from_election: 999,
        };
        assert!(!pending.is_identified());
        assert!(pending.is_pending());
        assert!(!pending.is_connected());

        let connected = PeerState::Connected {
            connected_since: 20,
            last_keepalive: 25,
            election_wins: 5,
            election_attempts: 10,
            quality_score: 0.8,
        };
        assert!(!connected.is_identified());
        assert!(!connected.is_pending());
        assert!(connected.is_connected());
    }

    // ========================================================================
    // Construction and Basic Operations
    // ========================================================================

    #[test]
    fn test_new_peer_manager() {
        let peers = create_test_peers(12345);

        assert_eq!(peers.peer_id, 12345);
        assert_eq!(peers.num_peers(), 0);
        assert_eq!(peers.num_connected(), 0);
        assert_eq!(peers.distance_class_budgets.len(), 64);
    }

    #[test]
    fn test_with_config() {
        let mut config = PeerManagerConfig::default();
        config.total_budget = 100;
        config.election_interval = 30;

        let peers = create_test_peers_with_config(999, config.clone());

        assert_eq!(peers.peer_id, 999);
        assert_eq!(peers.config.total_budget, 100);
        assert_eq!(peers.config.election_interval, 30);

        // Verify budget allocation
        let total: usize = peers.distance_class_budgets.iter().sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_update_peer_adds_to_connected() {
        let mut peers = create_test_peers(1000);

        // Add a peer via update_peer (simulates seed_peer)
        peers.update_peer(&2000, 10);

        assert_eq!(peers.num_peers(), 1);
        assert_eq!(peers.num_connected(), 1);

        // Check peer is in Connected state
        let peer = peers.peers.get(&2000).unwrap();
        assert!(peer.state.is_connected());
    }

    #[test]
    fn test_update_peer_ignores_self() {
        let mut peers = create_test_peers(1000);

        peers.update_peer(&1000, 10);

        assert_eq!(peers.num_peers(), 0);
    }

    #[test]
    fn test_update_peer_maintains_sorted_order() {
        let mut peers = create_test_peers(500);

        // Add peers in non-sorted order
        peers.update_peer(&3000, 10);
        peers.update_peer(&1000, 10);
        peers.update_peer(&2000, 10);

        // Check active list is sorted
        assert_eq!(peers.active, vec![1000, 2000, 3000]);
    }

    #[test]
    fn test_update_peer_updates_keepalive() {
        let mut peers = create_test_peers(1000);

        peers.update_peer(&2000, 10);

        // Update again with new time
        peers.update_peer(&2000, 50);

        // Should still be 1 peer
        assert_eq!(peers.num_peers(), 1);

        // Check keepalive was updated
        let peer = peers.peers.get(&2000).unwrap();
        if let PeerState::Connected { last_keepalive, .. } = peer.state {
            assert_eq!(last_keepalive, 50);
        } else {
            panic!("Expected Connected state");
        }
    }

    // ========================================================================
    // Routing Tests (backward compatibility)
    // ========================================================================

    #[test]
    fn test_peers_for_empty() {
        let peers = create_test_peers(1000);
        let result = peers.peers_for(&5000, 0);

        // Should return [0, 0] for empty list
        assert_eq!(result, [0, 0]);
    }

    #[test]
    fn test_peer_for_empty() {
        let peers = create_test_peers(1000);
        let result = peers.peer_for(&5000, 0);

        // Should return 0 for empty list
        assert_eq!(result, 0);
    }

    #[test]
    fn test_peers_for_returns_two_peers() {
        let mut peers = create_test_peers(1000);

        // Add several peers
        for i in 1..10 {
            peers.update_peer(&(i * 1000), 0);
        }

        let result = peers.peers_for(&5500, 0);

        // Should return two different peers
        assert_ne!(result[0], 0);
        assert_ne!(result[1], 0);
        assert_ne!(result[0], result[1]);
    }

    #[test]
    fn test_trusted_peer() {
        let mut peers = create_test_peers(1000);

        peers.update_peer(&2000, 0);
        peers.update_peer(&3000, 0);

        // Peer in list should be trusted
        assert!(peers.trusted_peer(&2000).is_some());
        assert!(peers.trusted_peer(&3000).is_some());

        // Peer not in list should not be trusted
        assert!(peers.trusted_peer(&9999).is_none());
    }

    #[test]
    fn test_peer_range() {
        let mut peers = create_test_peers(1000);

        // Add many peers
        for i in 0..20 {
            peers.update_peer(&(i * 1000), 0);
        }

        let range = peers.peer_range(&10000);

        // Range should include peers on both sides
        assert!(range.low < 10000 || range.high > 10000);
    }

    #[test]
    fn test_peer_range_small_network() {
        let mut peers = create_test_peers(1000);

        // Add only 5 peers (less than threshold)
        for i in 1..6 {
            peers.update_peer(&(i * 1000), 0);
        }

        let range = peers.peer_range(&3000);

        // Should return full range for small networks
        assert_eq!(range.low, PeerId::MIN);
        assert_eq!(range.high, PeerId::MAX);
    }

    // ========================================================================
    // Configuration Tests
    // ========================================================================

    #[test]
    fn test_config_default_values() {
        let config = PeerManagerConfig::default();

        assert_eq!(config.total_budget, 50);
        assert_eq!(config.budget_enforcement_interval, 60);
        assert_eq!(config.election_interval, 60);
        assert_eq!(config.min_collection_time, 2);
        assert_eq!(config.election_timeout, 8);
        assert_eq!(config.pending_timeout, 10);
        assert_eq!(config.connection_timeout, 300);
        assert_eq!(config.quality_decay_alpha, 0.3);
        assert_eq!(config.churn_rate, 0.4);
        assert_eq!(config.churn_interval, 1800);
        assert_eq!(config.churn_protection_time, 600);
        assert_eq!(config.election_channel_count, 3);
        assert_eq!(config.channel_candidate_count, 8);
        assert_eq!(config.max_discovered_peers, 1000);
        assert_eq!(config.election_cache_ttl, 300);
    }

    // ========================================================================
    // Tick Tests (basic - more will be added in later phases)
    // ========================================================================

    #[test]
    fn test_tick_returns_empty_initially() {
        let mut peers = create_test_peers(1000);
        let token_storage = MemTokens::new();
        let actions = peers.tick(&token_storage, 0);

        // At time 0 with no peers, elections start immediately
        // We get 1 action: SendQuery to the challenge token directly
        assert!(actions.len() <= 1, "Expected at most 1 action, got {}", actions.len());
    }

    // ========================================================================
    // Distance Class Operations Tests
    // ========================================================================

    #[test]
    fn test_distance_bounds() {
        // Class 0: [1, 2)
        assert_eq!(EcPeers::distance_bounds(0), (1, 2));

        // Class 1: [2, 4)
        assert_eq!(EcPeers::distance_bounds(1), (2, 4));

        // Class 2: [4, 8)
        assert_eq!(EcPeers::distance_bounds(2), (4, 8));

        // Class 5: [32, 64)
        assert_eq!(EcPeers::distance_bounds(5), (32, 64));
    }

    #[test]
    fn test_is_peer_in_class() {
        let peers = create_test_peers(1000);

        // Distance 1 should be in class 0 [1, 2)
        assert!(peers.is_peer_in_class(1001, 0)); // Forward
        assert!(peers.is_peer_in_class(999, 0));  // Backward

        // Distance 3 should be in class 1 [2, 4)
        assert!(peers.is_peer_in_class(1003, 1)); // Forward
        assert!(peers.is_peer_in_class(997, 1));  // Backward

        // Distance 50 should be in class 5 [32, 64)
        assert!(peers.is_peer_in_class(1050, 5)); // Forward
        assert!(peers.is_peer_in_class(950, 5));  // Backward

        // Distance 1 should NOT be in class 1
        assert!(!peers.is_peer_in_class(1001, 1));
    }

    #[test]
    fn test_connected_count_in_class() {
        let mut peers = create_test_peers(1000);

        // Add peers in different distance classes
        peers.update_peer(&1001, 0); // Distance 1, class 0
        peers.update_peer(&999, 0);   // Distance 1, class 0
        peers.update_peer(&1003, 0); // Distance 3, class 1
        peers.update_peer(&1050, 0); // Distance 50, class 5

        // Class 0 should have 2 peers (bidirectional)
        assert_eq!(peers.connected_count_in_class(0), 2);

        // Class 1 should have 1 peer
        assert_eq!(peers.connected_count_in_class(1), 1);

        // Class 5 should have 1 peer
        assert_eq!(peers.connected_count_in_class(5), 1);

        // Class 10 should have 0 peers
        assert_eq!(peers.connected_count_in_class(10), 0);
    }

    #[test]
    fn test_connected_peers_in_class() {
        let mut peers = create_test_peers(1000);

        peers.update_peer(&1001, 0); // Class 0
        peers.update_peer(&999, 0);   // Class 0
        peers.update_peer(&1003, 0); // Class 1

        let class_0_peers = peers.connected_peers_in_class(0);
        assert_eq!(class_0_peers.len(), 2);
        assert!(class_0_peers.contains(&1001));
        assert!(class_0_peers.contains(&999));

        let class_1_peers = peers.connected_peers_in_class(1);
        assert_eq!(class_1_peers.len(), 1);
        assert!(class_1_peers.contains(&1003));
    }

    // ========================================================================
    // BTreeMap Walking Tests
    // ========================================================================

    #[test]
    fn test_find_closest_peers_btreemap() {
        let mut peers = create_test_peers(5000);

        // Add peers in BTreeMap (sorted order)
        peers.update_peer(&1000, 0);
        peers.update_peer(&2000, 0);
        peers.update_peer(&3000, 0);
        peers.update_peer(&4000, 0);
        peers.update_peer(&6000, 0);
        peers.update_peer(&7000, 0);
        peers.update_peer(&8000, 0);

        // Find 4 closest peers to 5500
        let closest = peers.find_closest_peers(5500, 4);

        // Should get peers closest by ring distance
        assert_eq!(closest.len(), 4);
        // 6000 is distance 500, 4000 is distance 1500
        assert!(closest.contains(&6000));
        assert!(closest.contains(&4000));
    }

    #[test]
    fn test_find_closest_peers_wrapping() {
        let mut peers = create_test_peers(100);

        // Add peers near both ends of u64 space (tests wrapping)
        let high_1 = u64::MAX - 50;
        let high_2 = u64::MAX - 100;

        peers.update_peer(&50, 0);
        peers.update_peer(&150, 0);
        peers.update_peer(&200, 0);
        peers.update_peer(&high_1, 0);
        peers.update_peer(&high_2, 0);

        // Find closest to 100
        let closest = peers.find_closest_peers(100, 3);

        assert_eq!(closest.len(), 3);
        // Should include peers on both sides
        assert!(closest.contains(&50) || closest.contains(&150));
    }

    #[test]
    fn test_pick_target_in_distance_class() {
        let peers = create_test_peers(1000);

        // Pick 100 targets in class 5 [32, 64)
        for _ in 0..100 {
            let target = peers.pick_target_in_distance_class(5);
            let dist = EcPeers::ring_distance(peers.peer_id, target);

            // Distance should be in range [32, 64)
            assert!(dist >= 32 && dist < 64,
                "Distance {} not in range [32, 64)", dist);
        }
    }

    #[test]
    fn test_random_sample() {
        let peers = vec![100, 200, 300, 400, 500];

        // Sample 3 from 5
        let sample = EcPeers::random_sample(&peers, 3);
        assert_eq!(sample.len(), 3);

        // All sampled peers should be from original
        for peer in &sample {
            assert!(peers.contains(peer));
        }

        // Sample more than available
        let sample_all = EcPeers::random_sample(&peers, 10);
        assert_eq!(sample_all.len(), 5);
    }

    // ========================================================================
    // Ring Wrapping Tests
    // ========================================================================

    #[test]
    fn test_peer_range_wrapping_at_zero() {
        let mut peers = create_test_peers(50);

        // Add peers that will cause range to wrap around 0
        for i in 0..20 {
            peers.update_peer(&(i * 100), 0);
        }

        let range = peers.peer_range(&100);

        // Range should handle wrapping
        // Either both low/high are valid, or range wraps (low > high)
        assert!(range.low <= u64::MAX);
        assert!(range.high <= u64::MAX);
    }

    #[test]
    fn test_peer_range_in_range_wrapping() {
        // Test normal case
        let range = PeerRange {
            low: 100,
            high: 200,
        };
        assert!(range.in_range(&150));
        assert!(!range.in_range(&50));
        assert!(!range.in_range(&250));

        // Test wrapping case
        let wrap_range = PeerRange {
            low: u64::MAX - 100,
            high: 100,
        };
        assert!(wrap_range.in_range(&(u64::MAX - 50))); // In range (high side)
        assert!(wrap_range.in_range(&50));              // In range (low side)
        assert!(!wrap_range.in_range(&500));            // Outside range
    }

    #[test]
    fn test_active_list_btreemap_consistency() {
        let mut peers = create_test_peers(1000);

        // Add peers (not including self at 1000)
        peers.update_peer(&3000, 0);
        peers.update_peer(&1500, 0);
        peers.update_peer(&2000, 0);

        // Active list should be sorted
        assert_eq!(peers.active, vec![1500, 2000, 3000]);

        // BTreeMap should have same peers
        assert_eq!(peers.peers.len(), 3);
        assert!(peers.peers.contains_key(&1500));
        assert!(peers.peers.contains_key(&2000));
        assert!(peers.peers.contains_key(&3000));

        // BTreeMap iteration should be in sorted order
        let btree_keys: Vec<_> = peers.peers.keys().copied().collect();
        assert_eq!(btree_keys, vec![1500, 2000, 3000]);
    }

    // ========================================================================
    // State Transition Tests
    // ========================================================================

    #[test]
    fn test_add_identified_peer() {
        let mut peers = create_test_peers(1000);

        // Add new peer
        assert!(peers.add_identified_peer(2000, 10));
        assert_eq!(peers.peers.len(), 1);

        // Check state is Identified
        let peer = peers.peers.get(&2000).unwrap();
        assert!(peer.state.is_identified());

        // Adding same peer again should fail
        assert!(!peers.add_identified_peer(2000, 20));

        // Adding self should fail
        assert!(!peers.add_identified_peer(1000, 10));
    }

    #[test]
    fn test_promote_to_pending() {
        let mut peers = create_test_peers(1000);

        // Add peer as Identified
        peers.add_identified_peer(2000, 10);

        // Promote to Pending
        assert!(peers.promote_to_pending(2000, 12345, 20));

        // Check state is Pending
        let peer = peers.peers.get(&2000).unwrap();
        assert!(peer.state.is_pending());

        // Cannot promote again (not Identified)
        assert!(!peers.promote_to_pending(2000, 12345, 30));
    }

    #[test]
    fn test_promote_to_connected_from_identified() {
        let mut peers = create_test_peers(1000);

        // Add peer as Identified
        peers.add_identified_peer(2000, 10);

        // Promote directly to Connected
        assert!(peers.promote_to_connected(2000, 20));

        // Check state is Connected
        let peer = peers.peers.get(&2000).unwrap();
        assert!(peer.state.is_connected());

        // Should be in active list
        assert!(peers.active.contains(&2000));
        assert_eq!(peers.num_connected(), 1);
    }

    #[test]
    fn test_promote_to_connected_from_pending() {
        let mut peers = create_test_peers(1000);

        // Add peer as Identified, then Pending
        peers.add_identified_peer(2000, 10);
        peers.promote_to_pending(2000, 12345, 20);

        // Promote to Connected
        assert!(peers.promote_to_connected(2000, 30));

        // Check state is Connected
        let peer = peers.peers.get(&2000).unwrap();
        assert!(peer.state.is_connected());

        // Should be in active list
        assert!(peers.active.contains(&2000));
    }

    #[test]
    fn test_demote_from_connected() {
        let mut peers = create_test_peers(1000);

        // Add peer as Connected (via seed_peer)
        peers.update_peer(&2000, 10);
        assert!(peers.active.contains(&2000));

        // Demote to Identified
        assert!(peers.demote_from_connected(2000, 20));

        // Check state is Identified
        let peer = peers.peers.get(&2000).unwrap();
        assert!(peer.state.is_identified());

        // Should not be in active list
        assert!(!peers.active.contains(&2000));
        assert_eq!(peers.num_connected(), 0);
    }

    #[test]
    fn test_demote_to_identified_from_pending() {
        let mut peers = create_test_peers(1000);

        // Add peer as Pending
        peers.add_identified_peer(2000, 10);
        peers.promote_to_pending(2000, 12345, 20);

        // Demote to Identified
        assert!(peers.demote_to_identified(2000, 30));

        // Check state is Identified
        let peer = peers.peers.get(&2000).unwrap();
        assert!(peer.state.is_identified());
    }

    #[test]
    fn test_update_keepalive() {
        let mut peers = create_test_peers(1000);

        // Add Connected peer
        peers.update_peer(&2000, 10);

        // Update keepalive
        assert!(peers.update_keepalive(2000, 50));

        // Check timestamp was updated
        let peer = peers.peers.get(&2000).unwrap();
        if let PeerState::Connected { last_keepalive, .. } = peer.state {
            assert_eq!(last_keepalive, 50);
        } else {
            panic!("Expected Connected state");
        }

        // Cannot update keepalive for non-Connected peer
        peers.add_identified_peer(3000, 10);
        assert!(!peers.update_keepalive(3000, 50));
    }

    // ========================================================================
    // Timeout Detection Tests
    // ========================================================================

    #[test]
    fn test_detect_pending_timeouts() {
        let mut peers = create_test_peers(1000);

        // Add Pending peer at time 10
        peers.add_identified_peer(2000, 5);
        peers.promote_to_pending(2000, 12345, 10);

        // No timeout at time 19 (just under threshold)
        let timed_out = peers.detect_pending_timeouts(19);
        assert_eq!(timed_out.len(), 0);
        assert!(peers.peers.get(&2000).unwrap().state.is_pending());

        // Timeout at time 20 (10 + pending_timeout=10)
        let timed_out = peers.detect_pending_timeouts(20);
        assert_eq!(timed_out.len(), 1);
        assert!(timed_out.contains(&2000));

        // Peer should now be Identified
        assert!(peers.peers.get(&2000).unwrap().state.is_identified());
    }

    #[test]
    fn test_detect_connection_timeouts() {
        let mut peers = create_test_peers(1000);

        // Add Connected peer at time 10
        peers.update_peer(&2000, 10);

        // No timeout at time 309 (just under threshold)
        let timed_out = peers.detect_connection_timeouts(309);
        assert_eq!(timed_out.len(), 0);
        assert!(peers.peers.get(&2000).unwrap().state.is_connected());

        // Timeout at time 310 (10 + connection_timeout=300)
        let timed_out = peers.detect_connection_timeouts(310);
        assert_eq!(timed_out.len(), 1);
        assert!(timed_out.contains(&2000));

        // Peer should now be Identified
        assert!(peers.peers.get(&2000).unwrap().state.is_identified());
        // Should not be in active list
        assert!(!peers.active.contains(&2000));
    }

    #[test]
    fn test_multiple_timeouts() {
        let mut peers = create_test_peers(1000);

        // Add multiple peers with different timestamps
        peers.add_identified_peer(2000, 0);
        peers.promote_to_pending(2000, 12345, 5);

        peers.add_identified_peer(3000, 0);
        peers.promote_to_pending(3000, 12346, 10);

        peers.update_peer(&4000, 15);

        // At time 20:
        // - Peer 2000: Pending since 5 -> timed out (15 > 10)
        // - Peer 3000: Pending since 10 -> timed out (10 >= 10)
        // - Peer 4000: Connected since 15 -> not timed out (5 < 300)

        let pending_timed_out = peers.detect_pending_timeouts(20);
        assert_eq!(pending_timed_out.len(), 2);
        assert!(pending_timed_out.contains(&2000));
        assert!(pending_timed_out.contains(&3000));

        let connection_timed_out = peers.detect_connection_timeouts(20);
        assert_eq!(connection_timed_out.len(), 0);
    }

    #[test]
    fn test_tick_calls_timeout_detection() {
        let mut peers = create_test_peers(1000);
        let token_storage = MemTokens::new();

        // Add peers
        peers.add_identified_peer(2000, 0);
        peers.promote_to_pending(2000, 12345, 5);
        peers.update_peer(&3000, 10);

        // Call tick at time 20
        peers.tick(&token_storage, 20);

        // Pending peer should have timed out
        assert!(peers.peers.get(&2000).unwrap().state.is_identified());

        // Connected peer should still be connected
        assert!(peers.peers.get(&3000).unwrap().state.is_connected());
    }

    // ========================================================================
    // Election State and Caching Tests
    // ========================================================================

    #[test]
    fn test_election_state_running() {
        let election = OngoingElection::new(
            PeerElection::new(12345, 1000, ElectionConfig::default()),
            10,
            ElectionPurpose::Bootstrap,
        );

        assert!(matches!(election.state, ElectionState::Running));
        assert!(!election.is_cache_valid(20, 300));
        assert_eq!(election.get_winner(), None);
    }

    #[test]
    fn test_election_complete_and_cache() {
        let mut election = OngoingElection::new(
            PeerElection::new(12345, 1000, ElectionConfig::default()),
            10,
            ElectionPurpose::Bootstrap,
        );

        // Complete election
        election.complete(2000, 20);

        assert!(matches!(election.state, ElectionState::Completed { .. }));
        assert_eq!(election.get_winner(), Some(2000));

        // Cache is valid within TTL
        assert!(election.is_cache_valid(220, 300)); // 20 + 200 < 20 + 300

        // Cache expires after TTL
        assert!(!election.is_cache_valid(320, 300)); // 20 + 300 = 320
    }

    #[test]
    fn test_election_timeout() {
        let mut election = OngoingElection::new(
            PeerElection::new(12345, 1000, ElectionConfig::default()),
            10,
            ElectionPurpose::Bootstrap,
        );

        // Timeout election
        election.timeout(18);

        assert!(matches!(election.state, ElectionState::TimedOut { .. }));
        assert_eq!(election.get_winner(), None);
        assert!(!election.is_cache_valid(50, 300));
    }

    // ========================================================================
    // Phase 3: Election Management Tests
    // ========================================================================

    #[test]
    fn test_start_election() {
        let mut peers = create_test_peers(1000);

        // Add some connected peers
        peers.update_peer(&2000, 0);
        peers.update_peer(&3000, 0);
        peers.update_peer(&4000, 0);

        // Start election
        let challenge_token = 12345;
        let actions = peers.start_election(challenge_token, 10);

        // Should have created election
        assert!(peers.active_elections.contains_key(&challenge_token));

        // Should have spawned some channels (depends on connected peers)
        assert!(actions.len() > 0);
    }

    #[test]
    fn test_start_election_duplicate() {
        let mut peers = create_test_peers(1000);
        peers.update_peer(&2000, 0);

        let challenge_token = 12345;

        // Start first election
        let actions1 = peers.start_election(challenge_token, 10);
        assert!(actions1.len() > 0);

        // Try to start duplicate election
        let actions2 = peers.start_election(challenge_token, 20);
        assert_eq!(actions2.len(), 0); // Should not create duplicate
    }

    #[test]
    fn test_pick_challenge_token() {
        let peers = create_test_peers(1000);

        // Pick 100 tokens - they should be random
        let mut tokens = Vec::new();
        for _ in 0..100 {
            tokens.push(peers.pick_challenge_token());
        }

        // Should have variety (not all the same)
        let unique_count = tokens.iter().collect::<std::collections::HashSet<_>>().len();
        assert!(unique_count > 50, "Expected variety in challenge tokens");
    }

    #[test]
    fn test_pick_peer_for_reelection() {
        let mut peers = create_test_peers(1000);

        // No peers - should return None
        assert_eq!(peers.pick_peer_for_reelection(), None);

        // Add some connected peers
        peers.update_peer(&2000, 0);
        peers.update_peer(&3000, 0);
        peers.update_peer(&4000, 0);

        // Should pick one of them
        let selected = peers.pick_peer_for_reelection();
        assert!(selected.is_some());
        let peer = selected.unwrap();
        assert!(peer == 2000 || peer == 3000 || peer == 4000);
    }

    #[test]
    fn test_pick_token_near_peer() {
        let peers = create_test_peers(1000);

        // Pick token near peer 5000
        let token = peers.pick_token_near_peer(5000);

        // Should be within 1000 of peer
        let dist = EcPeers::ring_distance(5000, token);
        assert!(dist < 1000, "Token {} should be within 1000 of peer 5000 (distance: {})", token, dist);
    }

    #[test]
    fn test_handle_election_success_new_peer() {
        let mut peers = create_test_peers(1000);
        let token_storage = MemTokens::new();

        // Handle success for unknown peer
        let _actions = peers.handle_election_success(&token_storage, 12345, 2000, 10);

        // Peer should be added in Pending state
        assert!(peers.peers.contains_key(&2000));
        let peer = peers.peers.get(&2000).unwrap();
        assert!(peer.state.is_pending());
    }

    #[test]
    fn test_handle_election_success_existing_connected() {
        let mut peers = create_test_peers(1000);
        let token_storage = MemTokens::new();

        // Add peer as Connected
        peers.update_peer(&2000, 0);

        // Handle election success
        let _actions = peers.handle_election_success(&token_storage, 12345, 2000, 10);

        // Should update stats
        let peer = peers.peers.get(&2000).unwrap();
        if let PeerState::Connected { election_wins, election_attempts, .. } = peer.state {
            assert_eq!(election_wins, 1);
            assert_eq!(election_attempts, 1);
        } else {
            panic!("Expected Connected state");
        }
    }

    #[test]
    fn test_handle_election_success_ignores_self() {
        let mut peers = create_test_peers(1000);
        let token_storage = MemTokens::new();

        // Try to add self as winner
        let _actions = peers.handle_election_success(&token_storage, 12345, 1000, 10);

        // Should not add self
        assert_eq!(peers.peers.len(), 0);
    }

    #[test]
    fn test_evict_worst_peer() {
        let mut peers = create_test_peers(1000);

        // Add peers with different quality scores
        peers.update_peer(&2000, 0);
        peers.update_peer(&3000, 0);
        peers.update_peer(&4000, 0);

        // Manually set quality scores
        if let Some(peer) = peers.peers.get_mut(&2000) {
            if let PeerState::Connected { quality_score, .. } = &mut peer.state {
                *quality_score = 0.9; // High quality
            }
        }

        if let Some(peer) = peers.peers.get_mut(&3000) {
            if let PeerState::Connected { quality_score, .. } = &mut peer.state {
                *quality_score = 0.3; // Low quality (worst)
            }
        }

        if let Some(peer) = peers.peers.get_mut(&4000) {
            if let PeerState::Connected { quality_score, .. } = &mut peer.state {
                *quality_score = 0.7; // Medium quality
            }
        }

        // Evict worst peer
        peers.evict_worst_peer(10);

        // Peer 3000 (worst quality) should be demoted
        let peer3000 = peers.peers.get(&3000).unwrap();
        assert!(peer3000.state.is_identified());
        assert!(!peers.active.contains(&3000));

        // Other peers should still be connected
        assert!(peers.peers.get(&2000).unwrap().state.is_connected());
        assert!(peers.peers.get(&4000).unwrap().state.is_connected());
    }

    #[test]
    fn test_tick_triggers_election() {
        let mut peers = create_test_peers(1000);
        let token_storage = MemTokens::new();

        // Add connected peers
        peers.update_peer(&2000, 0);
        peers.update_peer(&3000, 0);

        // Set next election time to now
        peers.next_election_time = 100;

        // Call tick at time 100
        peers.tick(&token_storage, 100);

        // Should have started an election
        assert!(!peers.active_elections.is_empty());

        // Next election time should be updated
        assert_eq!(peers.next_election_time, 100 + peers.config.election_interval);
    }

    #[test]
    fn test_tick_does_not_trigger_election_too_early() {
        let mut peers = create_test_peers(1000);
        let token_storage = MemTokens::new();

        // Add connected peers
        peers.update_peer(&2000, 0);

        // Set next election time to future
        peers.next_election_time = 100;

        // Call tick before next election time
        peers.tick(&token_storage, 50);

        // Should not have started an election
        assert!(peers.active_elections.is_empty());
    }

    #[test]
    fn test_trigger_next_election_discovery() {
        let mut peers = create_test_peers(1000);
        peers.update_peer(&2000, 0);

        // Set random seed to always pick discovery (not re-election)
        // Note: This is probabilistic, but should work most of the time

        let initial_count = peers.active_elections.len();

        // Trigger election multiple times
        for i in 0..10 {
            peers.trigger_next_election(i * 10);
        }

        // Should have created some elections
        assert!(peers.active_elections.len() > initial_count);
    }

    #[test]
    fn test_election_eviction_on_budget_overflow() {
        let mut config = PeerManagerConfig::default();
        config.total_budget = 3; // Very small budget

        let mut peers = create_test_peers_with_config(1000, config);
        let token_storage = MemTokens::new();

        // Add 3 connected peers (at budget limit)
        peers.update_peer(&2000, 0);
        peers.update_peer(&3000, 0);
        peers.update_peer(&4000, 0);

        // Set quality scores
        if let Some(peer) = peers.peers.get_mut(&2000) {
            if let PeerState::Connected { quality_score, .. } = &mut peer.state {
                *quality_score = 0.9;
            }
        }
        if let Some(peer) = peers.peers.get_mut(&3000) {
            if let PeerState::Connected { quality_score, .. } = &mut peer.state {
                *quality_score = 0.2; // Worst
            }
        }
        if let Some(peer) = peers.peers.get_mut(&4000) {
            if let PeerState::Connected { quality_score, .. } = &mut peer.state {
                *quality_score = 0.7;
            }
        }

        // Handle election success for new peer (adds as Pending, doesn't trigger eviction yet)
        let _actions = peers.handle_election_success(&token_storage, 12345, 5000, 10);

        // New peer should be added in Pending (not Connected yet)
        assert!(peers.peers.contains_key(&5000));
        let peer5000 = peers.peers.get(&5000).unwrap();
        assert!(peer5000.state.is_pending());

        // All 3 original peers should still be connected (no eviction yet)
        assert_eq!(peers.num_connected(), 3);

        // Now promote peer 5000 to Connected (simulates mutual invitation)
        peers.promote_to_connected(5000, 20);

        // Should have 4 connected peers now, which is over budget
        // Eviction should happen (but we need to manually trigger it or call tick)
        // For this test, manually evict
        peers.evict_worst_peer(20);

        // Should have evicted worst peer (3000)
        let peer3000 = peers.peers.get(&3000).unwrap();
        assert!(peer3000.state.is_identified());
        assert!(!peers.active.contains(&3000));

        // Now should be at budget (3 connected)
        assert_eq!(peers.num_connected(), 3);
    }

    #[test]
    fn test_spawn_election_channels_respects_active_peers() {
        let mut peers = create_test_peers(1000);

        // Add peers, but only some are connected
        peers.add_identified_peer(2000, 0); // Not connected
        peers.update_peer(&3000, 0); // Connected
        peers.update_peer(&4000, 0); // Connected

        // Start election
        let challenge_token = 12345;
        peers.start_election(challenge_token, 10);

        // Should only use connected peers for channels
        let election = peers.active_elections.get(&challenge_token).unwrap();
        let channel_count = election.election.channel_count();

        // Should have at least 1 channel (from connected peers)
        assert!(channel_count > 0);
    }
}
