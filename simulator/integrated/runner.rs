use std::cell::RefCell;
use std::cmp::min;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, RngCore, SeedableRng};

use ec_rust::ec_interface::{
    Block, BlockId, EcBlocks, MessageEnvelope, PeerId, PublicKeyReference, TokenBlock,
    TokenId, GENESIS_BLOCK_ID, TOKENS_PER_BLOCK,
};
use ec_rust::ec_memory_backend::{MemTokens, MemoryBackend};
use ec_rust::ec_node::EcNode;
use ec_rust::ec_proof_of_storage::TokenStorageBackend;

use crate::integrated::{
    DistributionSummary, IntegratedSimConfig, OnboardingSummary, RecoverySummary, RoundMetrics,
    SimResult, TransactionSourcePolicy,
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

#[derive(Clone, Copy)]
struct TrackedBlock {
    owner: PeerId,
    submitted_round: usize,
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

enum TokenSpace {
    Random(GlobalTokenMapping),
    Genesis(GenesisTokenSet),
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
    submission_attempts: usize,
    submitted_blocks: usize,
    skipped_submissions: usize,
    committed_blocks: usize,
    total_messages_delivered: usize,
    peak_active_traces: usize,
    peak_active_elections: usize,
    commit_latencies: Vec<usize>,
    network_transit_samples: Vec<usize>,
    round_commits: Vec<usize>,
    round_metrics: Vec<RoundMetrics>,
    recovery_watches: Vec<RecoveryWatch>,
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
            submission_attempts: 0,
            submitted_blocks: 0,
            skipped_submissions: 0,
            committed_blocks: 0,
            total_messages_delivered: 0,
            peak_active_traces: 0,
            peak_active_elections: 0,
            commit_latencies: Vec::new(),
            network_transit_samples: Vec::new(),
            round_commits: Vec::new(),
            round_metrics: Vec::new(),
            recovery_watches: Vec::new(),
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

        EcNode::new(backend, peer_id, 0, token_storage, node_rng)
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
                self.commit_latencies
                    .push(self.current_round.saturating_sub(tracked.submitted_round));
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

    fn inject_blocks(&mut self) {
        let eligible_peers = self.eligible_transaction_sources();
        if eligible_peers.is_empty() {
            self.submission_attempts += self.config.transactions.blocks_per_round;
            self.skipped_submissions += self.config.transactions.blocks_per_round;
            return;
        }

        for _ in 0..self.config.transactions.blocks_per_round {
            self.submission_attempts += 1;
            let used = self.rng.gen_range(
                self.config.transactions.block_size_range.0
                    ..=self.config.transactions.block_size_range.1,
            );
            let mut block = Block {
                id: self.rng.next_u64(),
                time: self.current_round as u64,
                used: min(used, TOKENS_PER_BLOCK) as u8,
                parts: [TokenBlock::default(); TOKENS_PER_BLOCK],
                signatures: [None; TOKENS_PER_BLOCK],
            };

            for idx in 0..block.used as usize {
                block.parts[idx].token = self.rng.next_u64();
                block.parts[idx].last = 0;
                block.parts[idx].key = self.rng.next_u64();
                block.signatures[idx] = Some(PublicKeyReference::default());
            }

            let target = *eligible_peers
                .choose(&mut self.rng)
                .expect("eligible peers should not be empty");
            if let Some(peer) = self.peers.get_mut(&target) {
                peer.node.block(&block);
                self.tracked_blocks.insert(
                    block.id,
                    TrackedBlock {
                        owner: target,
                        submitted_round: self.current_round,
                    },
                );
                self.submitted_blocks += 1;
            }
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
            self.in_flight_messages.push(ScheduledMessage {
                deliver_round: self.current_round + additional_delay,
                envelope,
            });
        }
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

            self.total_messages_delivered += 1;
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
            total_messages_delivered: self.total_messages_delivered,
            commits_this_round,
            recent_commit_rate: self.recent_average_commits(),
            skipped_submissions: self.skipped_submissions,
        }
    }

    fn print_checkpoint(&self, label: &str) {
        let snapshot = self.current_snapshot(*self.round_commits.last().unwrap_or(&0));
        let latency = DistributionSummary::from_samples(&self.commit_latencies);

        println!(
            "[round {}] {}: active peers {}, eligible tx sources {}, in-flight {}, avg known {:.1}, avg connected {:.1}, heads {:.1}, committed {}, pending {}, skipped {}, traces {}, elections {}, recent rate {:.2}/round{}",
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
        let avg_eligible_transaction_sources = if self.round_metrics.is_empty() {
            final_snapshot.eligible_transaction_sources as f64
        } else {
            self.round_metrics
                .iter()
                .map(|round| round.eligible_transaction_sources as f64)
                .sum::<f64>()
                / self.round_metrics.len() as f64
        };

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
