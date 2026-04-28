// Peer Lifecycle Simulator Runner

use super::config::{BootstrapMethod, PeerLifecycleConfig};
use super::stats::*;
use super::token_allocation::{GenesisPeerTokens, GlobalTokenMapping};
use super::topology::{
    build_linear_probability_ring_topology, build_probabilistic_ring_gradient_topology,
    build_ring_core_tail_topology, build_ring_gradient_topology,
};
use ec_rust::ec_interface::{
    BlockId, BlockTime, EcTime, MessageTicket, PeerId, TokenId, TokenMapping,
    TOKENS_SIGNATURE_SIZE,
};
use ec_rust::ec_memory_backend::MemTokens;
use ec_rust::ec_peers::{EcPeers, PeerAction};
use ec_rust::ec_proof_of_storage::{
    SignatureSearchResult, TokenStorageBackend, SIGNATURE_CHUNKS,
};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use std::collections::{BTreeMap, HashSet, VecDeque};

// ============================================================================
// Core Structures
// ============================================================================

/// Main simulator runner
pub struct PeerLifecycleRunner {
    config: PeerLifecycleConfig,
    rng: StdRng,
    seed: [u8; 32], // Stored for reproducibility reporting
    current_round: usize,

    // Network state
    peers: BTreeMap<PeerId, SimPeer>,
    global_mapping: Option<GlobalTokenMapping>, // Stored for dynamic peer allocation

    // Peer group tracking
    peer_groups: BTreeMap<String, PeerGroup>,
    peer_to_group: BTreeMap<PeerId, String>, // Maps peer ID to group name

    // Message queue
    messages: VecDeque<MessageEnvelope>,
    delayed_messages: VecDeque<MessageEnvelope>,

    // Metrics tracking
    metrics_history: Vec<RoundMetrics>,
    total_messages: MessageCounter,

    // Event state
    elections_paused_until: Option<usize>,
}

/// A group of peers for tracking and analysis
#[derive(Debug, Clone)]
pub struct PeerGroup {
    pub name: String,
    pub peer_ids: Vec<PeerId>,
    pub join_round: usize,
    pub coverage_fraction: f64,
}

/// A simulated peer
struct SimPeer {
    peer_id: PeerId,
    peer_manager: EcPeers,
    token_storage: SimTokenStorage,
    known_tokens: Vec<TokenId>, // Tokens in this peer's view
    active: bool,
}

enum SimTokenStorage {
    Memory(MemTokens),
    Genesis(GenesisPeerTokens),
}

impl TokenStorageBackend for SimTokenStorage {
    fn lookup(&self, token: &TokenId) -> Option<BlockTime> {
        match self {
            Self::Memory(storage) => storage.lookup(token),
            Self::Genesis(storage) => storage.lookup(token),
        }
    }

    fn set(&mut self, token: &TokenId, block: &BlockId, parent: &BlockId, time: EcTime) {
        match self {
            Self::Memory(storage) => storage.set(token, block, parent, time),
            Self::Genesis(storage) => storage.set(token, block, parent, time),
        }
    }

    fn search_signature(
        &self,
        lookup_token: &TokenId,
        signature_chunks: &[u16; SIGNATURE_CHUNKS],
    ) -> SignatureSearchResult {
        match self {
            Self::Memory(storage) => storage.search_signature(lookup_token, signature_chunks),
            Self::Genesis(storage) => storage.search_signature(lookup_token, signature_chunks),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Memory(storage) => storage.len(),
            Self::Genesis(storage) => storage.len(),
        }
    }
}

/// Message envelope for routing
#[derive(Clone, Debug)]
struct MessageEnvelope {
    from: PeerId,
    to: PeerId,
    message: SimMessage,
}

/// Simplified message types for simulation
#[derive(Clone, Debug)]
enum SimMessage {
    QueryToken {
        token: TokenId,
        ticket: MessageTicket,
    },
    Answer {
        answer: TokenMapping,
        signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
        ticket: MessageTicket,
    },
    Referral {
        token: TokenId,
        ticket: MessageTicket,
        suggested_peers: [PeerId; 2],
    },
}

/// Message counters
#[derive(Default)]
struct MessageCounter {
    queries: usize,
    answers: usize,
    referrals: usize,
}

#[derive(Debug, Clone, Default)]
struct ProbeReachSummary {
    probes: usize,
    active_peers: usize,
    avg_found_per_probe: f64,
    p50_found_per_probe: usize,
    p95_found_per_probe: usize,
    max_found_per_probe: usize,
    cumulative_found: usize,
    cumulative_fraction: f64,
}

// ============================================================================
// Implementation
// ============================================================================

impl PeerLifecycleRunner {
    /// Create new simulator
    pub fn new(config: PeerLifecycleConfig) -> Self {
        // Initialize RNG with seed
        let seed = config.seed.unwrap_or_else(|| {
            let mut seed = [0u8; 32];
            rand::thread_rng().fill(&mut seed);
            seed
        });
        let rng = StdRng::from_seed(seed);

        Self {
            config,
            rng,
            seed,
            current_round: 0,
            peers: BTreeMap::new(),
            global_mapping: None,
            peer_groups: BTreeMap::new(),
            peer_to_group: BTreeMap::new(),
            messages: VecDeque::new(),
            delayed_messages: VecDeque::new(),
            metrics_history: Vec::new(),
            total_messages: MessageCounter::default(),
            elections_paused_until: None,
        }
    }

    /// Run the simulation
    pub fn run(mut self) -> SimulationResult {
        // Report seed for reproducibility
        println!("╔════════════════════════════════════════════════════════╗");
        println!("║  Simulation Seed (for reproducibility)                ║");
        println!("╚════════════════════════════════════════════════════════╝");
        println!("Seed: {:?}", self.seed);
        println!("  (Store this seed to reproduce exact same simulation)\n");

        // 1. Initialize network
        self.initialize_network();

        // 2. Run simulation rounds
        for round in 0..self.config.rounds {
            self.current_round = round;

            // Process scheduled events for this round
            self.process_scheduled_events();

            // Deliver delayed messages from previous round
            self.process_delayed_messages();

            // Process current messages
            self.deliver_messages();

            // Tick all peers
            self.tick_all_peers();

            // Collect metrics
            if self.should_sample_metrics() {
                self.collect_metrics();
            }

            if round % 10 == 0 {
                println!("round {}", round);
            }
        }

        // 3. Build final result
        self.build_result()
    }

    /// Initialize the peer network (dispatches to Random or Genesis mode)
    fn initialize_network(&mut self) {
        if let Some(ref genesis_config) = self.config.token_distribution.genesis_config {
            // Genesis mode: deterministic bootstrap from genesis tokens
            self.initialize_network_with_genesis(genesis_config.clone());
        } else {
            // Random mode: random token allocation
            self.initialize_network_with_random();
        }
    }

    /// Initialize network with random token allocation (original implementation)
    fn initialize_network_with_random(&mut self) {
        let num_peers = self.config.initial_state.num_peers;

        // Create global token mapping (no peer IDs yet - they will be allocated)
        let mut global_mapping = GlobalTokenMapping::new(
            StdRng::from_seed(self.rng.gen()),
            self.config.token_distribution.total_tokens,
        );

        // Allocate peer IDs from the token pool
        let mut peer_ids = Vec::with_capacity(num_peers);
        for _ in 0..num_peers {
            let peer_id = global_mapping
                .allocate_peer_id()
                .expect("Failed to allocate peer ID from token pool - increase total_tokens");
            peer_ids.push(peer_id);
        }

        // Calculate view_width from neighbor_overlap parameter
        let view_width = GlobalTokenMapping::calculate_view_width(
            num_peers,
            self.config.token_distribution.neighbor_overlap,
        );

        // Create peers with views of the global mapping
        for peer_id in peer_ids {
            // Get this peer's view as ready-to-use MemTokens
            let token_storage = global_mapping.get_peer_view(
                peer_id,
                view_width,
                self.config.token_distribution.coverage_fraction,
            );

            // known_tokens is now just for tracking (empty for now)
            let known_tokens = Vec::new();

            // Create peer manager with seeded RNG
            let peer_rng = StdRng::from_seed(self.rng.gen());
            let peer_manager =
                EcPeers::with_config_and_rng(peer_id, self.config.peer_config.clone(), peer_rng);

            let peer = SimPeer {
                peer_id,
                peer_manager,
                token_storage: SimTokenStorage::Memory(token_storage),
                known_tokens,
                active: true,
            };

            self.peers.insert(peer_id, peer);
        }

        // Initialize topology (seed initial peer knowledge and connections)
        self.initialize_topology(&global_mapping, view_width);

        // Store global mapping for dynamic peer allocation
        self.global_mapping = Some(global_mapping);

        // Create "initial" peer group with all initial peers
        let peer_ids: Vec<PeerId> = self.peers.keys().copied().collect();
        let initial_group = PeerGroup {
            name: "initial".to_string(),
            peer_ids: peer_ids.clone(),
            join_round: 0,
            coverage_fraction: self.config.token_distribution.coverage_fraction,
        };

        self.peer_groups
            .insert("initial".to_string(), initial_group);
        for peer_id in peer_ids {
            self.peer_to_group.insert(peer_id, "initial".to_string());
        }
    }

