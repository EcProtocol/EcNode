// Simulation Runner

use std::cell::RefCell;
use std::cmp::min;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, RngCore, SeedableRng};

use ec_rust::ec_blocks::MemBlocks;
use ec_rust::ec_interface::{
    Block, BlockId, Message, MessageEnvelope, PeerId, PublicKeyReference, TokenBlock, TokenId,
    TOKENS_PER_BLOCK,
};
use ec_rust::ec_node::EcNode;
use ec_rust::ec_tokens::MemTokens;

use super::config::{SimConfig, TopologyConfig, TopologyMode};
use super::event_sink::LoggingEventSink;
use super::stats::{MessageCounts, PeerStats, SimResult, SimStatistics};

/// Simulation runner that executes consensus simulation
pub struct SimRunner {
    config: SimConfig,
    rng: StdRng,
    seed_used: [u8; 32],
    nodes: BTreeMap<PeerId, EcNode>,
    peers: Vec<PeerId>,
    tokens: Vec<(TokenId, BlockId, PublicKeyReference)>,
    blocks: BTreeMap<BlockId, PeerId>,
    messages: Vec<MessageEnvelope>,

    // Statistics tracking
    message_count: usize,
    message_counters: (usize, usize, usize, usize), // (Query, Vote, Block, Answer)
    committed: usize,
}

impl SimRunner {
    /// Create a new simulation runner with the given configuration
    pub fn new(config: SimConfig) -> Self {
        let seed = config.seed.unwrap_or_else(|| {
            let mut seed = [0u8; 32];
            rand::thread_rng().fill(&mut seed);
            seed
        });

        let mut rng = StdRng::from_seed(seed);

        // Create peer IDs
        let peers: Vec<PeerId> = (0..config.num_peers).map(|_| rng.next_u64()).collect();

        // Create initial tokens
        let mut tokens = Vec::new();
        for _ in 0..config.transactions.initial_tokens {
            tokens.push((rng.next_u64(), 0, 0));
        }

        // Create nodes with topology
        let mut nodes: BTreeMap<PeerId, EcNode> = BTreeMap::new();
        for peer_id in &peers {
            let token_store = Rc::new(RefCell::new(MemTokens::new()));
            let block_store = Rc::new(RefCell::new(MemBlocks::new()));

            // Create node with logging event sink (configurable via SimConfig)
            let event_sink = Box::new(LoggingEventSink::new(config.enable_event_logging));
            let mut node = EcNode::new_with_sink(token_store, block_store, *peer_id, 0, event_sink);

            // Apply topology configuration
            Self::apply_topology(&mut node, peer_id, &peers, &config.topology, &mut rng);

            nodes.insert(*peer_id, node);
        }

        Self {
            config,
            rng,
            seed_used: seed,
            nodes,
            peers,
            tokens,
            blocks: BTreeMap::new(),
            messages: Vec::new(),
            message_count: 0,
            message_counters: (0, 0, 0, 0),
            committed: 0,
        }
    }

    fn apply_topology(
        node: &mut EcNode,
        peer_id: &PeerId,
        peers: &[PeerId],
        topology: &TopologyConfig,
        rng: &mut StdRng,
    ) {
        match &topology.mode {
            TopologyMode::Random { connectivity } => {
                let num_peers = (peers.len() as f64 * connectivity) as usize;
                for add_peer in peers.choose_multiple(rng, num_peers) {
                    node.seed_peer(add_peer);
                }
            }
            TopologyMode::RingGradient { min_prob, max_prob } => {
                let ring_size = u64::MAX;
                for p in peers {
                    let forward_dist = if *p >= *peer_id {
                        *p - peer_id
                    } else {
                        ring_size - peer_id + *p
                    };
                    let backward_dist = if *peer_id >= *p {
                        peer_id - *p
                    } else {
                        ring_size - *p + peer_id
                    };
                    let ring_dist = forward_dist.min(backward_dist);

                    let normalized_dist = (ring_dist as f64) / ((ring_size / 2) as f64);
                    let probability = max_prob - ((max_prob - min_prob) * normalized_dist);

                    let r = rng.next_u64() as f64 / u64::MAX as f64;
                    if r < probability {
                        node.seed_peer(p);
                    }
                }
            }
            TopologyMode::RingGaussian { sigma } => {
                let ring_size = u64::MAX;
                let sigma = (ring_size as f64 / 8.0) * sigma;

                for p in peers {
                    let forward_dist = if *p >= *peer_id {
                        *p - peer_id
                    } else {
                        ring_size - peer_id + *p
                    };
                    let backward_dist = if *peer_id >= *p {
                        peer_id - *p
                    } else {
                        ring_size - *p + peer_id
                    };
                    let ring_dist = forward_dist.min(backward_dist);

                    let d = ring_dist as f64;
                    let exponent = -(d * d) / (2.0 * sigma * sigma);
                    let probability = exponent.exp();

                    let r = rng.next_u64() as f64 / u64::MAX as f64;
                    if r < probability {
                        node.seed_peer(p);
                    }
                }
            }
        }
    }

