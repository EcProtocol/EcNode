use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rand::Rng;

use crate::ec_interface::{
    BatchRequestItem, BatchedBackend, Block, BlockId, BlockUseCase, EcBlocks, EcCommitChainAccess,
    EcTime, EcTokensV2, Event, EventSink, Message, MessageEnvelope, MessageTicket, NoOpSink,
    PeerId, TokenId,
};
use crate::ec_mempool::{BlockState, EcMemPool, InitialVoteRequest, MempoolDiagnostics};
use crate::ec_peers::{EcPeers, PeerAction, PeerManagerConfig};
use crate::ec_proof_of_storage::TokenStorageBackend;
use crate::ec_ticket_manager::TicketManager;

use crate::ec_mempool::MessageRequest;

pub struct EcNode<
    B: BatchedBackend + EcTokensV2 + EcBlocks + EcCommitChainAccess + 'static,
    T: TokenStorageBackend,
> {
    backend: Rc<RefCell<B>>,
    token_storage: T,
    peers: EcPeers,
    mem_pool: EcMemPool,
    peer_id: PeerId,
    time: EcTime,
    ticket_manager: TicketManager,
    event_sink: Box<dyn EventSink>,
    rng: rand::rngs::StdRng,
    vote_diagnostics: VoteIngressDiagnostics,
    enable_request_batching: bool,
    enable_commit_chain_sync: bool,
    batch_vote_replies: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VoteIngressDiagnostics {
    pub trusted_votes_recorded: usize,
    pub untrusted_votes_received: usize,
    pub block_requests_triggered: usize,
    pub parent_validation_requests_triggered: usize,
    pub missing_parent_requests_triggered: usize,
}

impl<
        B: BatchedBackend + EcTokensV2 + EcBlocks + EcCommitChainAccess + 'static,
        T: TokenStorageBackend,
    > EcNode<B, T>
{
    /// Create a new node with default NoOpSink (zero overhead)
    pub fn new(
        backend: Rc<RefCell<B>>,
        id: PeerId,
        time: EcTime,
        token_storage: T,
        rng: rand::rngs::StdRng,
    ) -> Self {
        Self::new_with_peer_config_and_sink(
            backend,
            id,
            time,
            token_storage,
            PeerManagerConfig::default(),
            Box::new(NoOpSink),
            rng,
        )
    }

    pub fn new_with_peer_config(
        backend: Rc<RefCell<B>>,
        id: PeerId,
        time: EcTime,
        token_storage: T,
        peer_config: PeerManagerConfig,
        rng: rand::rngs::StdRng,
    ) -> Self {
        Self::new_with_peer_config_and_sink(
            backend,
            id,
            time,
            token_storage,
            peer_config,
            Box::new(NoOpSink),
            rng,
        )
    }

    /// Create a new node with a custom event sink for debugging/analysis
    pub fn new_with_sink(
        backend: Rc<RefCell<B>>,
        id: PeerId,
        time: EcTime,
        token_storage: T,
        event_sink: Box<dyn EventSink>,
        rng: rand::rngs::StdRng,
    ) -> Self {
        Self::new_with_peer_config_and_sink(
            backend,
            id,
            time,
            token_storage,
            PeerManagerConfig::default(),
            event_sink,
            rng,
        )
    }

    pub fn new_with_peer_config_and_sink(
        backend: Rc<RefCell<B>>,
        id: PeerId,
        time: EcTime,
        token_storage: T,
        peer_config: PeerManagerConfig,
        event_sink: Box<dyn EventSink>,
        rng: rand::rngs::StdRng,
    ) -> Self {
        let enable_request_batching = peer_config.enable_request_batching;
        let enable_commit_chain_sync = peer_config.enable_commit_chain_sync;
        let batch_vote_replies = peer_config.batch_vote_replies;
        let vote_balance_threshold = peer_config.vote_balance_threshold;
        let vote_request_resend_cooldown = peer_config.vote_request_resend_cooldown;
        let vote_request_active_rounds = peer_config.vote_request_active_rounds;
        let vote_request_pairs_per_tick = peer_config.vote_request_pairs_per_tick;
        Self {
            mem_pool: EcMemPool::with_vote_policy(
                vote_balance_threshold,
                vote_request_resend_cooldown,
                vote_request_active_rounds,
                vote_request_pairs_per_tick,
            ),
            backend,
            token_storage,
            peers: EcPeers::with_config(id, peer_config),
            peer_id: id,
            time,
            ticket_manager: TicketManager::new(100), // 100 tick rotation period for simulation
            event_sink,
            rng,
            vote_diagnostics: VoteIngressDiagnostics::default(),
            enable_request_batching,
            enable_commit_chain_sync,
            batch_vote_replies,
        }
    }

    pub fn get_peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn seed_peer(&mut self, peer: &PeerId) {
        self.peers.update_peer(peer, self.time);
    }

    pub fn add_identified_peer(&mut self, peer: PeerId) -> bool {
        self.peers.add_identified_peer(peer, self.time)
    }

    pub fn seed_genesis_token(&mut self, token: u64) -> bool {
        self.peers.seed_genesis_token(token)
    }

    pub fn num_peers(&self) -> usize {
        self.peers.num_peers()
    }

    pub fn num_connected_peers(&self) -> usize {
        self.peers.num_connected()
    }

    pub fn connected_peer_ids(&self) -> &[PeerId] {
        self.peers.get_active_peers()
    }

    pub fn num_identified_peers(&self) -> usize {
        self.peers.num_identified()
    }

    pub fn num_pending_peers(&self) -> usize {
        self.peers.num_pending()
    }

    pub fn num_active_elections(&self) -> usize {
        self.peers.num_active_elections()
    }

    pub fn num_peers_with_commit_chain_heads(&self) -> usize {
        self.peers.num_peers_with_commit_chain_heads()
    }

    pub fn block(&mut self, block: &Block) {
        let _ = self.mem_pool.block(block, self.time);
    }

    pub fn submit_local_block(
        &mut self,
        block: &Block,
        outbound_messages: &mut Vec<MessageEnvelope>,
    ) {
        let reactive_requests = {
            let backend = self.backend.borrow();
            self.mem_pool
                .reactive_add_block(block, &self.peers, &*backend, self.time)
        };

        self.emit_reactive_initial_vote_requests(&reactive_requests, outbound_messages);
    }

    pub fn committed_block(&self, block_id: &BlockId) -> Option<Block> {
        EcBlocks::lookup(&*self.backend.borrow(), block_id)
    }

    pub fn knows_block(&self, block_id: &BlockId) -> bool {
        let backend = self.backend.borrow();
        self.mem_pool.status(block_id, &*backend).is_some()
    }

    pub fn mempool_diagnostics(&self) -> MempoolDiagnostics {
        self.mem_pool.diagnostics(self.time)
    }

    pub fn vote_ingress_diagnostics(&self) -> VoteIngressDiagnostics {
        self.vote_diagnostics
    }

    pub fn local_scope_contains(&self, token: TokenId) -> bool {
        self.peers.local_scope_contains(token)
    }

    pub fn vote_eligible_peer_count(&self, token: TokenId) -> usize {
        self.peers.vote_eligible_peer_count(token)
    }

    pub fn active_hop_distance_to_token(&self, token: TokenId) -> Option<usize> {
        self.peers.active_hop_distance(self.peer_id, token)
    }

    pub fn vote_targets_for_token_at(&self, token: TokenId, time: EcTime) -> Vec<PeerId> {
        self.peers.vote_target_peers_for(&token, time)
    }

    /**
     * TODO move all this into an ec_orchestrator. A module to control "ticks" and to collect and schedule messages
     *
     * TODO
     * Here we need to see all at least vote-for tokens - such that conflicts can be detected.
     * In case of competing (possitive) updates to a token we vote for "higest" block_id.
     *
     * TODO
     * We should test collecting multi-messages. Such that votes to the same node gets send together.
    // TODO pack messages in Message2 style
    // - idea: the oldest transaction (longest in mempool) "sucks" all overlapping into message - sync. on roll.
    // (when commited or timeout - this schedule of a neighborhood is then "freed" of the next oldest etc)
    // TODO could e.g make sure vote msg. for all trxs go to same peers - such that all detect the conflict
     *
     * We should also investigate if (like in earlier prototypes) we can reduce the votes by only sending to trusted nodes that hasn't responded yet.
     */
    pub fn tick(&mut self, outbound_messages: &mut Vec<MessageEnvelope>) {
        self.time += 1;
        let mut local_responses = Vec::new();
        let responses = &mut local_responses;

        // Rotate ticket secrets if needed
        self.ticket_manager.tick(self.time);

        // Process mempool in phases
        let mut messages = {
            // Phase 0: Cleanup expired blocks
            self.mem_pool.cleanup_expired(self.time);

            // Phase 1: Evaluate pending blocks (immutable borrow)
            // This checks token chains and generates block/parent repair requests.
            let (evaluations, reorg_messages) = {
                let backend = self.backend.borrow();
                self.mem_pool.evaluate_pending_blocks(
                    &*backend,
                    self.time,
                    self.peer_id,
                    &mut *self.event_sink,
                )
            };

            let mut all_messages = reorg_messages;

            // Phase 2: Process committable blocks (mutable borrow + batch)
            let (commit_messages, commit_transitions) = {
                let mut backend_ref = self.backend.borrow_mut();
                let mut batch = backend_ref.begin_batch();

                let outcome = self.mem_pool.tick_with_evaluations(
                    &self.peers,
                    self.time,
                    self.peer_id,
                    &mut *self.event_sink,
                    &evaluations,
                    &mut *batch,
                );

                // Commit the batch - all blocks and tokens committed atomically
                if let Err(e) = batch.commit() {
                    // Infrastructure error - log and continue (batch is discarded)
                    eprintln!("Failed to commit batch at time {}: {}", self.time, e);
                }

                outcome
            };

            for transition in commit_transitions {
                for voter in transition.interested_voters {
                    if voter == self.peer_id {
                        continue;
                    }

                    responses.push(self.send_commit_update(
                        voter,
                        transition.committed_block_id,
                        transition.competing_block_id,
                    ));
                }
            }

            // Combine reorg requests (phase 1) with vote requests (phase 2)
            all_messages.extend(commit_messages);
            all_messages
        };

        messages.sort_unstable_by_key(MessageRequest::sort_key);

        // Keep only the first vote we emit per token in this tick. Because requests are
        // sorted by token and highest block_id first, later conflicts for the same token
        // are suppressed here.
        let mut previous_token: Option<TokenId> = None;
        let mut primary_block_for_token: Option<BlockId> = None;
        for request in &messages {
            match request {
                MessageRequest::Vote(block_id, token_id, vote, _, sequence) => {
                    if previous_token != Some(*token_id) {
                        previous_token = Some(*token_id);
                        primary_block_for_token = Some(*block_id);
                    }
                    let primary_for_token = primary_block_for_token == Some(*block_id);
                    let vote = if !primary_for_token { 0 } else { *vote };

                    for peer_id in self
                        .peers
                        .vote_target_pair_for_sequence(token_id, *sequence as usize)
                    {
                        responses.push(MessageEnvelope {
                            sender: self.peer_id,
                            receiver: peer_id,
                            ticket: 0,
                            time: self.time,
                            message: Message::Vote {
                                block_id: *block_id,
                                vote,
                                reply: true,
                            },
                        });
                    }
                }
                MessageRequest::Block(block_id) => {
                    let peer_id = self
                        .mem_pool
                        .last_interested_voter(&block_id)
                        .unwrap_or_else(|| self.peers.peer_for(&block_id, self.time));
                    self.vote_diagnostics.block_requests_triggered += 1;
                    responses.push(self.request_block(
                        &peer_id,
                        &block_id,
                        BlockUseCase::MempoolBlock,
                    ));
                }
                MessageRequest::Parent(block_id, parent_id) => {
                    self.emit_parent_validation_request(*block_id, *parent_id, responses);
                }
                MessageRequest::MissingParent(parent_id, child_id, token_idx) => {
                    self.emit_missing_parent_request(*parent_id, *child_id, *token_idx, responses);
                }
            }
        }

        // Phase 4: Drive peer discovery/lifecycle so full-node simulations exercise
        // both elections and commit-chain head exchange.
        let peer_actions = self.peers.tick(&self.token_storage, self.time);

        // Phase 5: Commit chain sync
        // Periodically query nearby peers to keep our commit chain up to date
        let sync_actions = if self.enable_commit_chain_sync {
            let mut backend = self.backend.borrow_mut();
            backend.commit_chain_tick(&self.peers, &mut self.mem_pool, self.time)
        } else {
            Vec::new()
        };

        let head_of_chain = self.backend.borrow().get_commit_chain_head().unwrap_or(0);

        for action in peer_actions {
            match action {
                PeerAction::SendQuery { receiver, .. }
                | PeerAction::SendInvitation { receiver, .. } => {
                    responses.push(action.into_envelope(
                        self.peer_id,
                        receiver,
                        self.time,
                        head_of_chain,
                    ));
                }
                PeerAction::SendAnswer { .. } | PeerAction::SendReferral { .. } => {
                    unreachable!("EcPeers::tick only produces query/invitation actions")
                }
            }
        }

        // Convert commit chain actions to message envelopes
        for (receiver, tick_message) in sync_actions {
            use crate::ec_commit_chain::TickMessage;
            match tick_message {
                TickMessage::QueryBlock { block_id, .. } => {
                    let ticket = self
                        .ticket_manager
                        .generate_ticket(block_id, BlockUseCase::CommitChain);
                    responses.push(MessageEnvelope {
                        sender: self.peer_id,
                        receiver,
                        ticket,
                        time: self.time,
                        message: Message::QueryBlock {
                            block_id,
                            target: self.peer_id, // We're the target for the response
                            ticket,
                        },
                    });
                }
                TickMessage::QueryCommitBlock { block_id, ticket } => {
                    // For commit blocks, use the specified receiver (peer from tracked_peers)
                    responses.push(MessageEnvelope {
                        sender: self.peer_id,
                        receiver,
                        ticket,
                        time: self.time,
                        message: Message::QueryCommitBlock { block_id, ticket },
                    });
                }
            }
        }

        self.coalesce_request_batches(responses);
        outbound_messages.extend(local_responses);
    }

    /*
    Vote cases:

        Block in mem-pool (or previously committed)
            IF block is blocked -> reply negative vote
            ELSE IF block is committed -> reply positive vote
            ELSE IF trusted peer -> vote

        Block not in mem-pool
            IF trusted peer - record vote placeholder and fetch block on tick
            ELSE IF client ticket - record vote placeholder and fetch block on tick
            InitialVote also tries to supply the block directly via reactive_add_block
    */

    /// Validate an identity-block (test mode)
    ///
    /// In test mode, validation is minimal:
    /// - Must have at least 2 tokens (peer-id + salt)
    /// - First token should be the peer-id
    /// - First token must be genesis (last == 0)
    fn validate_identity_block_test(&self, block: &Block, _sender: PeerId) -> bool {
        // 1. Must have at least 2 tokens (peer-id + salt)
        if block.used < 2 {
            log::warn!(
                "Identity-block rejected: insufficient tokens (need at least 2, got {})",
                block.used
            );
            return false;
        }

        // 2. GENESIS REQUIREMENT: Must be new peer-id
        if block.parts[0].last != 0 {
            log::warn!(
                "Identity-block rejected: not genesis (last = {})",
                block.parts[0].last
            );
            return false;
        }

        // 3. In test mode, we just accept it if it has the right structure
        // Production mode will add PoW validation here
        true
    }

    pub fn handle_message(
        &mut self,
        msg: &MessageEnvelope,
        outbound_messages: &mut Vec<MessageEnvelope>,
    ) {
        let mut local_responses = Vec::new();
        self.handle_message_inner(msg, &mut local_responses);
        self.coalesce_request_batches(&mut local_responses);
        outbound_messages.extend(local_responses);
    }

    fn emit_reactive_initial_vote_requests(
        &self,
        requests: &[InitialVoteRequest],
        responses: &mut Vec<MessageEnvelope>,
    ) {
        for request in requests {
            if request.receiver == self.peer_id {
                continue;
            }

            responses.push(MessageEnvelope {
                sender: self.peer_id,
                receiver: request.receiver,
                ticket: 1,
                time: self.time,
                message: Message::InitialVote {
                    block: request.block,
                    vote: request.vote,
                },
            });
        }
    }

    fn emit_parent_validation_request(
        &mut self,
        block_id: BlockId,
        parent_id: BlockId,
        responses: &mut Vec<MessageEnvelope>,
    ) {
        let backend = self.backend.borrow();
        if let Some(parent) = self.mem_pool.query(&parent_id, &*backend) {
            drop(backend);
            self.mem_pool.validate_with(&parent, &block_id);
        } else {
            drop(backend);
            let peer_id = self.peers.peer_for(&parent_id, self.time);
            self.vote_diagnostics.parent_validation_requests_triggered += 1;
            responses.push(self.request_block(&peer_id, &block_id, BlockUseCase::ValidateWith));
        }
    }

    fn emit_missing_parent_request(
        &mut self,
        parent_block_id: BlockId,
        child_block_id: BlockId,
        token_idx: usize,
        responses: &mut Vec<MessageEnvelope>,
    ) {
        // Try to find a voter who voted positive for this token - they have the parent
        let peer_id = self
            .mem_pool
            .positive_voter_for_token(&child_block_id, token_idx)
            .unwrap_or_else(|| self.peers.peer_for(&parent_block_id, self.time));

        // Mark that we've requested parents for cooldown tracking
        self.mem_pool.mark_parent_fetch(&child_block_id, self.time);

        self.vote_diagnostics.missing_parent_requests_triggered += 1;
        responses.push(self.request_block(&peer_id, &parent_block_id, BlockUseCase::ParentBlock));
    }

    fn reactive_accept_mempool_block(
        &mut self,
        block: &Block,
        sender: PeerId,
        responses: &mut Vec<MessageEnvelope>,
    ) -> bool {
        let reactive_requests = {
            let backend = self.backend.borrow();
            self.mem_pool
                .reactive_add_block(block, &self.peers, &*backend, self.time)
        };

        self.event_sink.log(
            self.time,
            self.peer_id,
            Event::BlockReceived {
                block_id: block.id,
                peer: sender,
                size: block.used,
            },
        );

        self.emit_reactive_initial_vote_requests(&reactive_requests, responses);

        true
    }

    fn handle_message_inner(
        &mut self,
        msg: &MessageEnvelope,
        responses: &mut Vec<MessageEnvelope>,
    ) {
        match &msg.message {
            Message::RequestBatch { items } => {
                for item in items.iter().cloned() {
                    let submessage = MessageEnvelope {
                        sender: msg.sender,
                        receiver: msg.receiver,
                        ticket: item.ticket(),
                        time: msg.time,
                        message: item.into_message(),
                    };
                    self.handle_message_inner(&submessage, responses);
                }
            }
            Message::Vote {
                block_id: block,
                vote,
                reply,
            } => {
                self.event_sink.log(
                    self.time,
                    self.peer_id,
                    Event::VoteReceived {
                        block_id: *block,
                        from_peer: msg.sender,
                    },
                );

                let trusted_sender = self.peers.trusted_peer(&msg.sender).is_some();
                let block_status = {
                    let backend = self.backend.borrow();
                    self.mem_pool.status(block, &*backend)
                };

                match (block_status, trusted_sender) {
                    (Some(BlockState::Pending), true) => {
                        self.vote_diagnostics.trusted_votes_recorded += 1;
                        self.mem_pool
                            .vote(block, *vote, &msg.sender, msg.time, *reply);
                    }
                    (Some(BlockState::Commit), _) => {
                        if *reply {
                            responses.push(self.reply_direct(&msg.sender, block, false));
                        }
                    }
                    (Some(BlockState::Blocked), _) => {
                        if *reply {
                            responses.push(self.send_blocked_update(msg.sender, *block, None));
                        }
                    }
                    (None, true) => {
                        self.vote_diagnostics.trusted_votes_recorded += 1;
                        self.mem_pool
                            .vote(block, *vote, &msg.sender, msg.time, *reply);
                    }
                    (None, false) => {
                        self.vote_diagnostics.untrusted_votes_received += 1;

                        if msg.ticket > 0 {
                            self.mem_pool
                                .vote(block, *vote, &msg.sender, msg.time, *reply);
                        }
                    }
                    _ => {} // discard - do nothing
                }
            }
            Message::InitialVote { block, vote } => {
                if self.peers.trusted_peer(&msg.sender).is_none() {
                    self.vote_diagnostics.untrusted_votes_received += 1;

                    // TODO client ticket check
                    if msg.ticket == 0 {
                        return;
                    }
                }

                self.mem_pool
                    .vote(&block.id, *vote, &msg.sender, msg.time, true);

                self.reactive_accept_mempool_block(block, msg.sender, responses);
            }
            Message::QueryBlock {
                block_id,
                target,
                ticket,
            } => {
                let receiver = if *target == 0 { msg.sender } else { *target };

                let backend = self.backend.borrow();
                if let Some(me) =
                    self.mem_pool
                        .query(block_id, &*backend)
                        .map(|block| MessageEnvelope {
                            sender: self.peer_id,
                            receiver,
                            ticket: *ticket,
                            time: self.time,
                            message: Message::Block { block },
                        })
                {
                    // this node has this block
                    responses.push(me)
                } else if let None = self.peers.trusted_peer(&msg.sender) {
                    // this is not a trusted peer - send Referral
                    responses.push(self.send_referral(msg.sender, *block_id, *ticket));
                } else {
                    // trusted peer - forward with 2/3 probability, otherwise send Referral
                    use rand::Rng;
                    if self.rng.gen_bool(2.0 / 3.0) {
                        // Forward the query on-behalf-of the original requester
                        let peer_id = self.peers.peer_for(block_id, self.time);

                        responses.push(MessageEnvelope {
                            sender: self.peer_id,
                            receiver: peer_id,
                            ticket: *ticket,
                            time: self.time,
                            message: Message::QueryBlock {
                                block_id: *block_id,
                                target: receiver,
                                ticket: *ticket,
                            },
                        });

                        self.event_sink.log(
                            self.time,
                            self.peer_id,
                            Event::BlockNotFound {
                                block_id: *block_id,
                                peer: self.peer_id,
                                from_peer: receiver,
                            },
                        );
                    } else {
                        // Send Referral instead of forwarding
                        responses.push(self.send_referral(msg.sender, *block_id, *ticket));
                    }
                }
            }
            Message::QueryToken {
                token_id,
                target,
                ticket,
            } => {
                let receiver = if *target == 0 { msg.sender } else { *target };

                // Forward to EcPeers for token lookup
                if let Some(action) =
                    self.peers
                        .handle_query(&self.token_storage, *token_id, *ticket, msg.sender)
                {
                    // Convert PeerAction to MessageEnvelope
                    match action {
                        PeerAction::SendAnswer { .. } => {
                            // Simple case: use helper to convert action to envelope
                            let head_of_chain =
                                self.backend.borrow().get_commit_chain_head().unwrap_or(0);
                            responses.push(action.into_envelope(
                                self.peer_id,
                                receiver,
                                self.time,
                                head_of_chain,
                            ));
                        }
                        PeerAction::SendReferral {
                            token,
                            ticket,
                            suggested_peers: _,
                        } => {
                            // Check if requesting peer is Connected and forward with 2/3 probability
                            let should_forward = if self.peers.trusted_peer(&msg.sender).is_some() {
                                use rand::Rng;
                                self.rng.gen_bool(2.0 / 3.0)
                            } else {
                                false
                            };

                            if should_forward {
                                // Forward the query on-behalf-of the original requester
                                let forward_to = self.peers.peer_for(&token, self.time);
                                responses.push(MessageEnvelope {
                                    sender: self.peer_id,
                                    receiver: forward_to,
                                    ticket,
                                    time: self.time,
                                    message: Message::QueryToken {
                                        token_id: token,
                                        target: receiver,
                                        ticket,
                                    },
                                });
                            } else {
                                // Send Referral (for non-connected peers or 1/3 probability)
                                responses.push(self.send_referral(receiver, token, ticket));
                            }
                        }
                        // handle_query only returns SendAnswer or SendReferral
                        _ => unreachable!("handle_query only returns SendAnswer or SendReferral"),
                    }
                }
            }
            Message::Answer {
                answer,
                signature,
                head_of_chain,
            } => {
                let actions = self.peers.handle_answer(
                    answer,
                    signature,
                    msg.ticket,
                    msg.sender,
                    self.time,
                    &self.token_storage,
                    *head_of_chain,
                );

                // Process returned actions (e.g., Invitations, Queries)
                // Use helper to convert each action to envelope
                let head_of_chain = self.backend.borrow().get_commit_chain_head().unwrap_or(0);
                for action in actions {
                    match action {
                        PeerAction::SendInvitation { receiver, .. }
                        | PeerAction::SendQuery { receiver, .. } => {
                            responses.push(action.into_envelope(
                                self.peer_id,
                                receiver,
                                self.time,
                                head_of_chain,
                            ));
                        }
                        _ => {
                            // Ignore other action types (shouldn't happen from handle_answer)
                        }
                    }
                }
            }
            Message::Block { block } => {
                // TODO basic common block-validation (like SHA of content match block.id)
                // Special case: Identity-block (zero-ticket)
                if msg.ticket == 0 {
                    if self.validate_identity_block_test(&block, msg.sender) {
                        // Submit to mempool like normal blocks
                        self.reactive_accept_mempool_block(block, msg.sender, responses);

                        self.event_sink.log(
                            self.time,
                            self.peer_id,
                            Event::IdentityBlockReceived {
                                peer_id: block.parts[0].token,
                                sender: msg.sender,
                            },
                        );
                    } else {
                        log::warn!("Invalid identity-block received from peer {}", msg.sender);
                    }
                } else if let Some(use_case) =
                    self.ticket_manager.validate_ticket(msg.ticket, block.id)
                {
                    // Ticket is valid - route based on use case
                    match use_case {
                        BlockUseCase::MempoolBlock | BlockUseCase::ParentBlock => {
                            self.reactive_accept_mempool_block(block, msg.sender, responses);
                        }
                        BlockUseCase::CommitChain => {
                            // Commit chain block - delegate to backend
                            let mut backend = self.backend.borrow_mut();
                            backend.handle_block(block.clone(), msg.ticket);
                        }
                        BlockUseCase::ValidateWith => {
                            // Validation request - delegate to mempool
                            self.mem_pool.validate_with(block, &msg.ticket)
                        }
                    }
                } else {
                    // Invalid ticket - reject the block
                    log::debug!(
                        "Rejected block {} from peer {} - invalid ticket",
                        block.id,
                        msg.sender
                    );
                }
            }
            Message::Referral { token, high, low } => {
                // TODO basic common block-validation (like SHA of content match block.id)
                if let Some(use_case) = self.ticket_manager.validate_ticket(msg.ticket, *token) {
                    // Valid ticket for MempoolBlock or ParentBlock requests
                    if matches!(
                        use_case,
                        BlockUseCase::MempoolBlock | BlockUseCase::ParentBlock | BlockUseCase::ValidateWith
                    ) {
                        let receiver = if self.rng.gen_bool(1.0/2.0) {
                            low
                        } else {
                            high
                        };

                        responses.push(MessageEnvelope {
                            sender: self.peer_id,
                            receiver: *receiver,
                            ticket: 0,
                            time: self.time,
                            message: Message::QueryBlock {
                                block_id: *token,
                                target: 0,
                                ticket: msg.ticket,
                            },
                        });
                    }
                } else if let Some(_peer_action) = self.peers.handle_referral(
                    msg.ticket,
                    *token,
                    [*high, *low],
                    msg.sender,
                    self.time,
                ) {
                    // Referral handled by peer manager
                }
            }
            Message::QueryCommitBlock { block_id, ticket } => {
                // Query our commit chain for the requested block
                let backend = self.backend.borrow();
                if let Some(commit_block) = backend.query_commit_block(*block_id) {
                    // We have it - send it back
                    responses.push(MessageEnvelope {
                        sender: self.peer_id,
                        receiver: msg.sender,
                        ticket: *ticket,
                        time: self.time,
                        message: Message::CommitBlock {
                            block: commit_block,
                        },
                    });
                }
                // If we don't have it, ignore the query
            }
            Message::CommitBlock { block } => {
                // Handle incoming commit block from peer
                let mut backend = self.backend.borrow_mut();
                if let Some(request) =
                    backend.handle_commit_block(block.clone(), msg.sender, msg.ticket, self.time)
                {
                    // Block didn't connect - need to request parent
                    responses.push(MessageEnvelope {
                        sender: self.peer_id,
                        receiver: request.receiver,
                        ticket: request.ticket,
                        time: self.time,
                        message: Message::QueryCommitBlock {
                            block_id: request.block_id,
                            ticket: request.ticket,
                        },
                    });
                }
            }
        }
    }

    fn coalesce_request_batches(&self, responses: &mut Vec<MessageEnvelope>) {
        if !self.enable_request_batching {
            return;
        }

        if responses.len() < 2 {
            return;
        }

        let original = std::mem::take(responses);
        let mut coalesced: Vec<MessageEnvelope> = Vec::with_capacity(original.len());
        let mut open_batches: HashMap<PeerId, usize> = HashMap::new();

        for envelope in original {
            if let Some(item) =
                BatchRequestItem::from_message(&envelope.message, self.batch_vote_replies)
            {
                if let Some(&idx) = open_batches.get(&envelope.receiver) {
                    if let Message::RequestBatch { items } = &mut coalesced[idx].message {
                        items.push(item);
                    }
                } else {
                    coalesced.push(MessageEnvelope {
                        sender: envelope.sender,
                        receiver: envelope.receiver,
                        ticket: 0,
                        time: envelope.time,
                        message: Message::RequestBatch { items: vec![item] },
                    });
                    open_batches.insert(envelope.receiver, coalesced.len() - 1);
                }
            } else {
                open_batches.remove(&envelope.receiver);
                coalesced.push(envelope);
            }
        }

        for envelope in &mut coalesced {
            let Message::RequestBatch { items } = &mut envelope.message else {
                continue;
            };

            if items.len() == 1 {
                let item = items
                    .pop()
                    .expect("single-item batch should contain one item");
                envelope.ticket = item.ticket();
                envelope.message = item.into_message();
            }
        }

        *responses = coalesced;
    }

    fn reply_direct(&self, target: &PeerId, block: &BlockId, blocked: bool) -> MessageEnvelope {
        self.reply_direct_vote(target, block, if blocked { 0 } else { 0xFF })
    }

    fn reply_direct_vote(&self, target: &PeerId, block: &BlockId, vote: u8) -> MessageEnvelope {
        MessageEnvelope {
            sender: self.peer_id,
            receiver: *target,
            ticket: 0,
            time: self.time,
            message: Message::Vote {
                block_id: *block,
                vote,
                reply: false,
            },
        }
    }

    fn send_commit_update(
        &self,
        target: PeerId,
        committed_block: BlockId,
        competing_block: Option<BlockId>,
    ) -> MessageEnvelope {
        if let Some(competing_block) = competing_block.filter(|block| *block < committed_block) {
            return MessageEnvelope {
                sender: self.peer_id,
                receiver: target,
                ticket: 0,
                time: self.time,
                message: Message::RequestBatch {
                    items: vec![
                        BatchRequestItem::Vote {
                            block_id: competing_block,
                            vote: 0,
                            reply: false,
                        },
                        BatchRequestItem::Vote {
                            block_id: committed_block,
                            vote: 0xFF,
                            reply: false,
                        },
                    ],
                },
            };
        }

        self.reply_direct_vote(&target, &committed_block, 0xFF)
    }

    fn send_blocked_update(
        &self,
        target: PeerId,
        blocked_block: BlockId,
        preferred_higher_block: Option<BlockId>,
    ) -> MessageEnvelope {
        let Some(higher_block) = preferred_higher_block else {
            return self.reply_direct(&target, &blocked_block, true);
        };
        let higher_vote = match self.mem_pool.status(&higher_block, &*self.backend.borrow()) {
            Some(BlockState::Commit) => 0xFF,
            _ => 0,
        };

        MessageEnvelope {
            sender: self.peer_id,
            receiver: target,
            ticket: 0,
            time: self.time,
            message: Message::RequestBatch {
                items: vec![
                    BatchRequestItem::Vote {
                        block_id: blocked_block,
                        vote: 0,
                        reply: false,
                    },
                    BatchRequestItem::Vote {
                        block_id: higher_block,
                        vote: higher_vote,
                        reply: false,
                    },
                ],
            },
        }
    }

    fn request_block(
        &self,
        receiver: &PeerId,
        block: &BlockId,
        use_case: BlockUseCase,
    ) -> MessageEnvelope {
        let ticket = self.ticket_manager.generate_ticket(*block, use_case);

        MessageEnvelope {
            sender: self.peer_id,
            receiver: *receiver,
            ticket: 0,
            time: self.time,
            message: Message::QueryBlock {
                block_id: *block,
                target: 0,
                ticket,
            },
        }
    }

    fn send_referral(
        &self,
        receiver: PeerId,
        token: u64,
        ticket: MessageTicket,
    ) -> MessageEnvelope {
        let peers = self.peers.peers_for(&token, self.time);
        MessageEnvelope {
            sender: self.peer_id,
            receiver,
            ticket,
            time: self.time,
            message: Message::Referral {
                token,
                high: peers[1],
                low: peers[0],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use rand::SeedableRng;

    use crate::ec_interface::{BatchRequestItem, Message, MessageEnvelope, TokenBlock};
    use crate::ec_memory_backend::{MemTokens, MemoryBackend};
    use crate::ec_peers::PeerManagerConfig;
    use crate::ec_proof_of_storage::TokenStorageBackend;

    use super::EcNode;

    #[test]
    fn pending_vote_request_does_not_fast_reply_before_terminal_state() {
        let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(1)));
        TokenStorageBackend::set(backend.borrow_mut().tokens_mut(), &11, &100, &0, 0);
        TokenStorageBackend::set(backend.borrow_mut().tokens_mut(), &12, &555, &0, 0);

        let rng = rand::rngs::StdRng::from_seed([7u8; 32]);
        let mut node = EcNode::new(backend.clone(), 1, 0, MemTokens::new(), rng);
        node.seed_peer(&2);

        let block = crate::ec_interface::Block {
            id: 77,
            time: 0,
            used: 2,
            parts: [
                TokenBlock {
                    token: 11,
                    last: 100,
                    key: 0,
                },
                TokenBlock {
                    token: 12,
                    last: 999,
                    key: 0,
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
            signatures: [None; crate::ec_interface::TOKENS_PER_BLOCK],
        };
        node.block(&block);

        let inbound = MessageEnvelope {
            sender: 2,
            receiver: 1,
            ticket: 0,
            time: 1,
            message: Message::Vote {
                block_id: block.id,
                vote: 0,
                reply: true,
            },
        };

        let mut responses = Vec::new();
        node.handle_message(&inbound, &mut responses);

        assert!(
            responses.is_empty(),
            "pending blocks should no longer fast reply before they reach Commit or Blocked"
        );
    }

    #[test]
    fn pending_vote_request_without_prior_state_does_not_fast_reply() {
        let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(1)));
        TokenStorageBackend::set(backend.borrow_mut().tokens_mut(), &11, &100, &0, 0);

        let rng = rand::rngs::StdRng::from_seed([9u8; 32]);
        let mut node = EcNode::new(backend.clone(), 1, 0, MemTokens::new(), rng);
        node.seed_peer(&2);

        let block = crate::ec_interface::Block {
            id: 88,
            time: 0,
            used: 2,
            parts: [
                TokenBlock {
                    token: 11,
                    last: 100,
                    key: 0,
                },
                TokenBlock {
                    token: 12,
                    last: 0,
                    key: 0,
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
            signatures: [None; crate::ec_interface::TOKENS_PER_BLOCK],
        };
        node.block(&block);

        let inbound = MessageEnvelope {
            sender: 2,
            receiver: 1,
            ticket: 0,
            time: 1,
            message: Message::Vote {
                block_id: block.id,
                vote: 0,
                reply: true,
            },
        };

        let mut responses = Vec::new();
        node.handle_message(&inbound, &mut responses);

        assert!(
            responses.is_empty(),
            "proxy-only pending blocks should not fast reply without prior token state"
        );
    }

    #[test]
    fn untrusted_plain_vote_without_client_ticket_is_ignored() {
        let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(1)));
        let rng = rand::rngs::StdRng::from_seed([23u8; 32]);
        let mut node = EcNode::new(backend, 1, 0, MemTokens::new(), rng);

        let mut responses = Vec::new();
        node.handle_message(
            &MessageEnvelope {
                sender: 2,
                receiver: 1,
                ticket: 0,
                time: 1,
                message: Message::Vote {
                    block_id: 333,
                    vote: 0,
                    reply: true,
                },
            },
            &mut responses,
        );

        assert!(responses.is_empty());
        assert_eq!(node.vote_ingress_diagnostics().untrusted_votes_received, 1);

        node.tick(&mut responses);

        assert!(
            !responses.iter().any(|envelope| matches!(
                envelope.message,
                Message::QueryBlock { block_id, .. } if block_id == 333
            )),
            "plain untrusted votes without a client ticket should not trigger block fetches",
        );
        assert!(!node.knows_block(&333));
    }

    #[test]
    fn initial_vote_arrival_before_tick_skips_missing_block_fetch() {
        let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(1)));
        TokenStorageBackend::set(backend.borrow_mut().tokens_mut(), &11, &100, &0, 0);

        let rng = rand::rngs::StdRng::from_seed([22u8; 32]);
        let mut node = EcNode::new(backend.clone(), 1, 0, MemTokens::new(), rng);
        for peer_id in [2, 3, 4, 5, 6] {
            node.seed_peer(&peer_id);
        }

        let block = crate::ec_interface::Block {
            id: 192,
            time: 1,
            used: 1,
            parts: [
                TokenBlock {
                    token: 11,
                    last: 100,
                    key: 0,
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
            signatures: [None; crate::ec_interface::TOKENS_PER_BLOCK],
        };

        let early_vote = MessageEnvelope {
            sender: 2,
            receiver: 1,
            ticket: 0,
            time: 1,
            message: Message::Vote {
                block_id: block.id,
                vote: 0,
                reply: true,
            },
        };

        let mut responses = Vec::new();
        node.handle_message(&early_vote, &mut responses);
        assert!(responses.is_empty());

        let initial_vote = MessageEnvelope {
            sender: 2,
            receiver: 1,
            ticket: 0,
            time: 1,
            message: Message::InitialVote { block, vote: 0 },
        };
        node.handle_message(&initial_vote, &mut responses);

        responses.clear();
        node.tick(&mut responses);

        assert!(
            !responses
                .iter()
                .any(|envelope| matches!(envelope.message, Message::QueryBlock { block_id, .. } if block_id == block.id)),
            "once the block arrived through InitialVote, the pending missing-block fetch should be skipped",
        );
    }

    #[test]
    fn trusted_missing_block_fetch_prefers_last_interested_voter_on_tick() {
        let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(1)));
        let rng = rand::rngs::StdRng::from_seed([29u8; 32]);
        let mut node = EcNode::new(backend, 1, 0, MemTokens::new(), rng);
        for peer_id in [2, 3, 4] {
            node.seed_peer(&peer_id);
        }

        let mut responses = Vec::new();
        for (sender, reply) in [(2, true), (3, false), (4, true)] {
            node.handle_message(
                &MessageEnvelope {
                    sender,
                    receiver: 1,
                    ticket: 0,
                    time: 1,
                    message: Message::Vote {
                        block_id: 333,
                        vote: 0,
                        reply,
                    },
                },
                &mut responses,
            );
        }

        assert!(responses.is_empty());

        node.tick(&mut responses);

        assert!(responses.iter().any(|envelope| {
            if envelope.receiver != 4 {
                return false;
            }

            match &envelope.message {
                Message::QueryBlock { block_id, .. } => *block_id == 333,
                Message::RequestBatch { items } => items.iter().any(|item| {
                    matches!(item, BatchRequestItem::QueryBlock { block_id, .. } if *block_id == 333)
                }),
                _ => false,
            }
        }));
        assert!(!responses.iter().any(|envelope| {
            if envelope.receiver != 3 {
                return false;
            }

            match &envelope.message {
                Message::QueryBlock { block_id, .. } => *block_id == 333,
                Message::RequestBatch { items } => items.iter().any(|item| {
                    matches!(item, BatchRequestItem::QueryBlock { block_id, .. } if *block_id == 333)
                }),
                _ => false,
            }
        }));
    }

    #[test]
    fn submit_local_block_emits_reactive_initial_votes() {
        let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(1)));
        TokenStorageBackend::set(backend.borrow_mut().tokens_mut(), &11, &100, &0, 0);

        let rng = rand::rngs::StdRng::from_seed([32u8; 32]);
        let mut node = EcNode::new(backend, 1, 0, MemTokens::new(), rng);
        for peer_id in [2, 3, 4, 5, 6] {
            node.seed_peer(&peer_id);
        }

        let block = crate::ec_interface::Block {
            id: 194,
            time: 1,
            used: 1,
            parts: [
                TokenBlock {
                    token: 11,
                    last: 100,
                    key: 0,
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
            signatures: [None; crate::ec_interface::TOKENS_PER_BLOCK],
        };

        let mut responses = Vec::new();
        node.submit_local_block(&block, &mut responses);

        assert!(node.knows_block(&block.id));
        assert_eq!(
            responses
                .iter()
                .filter(|envelope| matches!(
                    envelope.message,
                    Message::InitialVote { block: initial_block, vote: 0b0000_0001 }
                        if initial_block.id == block.id
                ))
                .count(),
            4,
            "local submission should seed one InitialVote per nearest active peer",
        );
    }

    #[test]
    fn coalesces_request_messages_by_receiver_without_swallowing_vote_replies() {
        let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(1)));
        let rng = rand::rngs::StdRng::from_seed([12u8; 32]);
        let node = EcNode::new(backend, 1, 0, MemTokens::new(), rng);

        let mut responses = vec![
            MessageEnvelope {
                sender: 1,
                receiver: 2,
                ticket: 0,
                time: 5,
                message: Message::Vote {
                    block_id: 10,
                    vote: 0b0000_0001,
                    reply: true,
                },
            },
            MessageEnvelope {
                sender: 1,
                receiver: 2,
                ticket: 99,
                time: 5,
                message: Message::QueryBlock {
                    block_id: 10,
                    target: 0,
                    ticket: 99,
                },
            },
            MessageEnvelope {
                sender: 1,
                receiver: 2,
                ticket: 0,
                time: 5,
                message: Message::Vote {
                    block_id: 10,
                    vote: 0b0000_0001,
                    reply: false,
                },
            },
        ];

        node.coalesce_request_batches(&mut responses);

        assert_eq!(responses.len(), 2);
        match &responses[0].message {
            Message::RequestBatch { items } => assert_eq!(items.len(), 2),
            other => panic!(
                "expected request batch, got {:?}",
                std::mem::discriminant(other)
            ),
        }
        match &responses[1].message {
            Message::Vote { reply, .. } => assert!(!reply),
            other => panic!(
                "expected standalone vote reply, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn batches_vote_replies_when_phase_two_is_enabled() {
        let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(1)));
        let rng = rand::rngs::StdRng::from_seed([13u8; 32]);
        let mut config = PeerManagerConfig::default();
        config.batch_vote_replies = true;
        let node = EcNode::new_with_peer_config(backend, 1, 0, MemTokens::new(), config, rng);

        let mut responses = vec![
            MessageEnvelope {
                sender: 1,
                receiver: 2,
                ticket: 0,
                time: 5,
                message: Message::Vote {
                    block_id: 10,
                    vote: 0b0000_0001,
                    reply: true,
                },
            },
            MessageEnvelope {
                sender: 1,
                receiver: 2,
                ticket: 0,
                time: 5,
                message: Message::Vote {
                    block_id: 10,
                    vote: 0b0000_0001,
                    reply: false,
                },
            },
        ];

        node.coalesce_request_batches(&mut responses);

        assert_eq!(responses.len(), 1);
        match &responses[0].message {
            Message::RequestBatch { items } => assert_eq!(items.len(), 2),
            other => panic!(
                "expected request batch with vote reply included, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn leaves_messages_standalone_when_batching_is_disabled() {
        let backend = Rc::new(RefCell::new(MemoryBackend::new_with_peer_id(1)));
        let rng = rand::rngs::StdRng::from_seed([14u8; 32]);
        let mut config = PeerManagerConfig::default();
        config.enable_request_batching = false;
        let node = EcNode::new_with_peer_config(backend, 1, 0, MemTokens::new(), config, rng);

        let original = vec![
            MessageEnvelope {
                sender: 1,
                receiver: 2,
                ticket: 0,
                time: 5,
                message: Message::Vote {
                    block_id: 10,
                    vote: 0b0000_0001,
                    reply: true,
                },
            },
            MessageEnvelope {
                sender: 1,
                receiver: 2,
                ticket: 99,
                time: 5,
                message: Message::QueryBlock {
                    block_id: 10,
                    target: 0,
                    ticket: 99,
                },
            },
        ];
        let mut responses = original.clone();

        node.coalesce_request_batches(&mut responses);

        assert_eq!(responses.len(), original.len());
        assert!(matches!(responses[0].message, Message::Vote { .. }));
        assert!(matches!(responses[1].message, Message::QueryBlock { .. }));
    }
}