    /// Initialize network with genesis token allocation (fast simulation mode)
    ///
    /// This uses pre-computed token IDs instead of full block generation,
    /// which is much faster for simulation purposes.
    fn initialize_network_with_genesis(
        &mut self,
        genesis_config: ec_rust::ec_genesis::GenesisConfig,
    ) {
        use super::token_allocation::GenesisTokenSet;

        let num_peers = self.config.initial_state.num_peers;
        let storage_fraction = self.config.token_distribution.genesis_storage_fraction;

        println!("╔════════════════════════════════════════════════════════╗");
        println!("║  Genesis Bootstrap Mode (Fast Simulation)             ║");
        println!("╚════════════════════════════════════════════════════════╝");
        println!(
            "Pre-generating {} genesis token IDs...",
            genesis_config.block_count
        );

        // 1. Pre-generate all genesis token IDs (fast - just hashing)
        let mut genesis_set =
            GenesisTokenSet::new(&genesis_config, StdRng::from_seed(self.rng.gen()));

        println!("✓ Genesis token IDs generated");
        println!("Allocating {} peer IDs from genesis tokens...", num_peers);

        // 2. Allocate peer IDs from genesis tokens
        let peer_ids = genesis_set
            .allocate_peer_ids(num_peers)
            .expect("Failed to allocate peer IDs from genesis tokens");

        println!("✓ Allocated {} peer IDs", peer_ids.len());
        println!(
            "Setting up token ownership ({:.0}% of ring per peer)...",
            storage_fraction * 100.0
        );

        let shared_tokens = genesis_set.shared_tokens();

        // 3. Create each peer with lazy token ownership (no full block
        // generation and no per-peer materialized token table).
        let expected_tokens_per_peer =
            (genesis_config.block_count as f64 * storage_fraction) as usize;

        for (idx, peer_id) in peer_ids.iter().enumerate() {
            // Create peer manager
            let peer_rng = StdRng::from_seed(self.rng.gen());
            let mut peer_manager =
                EcPeers::with_config_and_rng(*peer_id, self.config.peer_config.clone(), peer_rng);

            let token_storage = shared_tokens.peer_view(*peer_id, storage_fraction);
            let stored_count = token_storage.len();

            // Seed TokenSampleCollection with some genesis tokens for discovery
            let seed_tokens = genesis_set.sample_seed_tokens(&mut self.rng, 0.01);
            for token_id in seed_tokens.into_iter().take(100) {
                peer_manager.seed_genesis_token(token_id);
            }

            // Progress reporting (less verbose)
            if (idx + 1) % 100 == 0 || idx + 1 == peer_ids.len() {
                println!(
                    "  [{}/{}] peers initialized (~{} tokens each)",
                    idx + 1,
                    peer_ids.len(),
                    stored_count
                );
            }

            let peer = SimPeer {
                peer_id: *peer_id,
                peer_manager,
                token_storage: SimTokenStorage::Genesis(token_storage),
                known_tokens: Vec::new(),
                active: true,
            };

            self.peers.insert(*peer_id, peer);
        }

        println!(
            "\n✓ All {} peers initialized with ~{} tokens each (fast mode)",
            peer_ids.len(),
            expected_tokens_per_peer
        );

        // 4. Initialize topology for genesis mode
        self.initialize_topology_genesis(&genesis_set);

        // 5. Create "genesis-cold-start" peer group
        let initial_group = PeerGroup {
            name: "genesis-cold-start".to_string(),
            peer_ids: peer_ids.clone(),
            join_round: 0,
            coverage_fraction: storage_fraction,
        };

        self.peer_groups
            .insert("genesis-cold-start".to_string(), initial_group);
        for peer_id in peer_ids {
            self.peer_to_group
                .insert(peer_id, "genesis-cold-start".to_string());
        }

        println!("✓ Genesis bootstrap complete\n");
    }

    /// Initialize peer topology based on configuration
    fn initialize_topology(&mut self, global_mapping: &GlobalTokenMapping, view_width: u64) {
        use super::config::TopologyMode;
        use rand::seq::SliceRandom;

        let peer_ids: Vec<PeerId> = self.peers.keys().copied().collect();

        match &self.config.initial_state.initial_topology {
            TopologyMode::FullyKnown { connected_fraction } => {
                // Every peer knows every other peer
                for (peer_id, peer) in &mut self.peers {
                    let mut known_peers: Vec<PeerId> = peer_ids
                        .iter()
                        .filter(|&&id| id != *peer_id)
                        .copied()
                        .collect();

                    // Shuffle and select connected_fraction to make Connected
                    known_peers.shuffle(&mut self.rng);
                    let num_connected = (known_peers.len() as f64 * connected_fraction) as usize;

                    for (idx, &other_id) in known_peers.iter().enumerate() {
                        if idx < num_connected {
                            // Add as seed peer (will be promoted to Connected)
                            peer.peer_manager.add_identified_peer(other_id, 0);
                        } else {
                            // Add as Connected
                            peer.peer_manager.update_peer(&other_id, 0);
                        }
                    }
                }
            }

            TopologyMode::LocalKnowledge {
                peer_knowledge_fraction,
                connected_fraction,
            } => {
                // Peers know subset of neighbors based on view_width
                for (peer_id, peer) in &mut self.peers {
                    // Get nearby peers within view_width
                    let nearby_peers = global_mapping.get_nearby_peers(*peer_id, view_width);

                    // Sample peer_knowledge_fraction of nearby peers
                    let mut known_peers = nearby_peers;
                    known_peers.shuffle(&mut self.rng);
                    let num_known = (known_peers.len() as f64 * peer_knowledge_fraction) as usize;
                    known_peers.truncate(num_known);

                    // Of the known peers, make connected_fraction Connected
                    let num_connected = (known_peers.len() as f64 * connected_fraction) as usize;

                    for (idx, &other_id) in known_peers.iter().enumerate() {
                        if idx < num_connected {
                            // Add as seed peer (will be promoted to Connected)
                            peer.peer_manager.add_identified_peer(other_id, 0);
                        } else {
                            // Add as Connected
                            peer.peer_manager.update_peer(&other_id, 0);
                        }
                    }
                }
            }

            TopologyMode::Ring { neighbors } => {
                let adjacency = build_ring_gradient_topology(&peer_ids, *neighbors, &mut self.rng);

                for (peer_id, connected_peers) in adjacency {
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        for other_id in connected_peers {
                            peer.peer_manager.update_peer(&other_id, 0);
                        }
                    }
                }
            }

            TopologyMode::RingCoreTail {
                neighbors,
                tail_peers_per_side,
            } => {
                let adjacency = build_ring_core_tail_topology(
                    &peer_ids,
                    *neighbors,
                    *tail_peers_per_side,
                    &mut self.rng,
                );

                for (peer_id, connected_peers) in adjacency {
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        for other_id in connected_peers {
                            peer.peer_manager.update_peer(&other_id, 0);
                        }
                    }
                }
            }

