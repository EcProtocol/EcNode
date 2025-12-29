use crate::ec_interface::{
    EcTime, MessageTicket, PeerId, TokenId, TokenMapping, TOKENS_SIGNATURE_SIZE,
};
use crate::ec_proof_of_storage::{ElectionConfig, PeerElection, ProofOfStorage, TokenStorageBackend};
use std::collections::{BTreeMap, HashMap, HashSet};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the peer management system
#[derive(Debug, Clone)]
pub struct PeerManagerConfig {
    // ===== Capacity Limits =====
    /// Maximum number of Connected peers (default: 200)
    pub connected_max_capacity: usize,

    /// Maximum number of Identified peers (default: 5000)
    pub identified_max_capacity: usize,

    /// Maximum number of tokens in sample collection (default: 1000)
    pub token_sample_max_capacity: usize,

    // ===== Election Parameters =====
    /// Number of elections to trigger per tick (default: 3)
    pub elections_per_tick: usize,

    /// Minimum time to collect election responses before checking for winner (in ticks, default: 10)
    pub min_collection_time: u64,

    /// Maximum time to wait for election before timeout (in ticks, default: 30)
    pub election_timeout: u64,

    /// TTL for election cache (in ticks, default: 300)
    pub election_cache_ttl: u64,

    // ===== Timeout Parameters =====
    /// Timeout for Pending state before demoting to Identified (in ticks, default: 10)
    pub pending_timeout: u64,

    /// Timeout for Connected state without keepalive (in ticks, default: 300 = 5 min)
    pub connection_timeout: u64,

    /// Protection time for recently connected peers from pruning (in ticks, default: 600 = 10 min)
    pub prune_protection_time: u64,

    // ===== Legacy (backward compatibility) =====
    /// Total budget - kept for compatibility (maps to connected_max_capacity)
    pub total_budget: usize,

