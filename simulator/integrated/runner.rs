use std::cell::RefCell;
use std::cmp::min;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, RngCore, SeedableRng};

use ec_rust::ec_interface::{
    BatchRequestItem, Block, BlockId, EcBlocks, Message, MessageEnvelope, PeerId,
    PublicKeyReference, TokenBlock, TokenId, GENESIS_BLOCK_ID, TOKENS_PER_BLOCK,
};
use ec_rust::ec_memory_backend::{MemTokens, MemoryBackend};
use ec_rust::ec_node::{EcNode, VoteIngressDiagnostics};
use ec_rust::ec_proof_of_storage::TokenStorageBackend;

use crate::integrated::{
    ConflictWorkloadSummary, DistributionSummary, FloatDistributionSummary, IntegratedSimConfig,
    MempoolPressureSummary, MessageTypeBreakdown, NeighborhoodBucketSummary, NeighborhoodSummary,
    OnboardingSummary, RecoverySummary, RoundMetrics, SimResult, TransactionSourcePolicy,
    TransactionSpreadSummary, TransactionWorkloadSummary, VoteIngressSummary,
};
use crate::peer_lifecycle::{
    BootstrapMethod, GlobalTokenMapping, NetworkEvent, PeerSelection, ScheduledEvent, TopologyMode,
};
use crate::peer_lifecycle::token_allocation::GenesisTokenSet;

const RECOVERY_WINDOW: usize = 12;
const RECOVERY_THRESHOLD: f64 = 0.90;

struct SimPeer {
    backend: Rc<RefCell<MemoryBackend>>,
    token_storage: MemTokens,
    node: EcNode<MemoryBackend, MemTokens>,
    active: bool,
    coverage_fraction: f64,
    restart_count: u64,
}

struct TrackedBlock {
    owner: PeerId,
    submitted_round: usize,
    max_entry_hops: usize,
    ideal_role_sum_lower_bound_messages: usize,
    ideal_coalesced_lower_bound_messages: usize,
    delivered_block_messages: usize,
    touched_peers: HashSet<PeerId>,
}

struct ConflictFamily {
    token: TokenId,
    parent_block: BlockId,
    candidate_block_ids: Vec<BlockId>,
    highest_candidate: BlockId,
    created_round: usize,
    owner_committed_candidates: HashSet<BlockId>,
    conflict_signal_receivers: HashSet<PeerId>,
}

struct ScheduledMessage {
    deliver_round: usize,
    envelope: MessageEnvelope,
}

#[derive(Default)]
struct PeerProgress {
    joined_round: usize,
    bootstrap_seed_count: usize,
    late_joiner: bool,
    connected_round: Option<usize>,
    known_head_round: Option<usize>,
    sync_trace_round: Option<usize>,
}

struct RejoinProgress {
    restarted_round: usize,
    bootstrap_seed_count: usize,
    connected_round: Option<usize>,
    known_head_round: Option<usize>,
    sync_trace_round: Option<usize>,
}

struct RecoveryWatch {
    label: String,
    start_round: usize,
    baseline_commit_rate: f64,
    recovered_round: Option<usize>,
}

#[derive(Clone, Default)]
struct NeighborhoodBucketAccumulator {
    coverage_sizes: Vec<usize>,
    vote_eligible_sizes: Vec<usize>,
    entry_hops: Vec<usize>,
    commit_latencies: Vec<usize>,
    committed_blocks: usize,
}

#[derive(Default)]
struct TransactionSpreadAccumulator {
    reachable_vote_peers: Vec<usize>,
    reachable_vote_edges: Vec<usize>,
    witness_coverage: Vec<usize>,
    ideal_role_sum_lower_bound_messages: Vec<usize>,
    ideal_coalesced_lower_bound_messages: Vec<usize>,
    settled_peer_spread: Vec<usize>,
    settled_block_messages: Vec<usize>,
    actual_to_role_sum_ratio: Vec<f64>,
    actual_to_coalesced_ratio: Vec<f64>,
    total_actual_block_messages: usize,
    total_ideal_role_sum_lower_bound_messages: usize,
    total_ideal_coalesced_lower_bound_messages: usize,
}

const LOCAL_ENTRY_MAX_HOPS: usize = 4;
const NEAR_ENTRY_MAX_HOPS: usize = 16;
const MID_ENTRY_MAX_HOPS: usize = 64;
const NEIGHBORHOOD_BUCKET_LABELS: [&str; 4] = [
    "local (<=4 hops)",
    "near (5-16 hops)",
    "mid (17-64 hops)",
    "far (65+ hops)",
];

enum TokenSpace {
    Random(GlobalTokenMapping),
    Genesis(GenesisTokenSet),
}

fn message_logical_count(message: &Message) -> usize {
    match message {
        Message::RequestBatch { items } => items.len(),
        _ => 1,
    }
}

pub struct IntegratedRunner {
    config: IntegratedSimConfig,
    rng: StdRng,
    seed_used: [u8; 32],
    current_round: usize,
    peers: BTreeMap<PeerId, SimPeer>,
    peer_progress: BTreeMap<PeerId, PeerProgress>,
    rejoin_progress: Vec<RejoinProgress>,
    active_rejoins: BTreeMap<PeerId, usize>,
    token_space: TokenSpace,
    scheduled_events: Vec<ScheduledEvent>,
    next_event_idx: usize,
    outbound_messages: Vec<MessageEnvelope>,
    in_flight_messages: Vec<ScheduledMessage>,
    tracked_blocks: BTreeMap<BlockId, TrackedBlock>,
    conflict_families: Vec<ConflictFamily>,
    conflict_block_to_family: BTreeMap<BlockId, usize>,
    submission_attempts: usize,
    submitted_blocks: usize,
    skipped_submissions: usize,
    committed_blocks: usize,
    total_messages_delivered: usize,
    total_wire_messages_delivered: usize,
    peak_in_flight_messages: usize,
    peak_active_traces: usize,
    peak_active_elections: usize,
    scheduled_message_types: MessageTypeBreakdown,
    delivered_message_types: MessageTypeBreakdown,
    scheduled_wire_message_types: MessageTypeBreakdown,
    delivered_wire_message_types: MessageTypeBreakdown,
    last_vote_diagnostics: BTreeMap<PeerId, VoteIngressDiagnostics>,
    cumulative_trusted_votes_recorded: usize,
    cumulative_untrusted_votes_received: usize,
    cumulative_vote_block_requests: usize,
    commit_latencies: Vec<usize>,
    network_transit_samples: Vec<usize>,
    neighborhood_coverage_samples: Vec<usize>,
    neighborhood_vote_eligible_samples: Vec<usize>,
    neighborhood_entry_hop_samples: Vec<usize>,
    local_entry_token_samples: usize,
    neighborhood_buckets: [NeighborhoodBucketAccumulator; 4],
    transaction_spread: TransactionSpreadAccumulator,
    round_commits: Vec<usize>,
    round_metrics: Vec<RoundMetrics>,
    recovery_watches: Vec<RecoveryWatch>,
    existing_token_parts_generated: usize,
    new_token_parts_generated: usize,
    blocks_with_existing_tokens: usize,
}

impl IntegratedRunner {
    pub fn new(config: IntegratedSimConfig) -> Self {
        let seed_used = config.seed.unwrap_or_else(|| {
            let mut seed = [0u8; 32];
            rand::thread_rng().fill(&mut seed);
            seed
        });
        let rng = StdRng::from_seed(seed_used);

        let token_space = {
            let mut seeded = StdRng::from_seed(seed_used);
            let mapping_seed = seeded.gen();
            if let Some(genesis_config) = &config.token_distribution.genesis_config {
                TokenSpace::Genesis(GenesisTokenSet::new(
                    genesis_config,
                    StdRng::from_seed(mapping_seed),
                ))
            } else {
                TokenSpace::Random(GlobalTokenMapping::new(
                    StdRng::from_seed(mapping_seed),
                    config.token_distribution.total_tokens,
                ))
            }
        };

        let mut scheduled_events = config.events.events.clone();
        scheduled_events.sort_by_key(|event| event.round);

        Self {
            config,
            rng,
            seed_used,
            current_round: 0,
            peers: BTreeMap::new(),
            peer_progress: BTreeMap::new(),
            rejoin_progress: Vec::new(),
            active_rejoins: BTreeMap::new(),
            token_space,
            scheduled_events,
            next_event_idx: 0,
            outbound_messages: Vec::new(),
            in_flight_messages: Vec::new(),
            tracked_blocks: BTreeMap::new(),
            conflict_families: Vec::new(),
            conflict_block_to_family: BTreeMap::new(),
            submission_attempts: 0,
            submitted_blocks: 0,
            skipped_submissions: 0,
            committed_blocks: 0,
            total_messages_delivered: 0,
            total_wire_messages_delivered: 0,
            peak_in_flight_messages: 0,
            peak_active_traces: 0,
            peak_active_elections: 0,
            scheduled_message_types: MessageTypeBreakdown::default(),
            delivered_message_types: MessageTypeBreakdown::default(),
            scheduled_wire_message_types: MessageTypeBreakdown::default(),
            delivered_wire_message_types: MessageTypeBreakdown::default(),
            last_vote_diagnostics: BTreeMap::new(),
            cumulative_trusted_votes_recorded: 0,
            cumulative_untrusted_votes_received: 0,
            cumulative_vote_block_requests: 0,
            commit_latencies: Vec::new(),
            network_transit_samples: Vec::new(),
            neighborhood_coverage_samples: Vec::new(),
            neighborhood_vote_eligible_samples: Vec::new(),
            neighborhood_entry_hop_samples: Vec::new(),
            local_entry_token_samples: 0,
            neighborhood_buckets: std::array::from_fn(|_| NeighborhoodBucketAccumulator::default()),
            transaction_spread: TransactionSpreadAccumulator::default(),
            round_commits: Vec::new(),
            round_metrics: Vec::new(),
            recovery_watches: Vec::new(),
            existing_token_parts_generated: 0,
            new_token_parts_generated: 0,
            blocks_with_existing_tokens: 0,
        }
    }