    /// Run the complete simulation and return results
    pub fn run(&mut self) -> SimResult {
        for i in 0..self.config.rounds {
            self.step(i);
        }

        self.build_result()
    }

    fn step(&mut self, round: usize) {
        // Check for committed blocks
        let mut clear = HashSet::new();
        for (block_id, peer_id) in &self.blocks {
            if let Some(block) = self.nodes.get(peer_id).unwrap().committed_block(block_id) {
                for x in 0..block.used as usize {
                    self.tokens
                        .push((block.parts[x].token, *block_id, block.parts[x].key));
                }

                clear.insert(*block_id);
                self.committed += 1;
            }
        }
        self.blocks.retain(|b, _| !clear.contains(b));

        // Create new blocks
        self.tokens.shuffle(&mut self.rng);
        let mut x = 0;
        while x < self.tokens.len() {
            let used = min(
                self.rng
                    .gen_range(self.config.transactions.block_size_range.0
                        ..=self.config.transactions.block_size_range.1),
                self.tokens.len() - x,
            );

            let mut new_block = Block {
                id: self.rng.next_u64(),
                time: round as u64,
                used: used as u8,
                parts: [TokenBlock {
                    token: 0,
                    last: 0,
                    key: 0,
                }; TOKENS_PER_BLOCK],
                signatures: [None; TOKENS_PER_BLOCK],
            };

            for (y, (t, b, k)) in self.tokens[x..(x + used)].iter().enumerate() {
                new_block.parts[y].token = *t;
                new_block.parts[y].last = *b;
                new_block.parts[y].key = self.rng.next_u64();
                new_block.signatures[y] = Some(*k);
            }

            let target = self.peers.choose(&mut self.rng).unwrap();
            let node = self.nodes.get_mut(target).unwrap();
            node.block(&new_block);
            self.blocks.insert(new_block.id, *target);

            x += used;
        }
        self.tokens.clear();

        // Process messages with network simulation
        let mut next: Vec<MessageEnvelope> = Vec::new();

        let number_of_messages = self.messages.len();
        if number_of_messages > 0 {
            self.messages.shuffle(&mut self.rng);

            // Delay: push a fraction to next round
            let delay_count = (number_of_messages as f64 * self.config.network.delay_fraction) as usize;
            next.extend_from_slice(&self.messages[delay_count..]);

            // Loss: drop a fraction
            let after_delay = delay_count;
            let loss_count = (after_delay as f64 * self.config.network.loss_fraction) as usize;
            self.messages.truncate(after_delay - loss_count);
        }

        // Deliver messages
        for m in &self.messages {
            if let Some(node) = self.nodes.get_mut(&m.receiver) {
                match m.message {
                    Message::Query { .. } => self.message_counters.0 += 1,
                    Message::Vote { .. } => self.message_counters.1 += 1,
                    Message::Block { .. } => self.message_counters.2 += 1,
                    Message::Answer { .. } => self.message_counters.3 += 1,
                };
                node.handle_message(m, &mut next);
            }
        }

        // Tick all nodes
        for (_, node) in &mut self.nodes {
            node.tick(&mut next);
        }

        self.message_count += self.messages.len();
        self.messages.clear();
        self.messages.append(&mut next);
    }

    fn build_result(&self) -> SimResult {
        let peer_stats = self
            .nodes
            .iter()
            .map(|(_, node)| node.num_peers())
            .fold((usize::MIN, usize::MAX, 0usize), |acc, e| {
                (usize::max(acc.0, e), usize::min(acc.1, e), acc.2 + e)
            });

        let avg_peers = if self.nodes.is_empty() {
            0.0
        } else {
            peer_stats.2 as f64 / self.nodes.len() as f64
        };

        let rounds_per_commit = if self.committed > 0 {
            self.config.rounds as f64 / self.committed as f64
        } else {
            0.0
        };

        let messages_per_commit = if self.committed > 0 {
            self.message_count as f64 / self.committed as f64
        } else {
            0.0
        };

        SimResult {
            statistics: SimStatistics {
                message_counts: MessageCounts {
                    query: self.message_counters.0,
                    vote: self.message_counters.1,
                    block: self.message_counters.2,
                    answer: self.message_counters.3,
                },
                peer_stats: PeerStats {
                    max_peers: peer_stats.0,
                    min_peers: peer_stats.1,
                    avg_peers,
                },
                rounds_per_commit,
                messages_per_commit,
            },
            committed_blocks: self.committed,
            total_messages: self.message_count,
            seed_used: self.seed_used,
        }
    }
}