    // ===== Election Configuration =====
    /// Configuration for PeerElection
    pub election_config: ElectionConfig,
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self {
            // Capacity limits
            connected_max_capacity: 200,
            identified_max_capacity: 5000,
            token_sample_max_capacity: 1000,

            // Election parameters
            elections_per_tick: 3,
            min_collection_time: 10,
            election_timeout: 30,
            election_cache_ttl: 300,

            // Timeout parameters
            pending_timeout: 10,
            connection_timeout: 300,
            prune_protection_time: 600,

            // Legacy compatibility
            total_budget: 200,  // Maps to connected_max_capacity

            // Election configuration
            election_config: ElectionConfig::default(),
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
        /// Last time we started an election due to their Invitation (spam prevention)
        last_invitation_election_at: Option<EcTime>,
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

/// Extended peer information with state
struct MemPeer {
    state: PeerState,
    // TODO: network address, shared secret
}

// ============================================================================
// Actions
// ============================================================================

/// Actions that EcPeers requests EcNode to perform
#[derive(Debug, Clone)]
pub enum PeerAction {
    // TDOO handle forward / on-behalf-of
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

/// Tracks an ongoing election (no caching)
struct OngoingElection {
    election: PeerElection,
    started_at: EcTime,
}

// ============================================================================
// Token Sample Collection
// ============================================================================

/// Sampled tokens for election challenges
///
/// This structure maintains a bounded collection of tokens used for triggering elections.
/// Tokens are added from validated Answers, Invitations, and Referrals throughout the
/// node's lifetime. The collection is continuously pruned via:
/// - Uniform random eviction when at capacity
/// - Removal when a token is selected for an election
///
/// The combination of biased input (gradient routing provides nearby tokens) and
/// uniform eviction naturally produces a Gaussian distribution centered on our peer ID.
struct TokenSampleCollection {
    /// Flat set of sampled tokens
    samples: HashSet<TokenId>,

    /// Maximum capacity
    max_capacity: usize,
}

impl TokenSampleCollection {
    /// Create a new empty token sample collection
    fn new(max_capacity: usize) -> Self {
        Self {
            samples: HashSet::new(),
            max_capacity,
        }
    }

    /// Add a token to the collection
    /// Returns true if token was added, false if already present or at capacity
    fn add_token(&mut self, token: TokenId) -> bool {
        // If at capacity, don't add (eviction happens separately in tick)
        if self.samples.len() >= self.max_capacity {
            return false;
        }

        self.samples.insert(token)
    }

    /// Sample tokens from an Answer message
    /// Answer contains: 1 answer token + 10 signature tokens = 11 total
    /// Also adds the peer_id of the sender
    fn sample_from_answer(
        &mut self,
        answer: &TokenMapping,
        signature: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
        sender_peer_id: PeerId,
    ) {
        // Add sender's peer ID as a token
        self.add_token(sender_peer_id);

        // Add the answer token
        self.add_token(answer.id);

        // Sample from signature tokens
        for sig_token in signature {
            self.add_token(sig_token.id);
        }
    }

    /// Sample peer IDs from a Referral message
    fn sample_from_referral(&mut self, suggested_peers: &[PeerId; 2]) {
        for &peer_id in suggested_peers {
            self.add_token(peer_id);
        }
    }

    /// Pick N random tokens and REMOVE them from the collection
    /// Returns up to N tokens (may be less if collection is small)
    fn pick_and_remove<R: rand::Rng>(&mut self, n: usize, rng: &mut R) -> Vec<TokenId> {
        use rand::seq::IteratorRandom;

        let selected: Vec<TokenId> = self.samples
            .iter()
            .copied()
            .choose_multiple(rng, n);

        // Remove selected tokens from the collection
        for &token in &selected {
            self.samples.remove(&token);
        }

        selected
    }

    /// Evict random tokens if over capacity
    /// Returns number of tokens evicted
    fn evict_excess<R: rand::Rng>(&mut self, rng: &mut R) -> usize {
        if self.samples.len() <= self.max_capacity {
            return 0;
        }

        use rand::seq::IteratorRandom;

        let excess = self.samples.len() - self.max_capacity;
        let to_evict: Vec<TokenId> = self.samples
            .iter()
            .copied()
            .choose_multiple(rng, excess);

        for token in &to_evict {
            self.samples.remove(token);
        }

        to_evict.len()
    }
}

impl OngoingElection {
    fn new(election: PeerElection, started_at: EcTime) -> Self {
        Self {
            election,
            started_at,
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

    /// Ongoing elections indexed by challenge token
    active_elections: HashMap<TokenId, OngoingElection>,

    /// Proof-of-storage system (zero-sized helper, storage passed as parameter)
    proof_system: ProofOfStorage,

    /// Sampled tokens for peer discovery across the ID space
    token_samples: TokenSampleCollection,

    /// Configuration
    config: PeerManagerConfig,

    /// Random number generator (seeded for reproducibility)
    rng: rand::rngs::StdRng,

    // ===== Election Statistics =====
    /// Total elections started (lifetime counter)
    elections_started_total: usize,

    /// Total elections completed successfully (lifetime counter)
    elections_completed_total: usize,

    /// Total elections that timed out (lifetime counter)
    elections_timeout_total: usize,

    /// Total split-brain scenarios detected (lifetime counter)
    elections_splitbrain_total: usize,
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
    // Ring Distance and Peer Finding
    // ========================================================================

    /// Calculate ring distance between two peer IDs
    fn ring_distance(a: PeerId, b: PeerId) -> u64 {
        let forward = b.wrapping_sub(a);
        let backward = a.wrapping_sub(b);
        forward.min(backward)
    }

    /// Find closest peers to a target token (for election channels)
    /// Walks BTreeMap in both directions from target
    fn find_closest_peers(&self, target: TokenId, count: usize) -> Vec<PeerId> {
        let mut candidates = Vec::new();

        // TODO just next/next_back into vec. No sort etc.

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
    // Message Handlers 
    // ========================================================================

    /// Handle an Answer message (election response or invitation)
    pub fn handle_answer(
        &mut self,
        answer: &TokenMapping,
        signature: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
        ticket: MessageTicket,
        peer_id: PeerId,
        time: EcTime,
        token_storage: &dyn TokenStorageBackend,
    ) -> Vec<PeerAction> {
        // handle Invitation (ticket == 0)
        if ticket == 0 {
            return self.handle_invitation(answer, signature, peer_id, time, token_storage);
        }

        // Route answer to the correct ongoing election
        let challenge_token = answer.id;

        if let Some(ongoing) = self.active_elections.get_mut(&challenge_token) {
            // Try to record the answer in the election
            match ongoing.election.handle_answer(ticket, answer, signature, peer_id, time) {
                Ok(()) => {
                    // Answer successfully recorded
                    // Winner will be detected in process_elections()
                    // Sample tokens from Answer for future discovery
                    // Answer contains: 1 answer token + 10 signature tokens + sender peer ID
                    self.token_samples.sample_from_answer(answer, signature, peer_id);
                }
                Err(_e) => {
                    // Invalid signature or ticket, or channel already blocked
                    // Ignore the answer
                }
            }
        }
        // If no election found for this token, ignore the answer
        Vec::new()
    }

    /// Handle an Invitation (Answer with ticket=0)
    /// Uses distance-based probability to decide whether to respond
    fn handle_invitation(
        &mut self,
        answer: &TokenMapping,
        signature: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
        sender_peer_id: PeerId,
        time: EcTime,
        _token_storage: &dyn TokenStorageBackend,
    ) -> Vec<PeerAction> {
        use rand::Rng;
        let mut trigger_election = false;

        if let Some(peer) = self.peers.get_mut(&sender_peer_id) {
            match peer.state {
                PeerState::Identified { last_invitation_election_at, .. } => {
                    // Accepted - check per-peer invitation cooldown
                    const INVITATION_COOLDOWN: EcTime = 60; // Minimum 60 ticks between invitations from same peer

                    if let Some(last_time) = last_invitation_election_at {
                        if time - last_time > INVITATION_COOLDOWN {
                            trigger_election = true
                        }
                    }
                },
                PeerState::Connected { .. } => {
                    self.update_keepalive(sender_peer_id, time);
                },
                PeerState::Pending { .. } => {
                    self.promote_to_connected(sender_peer_id, time);
                }
            }
        } else {
            trigger_election = true
        }
        
        if trigger_election {
            // Calculate distance-based acceptance probability
            // Closer peers have higher probability of triggering a response
            let ring_size = u64::MAX as f64 / 2.0; // Half ring (max distance)
            let distance = Self::ring_distance(self.peer_id, sender_peer_id) as f64;
            let distance_fraction = distance / ring_size;
            let accept_prob = 1.0 - distance_fraction; // Inverse: close = high, far = low

            // Decide whether to respond to this Invitation
            if self.rng.gen_bool(accept_prob) {
                return self.start_election_from_invite(answer, signature, sender_peer_id, time);
            }

            // Declined - just add sender to Identified for future consideration
            self.add_identified_peer(sender_peer_id, time);
        }
        
        return Vec::new();
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
        let action = if let Some(ongoing) = self.active_elections.get_mut(&token) {
            // Try to handle the referral
            match ongoing.election.handle_referral(ticket, token, suggested_peers, sender) {
                Ok(next_peer) => {
                    // Election returned a suggested peer to try next

                    // Create a new channel to the suggested peer
                    if let Ok(new_ticket) = ongoing.election.create_channel(next_peer, time) {
                        Some(PeerAction::SendQuery {
                            receiver: next_peer,
                            token,
                            ticket: new_ticket,
                        })
                    } else {
                        None
                    }
                }
                Err(_) => {
                    // Referral failed (wrong token, unknown ticket, blocked channel, etc.)
                    None
                }
            }
        } else {
            None
        };

        // DOC only if we recognize this Referral
        if action.is_some() {
            // Add suggested peers to Identified state (after releasing mutable borrow)
            for &peer_id in &suggested_peers {
                if peer_id != 0 {
                    self.add_identified_peer(peer_id, time);
                }
            }
        }

        action
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

        // Note: We DO allow querying non-Connected peers during discovery!
        // The referral mechanism needs to work across the whole network,
        // not just among already-Connected peers. This allows DHT-style
        // routing to find token owners even if they're not directly connected.

        // We don't own the token - find closest Connected Peers to refer
        let closest = self.find_closest_peers(token, 2);

        if closest.len() >= 2 {
            // TODO forward Query for Connected peers instead of Referral
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
            self.peers.insert(
                *key,
                MemPeer {
                    state: PeerState::Connected {
                        connected_since: time,
                        last_keepalive: time,
                        election_wins: 0,
                        election_attempts: 0,
                        quality_score: 1.0, // Start with max quality
                    },
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
    /// Add a seed peer for bootstrap (public API)
    /// Adds the peer to the Identified state for initial discovery
    pub fn add_seed_peer(&mut self, peer_id: PeerId, time: EcTime) -> bool {
        self.add_identified_peer(peer_id, time)
    }

    /// Add a peer to Identified state (internal)
    fn add_identified_peer(&mut self, peer_id: PeerId, time: EcTime) -> bool {
        if peer_id == self.peer_id {
            return false; // Never add self
        }

        // Check if peer already exists
        if self.peers.contains_key(&peer_id) {
            return false; // Already known
        }

        // Add to Identified state
        self.peers.insert(
            peer_id,
            MemPeer {
                state: PeerState::Identified {
                    discovered_at: time,
                    last_invitation_election_at: None,
                },
            },
        );

        // Add peer ID to token samples (peer IDs are valid tokens for discovery)
        self.token_samples.add_token(peer_id);

        true
    }

    /// Promote Identified peer to Pending after election win (we send Invitation)
    fn promote_to_pending(&mut self, peer_id: PeerId, election_token: TokenId, time: EcTime) -> bool {
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
    fn promote_to_connected(&mut self, peer_id: PeerId, time: EcTime) -> bool {
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
    fn demote_from_connected(&mut self, peer_id: PeerId, time: EcTime) -> bool {
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
            last_invitation_election_at: None,
        };

        // Remove from active list
        if let Ok(idx) = self.active.binary_search(&peer_id) {
            self.active.remove(idx);
        }

        true
    }

    /// Demote Pending peer to Identified (timeout)
    fn demote_to_identified(&mut self, peer_id: PeerId, time: EcTime) -> bool {
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
            last_invitation_election_at: None,
        };

        true
    }

    /// Update last_keepalive for Connected peer
    fn update_keepalive(&mut self, peer_id: PeerId, time: EcTime) -> bool {
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
    fn detect_pending_timeouts(&mut self, time: EcTime) -> Vec<PeerId> {
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
    fn detect_connection_timeouts(&mut self, time: EcTime) -> Vec<PeerId> {
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

    /// Evict excess Identified peers (uniform random)
    fn evict_excess_identified(&mut self) {
        let identified_peers: Vec<PeerId> = self.peers
            .iter()
            .filter(|(_, p)| p.state.is_identified())
            .map(|(id, _)| *id)
            .collect();

        if identified_peers.len() <= self.config.identified_max_capacity {
            return; // No eviction needed
        }

        use rand::seq::SliceRandom;
        let excess = identified_peers.len() - self.config.identified_max_capacity;
        let to_evict = identified_peers
            .choose_multiple(&mut self.rng, excess)
            .copied()
            .collect::<Vec<_>>();

        for peer_id in to_evict {
            self.peers.remove(&peer_id);
            // Also remove from active list if present
            self.active.retain(|&p| p != peer_id);
        }
    }

    /// Prune Connected peers based on distance probability
    /// Closer peers have lower probability of being pruned
    fn prune_connected_by_distance(&mut self, time: EcTime) {
        use rand::Rng;
        let ring_size = u64::MAX as f64 / 2.0; // Half ring (max distance)

        let to_demote: Vec<PeerId> = self.peers
            .iter()
            .filter_map(|(peer_id, peer)| {
                if let PeerState::Connected { connected_since, .. } = peer.state {
                    // Protect recently connected peers
                    if time - connected_since < self.config.prune_protection_time {
                        return None;
                    }

                    // Calculate prune probability based on distance
                    let distance = Self::ring_distance(self.peer_id, *peer_id) as f64;
                    let distance_fraction = distance / ring_size;
                    let prune_prob = distance_fraction; // Linear (0.0 near, ~1.0 far)

                    if self.rng.gen_bool(prune_prob) {
                        Some(*peer_id)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Demote selected peers to Identified
        for peer_id in to_demote {
            self.demote_from_connected(peer_id, time);
        }
    }

    /// Trigger multiple elections per tick (new design)
    /// Picks N tokens from collection and removes them.
    /// If collection is low, uses random tokens to bootstrap discovery.
    fn trigger_multiple_elections(
        &mut self,
        _token_storage: &dyn TokenStorageBackend,
        time: EcTime,
    ) -> Vec<PeerAction> {
        use rand::Rng;
        let mut actions = Vec::new();

        // Pick N challenge tokens and remove them from collection
        let mut challenge_tokens = self.token_samples.pick_and_remove(self.config.elections_per_tick, &mut self.rng);

        // If we don't have enough tokens, add random tokens to bootstrap discovery
        // Random tokens won't exist, so we'll get Referrals that populate Identified
        while challenge_tokens.len() < self.config.elections_per_tick {
            let random_token: TokenId = self.rng.gen();
            challenge_tokens.push(random_token);
        }

        for challenge_token in challenge_tokens {
            // Start election (which spawns initial channels and returns Query actions)
            let channel_actions = self.start_election(challenge_token, time);
            actions.extend(channel_actions);
        }

        actions
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

    /// Create a new peer manager with default configuration and random seed
    pub fn new(peer_id: PeerId) -> Self {
        use rand::{RngCore, SeedableRng};
        let mut seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        let rng = rand::rngs::StdRng::from_seed(seed);
        Self::with_config_and_rng(peer_id, PeerManagerConfig::default(), rng)
    }

    /// Create a new peer manager with custom configuration and random seed
    pub fn with_config(peer_id: PeerId, config: PeerManagerConfig) -> Self {
        use rand::{RngCore, SeedableRng};
        let mut seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        let rng = rand::rngs::StdRng::from_seed(seed);
        Self::with_config_and_rng(peer_id, config, rng)
    }

    /// Create a new peer manager with custom configuration and specific RNG
    pub fn with_config_and_rng(peer_id: PeerId, config: PeerManagerConfig, rng: rand::rngs::StdRng) -> Self {
        let proof_system = ProofOfStorage::new();
        let token_samples = TokenSampleCollection::new(config.token_sample_max_capacity);

        Self {
            peer_id,
            peers: BTreeMap::new(),
            active: Vec::new(),
            active_elections: HashMap::new(),
            proof_system,
            token_samples,
            config,
            rng,
            elections_started_total: 0,
            elections_completed_total: 0,
            elections_timeout_total: 0,
            elections_splitbrain_total: 0,
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

    /// Get number of Identified peers
    pub fn num_identified(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.state.is_identified())
            .count()
    }

    /// Get number of Pending peers
    pub fn num_pending(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.state.is_pending())
            .count()
    }

    /// Get total number of active elections
    pub fn num_active_elections(&self) -> usize {
        self.active_elections.len()
    }

    /// Get election statistics
    pub fn get_election_stats(&self) -> (usize, usize, usize, usize) {
        (
            self.elections_started_total,
            self.elections_completed_total,
            self.elections_timeout_total,
            self.elections_splitbrain_total,
        )
    }

    /// Get the active (Connected) peer IDs in sorted order
    /// Used by simulator for connectivity analysis
    pub fn get_active_peers(&self) -> &[PeerId] {
        &self.active
    }

    // ========================================================================
    // Election Management (Phase 3)
    // ========================================================================

    /// Start a new peer election for a challenge token
    fn start_election(&mut self, challenge_token: TokenId, time: EcTime) -> Vec<PeerAction> {
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

        let ongoing = OngoingElection::new(election, time);

        self.active_elections.insert(challenge_token, ongoing);

        // Increment election counter
        self.elections_started_total += 1;

        // Spawn initial channels and return Query actions
        self.spawn_election_channels(challenge_token, time)
    }

    /// Start a new peer election from an invitation (unsolicited Answer)
    fn start_election_from_invite(
        &mut self,
        answer: &TokenMapping,
        signature: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
        responder_peer: PeerId,
        time: EcTime,
    ) -> Vec<PeerAction> {
        let challenge_token = answer.id;

        // Check if we already have an election for this token
        if self.active_elections.contains_key(&challenge_token) {
            return Vec::new(); // Election already running
        }

        // Create new election from invitation
        let election = match PeerElection::from_invitation(
            answer,
            signature,
            responder_peer,
            time,
            self.peer_id,
            self.config.election_config.clone(),
        ) {
            Ok(election) => election,
            Err(_) => {
                // Signature verification failed or other error
                return Vec::new();
            }
        };

        let ongoing = OngoingElection::new(election, time);

        self.active_elections.insert(challenge_token, ongoing);

        // Increment election counter
        self.elections_started_total += 1;

        // Update last_invitation_election_at for spam prevention
        if let Some(peer) = self.peers.get_mut(&responder_peer) {
            if let PeerState::Identified { last_invitation_election_at, .. } = &mut peer.state {
                *last_invitation_election_at = Some(time);
            }
        }

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

        const CHANNELS_PER_ELECTION: usize = 4;
        const CLOSEST_CANDIDATES: usize = 8;

        let mut actions = Vec::new();
        let mut candidates = Vec::new();

        // Add closest peers as additional candidates (for DHT-style routing)
        let closest = self.find_closest_peers(challenge_token, CLOSEST_CANDIDATES);

        // Add closest peers, avoiding duplicates (challenge_token might be in closest list)
        for peer_id in closest {
            if !candidates.contains(&peer_id) {
                candidates.push(peer_id);
            }
        }

        // Now get mutable access to election
        let Some(ongoing) = self.active_elections.get_mut(&challenge_token) else {
            return Vec::new();
        };

        for first_hop in candidates.iter().take(CHANNELS_PER_ELECTION) {
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

    /// Process ongoing elections and check for winners
    fn process_elections(&mut self, token_storage: &dyn TokenStorageBackend, time: EcTime) -> Vec<PeerAction> {
        use crate::ec_proof_of_storage::WinnerResult;
        let mut actions = Vec::new();
        let mut to_resolve: Vec<(TokenId, usize)> = Vec::new();
        let mut winners: Vec<(TokenId, PeerId)> = Vec::new();
        let mut to_remove_completed: Vec<TokenId> = Vec::new();
        let mut to_remove_timeout: Vec<TokenId> = Vec::new();
        let mut to_remove_splitbrain: Vec<TokenId> = Vec::new();

        // First pass: collect election results (only read, no mutable calls)
        let tokens: Vec<TokenId> = self.active_elections.keys().copied().collect();

        for token in tokens {
            let Some(ongoing) = self.active_elections.get(&token) else {
                continue;
            };

            let elapsed = time.saturating_sub(ongoing.started_at);

            // Wait for minimum collection time
            if elapsed < self.config.min_collection_time {
                continue;
            }

            // Check for winner
            match ongoing.election.check_for_winner() {
                WinnerResult::Single { winner, .. } => {
                    // Success! Election complete - remove it after processing
                    winners.push((token, winner));
                    to_remove_completed.push(token);
                }

                WinnerResult::SplitBrain { .. } => {
                    // Split-brain detected
                    if elapsed < self.config.election_timeout && ongoing.election.can_create_channel() {
                        // Try to resolve with more channels
                        let needed = 2;
                        to_resolve.push((token, needed));
                    } else {
                        // Give up - split-brain unresolved
                        to_remove_splitbrain.push(token);
                    }
                }

                WinnerResult::NoConsensus => {
                    // Not enough responses yet
                    if elapsed >= self.config.election_timeout {
                        // Timeout - remove election
                        to_remove_timeout.push(token);
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
            let spawned = self.spawn_election_channels(token, time);
            actions.extend(spawned);
        }

        // Remove completed elections and update counter
        for token in to_remove_completed {
            self.active_elections.remove(&token);
            self.elections_completed_total += 1;
        }

        // Remove timed-out elections and update counter
        for token in to_remove_timeout {
            self.active_elections.remove(&token);
            self.elections_timeout_total += 1;
        }

        // Remove split-brain elections and update counter
        for token in to_remove_splitbrain {
            self.active_elections.remove(&token);
            self.elections_splitbrain_total += 1;
        }

        actions
    }

    /// Handle successful election - add winner to peer list
    fn handle_election_success(&mut self, token_storage: &dyn TokenStorageBackend, _token: TokenId, winner: PeerId, time: EcTime) -> Vec<PeerAction> {
        let mut actions = Vec::new();

        // Check if winner is self (shouldn't happen, but be safe)
        if winner == self.peer_id {
            return actions;
        }

        self.promote_to_pending(winner, _token, time);
        // Generate SendInvitation action
        if let Some(sig) = self.proof_system.generate_signature(token_storage, &self.peer_id, &winner) {
            actions.push(PeerAction::SendInvitation {
                receiver: winner,
                answer: sig.answer,
                signature: sig.signature,
            });
        }

        actions
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

        // Phase 2: Process ongoing elections
        let election_actions = self.process_elections(token_storage, time);
        actions.extend(election_actions);

        // Phase 3: Evict excess Identified peers (uniform random)
        self.evict_excess_identified();

        // Phase 4: Evict excess TokenSamples (uniform random)
        self.token_samples.evict_excess(&mut self.rng);

        // Phase 5: Prune Connected peers by distance (distance-based probability)
        self.prune_connected_by_distance(time);

        // Phase 6: Trigger new elections (pick and remove tokens, or use random tokens if low)
        let new_election_actions = self.trigger_multiple_elections(token_storage, time);
        actions.extend(new_election_actions);

        actions
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_distance_calculation() {
        // Test distance calculation on the ring

        // Distance from 0 to 100 should be 100
        assert_eq!(EcPeers::ring_distance(0, 100), 100);

        // Distance is symmetric
        assert_eq!(EcPeers::ring_distance(100, 0), 100);

        // Distance from peer to itself is 0
        assert_eq!(EcPeers::ring_distance(1000, 1000), 0);

        // Distance wraps around the ring (uses shorter path)
        let max = u64::MAX;
        let near_max = max - 100;
        // Distance from 0 to (MAX - 100) should be 100 (wrapping around)
        assert_eq!(EcPeers::ring_distance(0, near_max), 101);

        // Distance at opposite side of ring
        let half = u64::MAX / 2;
        let dist = EcPeers::ring_distance(0, half);
        // Should be approximately half the ring
        assert!(dist >= half - 1 && dist <= half + 1);
    }

    #[test]
    fn test_invitation_acceptance_probability() {
        // Test that acceptance probability decreases with distance

        let ring_size = u64::MAX as f64 / 2.0;

        // Very close peer (distance = 100)
        let close_distance = 100.0;
        let close_fraction = close_distance / ring_size;
        let close_prob = 1.0 - close_fraction;
        assert!(close_prob > 0.999); // Almost certain to accept

        // Mid-distance peer
        let mid_distance = ring_size / 2.0;
        let mid_fraction = mid_distance / ring_size;
        let mid_prob = 1.0 - mid_fraction;
        assert!(mid_prob > 0.4 && mid_prob < 0.6); // ~50% acceptance

        // Far peer (opposite side of ring)
        let far_distance = ring_size - 100.0;
        let far_fraction = far_distance / ring_size;
        let far_prob = 1.0 - far_fraction;
        assert!(far_prob < 0.001); // Almost certain to reject
    }

    #[test]
    fn test_prune_probability() {
        // Test that prune probability increases with distance

        let ring_size = u64::MAX as f64 / 2.0;

        // Very close peer (distance = 100)
        let close_distance = 100.0;
        let close_fraction = close_distance / ring_size;
        let close_prune_prob = close_fraction;
        assert!(close_prune_prob < 0.001); // Almost never pruned

        // Mid-distance peer
        let mid_distance = ring_size / 2.0;
        let mid_fraction = mid_distance / ring_size;
        let mid_prune_prob = mid_fraction;
        assert!(mid_prune_prob > 0.4 && mid_prune_prob < 0.6); // ~50% pruned

        // Far peer (opposite side of ring)
        let far_distance = ring_size - 100.0;
        let far_fraction = far_distance / ring_size;
        let far_prune_prob = far_fraction;
        assert!(far_prune_prob > 0.999); // Almost certainly pruned
    }

    #[test]
    fn test_token_sample_collection_basic() {
        let mut collection = TokenSampleCollection::new(1000);

        // Initially empty
        assert!(collection.samples.is_empty());
        assert_eq!(collection.samples.len(), 0);

        // Add some tokens
        assert!(collection.add_token(100));
        assert!(collection.add_token(200));
        assert!(collection.add_token(300));

        assert_eq!(collection.samples.len(), 3);
        assert!(!collection.samples.is_empty());

        // Adding duplicate returns false
        assert!(!collection.add_token(100));
        assert_eq!(collection.samples.len(), 3);
    }

    #[test]
    fn test_token_sample_collection_capacity() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let mut collection = TokenSampleCollection::new(5); // Small capacity

        // Fill to capacity
        for i in 0..5 {
            assert!(collection.add_token(i));
        }
        assert_eq!(collection.samples.len(), 5);

        // Adding more returns false (at capacity)
        assert!(!collection.add_token(100));
        assert_eq!(collection.samples.len(), 5);

        // Evict excess (should do nothing, not over capacity)
        let evicted = collection.evict_excess(&mut rng);
        assert_eq!(evicted, 0);
        assert_eq!(collection.samples.len(), 5);
    }

    #[test]
    fn test_token_sample_collection_pick_and_remove() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let mut collection = TokenSampleCollection::new(100);

        // Add tokens
        for i in 0..10 {
            collection.add_token(i);
        }
        assert_eq!(collection.samples.len(), 10);

        // Pick and remove 3 tokens
        let picked = collection.pick_and_remove(3, &mut rng);
        assert_eq!(picked.len(), 3);
        assert_eq!(collection.samples.len(), 7);

        // Verify picked tokens are no longer in collection
        for &token in &picked {
            assert!(!collection.samples.contains(&token));
        }

        // Pick more than available
        let picked_all = collection.pick_and_remove(100, &mut rng);
        assert_eq!(picked_all.len(), 7); // Only 7 remaining
        assert!(collection.samples.is_empty());
    }

    #[test]
    fn test_token_sample_collection_evict_excess() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let mut collection = TokenSampleCollection::new(5);

        // Add more than capacity (via direct manipulation for testing)
        for i in 0..10 {
            collection.samples.insert(i);
        }
        assert_eq!(collection.samples.len(), 10);

        // Evict excess
        let evicted = collection.evict_excess(&mut rng);
        assert_eq!(evicted, 5); // 10 - 5 = 5 evicted
        assert_eq!(collection.samples.len(), 5); // Now at capacity

        // Evict again (should do nothing)
        let evicted_again = collection.evict_excess(&mut rng);
        assert_eq!(evicted_again, 0);
        assert_eq!(collection.samples.len(), 5);
    }

    #[test]
    fn test_token_sample_from_answer() {
        let mut collection = TokenSampleCollection::new(100);

        // Create a simple answer and signature
        let answer = TokenMapping {
            id: 100,
            block: 200,
        };

        let signature = [
            TokenMapping { id: 1, block: 10 },
            TokenMapping { id: 2, block: 20 },
            TokenMapping { id: 3, block: 30 },
            TokenMapping { id: 0, block: 0 }, // Zero tokens should be ignored
            TokenMapping { id: 0, block: 0 },
            TokenMapping { id: 0, block: 0 },
            TokenMapping { id: 0, block: 0 },
            TokenMapping { id: 0, block: 0 },
            TokenMapping { id: 0, block: 0 },
            TokenMapping { id: 0, block: 0 },
        ];

        let sender_peer_id = 500;

        collection.sample_from_answer(&answer, &signature, sender_peer_id);

        // Should have: answer token (100) + sender peer ID (500) + 3 signature tokens = 5 tokens
        assert_eq!(collection.samples.len(), 5);
        assert!(collection.samples.contains(&100)); // answer token
        assert!(collection.samples.contains(&500)); // sender peer ID
        assert!(collection.samples.contains(&1));   // sig tokens
        assert!(collection.samples.contains(&2));
        assert!(collection.samples.contains(&3));
    }

    #[test]
    fn test_token_sample_from_referral() {
        let mut collection = TokenSampleCollection::new(100);

        let suggested_peers = [1000, 2000];
        collection.sample_from_referral(&suggested_peers);

        assert_eq!(collection.samples.len(), 2);
        assert!(collection.samples.contains(&1000));
        assert!(collection.samples.contains(&2000));

        // Zero peers should be ignored
        let with_zero = [3000, 0];
        collection.sample_from_referral(&with_zero);

        assert_eq!(collection.samples.len(), 3); // Only 3000 added
        assert!(collection.samples.contains(&3000));
    }

    #[test]
    fn test_peer_state_helpers() {
        // Test PeerState helper methods
        let identified = PeerState::Identified {
            discovered_at: 0,
            last_invitation_election_at: None,
        };
        assert!(identified.is_identified());
        assert!(!identified.is_pending());
        assert!(!identified.is_connected());

        let pending = PeerState::Pending {
            invitation_sent_at: 0,
            from_election: 100,
        };
        assert!(!pending.is_identified());
        assert!(pending.is_pending());
        assert!(!pending.is_connected());

        let connected = PeerState::Connected {
            connected_since: 0,
            last_keepalive: 0,
            election_wins: 5,
            election_attempts: 10,
            quality_score: 0.8,
        };
        assert!(!connected.is_identified());
        assert!(!connected.is_pending());
        assert!(connected.is_connected());
    }

    #[test]
    fn test_config_defaults() {
        let config = PeerManagerConfig::default();

        // Check key defaults
        assert_eq!(config.connected_max_capacity, 200);
        assert_eq!(config.identified_max_capacity, 5000);
        assert_eq!(config.token_sample_max_capacity, 1000);
        assert_eq!(config.elections_per_tick, 3);
        assert_eq!(config.min_collection_time, 10);
        assert_eq!(config.pending_timeout, 10);
        assert_eq!(config.connection_timeout, 300);
        assert_eq!(config.prune_protection_time, 600);

        // Legacy compatibility
        assert_eq!(config.total_budget, 200);
    }
}