    pub fn run(mut self) -> SimResult {
        self.initialize_network();

        for round in 0..self.config.rounds {
            self.current_round = round;

            self.process_scheduled_events();
            let commits_this_round = self.collect_commits();
            self.round_commits.push(commits_this_round);
            self.inject_blocks();
            self.schedule_outbound_messages();
            self.deliver_messages();
            self.tick_nodes();
            self.observe_peer_progress();
            self.update_peaks();
            self.update_recovery_watches();
            self.record_round_metrics(commits_this_round);
        }

        self.build_result()
    }

    fn initialize_network(&mut self) {
        let num_peers = self.config.initial_state.num_peers;
        let mut peer_ids = Vec::with_capacity(num_peers);

        for _ in 0..num_peers {
            let peer_id = self
                .allocate_peer_id()
                .expect("Failed to allocate peer ID for integrated simulation");
            peer_ids.push(peer_id);
        }

        for &peer_id in &peer_ids {
            let peer = self.create_peer(
                peer_id,
                self.config.token_distribution.coverage_fraction,
                matches!(&self.token_space, TokenSpace::Genesis(_)),
            );
            self.peers.insert(peer_id, peer);
            self.peer_progress.insert(
                peer_id,
                PeerProgress {
                    joined_round: 0,
                    late_joiner: false,
                    ..PeerProgress::default()
                },
            );
        }

        self.apply_initial_topology(&peer_ids);
        self.observe_peer_progress();
        self.update_peaks();
    }

    fn allocate_peer_id(&mut self) -> Option<PeerId> {
        match &mut self.token_space {
            TokenSpace::Random(mapping) => mapping.allocate_peer_id(),
            TokenSpace::Genesis(set) => set.allocate_peer_id(),
        }
    }

    fn peer_count(&self) -> usize {
        match &self.token_space {
            TokenSpace::Random(mapping) => mapping.peer_count(),
            TokenSpace::Genesis(set) => set.peer_count(),
        }
    }

    fn view_width(&self, num_peers: usize) -> u64 {
        GlobalTokenMapping::calculate_view_width(
            num_peers.max(1),
            self.config.token_distribution.neighbor_overlap,
        )
    }

    fn create_genesis_block(token_id: TokenId, block_id: BlockId) -> Block {
        let mut parts = [TokenBlock::default(); TOKENS_PER_BLOCK];
        parts[0] = TokenBlock {
            token: token_id,
            last: 0,
            key: 0,
        };

        Block {
            id: block_id,
            time: 0,
            used: 1,
            parts,
            signatures: [None; TOKENS_PER_BLOCK],
        }
    }

    fn build_token_storage(&mut self, peer_id: PeerId, coverage_fraction: f64) -> MemTokens {
        let peer_count = self.peer_count();
        let view_width = self.view_width(peer_count);

        match &mut self.token_space {
            TokenSpace::Random(mapping) => mapping.get_peer_view(peer_id, view_width, coverage_fraction),
            TokenSpace::Genesis(set) => set.get_peer_view(
                peer_id,
                self.config.token_distribution.genesis_storage_fraction,
            ),
        }
    }

    fn nearby_peers(&self, peer_id: PeerId, view_width: u64) -> Vec<PeerId> {
        match &self.token_space {
            TokenSpace::Random(mapping) => mapping.get_nearby_peers(peer_id, view_width),
            TokenSpace::Genesis(set) => set.get_nearby_peers(peer_id, view_width),
        }
    }

    fn populate_genesis_backend(&self, backend: &mut MemoryBackend, peer_id: PeerId) {
        let TokenSpace::Genesis(set) = &self.token_space else {
            return;
        };

        let storage_fraction = self.config.token_distribution.genesis_storage_fraction;
        for (token_id, block_id) in set.peer_mappings(peer_id, storage_fraction) {
            TokenStorageBackend::set(
                backend.tokens_mut(),
                &token_id,
                &block_id,
                &GENESIS_BLOCK_ID,
                0,
            );
            EcBlocks::save(
                backend.blocks_mut(),
                &Self::create_genesis_block(token_id, block_id),
            );
        }
    }

    fn seed_genesis_samples(&mut self, node: &mut EcNode<MemoryBackend, MemTokens>) {
        const SEED_SAMPLE_PROBABILITY: f64 = 0.01;

        let TokenSpace::Genesis(set) = &self.token_space else {
            return;
        };

        for token in set.sample_seed_tokens(&mut self.rng, SEED_SAMPLE_PROBABILITY) {
            node.seed_genesis_token(token);
        }
    }

    fn build_node(
        &mut self,
        peer_id: PeerId,
        backend: Rc<RefCell<MemoryBackend>>,
        token_storage: MemTokens,
        restart_count: u64,
    ) -> EcNode<MemoryBackend, MemTokens> {
        let mut node_seed = [0u8; 32];
        node_seed[0..8].copy_from_slice(&peer_id.to_le_bytes());
        node_seed[8..16].copy_from_slice(&restart_count.to_le_bytes());
        node_seed[16..].copy_from_slice(&self.seed_used[16..]);
        let node_rng = StdRng::from_seed(node_seed);

        EcNode::new_with_peer_config(
            backend,
            peer_id,
            0,
            token_storage,
            self.config.peer_config.clone(),
            node_rng,
        )
    }

    fn create_peer(
        &mut self,
        peer_id: PeerId,
        coverage_fraction: f64,
        initialize_backend_from_genesis: bool,
    ) -> SimPeer {
        let mut backend = MemoryBackend::new_with_peer_id(peer_id);
        if initialize_backend_from_genesis && matches!(&self.token_space, TokenSpace::Genesis(_)) {
            self.populate_genesis_backend(&mut backend, peer_id);
        }
        let backend = Rc::new(RefCell::new(backend));
        let token_storage = self.build_token_storage(peer_id, coverage_fraction);
        let mut node = self.build_node(peer_id, backend.clone(), token_storage.clone(), 0);
        self.seed_genesis_samples(&mut node);
        self.last_vote_diagnostics.insert(peer_id, VoteIngressDiagnostics::default());

        SimPeer {
            backend,
            token_storage,
            node,
            active: true,
            coverage_fraction,
            restart_count: 0,
        }
    }

    fn apply_initial_topology(&mut self, peer_ids: &[PeerId]) {
        let view_width = GlobalTokenMapping::calculate_view_width(
            peer_ids.len().max(1),
            self.config.token_distribution.neighbor_overlap,
        );
        let sorted_peer_ids: Vec<PeerId> = {
            let mut ids = peer_ids.to_vec();
            ids.sort_unstable();
            ids
        };

        for &peer_id in peer_ids {
            let seeds = match &self.config.initial_state.initial_topology {
                TopologyMode::FullyKnown { .. } => peer_ids
                    .iter()
                    .copied()
                    .filter(|candidate| *candidate != peer_id)
                    .collect(),
                TopologyMode::LocalKnowledge {
                    peer_knowledge_fraction,
                    ..
                } => {
                    let nearby = self.nearby_peers(peer_id, view_width);
                    nearby
                        .into_iter()
                        .filter(|_| self.rng.gen_bool(*peer_knowledge_fraction))
                        .collect()
                }
                TopologyMode::Ring { neighbors } => {
                    let idx = sorted_peer_ids
                        .iter()
                        .position(|candidate| *candidate == peer_id)
                        .expect("peer_id should exist in sorted list");
                    let mut ring_seeds = HashSet::new();
                    for offset in 1..=*neighbors {
                        ring_seeds.insert(sorted_peer_ids[(idx + offset) % sorted_peer_ids.len()]);
                        ring_seeds.insert(
                            sorted_peer_ids
                                [(idx + sorted_peer_ids.len() - offset) % sorted_peer_ids.len()],
                        );
                    }
                    ring_seeds.into_iter().collect()
                }
                TopologyMode::RandomIdentified { peers_per_node } => peer_ids
                    .iter()
                    .copied()
                    .filter(|candidate| *candidate != peer_id)
                    .collect::<Vec<_>>()
                    .choose_multiple(&mut self.rng, *peers_per_node)
                    .copied()
                    .collect(),
                TopologyMode::Isolated => Vec::new(),
            };

            if let Some(peer) = self.peers.get_mut(&peer_id) {
                for seed in &seeds {
                    peer.node.seed_peer(seed);
                }
            }
            if let Some(progress) = self.peer_progress.get_mut(&peer_id) {
                progress.bootstrap_seed_count = seeds.len();
            }
        }
    }