            TopologyMode::RingProbabilistic => {
                let adjacency =
                    build_probabilistic_ring_gradient_topology(&peer_ids, &mut self.rng);

                for (peer_id, connected_peers) in adjacency {
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        for other_id in connected_peers {
                            peer.peer_manager.update_peer(&other_id, 0);
                        }
                    }
                }
            }

            TopologyMode::RingLinearProbability {
                center_prob,
                far_prob,
                guaranteed_neighbors,
            } => {
                let adjacency = build_linear_probability_ring_topology(
                    &peer_ids,
                    *center_prob,
                    *far_prob,
                    *guaranteed_neighbors,
                    &mut self.rng,
                );

                for (peer_id, connected_peers) in adjacency {
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        for other_id in connected_peers {
                            peer.peer_manager.update_peer(&other_id, 0);
                        }
                    }
                }
            }

            TopologyMode::RandomIdentified { peers_per_node } => {
                // Bootstrap scenario: Each peer gets N random peers in Identified state
                use rand::seq::SliceRandom;

                for (peer_id, peer) in &mut self.peers {
                    // Get all other peers
                    let mut available_peers: Vec<PeerId> = peer_ids
                        .iter()
                        .filter(|&&id| id != *peer_id)
                        .copied()
                        .collect();

                    // Shuffle and take N peers
                    available_peers.shuffle(&mut self.rng);
                    let selected_peers: Vec<PeerId> = available_peers
                        .iter()
                        .take(*peers_per_node)
                        .copied()
                        .collect();

                    // Add selected peers as Identified (using add_seed_peer which adds to Identified)
                    for &other_id in &selected_peers {
                        peer.peer_manager.add_identified_peer(other_id, 0);
                    }
                }
            }

            TopologyMode::Isolated => {
                // No initial connections - peers discover via elections
            }
        }
    }

    /// Initialize peer topology for genesis mode
    ///
    /// In genesis mode, peers don't have a GlobalTokenMapping with view_width,
    /// so we only support topologies that don't depend on ring distance knowledge:
    /// - Isolated: No initial connections (most realistic for cold start)
    /// - RandomIdentified: Each peer knows N random others (bootstrap scenario)
    fn initialize_topology_genesis(
        &mut self,
        _genesis_set: &super::token_allocation::GenesisTokenSet,
    ) {
        use super::config::TopologyMode;
        use rand::seq::SliceRandom;

        let peer_ids: Vec<PeerId> = self.peers.keys().copied().collect();

        match &self.config.initial_state.initial_topology {
            TopologyMode::Isolated => {
                // No initial connections - peers discover via genesis token elections
                println!("✓ Topology: Isolated (cold start from genesis tokens)");
            }

            TopologyMode::RandomIdentified { peers_per_node } => {
                // Each peer gets N random peers in Identified state
                println!(
                    "✓ Topology: RandomIdentified ({} peers per node)",
                    peers_per_node
                );

                for (peer_id, peer) in &mut self.peers {
                    // Get all other peers
                    let mut available_peers: Vec<PeerId> = peer_ids
                        .iter()
                        .filter(|&&id| id != *peer_id)
                        .copied()
                        .collect();

                    // Shuffle and take N peers
                    available_peers.shuffle(&mut self.rng);
                    let selected_peers: Vec<PeerId> = available_peers
                        .iter()
                        .take(*peers_per_node)
                        .copied()
                        .collect();

                    // Add selected peers as Identified
                    for &other_id in &selected_peers {
                        peer.peer_manager.add_identified_peer(other_id, 0);
                    }
                }
            }

            // Other modes not supported in genesis (they require ring distance knowledge)
            TopologyMode::FullyKnown { .. } => {
                println!("WARNING: FullyKnown topology not realistic for genesis mode");
                println!("         Using Isolated instead (peers will discover via elections)");
            }

            TopologyMode::LocalKnowledge { .. } => {
                println!("WARNING: LocalKnowledge topology not realistic for genesis mode");
                println!("         (peers don't know ring distances at genesis)");
                println!("         Using Isolated instead (peers will discover via elections)");
            }

            TopologyMode::Ring { .. }
            | TopologyMode::RingCoreTail { .. }
            | TopologyMode::RingProbabilistic
            | TopologyMode::RingLinearProbability { .. } => {
                println!("WARNING: Ring topology not realistic for genesis mode");
                println!("         (peers don't know ring positions at genesis)");
                println!("         Using Isolated instead (peers will discover via elections)");
            }
        }
    }

    /// Process delayed messages from previous round
    fn process_delayed_messages(&mut self) {
        self.messages.append(&mut self.delayed_messages);
    }

    /// Deliver messages with network simulation
    fn deliver_messages(&mut self) {
        while let Some(envelope) = self.messages.pop_front() {
            // Apply network loss
            if self.rng.gen_bool(self.config.network.loss_fraction) {
                continue; // Drop message
            }

            // Apply network delay
            if self.rng.gen_bool(self.config.network.delay_fraction) {
                self.delayed_messages.push_back(envelope);
                continue;
            }

            // Deliver message
            self.deliver_message(envelope);
        }
    }

    /// Deliver a single message to recipient
    fn deliver_message(&mut self, envelope: MessageEnvelope) {
        // Check if recipient exists and is active
        let is_active = self
            .peers
            .get(&envelope.to)
            .map(|p| p.active)
            .unwrap_or(false);

        if !is_active {
            return;
        }

        match envelope.message {
            SimMessage::QueryToken { token, ticket } => {
                // Use EcPeers.handle_query to generate response
                if let Some(peer) = self.peers.get(&envelope.to) {
                    let action = peer.peer_manager.handle_query(
                        &peer.token_storage,
                        token,
                        ticket,
                        envelope.from,
                    );

                    if let Some(action) = action {
                        let sender_id = envelope.to;
                        let receiver = envelope.from;
                        match action {
                            PeerAction::SendAnswer {
                                answer,
                                signature,
                                ticket,
                            } => {
                                self.send_message(
                                    sender_id,
                                    receiver,
                                    SimMessage::Answer {
                                        answer,
                                        signature,
                                        ticket,
                                    },
                                );
                            }
                            PeerAction::SendReferral {
                                token,
                                ticket,
                                suggested_peers,
                            } => {
                                self.send_message(
                                    sender_id,
                                    receiver,
                                    SimMessage::Referral {
                                        token,
                                        ticket,
                                        suggested_peers,
                                    },
                                );
                            }
                            _ => {
                                // Ignore other action types
                            }
                        }
                    }
                }
            }

            SimMessage::Answer {
                answer,
                signature,
                ticket,
            } => {
                // Peer received answer - route to election
                let actions = if let Some(peer) = self.peers.get_mut(&envelope.to) {
                    let current_time = self.current_round as EcTime;
                    peer.peer_manager.handle_answer(
                        &answer,
                        &signature,
                        ticket,
                        envelope.from,
                        current_time,
                        &peer.token_storage,
                        0, // head_of_chain not used in peer lifecycle sim
                    )
                } else {
                    Vec::new()
                };

                self.process_peer_actions(envelope.to, actions);
            }

            SimMessage::Referral {
                token,
                ticket,
                suggested_peers,
            } => {
                // Peer received referral - route to election and spawn new channels
                if let Some(peer) = self.peers.get_mut(&envelope.to) {
                    let current_time = self.current_round as EcTime;
                    let actions = peer.peer_manager.handle_referral(
                        ticket,
                        token,
                        suggested_peers,
                        envelope.from,
                        current_time,
                    );

                    self.process_peer_actions(envelope.to, actions.into_iter().collect());
                }
            }
        }
    }

    /// Send a message
    fn send_message(&mut self, from: PeerId, to: PeerId, message: SimMessage) {
        match &message {
            SimMessage::QueryToken { .. } => self.total_messages.queries += 1,
            SimMessage::Answer { .. } => self.total_messages.answers += 1,
            SimMessage::Referral { .. } => self.total_messages.referrals += 1,
        }

        self.messages
            .push_back(MessageEnvelope { from, to, message });
    }

    fn process_peer_actions(&mut self, peer_id: PeerId, actions: Vec<PeerAction>) {
        for action in actions {
            match action {
                PeerAction::SendQuery {
                    receiver,
                    token,
                    ticket,
                } => {
                    self.send_message(peer_id, receiver, SimMessage::QueryToken { token, ticket });
                }
                PeerAction::SendInvitation {
                    receiver,
                    answer,
                    signature,
                } => {
                    self.send_message(
                        peer_id,
                        receiver,
                        SimMessage::Answer {
                            answer,
                            signature,
                            ticket: 0,
                        },
                    );
                }
                PeerAction::SendAnswer { .. } | PeerAction::SendReferral { .. } => {
                    panic!("Unexpected direct response action outside query handling")
                }
            }
        }
    }

    /// Tick all active peers
    fn tick_all_peers(&mut self) {
        let current_time = self.current_round as EcTime;
        let peer_ids: Vec<PeerId> = self.peers.keys().copied().collect();

        for peer_id in peer_ids.clone() {
            if let Some(peer) = self.peers.get_mut(&peer_id) {
                if !peer.active {
                    continue;
                }
                if self
                    .elections_paused_until
                    .map(|until| self.current_round < until)
                    .unwrap_or(false)
                {
                    continue;
                }

                // Tick peer manager
                let actions = peer.peer_manager.tick(&peer.token_storage, current_time);
                self.process_peer_actions(peer_id, actions);
            }
        }
    }

    /// Check if should sample metrics this round
    fn should_sample_metrics(&self) -> bool {
        self.current_round % self.config.metrics.sample_interval == 0
    }

    /// Collect metrics for current round
    fn collect_metrics(&mut self) {
        use super::stats::calculate_connected_peer_distribution;
        use super::stats::calculate_gradient_distribution;
        use super::stats::calculate_gradient_steepness;
        use std::collections::BTreeMap;

        let mut metrics = RoundMetrics::new(
            self.current_round,
            self.current_round as u64 * self.config.tick_duration_ms,
        );

        // Collect peer state counts across all peers
        let mut total_identified = 0;
        let mut total_pending = 0;
        let mut total_connected = 0;
        let mut active_count = 0;
        let mut connected_counts: Vec<usize> = Vec::new();

        // Aggregate election stats from all peers
        let mut total_elections_started = 0;
        let mut total_elections_completed = 0;
        let mut total_elections_timeout = 0;
        let mut total_elections_splitbrain = 0;

        // Collect gradient steepness for each peer
        let mut peer_steepness_map: BTreeMap<PeerId, f64> = BTreeMap::new();

        for peer in self.peers.values() {
            if peer.active {
                active_count += 1;

                // Collect peer state counts
                let num_identified = peer.peer_manager.num_identified();
                let num_pending = peer.peer_manager.num_pending();
                let num_connected = peer.peer_manager.num_connected();

                total_identified += num_identified;
                total_pending += num_pending;
                total_connected += num_connected;
                connected_counts.push(num_connected);

                // Collect election stats from this peer
                let (started, completed, timeout, splitbrain) =
                    peer.peer_manager.get_election_stats();
                total_elections_started += started;
                total_elections_completed += completed;
                total_elections_timeout += timeout;
                total_elections_splitbrain += splitbrain;

                // Calculate gradient steepness for this peer
                let active_peers = peer.peer_manager.get_active_peers();
                let steepness = calculate_gradient_steepness(peer.peer_id, active_peers);
                peer_steepness_map.insert(peer.peer_id, steepness);
            }
        }

        // Calculate averages
        let avg_identified = if active_count > 0 {
            total_identified / active_count
        } else {
            0
        };
        let avg_pending = if active_count > 0 {
            total_pending / active_count
        } else {
            0
        };
        let avg_connected = if active_count > 0 {
            total_connected / active_count
        } else {
            0
        };

        metrics.peer_counts = PeerCounts {
            total_peers: self.peers.len(),
            active_peers: active_count,
            identified: avg_identified,
            pending: avg_pending,
            connected: avg_connected,
        };

        // Network health
        if !connected_counts.is_empty() {
            let min = *connected_counts.iter().min().unwrap_or(&0);
            let max = *connected_counts.iter().max().unwrap_or(&0);
            let avg = connected_counts.iter().sum::<usize>() as f64 / connected_counts.len() as f64;

            // Calculate standard deviation
            let variance = connected_counts
                .iter()
                .map(|&count| {
                    let diff = count as f64 - avg;
                    diff * diff
                })
                .sum::<f64>()
                / connected_counts.len() as f64;
            let stddev = variance.sqrt();

            // Calculate connected peer count distribution (quartiles by default)
            let connected_peer_distribution = if !connected_counts.is_empty() {
                Some(calculate_connected_peer_distribution(&connected_counts, 4))
            } else {
                None
            };

            // Calculate gradient steepness distribution (quartiles by default)
            let gradient_distribution = if !peer_steepness_map.is_empty() {
                Some(calculate_gradient_distribution(&peer_steepness_map, 4))
            } else {
                None
            };

            metrics.network_health = NetworkHealth {
                min_connected_peers: min,
                max_connected_peers: max,
                avg_connected_peers: avg,
                stddev_connected_peers: stddev,
                ring_coverage_percent: 0.0, // TODO: Calculate
                partition_detected: false,
                connected_peer_distribution,
                gradient_distribution,
                gradient_shape: Some(self.calculate_gradient_shape_metrics()),
                dense_linear_shape: Some(self.calculate_dense_linear_shape_metrics()),
            };
        }

        // Aggregate election stats from all peers
        metrics.election_stats.total_elections_started = total_elections_started;
        metrics.election_stats.total_elections_completed = total_elections_completed;
        metrics.election_stats.total_elections_timed_out = total_elections_timeout;
        metrics.election_stats.total_split_brain_detected = total_elections_splitbrain;

        self.metrics_history.push(metrics);
    }

    fn target_gradient_neighbors(&self) -> usize {
        self.config
            .token_distribution
            .neighbor_overlap
            .max(self.config.peer_config.neighborhood_width)
            .max(1)
    }

    fn active_peer_ids(&self) -> Vec<PeerId> {
        self.peers
            .iter()
            .filter_map(|(peer_id, peer)| peer.active.then_some(*peer_id))
            .collect()
    }

    fn calculate_gradient_shape_metrics(&self) -> GradientShapeMetrics {
        self.calculate_shape_metrics(
            self.target_gradient_neighbors(),
            |rank_distance, _max_step, guaranteed_steps, fade_steps| {
                if rank_distance <= guaranteed_steps {
                    1.0
                } else if rank_distance < fade_steps && fade_steps > guaranteed_steps {
                    let span = (fade_steps - guaranteed_steps) as f64;
                    let remaining = (fade_steps - rank_distance) as f64;
                    (remaining / span).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            },
        )
    }

    fn calculate_dense_linear_shape_metrics(&self) -> GradientShapeMetrics {
        const FIXED_DENSE_LINEAR_CENTER_PROB: f64 = 1.0;
        const FIXED_DENSE_LINEAR_FAR_PROB: f64 = 0.2;
        const FIXED_DENSE_LINEAR_GUARANTEED_NEIGHBORS: usize = 10;

        self.calculate_shape_metrics(
            FIXED_DENSE_LINEAR_GUARANTEED_NEIGHBORS,
            |rank_distance, max_step, guaranteed_steps, _fade_steps| {
                if rank_distance <= guaranteed_steps {
                    1.0
                } else if max_step == 0 {
                    FIXED_DENSE_LINEAR_CENTER_PROB
                } else {
                    let distance_fraction =
                        (rank_distance as f64 / max_step as f64).clamp(0.0, 1.0);
                    (FIXED_DENSE_LINEAR_CENTER_PROB
                        + ((FIXED_DENSE_LINEAR_FAR_PROB - FIXED_DENSE_LINEAR_CENTER_PROB)
                            * distance_fraction))
                        .clamp(0.0, 1.0)
                }
            },
        )
    }

    fn dense_linear_target_probability(rank_distance: usize, max_step: usize) -> f64 {
        const FIXED_DENSE_LINEAR_CENTER_PROB: f64 = 1.0;
        const FIXED_DENSE_LINEAR_FAR_PROB: f64 = 0.2;
        const FIXED_DENSE_LINEAR_GUARANTEED_NEIGHBORS: usize = 10;

        if rank_distance <= FIXED_DENSE_LINEAR_GUARANTEED_NEIGHBORS.min(max_step.max(1)) {
            1.0
        } else if max_step == 0 {
            FIXED_DENSE_LINEAR_CENTER_PROB
        } else {
            let distance_fraction = (rank_distance as f64 / max_step as f64).clamp(0.0, 1.0);
            (FIXED_DENSE_LINEAR_CENTER_PROB
                + ((FIXED_DENSE_LINEAR_FAR_PROB - FIXED_DENSE_LINEAR_CENTER_PROB)
                    * distance_fraction))
                .clamp(0.0, 1.0)
        }
    }

    fn diagnostic_rng(&self, salt: u64) -> StdRng {
        let mut seed = self.seed;
        for (idx, byte) in (self.current_round as u64).to_le_bytes().iter().enumerate() {
            seed[idx] ^= *byte;
        }
        for (idx, byte) in salt.to_le_bytes().iter().enumerate() {
            seed[24 + idx] ^= *byte;
        }
        StdRng::from_seed(seed)
    }

    fn referral_probe_reachability(
        &self,
        probes: usize,
        max_depth: usize,
        rng: &mut StdRng,
    ) -> ProbeReachSummary {
        let active_peers = self.active_peer_ids();
        if active_peers.is_empty() || probes == 0 {
            return ProbeReachSummary {
                probes,
                ..ProbeReachSummary::default()
            };
        }

        let active_peer_set = active_peers.iter().copied().collect::<HashSet<_>>();
        let mut cumulative_found = HashSet::new();
        let mut found_per_probe = Vec::with_capacity(probes);

        for _ in 0..probes {
            let token = rng.gen::<TokenId>();
            let Some(start_peer) = active_peers.choose(rng).copied() else {
                continue;
            };

            let mut found = HashSet::new();
            let mut queue = VecDeque::new();
            queue.push_back((start_peer, 0usize));

            while let Some((peer_id, depth)) = queue.pop_front() {
                if !active_peer_set.contains(&peer_id) || !found.insert(peer_id) {
                    continue;
                }

                if depth >= max_depth {
                    continue;
                }

                let Some(peer) = self.peers.get(&peer_id) else {
                    continue;
                };

                for referral in peer.peer_manager.find_closest_peers(token, 2) {
                    if active_peer_set.contains(&referral) && !found.contains(&referral) {
                        queue.push_back((referral, depth + 1));
                    }
                }
            }

            cumulative_found.extend(found.iter().copied());
            found_per_probe.push(found.len());
        }

        found_per_probe.sort_unstable();
        let avg_found_per_probe =
            found_per_probe.iter().sum::<usize>() as f64 / found_per_probe.len().max(1) as f64;
        let p50_found_per_probe = Self::percentile_usize(&found_per_probe, 0.50);
        let p95_found_per_probe = Self::percentile_usize(&found_per_probe, 0.95);
        let max_found_per_probe = found_per_probe.last().copied().unwrap_or(0);
        let cumulative_found_count = cumulative_found.len();

        ProbeReachSummary {
            probes,
            active_peers: active_peers.len(),
            avg_found_per_probe,
            p50_found_per_probe,
            p95_found_per_probe,
            max_found_per_probe,
            cumulative_found: cumulative_found_count,
            cumulative_fraction: cumulative_found_count as f64 / active_peers.len() as f64,
        }
    }

    fn percentile_usize(sorted: &[usize], percentile: f64) -> usize {
        if sorted.is_empty() {
            return 0;
        }

        let idx = ((sorted.len() - 1) as f64 * percentile.clamp(0.0, 1.0)).round() as usize;
        sorted[idx]
    }

    fn report_referral_probe_diagnostics(&self) {
        let mut rng = self.diagnostic_rng(0xA11C_E5EE_D1A9_051Cu64);
        let depth = 5;

        println!("\n  Random referral probing (depth {}):", depth);
        for probes in [20usize, 100, 500] {
            let summary = self.referral_probe_reachability(probes, depth, &mut rng);
            if summary.active_peers == 0 {
                println!("    {} probes: no active peers", summary.probes);
                continue;
            }

            println!(
                "    {:>3} probes: per-probe avg {:.1}, p50 {}, p95 {}, max {}; cumulative {}/{} ({:.1}%)",
                summary.probes,
                summary.avg_found_per_probe,
                summary.p50_found_per_probe,
                summary.p95_found_per_probe,
                summary.max_found_per_probe,
                summary.cumulative_found,
                summary.active_peers,
                summary.cumulative_fraction * 100.0
            );
        }
    }

    fn report_peer_set_hole_diagnostics(&self) {
        let mut active_peers = self.active_peer_ids();
        if active_peers.len() < 2 {
            return;
        }

        active_peers.sort_unstable();
        let index_by_peer = active_peers
            .iter()
            .enumerate()
            .map(|(idx, peer_id)| (*peer_id, idx))
            .collect::<BTreeMap<_, _>>();
        let max_step = active_peers.len() / 2;

        let mut rng = self.diagnostic_rng(0x5E7_0F_51A9E_u64);
        let mut sample_peers = active_peers.clone();
        sample_peers.shuffle(&mut rng);
        sample_peers.truncate(3);

        println!("\n  Sample peer-set holes vs fixed dense-linear target:");
        for peer_id in sample_peers {
            let Some(peer) = self.peers.get(&peer_id) else {
                continue;
            };

            let connected = peer
                .peer_manager
                .get_active_peers()
                .iter()
                .copied()
                .filter(|candidate| index_by_peer.contains_key(candidate))
                .collect::<HashSet<_>>();
            let peer_idx = *index_by_peer
                .get(&peer_id)
                .expect("sample peer should be indexed");

            let mut expected_degree = 0.0;
            let mut core_holes = Vec::new();
            let mut high_prob_holes = Vec::new();
            let mut far_excess = Vec::new();
            let mut bands = [
                (1usize, 10usize, 0usize, 0usize, 0.0),
                (11, 20, 0, 0, 0.0),
                (21, 40, 0, 0, 0.0),
                (41, usize::MAX, 0, 0, 0.0),
            ];

            for other_peer_id in &active_peers {
                if *other_peer_id == peer_id {
                    continue;
                }

                let other_idx = *index_by_peer
                    .get(other_peer_id)
                    .expect("active peer should be indexed");
                let clockwise_steps = peer_idx.abs_diff(other_idx);
                let counter_clockwise_steps = active_peers.len() - clockwise_steps;
                let rank_distance = clockwise_steps.min(counter_clockwise_steps);
                let target_prob = Self::dense_linear_target_probability(rank_distance, max_step);
                let is_connected = connected.contains(other_peer_id);
                expected_degree += target_prob;

                for (low, high, possible, actual, target_sum) in &mut bands {
                    if rank_distance >= *low && rank_distance <= *high {
                        *possible += 1;
                        if is_connected {
                            *actual += 1;
                        }
                        *target_sum += target_prob;
                        break;
                    }
                }

                if rank_distance <= 10 && !is_connected {
                    core_holes.push(rank_distance);
                } else if target_prob >= 0.75 && !is_connected {
                    high_prob_holes.push(rank_distance);
                } else if target_prob <= 0.35 && is_connected {
                    far_excess.push(rank_distance);
                }
            }

            core_holes.sort_unstable();
            high_prob_holes.sort_unstable();
            far_excess.sort_unstable();

            println!(
                "    peer {}: active connected {}, dense ideal {:.1}",
                peer_id,
                connected.len(),
                expected_degree
            );
            for (low, high, possible, actual, target_sum) in bands {
                if possible == 0 {
                    continue;
                }
                let high_label = if high == usize::MAX {
                    "max".to_string()
                } else {
                    high.to_string()
                };
                println!(
                    "      ranks {:>2}-{:>3}: {}/{} connected, target avg {:.3}",
                    low,
                    high_label,
                    actual,
                    possible,
                    target_sum / possible as f64
                );
            }
            println!(
                "      holes: core {} {:?}, high-prob fade {} {:?}, far excess {} {:?}",
                core_holes.len(),
                core_holes.iter().take(12).copied().collect::<Vec<_>>(),
                high_prob_holes.len(),
                high_prob_holes.iter().take(12).copied().collect::<Vec<_>>(),
                far_excess.len(),
                far_excess.iter().take(12).copied().collect::<Vec<_>>()
            );
        }
    }

    fn calculate_shape_metrics<F>(
        &self,
        target_neighbors: usize,
        mut target_for_rank: F,
    ) -> GradientShapeMetrics
    where
        F: FnMut(usize, usize, usize, usize) -> f64,
    {
        let mut sorted_active_peers = self.active_peer_ids();
        if sorted_active_peers.len() < 2 {
            return GradientShapeMetrics::default();
        }

        sorted_active_peers.sort_unstable();
        let index_by_peer = sorted_active_peers
            .iter()
            .enumerate()
            .map(|(idx, peer_id)| (*peer_id, idx))
            .collect::<BTreeMap<_, _>>();

        let max_step = sorted_active_peers.len() / 2;
        if max_step == 0 {
            return GradientShapeMetrics::default();
        }

        let guaranteed_steps = target_neighbors.max(1).min(max_step);
        let fade_steps = (guaranteed_steps * 2).min(max_step.max(guaranteed_steps));

        let mut active_connected_total = 0.0;
        let mut expected_degree_total = 0.0;
        let mut target_fit_total = 0.0;
        let mut core_coverage_total = 0.0;
        let mut fade_coverage_total = 0.0;
        let mut fade_target_total = 0.0;
        let mut far_coverage_total = 0.0;

        for peer_id in &sorted_active_peers {
            let Some(peer) = self.peers.get(peer_id) else {
                continue;
            };
            let active_connected = peer
                .peer_manager
                .get_active_peers()
                .iter()
                .copied()
                .filter(|candidate| index_by_peer.contains_key(candidate))
                .collect::<HashSet<_>>();

            let peer_idx = *index_by_peer
                .get(peer_id)
                .expect("active peer should be indexed");
            let mut expected_degree = 0.0;
            let mut absolute_error = 0.0;
            let mut comparisons = 0usize;

            let mut core_possible = 0usize;
            let mut core_connected = 0usize;
            let mut fade_possible = 0usize;
            let mut fade_connected = 0usize;
            let mut fade_expected = 0.0;
            let mut far_possible = 0usize;
            let mut far_connected = 0usize;

            for (other_peer_id, other_idx) in &index_by_peer {
                if other_peer_id == peer_id {
                    continue;
                }

                let clockwise_steps = peer_idx.abs_diff(*other_idx);
                let counter_clockwise_steps = sorted_active_peers.len() - clockwise_steps;
                let rank_distance = clockwise_steps.min(counter_clockwise_steps);
                let actual = if active_connected.contains(other_peer_id) {
                    1.0
                } else {
                    0.0
                };

                let target = target_for_rank(rank_distance, max_step, guaranteed_steps, fade_steps);

                if rank_distance <= guaranteed_steps {
                    core_possible += 1;
                    if actual > 0.0 {
                        core_connected += 1;
                    }
                } else if rank_distance < fade_steps && fade_steps > guaranteed_steps {
                    fade_possible += 1;
                    if actual > 0.0 {
                        fade_connected += 1;
                    }
                    fade_expected += target;
                } else {
                    far_possible += 1;
                    if actual > 0.0 {
                        far_connected += 1;
                    }
                }

                expected_degree += target;
                absolute_error += f64::abs(actual - target);
                comparisons += 1;
            }

            active_connected_total += active_connected.len() as f64;
            expected_degree_total += expected_degree;
            if comparisons > 0 {
                target_fit_total += 1.0 - (absolute_error / comparisons as f64);
            }
            core_coverage_total += if core_possible == 0 {
                1.0
            } else {
                core_connected as f64 / core_possible as f64
            };
            fade_coverage_total += if fade_possible == 0 {
                0.0
            } else {
                fade_connected as f64 / fade_possible as f64
            };
            fade_target_total += if fade_possible == 0 {
                0.0
            } else {
                fade_expected / fade_possible as f64
            };
            far_coverage_total += if far_possible == 0 {
                0.0
            } else {
                far_connected as f64 / far_possible as f64
            };
        }

        let active_count = sorted_active_peers.len() as f64;
        GradientShapeMetrics {
            avg_active_connected_peers: active_connected_total / active_count,
            avg_expected_active_degree: expected_degree_total / active_count,
            avg_target_fit: target_fit_total / active_count,
            avg_core_coverage: core_coverage_total / active_count,
            avg_fade_coverage: fade_coverage_total / active_count,
            avg_fade_target: fade_target_total / active_count,
            avg_far_coverage: far_coverage_total / active_count,
        }
    }

    fn resolve_bootstrap_peers(&mut self, bootstrap_method: BootstrapMethod) -> Vec<PeerId> {
        match bootstrap_method {
            BootstrapMethod::Random(count) => {
                use rand::seq::SliceRandom;
                self.active_peer_ids()
                    .choose_multiple(&mut self.rng, count)
                    .copied()
                    .collect()
            }
            BootstrapMethod::Specific(peer_ids) => peer_ids,
            BootstrapMethod::None => Vec::new(),
        }
    }

    fn select_peers(
        &mut self,
        selection: super::config::PeerSelection,
        active_only: bool,
    ) -> Vec<PeerId> {
        use rand::seq::SliceRandom;

        let mut candidates = self
            .peers
            .iter()
            .filter_map(|(peer_id, peer)| {
                if active_only && !peer.active {
                    return None;
                }
                if !active_only && peer.active {
                    return None;
                }
                Some(*peer_id)
            })
            .collect::<Vec<_>>();

        match selection {
            super::config::PeerSelection::Random { count } => {
                candidates.shuffle(&mut self.rng);
                candidates.truncate(count);
                candidates
            }
            super::config::PeerSelection::Specific { peer_ids } => peer_ids
                .into_iter()
                .filter(|peer_id| candidates.contains(peer_id))
                .collect(),
            super::config::PeerSelection::ByQuality { count, worst } => {
                candidates.sort_by_key(|peer_id| {
                    self.peers
                        .get(peer_id)
                        .map(|peer| peer.peer_manager.num_connected())
                        .unwrap_or(0)
                });
                if !worst {
                    candidates.reverse();
                }
                candidates.truncate(count);
                candidates
            }
            super::config::PeerSelection::ByTokenCount { count, most } => {
                candidates.sort_by_key(|peer_id| {
                    self.peers
                        .get(peer_id)
                        .map(|peer| peer.token_storage.len())
                        .unwrap_or(0)
                });
                if most {
                    candidates.reverse();
                }
                candidates.truncate(count);
                candidates
            }
        }
    }

    /// Process scheduled events for the current round
    fn process_scheduled_events(&mut self) {
        use super::config::NetworkEvent;

        // Find events scheduled for this round
        let events_for_round: Vec<NetworkEvent> = self
            .config
            .events
            .events
            .iter()
            .filter(|e| e.round == self.current_round)
            .map(|e| e.event.clone())
            .collect();

        for event in events_for_round {
            match event {
                NetworkEvent::ReportStats { label } => {
                    self.report_current_stats(label);
                }
                NetworkEvent::NetworkCondition {
                    delay_fraction,
                    loss_fraction,
                } => {
                    if let Some(delay) = delay_fraction {
                        self.config.network.delay_fraction = delay;
                        println!(
                            "  [Round {}] Network delay changed to {:.1}%",
                            self.current_round,
                            delay * 100.0
                        );
                    }
                    if let Some(loss) = loss_fraction {
                        self.config.network.loss_fraction = loss;
                        println!(
                            "  [Round {}] Network loss changed to {:.1}%",
                            self.current_round,
                            loss * 100.0
                        );
                    }
                }
                NetworkEvent::PeerJoin {
                    count,
                    coverage_fraction,
                    bootstrap_method,
                    group_name,
                } => {
                    self.handle_peer_join(count, coverage_fraction, bootstrap_method, group_name);
                }
                NetworkEvent::PeerCrash { selection } => {
                    let crashed = self.select_peers(selection, true);
                    for peer_id in &crashed {
                        if let Some(peer) = self.peers.get_mut(peer_id) {
                            peer.active = false;
                        }
                    }
                    println!(
                        "  [Round {}] {} peers crashed",
                        self.current_round,
                        crashed.len()
                    );
                }
                NetworkEvent::PeerLeave { selection } => {
                    let leaving = self.select_peers(selection, true);
                    for peer_id in &leaving {
                        if let Some(peer) = self.peers.get_mut(peer_id) {
                            peer.active = false;
                        }
                    }
                    println!(
                        "  [Round {}] {} peers left",
                        self.current_round,
                        leaving.len()
                    );
                }
                NetworkEvent::PeerReturn {
                    selection,
                    bootstrap_method,
                } => {
                    let returning = self.select_peers(selection, false);
                    let bootstrap_peers = self.resolve_bootstrap_peers(bootstrap_method);
                    for peer_id in &returning {
                        if let Some(peer) = self.peers.get_mut(peer_id) {
                            peer.active = true;
                            for known_peer_id in &bootstrap_peers {
                                if known_peer_id != peer_id {
                                    peer.peer_manager.add_identified_peer(
                                        *known_peer_id,
                                        self.current_round as EcTime,
                                    );
                                }
                            }
                        }
                    }
                    println!(
                        "  [Round {}] {} peers returned",
                        self.current_round,
                        returning.len()
                    );
                }
                NetworkEvent::PauseElections { duration } => {
                    let until = self.current_round.saturating_add(duration);
                    self.elections_paused_until = Some(until);
                    println!(
                        "  [Round {}] Peer manager ticks paused until round {}",
                        self.current_round, until
                    );
                }
            }
        }
    }

    /// Handle PeerJoin event (dispatches to genesis or random mode)
    fn handle_peer_join(
        &mut self,
        count: usize,
        coverage_fraction: f64,
        bootstrap_method: BootstrapMethod,
        group_name: Option<String>,
    ) {
        let group_name = group_name.unwrap_or_else(|| format!("join-r{}", self.current_round));

        println!(
            "  [Round {}] {} peers joining (group: '{}', coverage: {:.0}%)",
            self.current_round,
            count,
            group_name,
            coverage_fraction * 100.0
        );

        // Check if we're in genesis mode
        if self.config.token_distribution.genesis_config.is_some() {
            self.handle_peer_join_genesis(count, coverage_fraction, bootstrap_method, group_name);
        } else {
            self.handle_peer_join_random(count, coverage_fraction, bootstrap_method, group_name);
        }
    }

    /// Handle PeerJoin event in Genesis mode
    fn handle_peer_join_genesis(
        &mut self,
        count: usize,
        coverage_fraction: f64,
        bootstrap_method: BootstrapMethod,
        group_name: String,
    ) {
        use super::token_allocation::GenesisTokenSet;
        use ec_rust::ec_genesis::generate_genesis;
        use ec_rust::ec_memory_backend::MemoryBackend;

        // Get genesis config
        let genesis_config = self
            .config
            .token_distribution
            .genesis_config
            .clone()
            .expect("Genesis config should be Some in genesis mode");

        // Re-create GenesisTokenSet to allocate new peer IDs
        // (This regenerates all token IDs - we could optimize by caching)
        let mut genesis_set =
            GenesisTokenSet::new(&genesis_config, StdRng::from_seed(self.rng.gen()));

        // Get existing peer IDs for bootstrap
        let existing_peer_ids: Vec<PeerId> = self.peers.keys().copied().collect();

        // Resolve bootstrap method to actual peer IDs
        let initial_knowledge = match bootstrap_method {
            BootstrapMethod::Random(n) => {
                use rand::seq::SliceRandom;
                existing_peer_ids
                    .choose_multiple(&mut self.rng, n)
                    .copied()
                    .collect()
            }
            BootstrapMethod::Specific(peers) => peers,
            BootstrapMethod::None => vec![],
        };

        // Allocate peer IDs for new peers
        let new_peer_ids = genesis_set
            .allocate_peer_ids(count)
            .expect("Failed to allocate peer IDs from genesis tokens");

        // Create each new peer with genesis generation
        for peer_id in &new_peer_ids {
            // Create peer manager
            let peer_rng = StdRng::from_seed(self.rng.gen());
            let mut peer_manager =
                EcPeers::with_config_and_rng(*peer_id, self.config.peer_config.clone(), peer_rng);

            // Create backend and run genesis (using shared RNG)
            let mut backend = MemoryBackend::new();
            generate_genesis(
                &mut backend,
                genesis_config.clone(),
                &mut peer_manager,
                coverage_fraction,
                &mut self.rng,
            )
            .expect("Genesis generation failed for late joiner");

            // Extract token storage
            let token_storage = backend.tokens().clone();

            // Add initial knowledge (bootstrap peers)
            for &known_peer_id in &initial_knowledge {
                if known_peer_id != *peer_id && self.peers.contains_key(&known_peer_id) {
                    peer_manager.add_identified_peer(known_peer_id, self.current_round as EcTime);
                }
            }

            let peer = SimPeer {
                peer_id: *peer_id,
                peer_manager,
                token_storage: SimTokenStorage::Memory(token_storage),
                known_tokens: Vec::new(),
                active: true,
            };

            self.peers.insert(*peer_id, peer);
        }

        // Create or update peer group
        if let Some(group) = self.peer_groups.get_mut(&group_name) {
            group.peer_ids.extend(new_peer_ids.iter().copied());
        } else {
            let group = PeerGroup {
                name: group_name.clone(),
                peer_ids: new_peer_ids.clone(),
                join_round: self.current_round,
                coverage_fraction,
            };
            self.peer_groups.insert(group_name.clone(), group);
        }

        // Track group membership
        for peer_id in new_peer_ids {
            self.peer_to_group.insert(peer_id, group_name.clone());
        }

        println!("  ✓ {} genesis peers joined successfully", count);
    }

    /// Handle PeerJoin event in Random mode
    fn handle_peer_join_random(
        &mut self,
        count: usize,
        coverage_fraction: f64,
        bootstrap_method: BootstrapMethod,
        group_name: String,
    ) {
        let global_mapping = self
            .global_mapping
            .as_mut()
            .expect("Global mapping not initialized in Random mode");

        // Resolve bootstrap method to actual peer IDs
        let initial_knowledge = match bootstrap_method {
            BootstrapMethod::Random(n) => {
                // Get existing peer IDs and randomly select N
                use rand::seq::SliceRandom;
                let existing_peers: Vec<PeerId> = global_mapping
                    .allocated_peer_ids()
                    .iter()
                    .copied()
                    .collect();

                existing_peers
                    .choose_multiple(&mut self.rng, n)
                    .copied()
                    .collect()
            }
            BootstrapMethod::Specific(peers) => peers,
            BootstrapMethod::None => vec![],
        };

        // Get existing peer IDs
        let existing_peer_ids: Vec<PeerId> = self.peers.keys().copied().collect();

        // Calculate view_width (same as for initial peers)
        let total_peers = existing_peer_ids.len() + count;
        let view_width = GlobalTokenMapping::calculate_view_width(
            total_peers,
            self.config.token_distribution.neighbor_overlap,
        );

        // Allocate new peer IDs and create peers
        let mut new_peer_ids = Vec::new();
        for _ in 0..count {
            // Allocate peer ID from token pool
            let peer_id = global_mapping
                .allocate_peer_id()
                .expect("Failed to allocate peer ID from token pool - increase total_tokens");

            // Get this peer's view as ready-to-use MemTokens
            let token_storage =
                global_mapping.get_peer_view(peer_id, view_width, coverage_fraction);

            // known_tokens is just for tracking (empty for now)
            let known_tokens = Vec::new();

            // Create peer manager with seeded RNG
            let peer_rng = StdRng::from_seed(self.rng.gen());
            let mut peer_manager =
                EcPeers::with_config_and_rng(peer_id, self.config.peer_config.clone(), peer_rng);

            // Add initial knowledge (bootstrap peers)
            // Note: initial_knowledge is passed from the event but could also use a strategy
            for &known_peer_id in &initial_knowledge {
                if known_peer_id != peer_id && self.peers.contains_key(&known_peer_id) {
                    peer_manager.add_identified_peer(known_peer_id, self.current_round as EcTime);
                }
            }

            let peer = SimPeer {
                peer_id,
                peer_manager,
                token_storage: SimTokenStorage::Memory(token_storage),
                known_tokens,
                active: true,
            };

            self.peers.insert(peer_id, peer);
            new_peer_ids.push(peer_id);
        }

        // Create or update peer group
        if let Some(group) = self.peer_groups.get_mut(&group_name) {
            // Group already exists, add new peers to it
            group.peer_ids.extend(new_peer_ids.iter().copied());
        } else {
            // Create new group
            let group = PeerGroup {
                name: group_name.clone(),
                peer_ids: new_peer_ids.clone(),
                join_round: self.current_round,
                coverage_fraction,
            };
            self.peer_groups.insert(group_name.clone(), group);
        }

        // Map peers to group
        for peer_id in new_peer_ids {
            self.peer_to_group.insert(peer_id, group_name.clone());
        }

        println!("    ✓ {} peers added to group '{}'", count, group_name);
    }

    /// Report current statistics (for ReportStats event)
    fn report_current_stats(&mut self, label: Option<String>) {
        use super::stats::{calculate_gradient_distribution, calculate_gradient_steepness};
        use std::collections::BTreeMap;

        let checkpoint_label = label.unwrap_or_else(|| format!("Round {}", self.current_round));

        println!("\n╔════════════════════════════════════════════════════════╗");
        println!("║  CHECKPOINT: {:<43} ║", checkpoint_label);
        println!("╚════════════════════════════════════════════════════════╝");

        // Collect current metrics (similar to collect_metrics but for immediate display)
        let mut connected_counts: Vec<usize> = Vec::new();
        let mut peer_steepness_map: BTreeMap<PeerId, f64> = BTreeMap::new();

        let mut total_identified = 0;
        let mut total_pending = 0;
        let mut total_connected = 0;
        let mut active_count = 0;

        let mut total_elections_started = 0;
        let mut total_elections_completed = 0;
        let mut total_elections_timeout = 0;
        let mut total_elections_splitbrain = 0;

        for peer in self.peers.values() {
            if peer.active {
                active_count += 1;

                let num_identified = peer.peer_manager.num_identified();
                let num_pending = peer.peer_manager.num_pending();
                let num_connected = peer.peer_manager.num_connected();

                total_identified += num_identified;
                total_pending += num_pending;
                total_connected += num_connected;
                connected_counts.push(num_connected);

                let (started, completed, timeout, splitbrain) =
                    peer.peer_manager.get_election_stats();
                total_elections_started += started;
                total_elections_completed += completed;
                total_elections_timeout += timeout;
                total_elections_splitbrain += splitbrain;

                let active_peers = peer.peer_manager.get_active_peers();
                let steepness = calculate_gradient_steepness(peer.peer_id, active_peers);
                peer_steepness_map.insert(peer.peer_id, steepness);
            }
        }

        println!("\n  Peer States:");
        println!("    Active: {}", active_count);
        println!(
            "    Identified: {} avg",
            if active_count > 0 {
                total_identified / active_count
            } else {
                0
            }
        );
        println!(
            "    Pending: {} avg",
            if active_count > 0 {
                total_pending / active_count
            } else {
                0
            }
        );
        println!(
            "    Connected: {} avg",
            if active_count > 0 {
                total_connected / active_count
            } else {
                0
            }
        );

        println!("\n  Elections:");
        println!("    Started: {}", total_elections_started);
        println!("    Completed: {}", total_elections_completed);
        println!("    Timed Out: {}", total_elections_timeout);
        println!("    Split-brain detected: {}", total_elections_splitbrain);
        if total_elections_started > 0 {
            let success_rate =
                (total_elections_completed as f64 / total_elections_started as f64) * 100.0;
            println!("    Success Rate: {:.1}%", success_rate);
        }

        if !connected_counts.is_empty() {
            let min = *connected_counts.iter().min().unwrap();
            let max = *connected_counts.iter().max().unwrap();
            let avg = connected_counts.iter().sum::<usize>() as f64 / connected_counts.len() as f64;

            println!(
                "\n  Connected Peers: min={}, max={}, avg={:.1}",
                min, max, avg
            );
        }

        if !peer_steepness_map.is_empty() {
            let gradient_dist = calculate_gradient_distribution(&peer_steepness_map, 4);
            println!(
                "\n  Locality Gradient: avg={:.3}, strong (≥0.7)={:.1}%",
                gradient_dist.avg_steepness, gradient_dist.near_ideal_percent
            );
        }

        let shape = self.calculate_gradient_shape_metrics();
        println!(
            "\n  Corrected Ring Target: fit={:.3}, active connected={:.1}, ideal={:.1}",
            shape.avg_target_fit,
            shape.avg_active_connected_peers,
            shape.avg_expected_active_degree
        );
        println!(
            "    Core={:.3}, fade={:.3} (target {:.3}), far leakage={:.3}",
            shape.avg_core_coverage,
            shape.avg_fade_coverage,
            shape.avg_fade_target,
            shape.avg_far_coverage
        );

        let dense_shape = self.calculate_dense_linear_shape_metrics();
        println!(
            "  Fixed dense-linear target: fit={:.3}, active connected={:.1}, ideal={:.1}",
            dense_shape.avg_target_fit,
            dense_shape.avg_active_connected_peers,
            dense_shape.avg_expected_active_degree
        );
        println!(
            "    Core={:.3}, fade={:.3} (target {:.3}), far={:.3}",
            dense_shape.avg_core_coverage,
            dense_shape.avg_fade_coverage,
            dense_shape.avg_fade_target,
            dense_shape.avg_far_coverage
        );

        self.report_referral_probe_diagnostics();
        self.report_peer_set_hole_diagnostics();

        println!(
            "\n  Messages: {} total ({} queries, {} answers, {} referrals)",
            self.total_messages.queries
                + self.total_messages.answers
                + self.total_messages.referrals,
            self.total_messages.queries,
            self.total_messages.answers,
            self.total_messages.referrals
        );

        // Per-group statistics
        if self.peer_groups.len() > 1 {
            println!("\n╔════════════════════════════════════════════════════════╗");
            println!("║  Per-Group Statistics                                  ║");
            println!("╚════════════════════════════════════════════════════════╝");

            for (group_name, group) in &self.peer_groups {
                // Calculate metrics for this group's peers
                let mut group_connected: Vec<usize> = Vec::new();
                let mut group_steepness: Vec<f64> = Vec::new();
                let mut group_elections_started = 0;
                let mut group_elections_completed = 0;

                for &peer_id in &group.peer_ids {
                    if let Some(peer) = self.peers.get(&peer_id) {
                        if peer.active {
                            let num_connected = peer.peer_manager.num_connected();
                            group_connected.push(num_connected);

                            let active_peers = peer.peer_manager.get_active_peers();
                            let steepness = calculate_gradient_steepness(peer_id, active_peers);
                            group_steepness.push(steepness);

                            let (started, completed, _, _) = peer.peer_manager.get_election_stats();
                            group_elections_started += started;
                            group_elections_completed += completed;
                        }
                    }
                }

                if !group_connected.is_empty() {
                    let avg_connected =
                        group_connected.iter().sum::<usize>() as f64 / group_connected.len() as f64;
                    let avg_locality =
                        group_steepness.iter().sum::<f64>() / group_steepness.len() as f64;
                    let success_rate = if group_elections_started > 0 {
                        (group_elections_completed as f64 / group_elections_started as f64) * 100.0
                    } else {
                        0.0
                    };

                    println!(
                        "\n  Group '{}' ({} peers, joined round {}, coverage {:.0}%):",
                        group_name,
                        group.peer_ids.len(),
                        group.join_round,
                        group.coverage_fraction * 100.0
                    );
                    println!("    Avg Connected: {:.1}", avg_connected);
                    println!("    Locality: {:.3}", avg_locality);
                    println!("    Election Success: {:.1}%", success_rate);
                }
            }
        }

        println!();
    }

    /// Build final simulation result
    fn build_result(self) -> SimulationResult {
        let final_metrics = self
            .metrics_history
            .last()
            .cloned()
            .unwrap_or_else(|| RoundMetrics::new(0, 0));

        let total_messages = self.total_messages.queries
            + self.total_messages.answers
            + self.total_messages.referrals;
        let messages_per_peer_per_round = if self.config.rounds > 0 && !self.peers.is_empty() {
            total_messages as f64 / (self.config.rounds * self.peers.len()) as f64
        } else {
            0.0
        };

        SimulationResult {
            config_summary: format!(
                "Peers: {}, Rounds: {}, Topology: {:?}",
                self.config.initial_state.num_peers,
                self.config.rounds,
                self.config.initial_state.initial_topology
            ),
            seed_used: self.seed,
            total_rounds: self.config.rounds,
            final_metrics,
            metrics_history: self.metrics_history,
            event_log: Vec::new(),
            convergence: ConvergenceAnalysis {
                bootstrap_convergence_time: None,
                post_churn_recovery_times: Vec::new(),
                achieved_peer_count: 0, // TODO: Calculate
                converged: false,
            },
            message_overhead: MessageOverhead {
                total_messages,
                queries_sent: self.total_messages.queries,
                answers_received: self.total_messages.answers,
                invitations_sent: 0,
                referrals_sent: self.total_messages.referrals,
                messages_per_peer_per_round,
                messages_per_election: 0.0,
            },
        }
    }
}
