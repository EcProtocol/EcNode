//! Commit chain simulation runner

use super::config::CommitChainSimConfig;
use super::stats::{CommitStats, MessageCounts, MessageStats, SimResult, SyncStats};
use ec_rust::ec_commit_chain::TickMessage;
use ec_rust::ec_interface::{
    Block, BlockId, CommitBlock, EcBlocks, EcCommitChainAccess, EcCommitChainBackend, EcTime,
    EcTokens, Message, MessageTicket, PeerId, PublicKeyReference, TokenBlock, TokenId,
    GENESIS_BLOCK_ID, TOKENS_PER_BLOCK,
};
use ec_rust::ec_memory_backend::MemoryBackend;
use ec_rust::ec_peers::PeerRange;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

/// Message envelope for routing
#[derive(Clone)]
struct MessageEnvelope {
    from: PeerId,
    to: PeerId,
    message: Message,
}

/// Commit chain simulation runner
pub struct CommitChainRunner {
    config: CommitChainSimConfig,
    rng: StdRng,
    seed: [u8; 32],

    // Simplified node tracking - just backends
    backends: BTreeMap<PeerId, Rc<RefCell<MemoryBackend>>>,
    peer_ids: Vec<PeerId>,

    // Token pool and block tracking
    token_pool: Vec<(TokenId, BlockId, PublicKeyReference)>,
    block_counter: BlockId,

    // Message queues
    messages: VecDeque<MessageEnvelope>,
    delayed_messages: VecDeque<MessageEnvelope>,

    // Metrics
    current_round: usize,
    message_counts: MessageCounts,
    active_traces_history: Vec<usize>,
}

impl CommitChainRunner {
    /// Create a new commit chain runner
    pub fn new(config: CommitChainSimConfig) -> Self {
        let seed = config.resolve_seed();
        let rng = StdRng::from_seed(seed);

        let mut runner = Self {
            config,
            rng,
            seed,
            backends: BTreeMap::new(),
            peer_ids: Vec::new(),
            token_pool: Vec::new(),
            block_counter: 100, // Start after GENESIS_BLOCK_ID
            messages: VecDeque::new(),
            delayed_messages: VecDeque::new(),
            current_round: 0,
            message_counts: MessageCounts::default(),
            active_traces_history: Vec::new(),
        };

        runner.initialize_network();
        runner
    }

    /// Initialize the network with peers, backends, and token pool
    fn initialize_network(&mut self) {
        let num_peers = self.config.num_peers;

        // Create peer IDs evenly distributed on ring
        let spacing = u64::MAX / num_peers as u64;
        for i in 0..num_peers {
            let peer_id = spacing.wrapping_mul(i as u64);
            self.peer_ids.push(peer_id);
        }

        // Sort for ring ordering
        self.peer_ids.sort();

        // Initialize each peer with backend and peer manager
        for &peer_id in &self.peer_ids {
            // Calculate peer range (responsible for tokens around peer_id)
            let peer_range = self.calculate_peer_range(peer_id);

            // Create backend with commit chain
            // Note: Using default config for now, as there's no constructor that takes custom config
            let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(peer_id)));