    fn process_scheduled_events(&mut self) {
        while self
            .scheduled_events
            .get(self.next_event_idx)
            .is_some_and(|event| event.round == self.current_round)
        {
            let event = self.scheduled_events[self.next_event_idx].clone();
            self.next_event_idx += 1;
            self.handle_event(event);
        }
    }

    fn handle_event(&mut self, event: ScheduledEvent) {
        match event.event {
            NetworkEvent::PeerJoin {
                count,
                coverage_fraction,
                bootstrap_method,
                ..
            } => self.handle_peer_join(count, coverage_fraction, bootstrap_method),
            NetworkEvent::PeerCrash { selection } => {
                self.start_recovery_watch(format!("peer-crash@{}", self.current_round));
                let selected = self.select_peers_by_activity(selection, true);
                for peer_id in &selected {
                    if let Some(peer) = self.peers.get_mut(peer_id) {
                        peer.active = false;
                    }
                }
                println!(
                    "[round {}] deactivated {} peers",
                    self.current_round,
                    selected.len()
                );
            }
            NetworkEvent::PeerLeave { selection } => {
                self.start_recovery_watch(format!("peer-leave@{}", self.current_round));
                let selected = self.select_peers_by_activity(selection, true);
                for peer_id in &selected {
                    if let Some(peer) = self.peers.get_mut(peer_id) {
                        peer.active = false;
                    }
                }
                println!(
                    "[round {}] deactivated {} peers",
                    self.current_round,
                    selected.len()
                );
            }
            NetworkEvent::PeerReturn {
                selection,
                bootstrap_method,
            } => {
                self.handle_peer_return(selection, bootstrap_method);
            }
            NetworkEvent::NetworkCondition {
                delay_fraction,
                loss_fraction,
            } => {
                self.start_recovery_watch(format!("network-change@{}", self.current_round));
                if let Some(delay) = delay_fraction {
                    self.config.network.delay_fraction = delay;
                }
                if let Some(loss) = loss_fraction {
                    self.config.network.loss_fraction = loss;
                }
                println!(
                    "[round {}] network updated: delay {:.0}% loss {:.0}%",
                    self.current_round,
                    self.config.network.delay_fraction * 100.0,
                    self.config.network.loss_fraction * 100.0
                );
            }
            NetworkEvent::ReportStats { label } => {
                let label = label.unwrap_or_else(|| "checkpoint".to_string());
                self.print_checkpoint(&label);
            }
            NetworkEvent::PauseElections { duration } => {
                println!(
                    "[round {}] PauseElections({}) not implemented in IntegratedRunner yet",
                    self.current_round, duration
                );
            }
        }
    }

    fn handle_peer_join(
        &mut self,
        count: usize,
        coverage_fraction: f64,
        bootstrap_method: BootstrapMethod,
    ) {
        let active_peers = self.active_peer_ids();
        let bootstrap_peers = match bootstrap_method {
            BootstrapMethod::Random(n) => active_peers
                .choose_multiple(&mut self.rng, n)
                .copied()
                .collect::<Vec<_>>(),
            BootstrapMethod::Specific(peers) => peers,
            BootstrapMethod::None => Vec::new(),
        };

        let mut joined = 0;
        for _ in 0..count {
            let Some(peer_id) = self.allocate_peer_id() else {
                break;
            };

            let mut peer = self.create_peer(peer_id, coverage_fraction, false);
            for seed in &bootstrap_peers {
                peer.node.add_identified_peer(*seed);
            }
            self.peers.insert(peer_id, peer);
            self.peer_progress.insert(
                peer_id,
                PeerProgress {
                    joined_round: self.current_round,
                    bootstrap_seed_count: bootstrap_peers.len(),
                    late_joiner: true,
                    ..PeerProgress::default()
                },
            );
            joined += 1;
        }

        println!(
            "[round {}] joined {} new peers with {:.0}% coverage",
            self.current_round,
            joined,
            coverage_fraction * 100.0
        );
    }

    fn handle_peer_return(
        &mut self,
        selection: PeerSelection,
        bootstrap_method: BootstrapMethod,
    ) {
        let inactive_peers = self.inactive_peer_ids();
        let bootstrap_peers = match bootstrap_method {
            BootstrapMethod::Random(n) => self
                .active_peer_ids()
                .choose_multiple(&mut self.rng, n)
                .copied()
                .collect::<Vec<_>>(),
            BootstrapMethod::Specific(peers) => peers,
            BootstrapMethod::None => Vec::new(),
        };

        let selected = self.select_peers_by_activity(selection, false);
        let mut rejoined = 0;

        for peer_id in selected {
            let Some((backend, token_storage, restart_count)) = self.peers.get(&peer_id).map(|peer| {
                (
                    peer.backend.clone(),
                    peer.token_storage.clone(),
                    peer.restart_count + 1,
                )
            }) else {
                continue;
            };

            backend.borrow_mut().reset_runtime_state();
            let mut node = self.build_node(peer_id, backend, token_storage, restart_count);
            self.seed_genesis_samples(&mut node);
            for seed in &bootstrap_peers {
                if *seed != peer_id {
                    node.add_identified_peer(*seed);
                }
            }

            if let Some(peer) = self.peers.get_mut(&peer_id) {
                peer.node = node;
                peer.active = true;
                peer.restart_count = restart_count;
            }
            self.last_vote_diagnostics
                .insert(peer_id, VoteIngressDiagnostics::default());

            let idx = self.rejoin_progress.len();
            self.rejoin_progress.push(RejoinProgress {
                restarted_round: self.current_round,
                bootstrap_seed_count: bootstrap_peers.len(),
                connected_round: None,
                known_head_round: None,
                sync_trace_round: None,
            });
            self.active_rejoins.insert(peer_id, idx);
            rejoined += 1;
        }

        println!(
            "[round {}] reactivated {} peers from {} inactive",
            self.current_round,
            rejoined,
            inactive_peers.len()
        );
    }

    fn select_peers_by_activity(&mut self, selection: PeerSelection, active_only: bool) -> Vec<PeerId> {
        let mut active: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|(_, peer)| peer.active == active_only)
            .map(|(peer_id, _)| *peer_id)
            .collect();

