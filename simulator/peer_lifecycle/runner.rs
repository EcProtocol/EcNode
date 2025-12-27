// Peer Lifecycle Simulator Runner

use super::config::PeerLifecycleConfig;
use super::stats::*;
use super::token_dist::GlobalTokenMapping;
use ec_rust::ec_interface::{EcTime, MessageTicket, PeerId, TokenId, TokenMapping, TOKENS_SIGNATURE_SIZE};
use ec_rust::ec_memory_backend::MemTokens;
use ec_rust::ec_peers::{EcPeers, PeerAction};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::{BTreeMap, VecDeque};

// ============================================================================
// Core Structures
// ============================================================================

/// Main simulator runner
pub struct PeerLifecycleRunner {
    config: PeerLifecycleConfig,
    rng: StdRng,
    current_round: usize,

    // Network state
    peers: BTreeMap<PeerId, SimPeer>,

    // Message queue
    messages: VecDeque<MessageEnvelope>,
    delayed_messages: VecDeque<MessageEnvelope>,

    // Metrics tracking
    metrics_history: Vec<RoundMetrics>,
    total_messages: MessageCounter,

    // Election tracking
    elections_started_total: usize,
    elections_completed_total: usize,
    elections_timeout_total: usize,
    elections_splitbrain_total: usize,
}