            self.backends.insert(peer_id, backend);
        }

        // Create token pool
        self.initialize_token_pool();
    }

    /// Calculate peer range based on position in ring
    fn calculate_peer_range(&self, peer_id: PeerId) -> PeerRange {
        let num_peers = self.config.num_peers as u64;
        let range_size = u64::MAX / num_peers;
        let start = peer_id.wrapping_sub(range_size / 2);
        let end = peer_id.wrapping_add(range_size / 2);
        PeerRange::new(start, end)
    }

    /// Initialize token pool with random tokens
    fn initialize_token_pool(&mut self) {
        let total_tokens = self.config.block_injection.total_tokens;

        for _ in 0..total_tokens {
            let token_id = self.rng.gen::<TokenId>();
            let block_id = GENESIS_BLOCK_ID;
            let public_key = self.rng.gen::<PublicKeyReference>();

            self.token_pool.push((token_id, block_id, public_key));
        }
    }

    /// Main simulation loop
    pub fn run(mut self) -> SimResult {
        println!("Starting commit chain simulation...");
        println!("  Peers: {}", self.config.num_peers);
        println!("  Rounds: {}", self.config.rounds);
        println!("  Seed: {:?}", self.seed);
        println!();

        for round in 0..self.config.rounds {
            self.current_round = round;

            if round % 50 == 0 && round > 0 {
                println!("Round {}/{}", round, self.config.rounds);
            }

            // 1. Inject blocks
            self.inject_blocks();

            // 2. Process delayed messages from previous round
            self.process_delayed_messages();

            // 3. Deliver messages
            self.deliver_messages();

            // 4. Tick commit chains
            self.tick_commit_chains();

            // 5. Collect metrics
            self.collect_metrics();
        }

        println!("\nSimulation complete!");

        self.build_result()
    }

    /// Inject blocks randomly into peer backends
    fn inject_blocks(&mut self) {
        let blocks_this_round = self.config.block_injection.blocks_per_round;

        // Determine how many blocks to inject this round
        let num_blocks = if self.rng.gen_bool(blocks_this_round.fract()) {
            blocks_this_round.ceil() as usize
        } else {
            blocks_this_round.floor() as usize
        };

        for _ in 0..num_blocks {
            // Select random peer to create block
            let peer_idx = self.rng.gen_range(0..self.peer_ids.len());
            let peer_id = self.peer_ids[peer_idx];

            // Generate block
            if let Some(block) = self.generate_block(peer_id) {
                // Store in backend
                let backend = self.backends.get(&peer_id).unwrap();
                let mut backend_mut = backend.borrow_mut();

                // Save block and update tokens
                for i in 0..block.used as usize {
                    let token = block.parts[i].token;
                    let parent = block.parts[i].last;
                    backend_mut
                        .tokens_mut()
                        .set(&token, &block.id, &parent, block.time);
                }
                backend_mut.blocks_mut().save(&block);

                // Create commit block (simulating batch commit)
                let commit_block = backend_mut.commit_chain().create_commit_block(
                    backend_mut.commit_chain_backend(),
                    vec![block.id],
                    block.time,
                );

                // Save commit block
                backend_mut.commit_chain_backend_mut().save(&commit_block);
                backend_mut
                    .commit_chain_backend_mut()
                    .set_head(&commit_block.id);
            }
        }
    }

    /// Generate a random block for a peer
    fn generate_block(&mut self, _peer_id: PeerId) -> Option<Block> {
        let (min_size, max_size) = self.config.block_injection.block_size_range;
        let block_size = self.rng.gen_range(min_size..=max_size).min(TOKENS_PER_BLOCK);

        if self.token_pool.len() < block_size {
            return None; // Not enough tokens
        }

        // Select random tokens
        let mut parts = [TokenBlock::default(); TOKENS_PER_BLOCK];
        for i in 0..block_size {
            let token_idx = self.rng.gen_range(0..self.token_pool.len());
            let (token_id, parent_id, _public_key) = self.token_pool[token_idx];

            parts[i] = TokenBlock {
                token: token_id,
                last: parent_id,
                key: _public_key,
            };

            // Update token pool with new parent
            self.token_pool[token_idx].1 = self.block_counter;
        }

        self.block_counter += 1;

        Some(Block {
            id: self.block_counter - 1,
            time: self.current_round as EcTime,
            used: block_size as u8,
            parts,
            signatures: [None; TOKENS_PER_BLOCK],
        })
    }

    /// Process delayed messages from previous round
    fn process_delayed_messages(&mut self) {
        let delayed = std::mem::take(&mut self.delayed_messages);
        self.messages.extend(delayed);
    }

    /// Tick all commit chains and collect messages
    fn tick_commit_chains(&mut self) {
        // For now, skip commit chain ticking since it requires EcPeers
        // This is a simplified version that just tests block injection and storage
        // TODO: Add proper commit chain sync once EcPeers integration is fixed
    }

    /// Deliver messages with network simulation
    fn deliver_messages(&mut self) {
        let messages = std::mem::take(&mut self.messages);

        for envelope in messages {
            // Apply network simulation
            if self
                .rng
                .gen_bool(self.config.network.loss_fraction)
            {
                // Message lost
                continue;
            }

            if self
                .rng
                .gen_bool(self.config.network.delay_fraction)
            {
                // Message delayed
                self.delayed_messages.push_back(envelope);
                continue;
            }

            // Deliver message
            self.route_message(envelope);
        }
    }

    /// Route a message to the appropriate peer
    fn route_message(&mut self, envelope: MessageEnvelope) {
        match envelope.message {
            Message::QueryCommitBlock { block_id, ticket } => {
                // Route to target peer (owner of commit block)
                self.handle_query_commit_block(envelope.to, envelope.from, block_id, ticket);
            }
            Message::CommitBlock { block } => {
                // Response: deliver to requester
                self.message_counts.commit_block += 1;
                self.handle_commit_block(envelope.to, envelope.from, block);
            }
            Message::QueryBlock { block_id, ticket, .. } => {
                // Route to peer responsible for block-id
                self.handle_query_block(envelope.to, envelope.from, block_id, ticket);
            }
            Message::Block { block } => {
                // Deliver block to requester
                self.message_counts.block += 1;
                self.handle_block(envelope.to, block);
            }
            _ => {
                // Ignore other message types (not relevant for commit chain simulation)
            }
        }
    }

    /// Handle QueryCommitBlock - fetch commit block and respond
    fn handle_query_commit_block(
        &mut self,
        owner_peer: PeerId,
        requester: PeerId,
        block_id: u64,
        ticket: MessageTicket,
    ) {
        let backend = match self.backends.get(&owner_peer) {
            Some(b) => b.clone(),
            None => return,
        };

        let backend_ref = backend.borrow();
        if let Some(commit_block) = backend_ref.commit_chain_backend().lookup(&block_id) {
            // Send CommitBlock back to requester
            self.messages.push_back(MessageEnvelope {
                from: owner_peer,
                to: requester,
                message: Message::CommitBlock {
                    block: commit_block,
                },
            });
        }
    }

    /// Handle CommitBlock response
    fn handle_commit_block(
        &mut self,
        requester: PeerId,
        sender: PeerId,
        commit_block: CommitBlock,
    ) {
        let backend = match self.backends.get(&requester) {
            Some(b) => b.clone(),
            None => return,
        };

        // Extract ticket from commit block query (simplified - in reality would track pending queries)
        let ticket = 0; // Placeholder - will need to track query tickets properly

        let mut backend_mut = backend.borrow_mut();
        let _parent_request = backend_mut.handle_commit_block(
            commit_block,
            sender,
            ticket,
            self.current_round as EcTime,
        );
        // Note: Ignoring parent block requests for now in this simplified simulation
    }

    /// Handle QueryBlock - fetch block and respond
    fn handle_query_block(
        &mut self,
        owner_peer: PeerId,
        requester: PeerId,
        block_id: BlockId,
        ticket: MessageTicket,
    ) {
        let backend = match self.backends.get(&owner_peer) {
            Some(b) => b.clone(),
            None => return,
        };

        let backend_ref = backend.borrow();
        if let Some(block) = backend_ref.blocks().lookup(&block_id) {
            // Send Block back to requester
            self.messages.push_back(MessageEnvelope {
                from: owner_peer,
                to: requester,
                message: Message::Block {
                    block,
                },
            });
        }
    }

    /// Handle Block response
    fn handle_block(&mut self, requester: PeerId, block: Block) {
        let backend = match self.backends.get(&requester) {
            Some(b) => b.clone(),
            None => return,
        };

        // Extract ticket from block query (simplified - in reality would track pending queries)
        let ticket = 0; // Placeholder - will need to track query tickets properly

        let mut backend_mut = backend.borrow_mut();
        backend_mut.handle_block(block, ticket);
    }

    /// Collect metrics for this round
    fn collect_metrics(&mut self) {
        // Simplified metrics - just track that we're running
        // TODO: Add proper metrics once commit chain sync is working
        self.active_traces_history.push(0);
    }

    /// Build final result
    fn build_result(&self) -> SimResult {
        // Collect commit stats
        let mut total_commits = 0;
        let mut commits_per_peer = Vec::new();

        // Count commits by traversing commit chain for each peer
        for backend in self.backends.values() {
            let backend_ref = backend.borrow();

            // Count by iterating through commit chain from head to genesis
            let mut count = 0;
            let mut current = backend_ref.commit_chain_backend().get_head();

            while let Some(block_id) = current {
                count += 1;
                if let Some(commit_block) = backend_ref.commit_chain_backend().lookup(&block_id) {
                    if commit_block.previous == GENESIS_BLOCK_ID {
                        break;
                    }
                    current = Some(commit_block.previous);
                } else {
                    break;
                }
            }

            total_commits += count;
            commits_per_peer.push(count);
        }

        let min_commits = *commits_per_peer.iter().min().unwrap_or(&0);
        let max_commits = *commits_per_peer.iter().max().unwrap_or(&0);
        let avg_commits = if !commits_per_peer.is_empty() {
            commits_per_peer.iter().sum::<usize>() as f64 / commits_per_peer.len() as f64
        } else {
            0.0
        };

        let commit_stats = CommitStats {
            total_commits,
            commits_per_peer: (min_commits, max_commits, avg_commits),
        };

        // Collect sync stats
        let final_watermarks = BTreeMap::new();
        // TODO: Re-enable once commit chain sync is working
        // for (&peer_id, backend) in &self.backends {
        //     let backend_ref = backend.borrow();
        //     let watermark = backend_ref.commit_chain().watermark();
        //     final_watermarks.insert(peer_id, watermark);
        // }

        // Count total blocks synced (blocks created during simulation)
        let blocks_synced = (self.block_counter - 100) as usize;

        let sync_stats = SyncStats {
            final_watermarks,
            active_traces: self.active_traces_history.clone(),
            blocks_synced,
        };

        // Collect message stats
        let total_messages = self.message_counts.query_commit_block
            + self.message_counts.commit_block
            + self.message_counts.query_block
            + self.message_counts.block;

        let message_stats = MessageStats {
            total_messages,
            query_commit_block: self.message_counts.query_commit_block,
            commit_block_response: self.message_counts.commit_block,
            query_block: self.message_counts.query_block,
            block_response: self.message_counts.block,
        };

        SimResult {
            seed_used: self.seed,
            rounds_completed: self.config.rounds,
            commit_stats,
            sync_stats,
            message_stats,
        }
    }
}