        match selection {
            PeerSelection::Random { count } => {
                active.shuffle(&mut self.rng);
                active.into_iter().take(count).collect()
            }
            PeerSelection::Specific { peer_ids } => peer_ids
                .into_iter()
                .filter(|peer_id| self.peers.get(peer_id).is_some_and(|peer| peer.active == active_only))
                .collect(),
            PeerSelection::ByQuality { count, worst } => {
                let mut ranked: Vec<_> = self
                    .peers
                    .iter()
                    .filter(|(_, peer)| peer.active == active_only)
                    .map(|(peer_id, peer)| (*peer_id, peer.coverage_fraction))
                    .collect();
                ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                if !worst {
                    ranked.reverse();
                }
                ranked
                    .into_iter()
                    .take(count)
                    .map(|(peer_id, _)| peer_id)
                    .collect()
            }
            PeerSelection::ByTokenCount { count, most } => {
                let mut ranked: Vec<_> = self
                    .peers
                    .iter()
                    .filter(|(_, peer)| peer.active == active_only)
                    .map(|(peer_id, peer)| {
                        let token_count = TokenStorageBackend::len(&*peer.backend.borrow());
                        (*peer_id, token_count)
                    })
                    .collect();
                ranked.sort_by_key(|entry| entry.1);
                if most {
                    ranked.reverse();
                }
                ranked
                    .into_iter()
                    .take(count)
                    .map(|(peer_id, _)| peer_id)
                    .collect()
            }
        }
    }

    fn collect_commits(&mut self) -> usize {
        let committed: Vec<_> = self
            .tracked_blocks
            .iter()
            .filter_map(|(block_id, tracked)| {
                let Some(peer) = self.peers.get(&tracked.owner) else {
                    return None;
                };
                if peer.node.committed_block(block_id).is_some() {
                    Some(*block_id)
                } else {
                    None
                }
            })
            .collect();

        for block_id in &committed {
            if let Some(tracked) = self.tracked_blocks.remove(block_id) {
                if let Some(family_idx) = self.conflict_block_to_family.get(block_id).copied() {
                    if let Some(family) = self.conflict_families.get_mut(family_idx) {
                        family.owner_committed_candidates.insert(*block_id);
                    }
                }
                let latency = self.current_round.saturating_sub(tracked.submitted_round);
                self.commit_latencies.push(latency);
                let bucket = Self::entry_hop_bucket(tracked.max_entry_hops);
                self.neighborhood_buckets[bucket].commit_latencies.push(latency);
                self.neighborhood_buckets[bucket].committed_blocks += 1;
                self.transaction_spread
                    .settled_peer_spread
                    .push(tracked.touched_peers.len());
                self.transaction_spread
                    .settled_block_messages
                    .push(tracked.delivered_block_messages);
                self.transaction_spread
                    .ideal_role_sum_lower_bound_messages
                    .push(tracked.ideal_role_sum_lower_bound_messages);
                self.transaction_spread
                    .ideal_coalesced_lower_bound_messages
                    .push(tracked.ideal_coalesced_lower_bound_messages);
                self.transaction_spread.total_actual_block_messages += tracked.delivered_block_messages;
                self.transaction_spread.total_ideal_role_sum_lower_bound_messages +=
                    tracked.ideal_role_sum_lower_bound_messages;
                self.transaction_spread.total_ideal_coalesced_lower_bound_messages +=
                    tracked.ideal_coalesced_lower_bound_messages;
                if tracked.ideal_role_sum_lower_bound_messages > 0 {
                    self.transaction_spread.actual_to_role_sum_ratio.push(
                        tracked.delivered_block_messages as f64
                            / tracked.ideal_role_sum_lower_bound_messages as f64,
                    );
                }
                if tracked.ideal_coalesced_lower_bound_messages > 0 {
                    self.transaction_spread.actual_to_coalesced_ratio.push(
                        tracked.delivered_block_messages as f64
                            / tracked.ideal_coalesced_lower_bound_messages as f64,
                    );
                }
                self.committed_blocks += 1;
            }
        }

        committed.len()
    }

    fn eligible_transaction_sources(&self) -> Vec<PeerId> {
        let mut sources = self.active_peer_ids();
        if matches!(
            self.config.transactions.source_policy,
            TransactionSourcePolicy::ConnectedOnly
        ) {
            sources.retain(|peer_id| {
                self.peers
                    .get(peer_id)
                    .is_some_and(|peer| peer.node.num_connected_peers() > 0)
            });
        }
        sources
    }

    fn entry_hop_bucket(entry_hops: usize) -> usize {
        if entry_hops <= LOCAL_ENTRY_MAX_HOPS {
            0
        } else if entry_hops <= NEAR_ENTRY_MAX_HOPS {
            1
        } else if entry_hops <= MID_ENTRY_MAX_HOPS {
            2
        } else {
            3
        }
    }

    fn collect_covering_peers(&self, token: TokenId) -> Vec<PeerId> {
        self.peers
            .values()
            .filter_map(|peer| {
                if peer.active && peer.node.local_scope_contains(token) {
                    Some(peer.node.get_peer_id())
                } else {
                    None
                }
            })
            .collect()
    }

    fn count_covering_peers(&self, token: TokenId) -> usize {
        self.collect_covering_peers(token).len()
    }

    fn role_sum_lower_bound_messages(
        &self,
        token_neighborhoods: &[Vec<PeerId>],
        witness_neighborhood: &[PeerId],
    ) -> usize {
        token_neighborhoods
            .iter()
            .map(|peers| peers.len().saturating_mul(2))
            .sum::<usize>()
            + witness_neighborhood.len().saturating_mul(2)
    }

    fn coalesced_lower_bound_messages(
        &self,
        token_neighborhoods: &[Vec<PeerId>],
        witness_neighborhood: &[PeerId],
    ) -> usize {
        let mut union = HashSet::new();
        for peers in token_neighborhoods {
            union.extend(peers.iter().copied());
        }
        union.extend(witness_neighborhood.iter().copied());
        union.len().saturating_mul(2)
    }

    fn sample_existing_mapping(&mut self, peer_id: PeerId) -> Option<(TokenId, BlockId)> {
        let peer = self.peers.get(&peer_id)?;
        let backend = peer.backend.borrow();
        backend
            .tokens()
            .sample_current_mapping(&mut self.rng)
            .map(|(token_id, mapping)| (token_id, mapping.block))
    }

    fn record_neighborhood_sample(
        &mut self,
        coverage_size: usize,
        vote_eligible_size: usize,
        entry_hops: usize,
        entry_is_local: bool,
    ) {
        self.neighborhood_coverage_samples.push(coverage_size);
        self.neighborhood_vote_eligible_samples
            .push(vote_eligible_size);
        self.neighborhood_entry_hop_samples.push(entry_hops);
        if entry_is_local {
            self.local_entry_token_samples += 1;
        }

        let bucket = Self::entry_hop_bucket(entry_hops);
        self.neighborhood_buckets[bucket]
            .coverage_sizes
            .push(coverage_size);
        self.neighborhood_buckets[bucket]
            .vote_eligible_sizes
            .push(vote_eligible_size);
        self.neighborhood_buckets[bucket]
            .entry_hops
            .push(entry_hops);
    }

    fn extend_vote_graph(
        &self,
        origin: PeerId,
        token: TokenId,
        time: u64,
        reachable_peers: &mut HashSet<PeerId>,
        reachable_edges: &mut HashSet<(PeerId, PeerId)>,
    ) {
        let mut frontier = vec![origin];

        while let Some(peer_id) = frontier.pop() {
            if !reachable_peers.insert(peer_id) {
                continue;
            }

            let Some(peer) = self.peers.get(&peer_id) else {
                continue;
            };
            if !peer.active {
                continue;
            }

            for target in peer.node.vote_targets_for_token_at(token, time) {
                if target == 0 {
                    continue;
                }
                if !self.peers.get(&target).is_some_and(|candidate| candidate.active) {
                    continue;
                }

                reachable_edges.insert((peer_id, target));
                if !reachable_peers.contains(&target) {
                    frontier.push(target);
                }
            }
        }
    }

    fn block_ids_for_message(message: &Message) -> Vec<BlockId> {
        match message {
            Message::Vote { block_id, .. } => vec![*block_id],
            Message::QueryBlock { block_id, .. } => vec![*block_id],
            Message::Block { block } => vec![block.id],
            Message::RequestBatch { items } => items
                .iter()
                .filter_map(|item| match item {
                    BatchRequestItem::Vote { block_id, .. } => Some(*block_id),
                    BatchRequestItem::QueryBlock { block_id, .. } => Some(*block_id),
                    BatchRequestItem::QueryToken { .. } => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn track_delivered_block_message(&mut self, envelope: &MessageEnvelope) {
        let block_ids = Self::block_ids_for_message(&envelope.message);
        if block_ids.is_empty() {
            return;
        }

        for block_id in block_ids {
            let Some(tracked) = self.tracked_blocks.get_mut(&block_id) else {
                continue;
            };

            tracked.delivered_block_messages += 1;
            tracked.touched_peers.insert(envelope.sender);
            tracked.touched_peers.insert(envelope.receiver);
        }
    }

    fn record_conflict_signal_delivery(&mut self, envelope: &MessageEnvelope) {
        let mut lower_signal_blocks = Vec::new();

        match &envelope.message {
            Message::Vote { block_id, vote, .. } => {
                if *vote == 0 {
                    lower_signal_blocks.push(*block_id);
                }
            }
            Message::RequestBatch { items } => {
                for item in items {
                    if let BatchRequestItem::Vote { block_id, vote, .. } = item {
                        if *vote == 0 {
                            lower_signal_blocks.push(*block_id);
                        }
                    }
                }
            }
            _ => {}
        }

        for block_id in lower_signal_blocks {
            let Some(family_idx) = self.conflict_block_to_family.get(&block_id).copied() else {
                continue;
            };
            let Some(family) = self.conflict_families.get_mut(family_idx) else {
                continue;
            };
            if block_id == family.highest_candidate {
                continue;
            }
            family.conflict_signal_receivers.insert(envelope.receiver);
        }
    }

    fn inject_blocks(&mut self) {
        let eligible_peers = self.eligible_transaction_sources();
        if eligible_peers.is_empty() {
            self.submission_attempts += self.config.transactions.blocks_per_round;
            self.skipped_submissions += self.config.transactions.blocks_per_round;
            return;
        }

        for _ in 0..self.config.transactions.blocks_per_round {
            self.submission_attempts += 1;
            let should_inject_conflict = self.config.transactions.conflicts.family_fraction > 0.0
                && self.rng.gen_bool(
                    self.config
                        .transactions
                        .conflicts
                        .family_fraction
                        .clamp(0.0, 1.0),
                );
            if should_inject_conflict && self.inject_conflict_family(&eligible_peers) {
                continue;
            }

            let target = *eligible_peers
                .choose(&mut self.rng)
                .expect("eligible peers should not be empty");
            let block = self.build_regular_block(target);
            self.submit_block(target, block);
        }
    }

    fn build_regular_block(&mut self, target: PeerId) -> Block {
        let used = self.rng.gen_range(
            self.config.transactions.block_size_range.0..=self.config.transactions.block_size_range.1,
        );
        let mut block = Block {
            id: self.rng.next_u64(),
            time: self.current_round as u64,
            used: min(used, TOKENS_PER_BLOCK) as u8,
            parts: [TokenBlock::default(); TOKENS_PER_BLOCK],
            signatures: [None; TOKENS_PER_BLOCK],
        };
        let mut seen_tokens = HashSet::new();
        let mut existing_parts_in_block = 0;

        for idx in 0..block.used as usize {
            let prefer_existing = self.rng.gen_bool(
                self.config
                    .transactions
                    .existing_token_fraction
                    .clamp(0.0, 1.0),
            );
            let mut existing_mapping = None;

            if prefer_existing {
                for _ in 0..4 {
                    let Some((token_id, last_block)) = self.sample_existing_mapping(target) else {
                        break;
                    };
                    if seen_tokens.insert(token_id) {
                        existing_mapping = Some((token_id, last_block));
                        break;
                    }
                }
            }

            if let Some((token_id, last_block)) = existing_mapping {
                block.parts[idx].token = token_id;
                block.parts[idx].last = last_block;
                existing_parts_in_block += 1;
                self.existing_token_parts_generated += 1;
            } else {
                let token_id = loop {
                    let candidate = self.rng.next_u64();
                    if seen_tokens.insert(candidate) {
                        break candidate;
                    }
                };
                block.parts[idx].token = token_id;
                block.parts[idx].last = 0;
                self.new_token_parts_generated += 1;
            }
            block.parts[idx].key = self.rng.next_u64();
            block.signatures[idx] = Some(PublicKeyReference::default());
        }
        if existing_parts_in_block > 0 {
            self.blocks_with_existing_tokens += 1;
        }

        block
    }

    fn inject_conflict_family(&mut self, eligible_peers: &[PeerId]) -> bool {
        let contenders = self
            .config
            .transactions
            .conflicts
            .contenders
            .min(eligible_peers.len());
        if contenders < 2 {
            return false;
        }

        let targets: Vec<PeerId> = eligible_peers
            .choose_multiple(&mut self.rng, contenders)
            .copied()
            .collect();
        if targets.len() < 2 {
            return false;
        }

        let mut existing_mapping = None;
        for peer_id in &targets {
            if let Some(mapping) = self.sample_existing_mapping(*peer_id) {
                existing_mapping = Some(mapping);
                break;
            }
        }
        if existing_mapping.is_none() {
            for _ in 0..6 {
                let candidate = *eligible_peers
                    .choose(&mut self.rng)
                    .expect("eligible peers should not be empty");
                if let Some(mapping) = self.sample_existing_mapping(candidate) {
                    existing_mapping = Some(mapping);
                    break;
                }
            }
        }

        let Some((token_id, parent_block)) = existing_mapping else {
            return false;
        };

        let mut candidate_block_ids = Vec::with_capacity(targets.len());
        for target in targets {
            let mut block = Block {
                id: self.rng.next_u64(),
                time: self.current_round as u64,
                used: 1,
                parts: [TokenBlock::default(); TOKENS_PER_BLOCK],
                signatures: [None; TOKENS_PER_BLOCK],
            };
            block.parts[0] = TokenBlock {
                token: token_id,
                last: parent_block,
                key: self.rng.next_u64(),
            };
            block.signatures[0] = Some(PublicKeyReference::default());

            self.existing_token_parts_generated += 1;
            self.blocks_with_existing_tokens += 1;
            candidate_block_ids.push(block.id);
            self.submit_block(target, block);
        }

        let highest_candidate = *candidate_block_ids
            .iter()
            .max()
            .expect("conflict family should contain at least one candidate");
        let family_idx = self.conflict_families.len();
        self.conflict_families.push(ConflictFamily {
            token: token_id,
            parent_block,
            candidate_block_ids: candidate_block_ids.clone(),
            highest_candidate,
            created_round: self.current_round,
            owner_committed_candidates: HashSet::new(),
            conflict_signal_receivers: HashSet::new(),
        });
        for block_id in candidate_block_ids {
            self.conflict_block_to_family.insert(block_id, family_idx);
        }

        true
    }

    fn submit_block(&mut self, target: PeerId, block: Block) {
        let graph_time = self.current_round as u64 + 1;
        let Some(origin_peer) = self.peers.get(&target) else {
            return;
        };

        let mut token_samples = Vec::new();
        let mut token_neighborhoods = Vec::new();
        let mut reachable_vote_peers = HashSet::new();
        let mut reachable_vote_edges = HashSet::new();

        for idx in 0..block.used as usize {
            let token = block.parts[idx].token;
            let covering_peers = self.collect_covering_peers(token);
            let coverage_size = covering_peers.len();
            let vote_eligible_size = origin_peer.node.vote_eligible_peer_count(token);
            let entry_hops = origin_peer
                .node
                .active_hop_distance_to_token(token)
                .unwrap_or(0);
            let entry_is_local = origin_peer.node.local_scope_contains(token);

            token_samples.push((coverage_size, vote_eligible_size, entry_hops, entry_is_local));
            token_neighborhoods.push(covering_peers);
            self.extend_vote_graph(
                target,
                token,
                graph_time,
                &mut reachable_vote_peers,
                &mut reachable_vote_edges,
            );
        }

        let max_entry_hops = token_samples
            .iter()
            .map(|(_, _, entry_hops, _)| *entry_hops)
            .max()
            .unwrap_or(0);
        let witness_neighborhood = self.collect_covering_peers(block.id);
        let witness_coverage = witness_neighborhood.len();
        let ideal_role_sum_lower_bound_messages =
            self.role_sum_lower_bound_messages(&token_neighborhoods, &witness_neighborhood);
        let ideal_coalesced_lower_bound_messages = self
            .coalesced_lower_bound_messages(&token_neighborhoods, &witness_neighborhood);
        for (coverage_size, vote_eligible_size, entry_hops, entry_is_local) in token_samples {
            self.record_neighborhood_sample(
                coverage_size,
                vote_eligible_size,
                entry_hops,
                entry_is_local,
            );
        }

        self.transaction_spread
            .reachable_vote_peers
            .push(reachable_vote_peers.len());
        self.transaction_spread
            .reachable_vote_edges
            .push(reachable_vote_edges.len());
        self.transaction_spread
            .witness_coverage
            .push(witness_coverage);

        if let Some(peer) = self.peers.get_mut(&target) {
            peer.node.block(&block);
            let mut touched_peers = HashSet::new();
            touched_peers.insert(target);
            self.tracked_blocks.insert(
                block.id,
                TrackedBlock {
                    owner: target,
                    submitted_round: self.current_round,
                    max_entry_hops,
                    ideal_role_sum_lower_bound_messages,
                    ideal_coalesced_lower_bound_messages,
                    delivered_block_messages: 0,
                    touched_peers,
                },
            );
            self.submitted_blocks += 1;
        }
    }

    fn sample_additional_network_delay(&mut self) -> usize {
        let mut delay = self.config.network.base_delay_rounds;

        if self.config.network.jitter_rounds > 0 {
            delay += self.rng.gen_range(0..=self.config.network.jitter_rounds);
        }

        while self.rng.gen_bool(self.config.network.delay_fraction) {
            delay += 1;
        }

        delay
    }

    fn schedule_outbound_messages(&mut self) {
        let outbound = std::mem::take(&mut self.outbound_messages);

        for envelope in outbound {
            if self.rng.gen_bool(self.config.network.loss_fraction) {
                continue;
            }

            let additional_delay = self.sample_additional_network_delay();
            let transit_rounds = 1 + additional_delay;
            self.network_transit_samples.push(transit_rounds);
            self.scheduled_wire_message_types.record_wire(&envelope.message);
            self.scheduled_message_types.record_logical(&envelope.message);
            self.in_flight_messages.push(ScheduledMessage {
                deliver_round: self.current_round + additional_delay,
                envelope,
            });
        }

        self.peak_in_flight_messages = self
            .peak_in_flight_messages
            .max(self.in_flight_messages.len());
    }

    fn deliver_messages(&mut self) {
        let in_flight = std::mem::take(&mut self.in_flight_messages);
        let mut ready_messages = Vec::new();

        for scheduled in in_flight {
            if scheduled.deliver_round <= self.current_round {
                ready_messages.push(scheduled.envelope);
            } else {
                self.in_flight_messages.push(scheduled);
            }
        }

        for message in ready_messages {
            if !self
                .peers
                .get(&message.sender)
                .is_some_and(|peer| peer.active)
                || !self
                    .peers
                    .get(&message.receiver)
                    .is_some_and(|peer| peer.active)
            {
                continue;
            }

            self.total_wire_messages_delivered += 1;
            self.delivered_wire_message_types.record_wire(&message.message);
            self.total_messages_delivered += message_logical_count(&message.message);
            self.delivered_message_types.record_logical(&message.message);
            self.track_delivered_block_message(&message);
            self.record_conflict_signal_delivery(&message);
            if let Some(peer) = self.peers.get_mut(&message.receiver) {
                peer.node.handle_message(&message, &mut self.outbound_messages);
            }
        }
    }

    fn tick_nodes(&mut self) {
        let peer_ids = self.active_peer_ids();
        for peer_id in peer_ids {
            if let Some(peer) = self.peers.get_mut(&peer_id) {
                peer.node.tick(&mut self.outbound_messages);
            }
        }
    }

    fn observe_peer_progress(&mut self) {
        for (&peer_id, peer) in &self.peers {
            if !peer.active {
                continue;
            }

            let Some(progress) = self.peer_progress.get_mut(&peer_id) else {
                continue;
            };

            if progress.connected_round.is_none() && peer.node.num_connected_peers() > 0 {
                progress.connected_round = Some(self.current_round);
            }
            if progress.known_head_round.is_none() && peer.node.num_peers_with_commit_chain_heads() > 0 {
                progress.known_head_round = Some(self.current_round);
            }
            if progress.sync_trace_round.is_none()
                && peer.backend.borrow().commit_chain().active_traces() > 0
            {
                progress.sync_trace_round = Some(self.current_round);
            }

            if let Some(rejoin_idx) = self.active_rejoins.get(&peer_id).copied() {
                let rejoin = &mut self.rejoin_progress[rejoin_idx];
                if rejoin.connected_round.is_none() && peer.node.num_connected_peers() > 0 {
                    rejoin.connected_round = Some(self.current_round);
                }
                if rejoin.known_head_round.is_none()
                    && peer.node.num_peers_with_commit_chain_heads() > 0
                {
                    rejoin.known_head_round = Some(self.current_round);
                }
                if rejoin.sync_trace_round.is_none()
                    && peer.backend.borrow().commit_chain().active_traces() > 0
                {
                    rejoin.sync_trace_round = Some(self.current_round);
                }
            }
        }

        self.capture_vote_diagnostics();
    }

    fn capture_vote_diagnostics(&mut self) {
        for (&peer_id, peer) in &self.peers {
            let current = peer.node.vote_ingress_diagnostics();
            let previous = self
                .last_vote_diagnostics
                .entry(peer_id)
                .or_insert_with(VoteIngressDiagnostics::default);

            self.cumulative_trusted_votes_recorded += current
                .trusted_votes_recorded
                .saturating_sub(previous.trusted_votes_recorded);
            self.cumulative_untrusted_votes_received += current
                .untrusted_votes_received
                .saturating_sub(previous.untrusted_votes_received);
            self.cumulative_vote_block_requests += current
                .block_requests_triggered
                .saturating_sub(previous.block_requests_triggered);

            *previous = current;
        }
    }

    fn update_peaks(&mut self) {
        let active_traces: usize = self
            .peers
            .values()
            .filter(|peer| peer.active)
            .map(|peer| peer.backend.borrow().commit_chain().active_traces())
            .sum();
        self.peak_active_traces = self.peak_active_traces.max(active_traces);

        let active_elections: usize = self
            .peers
            .values()
            .filter(|peer| peer.active)
            .map(|peer| peer.node.num_active_elections())
            .sum();
        self.peak_active_elections = self.peak_active_elections.max(active_elections);
    }

    fn start_recovery_watch(&mut self, label: String) {
        let baseline_commit_rate = self.recent_average_commits();
        self.recovery_watches.push(RecoveryWatch {
            label,
            start_round: self.current_round,
            baseline_commit_rate,
            recovered_round: if baseline_commit_rate == 0.0 {
                Some(self.current_round)
            } else {
                None
            },
        });
    }

    fn update_recovery_watches(&mut self) {
        let recent_rate = self.recent_average_commits();
        for recovery in &mut self.recovery_watches {
            if recovery.recovered_round.is_some() || self.current_round <= recovery.start_round {
                continue;
            }

            if recent_rate >= recovery.baseline_commit_rate * RECOVERY_THRESHOLD {
                recovery.recovered_round = Some(self.current_round);
            }
        }
    }

    fn recent_average_commits(&self) -> f64 {
        let sample = self
            .round_commits
            .iter()
            .rev()
            .take(RECOVERY_WINDOW)
            .copied()
            .collect::<Vec<_>>();

        if sample.is_empty() {
            0.0
        } else {
            sample.iter().sum::<usize>() as f64 / sample.len() as f64
        }
    }

    fn record_round_metrics(&mut self, commits_this_round: usize) {
        let snapshot = self.current_snapshot(commits_this_round);
        self.round_metrics.push(snapshot);
    }

    fn current_snapshot(&self, commits_this_round: usize) -> RoundMetrics {
        let active_peers = self.active_peer_ids();
        let active_count = active_peers.len();
        let eligible_transaction_sources = self.eligible_transaction_sources().len();

        let (avg_known_peers, avg_connected_peers, avg_identified_peers, avg_pending_peers, avg_known_heads) =
            if active_count == 0 {
                (0.0, 0.0, 0.0, 0.0, 0.0)
            } else {
                let mut known_total = 0.0;
                let mut connected_total = 0.0;
                let mut identified_total = 0.0;
                let mut pending_total = 0.0;
                let mut head_total = 0.0;

                for peer_id in &active_peers {
                    if let Some(peer) = self.peers.get(peer_id) {
                        known_total += peer.node.num_peers() as f64;
                        connected_total += peer.node.num_connected_peers() as f64;
                        identified_total += peer.node.num_identified_peers() as f64;
                        pending_total += peer.node.num_pending_peers() as f64;
                        head_total += peer.node.num_peers_with_commit_chain_heads() as f64;
                    }
                }

                (
                    known_total / active_count as f64,
                    connected_total / active_count as f64,
                    identified_total / active_count as f64,
                    pending_total / active_count as f64,
                    head_total / active_count as f64,
                )
            };

        let active_traces: usize = active_peers
            .iter()
            .filter_map(|peer_id| self.peers.get(peer_id))
            .map(|peer| peer.backend.borrow().commit_chain().active_traces())
            .sum();

        let active_elections: usize = active_peers
            .iter()
            .filter_map(|peer_id| self.peers.get(peer_id))
            .map(|peer| peer.node.num_active_elections())
            .sum();

        let mut pending_without_block = 0;
        let mut pending_no_trusted_votes = 0;
        let mut pending_waiting_validation = 0;
        let mut pending_waiting_token_votes = 0;
        let mut pending_waiting_witness = 0;
        let mut pending_age_50_plus = 0;
        let mut pending_age_200_plus = 0;

        for peer_id in &active_peers {
            if let Some(peer) = self.peers.get(peer_id) {
                let diagnostics = peer.node.mempool_diagnostics();
                pending_without_block += diagnostics.pending_without_block;
                pending_no_trusted_votes += diagnostics.pending_no_trusted_votes;
                pending_waiting_validation += diagnostics.pending_waiting_validation;
                pending_waiting_token_votes += diagnostics.pending_waiting_token_votes;
                pending_waiting_witness += diagnostics.pending_waiting_witness;
                pending_age_50_plus += diagnostics.pending_age_50_plus;
                pending_age_200_plus += diagnostics.pending_age_200_plus;
            }
        }

        RoundMetrics {
            round: self.current_round,
            active_peers: active_count,
            eligible_transaction_sources,
            in_flight_messages: self.in_flight_messages.len(),
            avg_known_peers,
            avg_connected_peers,
            avg_identified_peers,
            avg_pending_peers,
            avg_known_heads,
            active_elections,
            active_traces,
            submitted_blocks: self.submitted_blocks,
            committed_blocks: self.committed_blocks,
            pending_blocks: self.tracked_blocks.len(),
            pending_without_block,
            pending_no_trusted_votes,
            pending_waiting_validation,
            pending_waiting_token_votes,
            pending_waiting_witness,
            pending_age_50_plus,
            pending_age_200_plus,
            total_messages_delivered: self.total_messages_delivered,
            commits_this_round,
            recent_commit_rate: self.recent_average_commits(),
            skipped_submissions: self.skipped_submissions,
            trusted_votes_recorded: self.cumulative_trusted_votes_recorded,
            untrusted_votes_received: self.cumulative_untrusted_votes_received,
            block_requests_triggered_by_votes: self.cumulative_vote_block_requests,
        }
    }

    fn print_checkpoint(&self, label: &str) {
        let snapshot = self.current_snapshot(*self.round_commits.last().unwrap_or(&0));
        let latency = DistributionSummary::from_samples(&self.commit_latencies);

        println!(
            "[round {}] {}: active peers {}, eligible tx sources {}, in-flight {}, avg known {:.1}, avg connected {:.1}, heads {:.1}, committed {}, pending {}, no-trusted-votes {}, wait-token-votes {}, wait-witness {}, skipped {}, traces {}, elections {}, recent rate {:.2}/round{}",
            self.current_round,
            label,
            snapshot.active_peers,
            snapshot.eligible_transaction_sources,
            snapshot.in_flight_messages,
            snapshot.avg_known_peers,
            snapshot.avg_connected_peers,
            snapshot.avg_known_heads,
            snapshot.committed_blocks,
            snapshot.pending_blocks,
            snapshot.pending_no_trusted_votes,
            snapshot.pending_waiting_token_votes,
            snapshot.pending_waiting_witness,
            snapshot.skipped_submissions,
            snapshot.active_traces,
            snapshot.active_elections,
            snapshot.recent_commit_rate,
            latency
                .as_ref()
                .map(|stats| format!(", latency p50/p95 {}/{}", stats.p50, stats.p95))
                .unwrap_or_default(),
        );
    }

    fn active_peer_ids(&self) -> Vec<PeerId> {
        self.peers
            .iter()
            .filter(|(_, peer)| peer.active)
            .map(|(peer_id, _)| *peer_id)
            .collect()
    }

    fn inactive_peer_ids(&self) -> Vec<PeerId> {
        self.peers
            .iter()
            .filter(|(_, peer)| !peer.active)
            .map(|(peer_id, _)| *peer_id)
            .collect()
    }

    fn build_onboarding_summary(&self) -> OnboardingSummary {
        let late_joiners: Vec<&PeerProgress> = self
            .peer_progress
            .values()
            .filter(|progress| progress.late_joiner)
            .collect();

        let connected_samples: Vec<usize> = late_joiners
            .iter()
            .filter_map(|progress| {
                progress
                    .connected_round
                    .map(|round| round.saturating_sub(progress.joined_round))
            })
            .collect();
        let head_samples: Vec<usize> = late_joiners
            .iter()
            .filter_map(|progress| {
                progress
                    .known_head_round
                    .map(|round| round.saturating_sub(progress.joined_round))
            })
            .collect();
        let sync_samples: Vec<usize> = late_joiners
            .iter()
            .filter_map(|progress| {
                progress
                    .sync_trace_round
                    .map(|round| round.saturating_sub(progress.joined_round))
            })
            .collect();

        OnboardingSummary {
            observed_peers: late_joiners.len(),
            bootstrap_seeded_peers: late_joiners
                .iter()
                .filter(|progress| progress.bootstrap_seed_count > 0)
                .count(),
            time_to_connected: DistributionSummary::from_samples(&connected_samples),
            time_to_known_head: DistributionSummary::from_samples(&head_samples),
            time_to_sync_trace: DistributionSummary::from_samples(&sync_samples),
            connected_before_known_head: late_joiners
                .iter()
                .filter(|progress| {
                    progress.connected_round.is_some()
                        && progress
                            .known_head_round
                            .map_or(true, |head| progress.connected_round.unwrap() < head)
                })
                .count(),
            connected_before_sync_trace: late_joiners
                .iter()
                .filter(|progress| {
                    progress.connected_round.is_some()
                        && progress
                            .sync_trace_round
                            .map_or(true, |sync| progress.connected_round.unwrap() < sync)
                })
                .count(),
        }
    }

    fn build_rejoin_summary(&self) -> OnboardingSummary {
        let connected_samples: Vec<usize> = self
            .rejoin_progress
            .iter()
            .filter_map(|progress| {
                progress
                    .connected_round
                    .map(|round| round.saturating_sub(progress.restarted_round))
            })
            .collect();
        let head_samples: Vec<usize> = self
            .rejoin_progress
            .iter()
            .filter_map(|progress| {
                progress
                    .known_head_round
                    .map(|round| round.saturating_sub(progress.restarted_round))
            })
            .collect();
        let sync_samples: Vec<usize> = self
            .rejoin_progress
            .iter()
            .filter_map(|progress| {
                progress
                    .sync_trace_round
                    .map(|round| round.saturating_sub(progress.restarted_round))
            })
            .collect();

        OnboardingSummary {
            observed_peers: self.rejoin_progress.len(),
            bootstrap_seeded_peers: self
                .rejoin_progress
                .iter()
                .filter(|progress| progress.bootstrap_seed_count > 0)
                .count(),
            time_to_connected: DistributionSummary::from_samples(&connected_samples),
            time_to_known_head: DistributionSummary::from_samples(&head_samples),
            time_to_sync_trace: DistributionSummary::from_samples(&sync_samples),
            connected_before_known_head: self
                .rejoin_progress
                .iter()
                .filter(|progress| {
                    progress.connected_round.is_some()
                        && progress
                            .known_head_round
                            .map_or(true, |head| progress.connected_round.unwrap() < head)
                })
                .count(),
            connected_before_sync_trace: self
                .rejoin_progress
                .iter()
                .filter(|progress| {
                    progress.connected_round.is_some()
                        && progress
                            .sync_trace_round
                            .map_or(true, |sync| progress.connected_round.unwrap() < sync)
                })
                .count(),
        }
    }

    fn build_result(&self) -> SimResult {
        let final_snapshot = self
            .round_metrics
            .last()
            .cloned()
            .unwrap_or_else(|| self.current_snapshot(0));
        let average_of = |selector: fn(&RoundMetrics) -> usize| -> f64 {
            if self.round_metrics.is_empty() {
                selector(&final_snapshot) as f64
            } else {
                self.round_metrics
                    .iter()
                    .map(|round| selector(round) as f64)
                    .sum::<f64>()
                    / self.round_metrics.len() as f64
            }
        };
        let peak_of = |selector: fn(&RoundMetrics) -> usize| -> usize {
            self.round_metrics
                .iter()
                .map(selector)
                .max()
                .unwrap_or_else(|| selector(&final_snapshot))
        };
        let avg_eligible_transaction_sources = if self.round_metrics.is_empty() {
            final_snapshot.eligible_transaction_sources as f64
        } else {
            self.round_metrics
                .iter()
                .map(|round| round.eligible_transaction_sources as f64)
                .sum::<f64>()
                / self.round_metrics.len() as f64
        };
        let neighborhood_buckets = self
            .neighborhood_buckets
            .iter()
            .enumerate()
            .filter_map(|(idx, bucket)| {
                let token_samples = bucket.entry_hops.len();
                if token_samples == 0 && bucket.committed_blocks == 0 {
                    return None;
                }

                Some(NeighborhoodBucketSummary {
                    label: NEIGHBORHOOD_BUCKET_LABELS[idx].to_string(),
                    token_samples,
                    committed_blocks: bucket.committed_blocks,
                    coverage_size: DistributionSummary::from_samples(&bucket.coverage_sizes),
                    vote_eligible_size: DistributionSummary::from_samples(
                        &bucket.vote_eligible_sizes,
                    ),
                    entry_hops: DistributionSummary::from_samples(&bucket.entry_hops),
                    commit_latency: DistributionSummary::from_samples(&bucket.commit_latencies),
                })
            })
            .collect();
        let mut families_without_visible_candidate = 0;
        let mut families_with_single_visible_candidate = 0;
        let mut families_split_across_candidates = 0;
        let mut families_unanimous_highest_candidate = 0;
        let mut families_with_highest_majority = 0;
        let mut families_with_any_majority = 0;
        let mut families_stalled_without_majority = 0;
        let mut families_with_any_lower_candidate_visible = 0;
        let mut families_with_lower_owner_commit = 0;
        let mut families_with_multiple_owner_commits = 0;
        let mut owner_committed_candidates = 0;
        let mut visible_candidates_per_family = Vec::new();
        let mut covering_peers_per_family = Vec::new();
        let mut participant_peers_per_family = Vec::new();
        let mut signaled_participant_peers_per_family = Vec::new();
        let mut candidate_coverers_per_family = Vec::new();
        let mut highest_candidate_coverer_share = Vec::new();
        let mut signal_coverage_among_participants = Vec::new();

        for family in &self.conflict_families {
            let covering_peers = self.collect_covering_peers(family.token);
            let mut candidate_counts: BTreeMap<BlockId, usize> = BTreeMap::new();

            for peer_id in &covering_peers {
                let Some(peer) = self.peers.get(peer_id) else {
                    continue;
                };
                let backend = peer.backend.borrow();
                let Some(mapping) = TokenStorageBackend::lookup(&*backend, &family.token) else {
                    continue;
                };
                if family.candidate_block_ids.contains(&mapping.block()) {
                    *candidate_counts.entry(mapping.block()).or_insert(0) += 1;
                }
            }

            let visible_candidates = candidate_counts.len();
            let candidate_coverers = candidate_counts.values().sum::<usize>();
            let highest_coverers = *candidate_counts.get(&family.highest_candidate).unwrap_or(&0);
            let majority_coverers = candidate_counts.values().copied().max().unwrap_or(0);
            let participant_peers = self
                .peers
                .values()
                .filter(|peer| {
                    peer.active
                        && family
                            .candidate_block_ids
                            .iter()
                            .any(|block_id| peer.node.knows_block(block_id))
                })
                .map(|peer| peer.node.get_peer_id())
                .collect::<HashSet<_>>();
            let signaled_participants = participant_peers
                .iter()
                .filter(|peer_id| family.conflict_signal_receivers.contains(peer_id))
                .count();

            visible_candidates_per_family.push(visible_candidates);
            covering_peers_per_family.push(covering_peers.len());
            participant_peers_per_family.push(participant_peers.len());
            signaled_participant_peers_per_family.push(signaled_participants);
            candidate_coverers_per_family.push(candidate_coverers);
            if !covering_peers.is_empty() {
                highest_candidate_coverer_share
                    .push(highest_coverers as f64 / covering_peers.len() as f64);
            }
            if !participant_peers.is_empty() {
                signal_coverage_among_participants
                    .push(signaled_participants as f64 / participant_peers.len() as f64);
            }

            match visible_candidates {
                0 => families_without_visible_candidate += 1,
                1 => families_with_single_visible_candidate += 1,
                _ => families_split_across_candidates += 1,
            }
            if majority_coverers * 2 > covering_peers.len() {
                families_with_any_majority += 1;
                if highest_coverers * 2 > covering_peers.len() {
                    families_with_highest_majority += 1;
                }
            } else {
                families_stalled_without_majority += 1;
            }
            if visible_candidates > 0
                && candidate_counts.keys().any(|block_id| *block_id != family.highest_candidate)
            {
                families_with_any_lower_candidate_visible += 1;
            }
            if visible_candidates == 1 && highest_coverers == covering_peers.len() {
                families_unanimous_highest_candidate += 1;
            }

            owner_committed_candidates += family.owner_committed_candidates.len();
            if family
                .owner_committed_candidates
                .iter()
                .any(|block_id| *block_id != family.highest_candidate)
            {
                families_with_lower_owner_commit += 1;
            }
            if family.owner_committed_candidates.len() > 1 {
                families_with_multiple_owner_commits += 1;
            }
        }

        SimResult {
            seed_used: self.seed_used,
            rounds_completed: self.config.rounds,
            total_peers: self.peers.len(),
            active_peers: final_snapshot.active_peers,
            transaction_source_policy: self.config.transactions.source_policy.label().to_string(),
            submission_attempts: self.submission_attempts,
            submitted_blocks: self.submitted_blocks,
            skipped_submissions: self.skipped_submissions,
            committed_blocks: self.committed_blocks,
            pending_blocks: self.tracked_blocks.len(),
            total_messages_delivered: self.total_messages_delivered,
            total_wire_messages_delivered: self.total_wire_messages_delivered,
            peak_in_flight_messages: self.peak_in_flight_messages,
            peak_active_traces: self.peak_active_traces,
            peak_active_elections: self.peak_active_elections,
            final_network_base_delay_rounds: self.config.network.base_delay_rounds,
            final_network_jitter_rounds: self.config.network.jitter_rounds,
            final_network_delay_fraction: self.config.network.delay_fraction,
            final_network_loss_fraction: self.config.network.loss_fraction,
            final_avg_known_peers: final_snapshot.avg_known_peers,
            final_avg_connected_peers: final_snapshot.avg_connected_peers,
            final_eligible_transaction_sources: final_snapshot.eligible_transaction_sources,
            avg_eligible_transaction_sources,
            final_recent_commit_rate: final_snapshot.recent_commit_rate,
            commit_latency: DistributionSummary::from_samples(&self.commit_latencies),
            network_transit_delay: DistributionSummary::from_samples(&self.network_transit_samples),
            scheduled_message_types: self.scheduled_message_types.clone(),
            delivered_message_types: self.delivered_message_types.clone(),
            scheduled_wire_message_types: self.scheduled_wire_message_types.clone(),
            delivered_wire_message_types: self.delivered_wire_message_types.clone(),
            mempool_pressure: MempoolPressureSummary {
                avg_pending_without_block: average_of(|round| round.pending_without_block),
                peak_pending_without_block: peak_of(|round| round.pending_without_block),
                avg_pending_no_trusted_votes: average_of(|round| round.pending_no_trusted_votes),
                peak_pending_no_trusted_votes: peak_of(|round| round.pending_no_trusted_votes),
                avg_pending_waiting_validation: average_of(|round| round.pending_waiting_validation),
                peak_pending_waiting_validation: peak_of(|round| round.pending_waiting_validation),
                avg_pending_waiting_token_votes: average_of(|round| round.pending_waiting_token_votes),
                peak_pending_waiting_token_votes: peak_of(|round| round.pending_waiting_token_votes),
                avg_pending_waiting_witness: average_of(|round| round.pending_waiting_witness),
                peak_pending_waiting_witness: peak_of(|round| round.pending_waiting_witness),
                avg_pending_age_50_plus: average_of(|round| round.pending_age_50_plus),
                peak_pending_age_50_plus: peak_of(|round| round.pending_age_50_plus),
                avg_pending_age_200_plus: average_of(|round| round.pending_age_200_plus),
                peak_pending_age_200_plus: peak_of(|round| round.pending_age_200_plus),
            },
            vote_ingress: VoteIngressSummary {
                trusted_votes_recorded: self.cumulative_trusted_votes_recorded,
                untrusted_votes_received: self.cumulative_untrusted_votes_received,
                block_requests_triggered_by_votes: self.cumulative_vote_block_requests,
            },
            neighborhoods: NeighborhoodSummary {
                token_samples: self.neighborhood_entry_hop_samples.len(),
                local_token_samples: self.local_entry_token_samples,
                coverage_size: DistributionSummary::from_samples(
                    &self.neighborhood_coverage_samples,
                ),
                vote_eligible_size: DistributionSummary::from_samples(
                    &self.neighborhood_vote_eligible_samples,
                ),
                entry_hops: DistributionSummary::from_samples(
                    &self.neighborhood_entry_hop_samples,
                ),
                buckets: neighborhood_buckets,
            },
            transaction_workload: TransactionWorkloadSummary {
                configured_existing_token_fraction: self.config.transactions.existing_token_fraction,
                existing_token_parts: self.existing_token_parts_generated,
                new_token_parts: self.new_token_parts_generated,
                blocks_with_existing_tokens: self.blocks_with_existing_tokens,
            },
            conflict_workload: ConflictWorkloadSummary {
                configured_family_fraction: self.config.transactions.conflicts.family_fraction,
                configured_contenders: self.config.transactions.conflicts.contenders,
                families_created: self.conflict_families.len(),
                candidate_blocks_submitted: self
                    .conflict_families
                    .iter()
                    .map(|family| family.candidate_block_ids.len())
                    .sum(),
                owner_committed_candidates,
                families_with_highest_majority,
                families_with_any_majority,
                families_stalled_without_majority,
                families_without_visible_candidate,
                families_with_single_visible_candidate,
                families_split_across_candidates,
                families_unanimous_highest_candidate,
                families_with_any_lower_candidate_visible,
                families_with_lower_owner_commit,
                families_with_multiple_owner_commits,
                visible_candidates_per_family: DistributionSummary::from_samples(
                    &visible_candidates_per_family,
                ),
                covering_peers_per_family: DistributionSummary::from_samples(
                    &covering_peers_per_family,
                ),
                participant_peers_per_family: DistributionSummary::from_samples(
                    &participant_peers_per_family,
                ),
                signaled_participant_peers_per_family: DistributionSummary::from_samples(
                    &signaled_participant_peers_per_family,
                ),
                candidate_coverers_per_family: DistributionSummary::from_samples(
                    &candidate_coverers_per_family,
                ),
                highest_candidate_coverer_share: FloatDistributionSummary::from_samples(
                    &highest_candidate_coverer_share,
                ),
                signal_coverage_among_participants: FloatDistributionSummary::from_samples(
                    &signal_coverage_among_participants,
                ),
            },
            transaction_spread: TransactionSpreadSummary {
                submitted_blocks: self.transaction_spread.reachable_vote_peers.len(),
                committed_blocks: self.transaction_spread.settled_block_messages.len(),
                reachable_vote_peers: DistributionSummary::from_samples(
                    &self.transaction_spread.reachable_vote_peers,
                ),
                reachable_vote_edges: DistributionSummary::from_samples(
                    &self.transaction_spread.reachable_vote_edges,
                ),
                witness_coverage: DistributionSummary::from_samples(
                    &self.transaction_spread.witness_coverage,
                ),
                ideal_role_sum_lower_bound_messages: DistributionSummary::from_samples(
                    &self.transaction_spread.ideal_role_sum_lower_bound_messages,
                ),
                ideal_coalesced_lower_bound_messages: DistributionSummary::from_samples(
                    &self.transaction_spread.ideal_coalesced_lower_bound_messages,
                ),
                settled_peer_spread: DistributionSummary::from_samples(
                    &self.transaction_spread.settled_peer_spread,
                ),
                settled_block_messages: DistributionSummary::from_samples(
                    &self.transaction_spread.settled_block_messages,
                ),
                actual_to_role_sum_ratio: FloatDistributionSummary::from_samples(
                    &self.transaction_spread.actual_to_role_sum_ratio,
                ),
                actual_to_coalesced_ratio: FloatDistributionSummary::from_samples(
                    &self.transaction_spread.actual_to_coalesced_ratio,
                ),
                total_actual_block_messages: self.transaction_spread.total_actual_block_messages,
                total_ideal_role_sum_lower_bound_messages: self
                    .transaction_spread
                    .total_ideal_role_sum_lower_bound_messages,
                total_ideal_coalesced_lower_bound_messages: self
                    .transaction_spread
                    .total_ideal_coalesced_lower_bound_messages,
            },
            late_joiner_onboarding: self.build_onboarding_summary(),
            rejoin_onboarding: self.build_rejoin_summary(),
            recoveries: self
                .recovery_watches
                .iter()
                .map(|recovery| RecoverySummary {
                    label: recovery.label.clone(),
                    start_round: recovery.start_round,
                    baseline_commit_rate: recovery.baseline_commit_rate,
                    recovered_round: recovery.recovered_round,
                })
                .collect(),
            round_metrics: self.round_metrics.clone(),
        }
    }
}