/// A simulated peer
struct SimPeer {
    peer_id: PeerId,
    peer_manager: EcPeers,
    token_storage: MemTokens,
    known_tokens: Vec<TokenId>,  // Tokens in this peer's view
    active: bool,
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
            current_round: 0,
            peers: BTreeMap::new(),
            messages: VecDeque::new(),
            delayed_messages: VecDeque::new(),
            metrics_history: Vec::new(),
            total_messages: MessageCounter::default(),
            elections_started_total: 0,
            elections_completed_total: 0,
            elections_timeout_total: 0,
            elections_splitbrain_total: 0,
        }
    }

    /// Run the simulation
    pub fn run(mut self) -> SimulationResult {
        // 1. Initialize network
        self.initialize_network();

        // 2. Run simulation rounds
        for round in 0..self.config.rounds {
            self.current_round = round;

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
        }

        // 3. Build final result
        self.build_result()
    }

    /// Initialize the peer network
    fn initialize_network(&mut self) {
        let num_peers = self.config.initial_state.num_peers;

        // Generate peer IDs
        let peer_ids: Vec<PeerId> = (0..num_peers)
            .map(|_| self.rng.gen())
            .collect();

        // Create global token mapping with peer IDs injected as tokens
        let mut global_mapping = GlobalTokenMapping::new(
            StdRng::from_seed(self.rng.gen()),
            peer_ids.clone(),
            self.config.token_distribution.total_tokens,
        );

        // Calculate view_width from neighbor_overlap parameter
        let view_width = GlobalTokenMapping::calculate_view_width(
            num_peers,
            self.config.token_distribution.neighbor_overlap,
        );

        // Create peers with views of the global mapping
        for peer_id in peer_ids {
            // Get this peer's view of the global token mapping
            let token_view = global_mapping.get_peer_view(
                peer_id,
                view_width,
                self.config.token_distribution.coverage_fraction,
            );

            // Create token storage backend (MemTokens)
            use ec_rust::ec_proof_of_storage::TokenStorageBackend;
            let mut token_storage = MemTokens::new();

            // Store all tokens from the view
            let mut known_tokens = Vec::new();
            let mut has_self_id = false;
            for (token_id, block_id) in token_view {
                token_storage.set(&token_id, &block_id, 0); // time=0 for simplicity
                known_tokens.push(token_id);

                if token_id == peer_id {
                    has_self_id = true;
                }
            }

            // Create peer manager
            let peer_manager = EcPeers::with_config(peer_id, self.config.peer_config.clone());

            let peer = SimPeer {
                peer_id,
                peer_manager,
                token_storage,
                known_tokens,
                active: true,
            };

            self.peers.insert(peer_id, peer);
        }

        // Initialize topology (seed initial peer knowledge and connections)
        self.initialize_topology(&global_mapping, view_width);
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
                    let mut known_peers: Vec<PeerId> = peer_ids.iter()
                        .filter(|&&id| id != *peer_id)
                        .copied()
                        .collect();

                    // Shuffle and select connected_fraction to make Connected
                    known_peers.shuffle(&mut self.rng);
                    let num_connected = (known_peers.len() as f64 * connected_fraction) as usize;

                    for (idx, &other_id) in known_peers.iter().enumerate() {
                        if idx < num_connected {
                            // Add as seed peer (will be promoted to Connected)
                            peer.peer_manager.add_seed_peer(other_id, 0);
                        } else {
                            // Add as Identified only
                            peer.peer_manager.update_peer(&other_id, 0);
                        }
                    }
                }
            }

            TopologyMode::LocalKnowledge { peer_knowledge_fraction, connected_fraction } => {
                // Peers know subset of neighbors based on view_width
                let mut total_known = 0;
                let mut total_connected = 0;

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

                    total_known += known_peers.len();
                    total_connected += num_connected;

                    for (idx, &other_id) in known_peers.iter().enumerate() {
                        if idx < num_connected {
                            // Add as seed peer (will be promoted to Connected)
                            peer.peer_manager.add_seed_peer(other_id, 0);
                        } else {
                            // Add as Identified only
                            peer.peer_manager.update_peer(&other_id, 0);
                        }
                    }
                }
            }

            TopologyMode::Ring { neighbors } => {
                // Ring topology - all neighbors are Connected
                let mut sorted_peers: Vec<_> = peer_ids.iter().copied().collect();
                sorted_peers.sort(); // Sort by ID for consistent ring

                for (i, peer_id) in sorted_peers.iter().enumerate() {
                    if let Some(peer) = self.peers.get_mut(peer_id) {
                        for offset in 1..=*neighbors {
                            // Forward neighbor
                            let forward_idx = (i + offset) % sorted_peers.len();
                            peer.peer_manager.add_seed_peer(sorted_peers[forward_idx], 0);

                            // Backward neighbor
                            let backward_idx = (i + sorted_peers.len() - offset) % sorted_peers.len();
                            peer.peer_manager.add_seed_peer(sorted_peers[backward_idx], 0);
                        }
                    }
                }
            }

            TopologyMode::Isolated => {
                // No initial connections - peers discover via elections
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
        let is_active = self.peers.get(&envelope.to)
            .map(|p| p.active)
            .unwrap_or(false);

        if !is_active {
            return;
        }

        match envelope.message {
            SimMessage::QueryToken { token, ticket } => {
                // Use EcPeers.handle_query to generate response
                if let Some(peer) = self.peers.get(&envelope.to) {
                    let action = peer.peer_manager.handle_query(&peer.token_storage, token, ticket, envelope.from);

                    if let Some(action) = action {
                        let sender_id = envelope.to;
                        let receiver = envelope.from;
                        match action {
                            PeerAction::SendAnswer { answer, signature, ticket } => {
                                self.send_message(sender_id, receiver, SimMessage::Answer {
                                    answer,
                                    signature,
                                    ticket,
                                });
                            }
                            PeerAction::SendReferral { token, ticket, suggested_peers } => {
                                self.send_message(sender_id, receiver, SimMessage::Referral {
                                    token,
                                    ticket,
                                    suggested_peers,
                                });
                            }
                            _ => {
                                // Ignore other action types
                            }
                        }
                    }
                }
            }

            SimMessage::Answer { answer, signature, ticket } => {
                // Peer received answer - route to election
                if let Some(peer) = self.peers.get_mut(&envelope.to) {
                    let current_time = self.current_round as EcTime;
                    peer.peer_manager.handle_answer(
                        &answer,
                        &signature,
                        ticket,
                        envelope.from,
                        current_time,
                        &peer.token_storage,
                    );
                }
            }

            SimMessage::Referral { token, ticket, suggested_peers } => {
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

                    // Process the returned actions (send new Query messages)
                    let peer_id = envelope.to;
                    for action in actions {
                        match action {
                            PeerAction::SendQuery { receiver, token, ticket } => {
                                self.send_message(peer_id, receiver, SimMessage::QueryToken { token, ticket });
                            }
                            PeerAction::SendAnswer { .. } |
                            PeerAction::SendReferral { .. } |
                            PeerAction::SendInvitation { .. } => {
                                // Ignore for now
                            }
                        }
                    }
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

        self.messages.push_back(MessageEnvelope { from, to, message });
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

                // Tick peer manager
                let actions = peer.peer_manager.tick(&peer.token_storage, current_time);

                // Process actions
                for action in actions {
                    match action {
                        PeerAction::SendQuery { receiver, token, ticket } => {
                            self.send_message(peer_id, receiver, SimMessage::QueryToken { token, ticket });
                        }
                        PeerAction::SendInvitation { receiver, answer, signature } => {
                            self.send_message(peer_id, receiver, SimMessage::Answer { answer, signature, ticket: 0 });
                        }
                        _ => {
                            panic!("Unexpected Action returned from tick")
                        }
                    }
                }
            }
        }
    }

    /// Check if should sample metrics this round
    fn should_sample_metrics(&self) -> bool {
        self.current_round % self.config.metrics.sample_interval == 0
    }

    /// Collect metrics for current round
    fn collect_metrics(&mut self) {
        let mut metrics = RoundMetrics::new(
            self.current_round,
            self.current_round as u64 * self.config.tick_duration_ms,
        );

        // Collect peer state counts
        let identified = 0;
        let pending = 0;
        let mut connected = 0;
        let mut active_count = 0;
        let mut connected_counts: Vec<usize> = Vec::new();
        let mut _quality_scores: Vec<f64> = Vec::new();

        for peer in self.peers.values() {
            if peer.active {
                active_count += 1;

                // Count states (simplified - we'd need to query peer_manager internal state)
                let num_connected = peer.peer_manager.num_connected();
                connected += num_connected;
                connected_counts.push(num_connected);

                // TODO: Track identified/pending counts
                // TODO: Track quality scores
            }
        }

        metrics.peer_counts = PeerCounts {
            total_peers: self.peers.len(),
            active_peers: active_count,
            identified,
            pending,
            connected: connected / active_count.max(1), // Average
        };

        // Network health
        if !connected_counts.is_empty() {
            let min = *connected_counts.iter().min().unwrap_or(&0);
            let max = *connected_counts.iter().max().unwrap_or(&0);
            let avg = connected_counts.iter().sum::<usize>() as f64 / connected_counts.len() as f64;

            metrics.network_health = NetworkHealth {
                min_connected_peers: min,
                max_connected_peers: max,
                avg_connected_peers: avg,
                stddev_connected_peers: 0.0, // TODO: Calculate
                ring_coverage_percent: 0.0, // TODO: Calculate
                partition_detected: false,
            };
        }

        // Election stats
        metrics.election_stats.total_elections_started = self.elections_started_total;
        metrics.election_stats.total_elections_completed = self.elections_completed_total;
        metrics.election_stats.total_elections_timed_out = self.elections_timeout_total;
        metrics.election_stats.total_split_brain_detected = self.elections_splitbrain_total;

        self.metrics_history.push(metrics);
    }

    /// Build final simulation result
    fn build_result(self) -> SimulationResult {
        let final_metrics = self.metrics_history.last().cloned().unwrap_or_else(|| {
            RoundMetrics::new(0, 0)
        });

        let total_messages = self.total_messages.queries + self.total_messages.answers + self.total_messages.referrals;
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
            seed_used: [0u8; 32], // TODO: Store actual seed
            total_rounds: self.config.rounds,
            final_metrics,
            metrics_history: self.metrics_history,
            event_log: Vec::new(),
            convergence: ConvergenceAnalysis {
                bootstrap_convergence_time: None,
                post_churn_recovery_times: Vec::new(),
                target_peer_count: self.config.peer_config.total_budget,
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
