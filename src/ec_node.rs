use std::cell::RefCell;
use std::rc::Rc;

use crate::ec_interface::{
    BatchedBackend, Block, BlockId, EcBlocks, EcCommitChainAccess, EcTime, EcTokens, Event,
    EventSink, Message, MessageEnvelope, MessageTicket, NoOpSink, PeerId,
};
use crate::ec_mempool::{BlockState, EcMemPool};
use crate::ec_peers::{EcPeers, PeerAction};
use crate::ec_proof_of_storage::TokenStorageBackend;

use crate::ec_mempool::MessageRequest;

pub struct EcNode<
    B: BatchedBackend + EcTokens + EcBlocks + EcCommitChainAccess + 'static,
    T: TokenStorageBackend,
> {
    backend: Rc<RefCell<B>>,
    token_storage: T,
    peers: EcPeers,
    mem_pool: EcMemPool,
    peer_id: PeerId,
    time: EcTime,
    block_req_ticket: MessageTicket,
    parent_block_req_ticket: MessageTicket,
    event_sink: Box<dyn EventSink>,
    rng: rand::rngs::StdRng,
}

impl<B: BatchedBackend + EcTokens + EcBlocks + EcCommitChainAccess + 'static, T: TokenStorageBackend>
    EcNode<B, T>
{
    /// Create a new node with default NoOpSink (zero overhead)
    pub fn new(backend: Rc<RefCell<B>>, id: PeerId, time: EcTime, token_storage: T, rng: rand::rngs::StdRng) -> Self {
        Self::new_with_sink(backend, id, time, token_storage, Box::new(NoOpSink), rng)
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
        Self {
            mem_pool: EcMemPool::new(),
            backend,
            token_storage,
            peers: EcPeers::new(id),
            peer_id: id,
            time,
            block_req_ticket: 2, // TODO shuffle
            parent_block_req_ticket: 3,
            event_sink,
            rng,
        }
    }

    pub fn get_peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn seed_peer(&mut self, peer: &PeerId) {
        self.peers.update_peer(peer, self.time);
    }

    pub fn num_peers(&self) -> usize {
        self.peers.num_peers()
    }

    pub fn block(&mut self, block: &Block) {
        self.mem_pool.block(block, self.time);
    }

    pub fn committed_block(&self, block_id: &BlockId) -> Option<Block> {
        EcBlocks::lookup(&*self.backend.borrow(), block_id)
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
    pub fn tick(&mut self, responses: &mut Vec<MessageEnvelope>) {
        self.time += 1;

        // Process mempool in phases
        let mut messages = {
            // Phase 0: Cleanup expired blocks
            self.mem_pool.cleanup_expired(self.time);

            // Phase 1: Evaluate pending blocks (immutable borrow)
            // This checks token chains and generates parent requests for reorg/skip scenarios
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

            // TODO must move after check for "conflict detection" - Rule: Never commit a block, if a "higher-named" conflict exists
            // Phase 2: Process committable blocks (mutable borrow + batch)
            let commit_messages = {
                let mut backend_ref = self.backend.borrow_mut();
                let mut batch = backend_ref.begin_batch();

                let messages = self.mem_pool.tick_with_evaluations(
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

                messages
            };

            // Combine reorg requests (phase 1) with vote requests (phase 2)
            all_messages.extend(commit_messages);
            all_messages
        };

        messages.sort_unstable_by_key(MessageRequest::sort_key);

        // TODO check - and also applied to parent - oldest ref first
        let mut token: u64 = 0;
        for request in &messages {
            match request {
                MessageRequest::Vote(block_id, token_id, vote, _) => {
                    // block from second vote
                    let vote = if token == *token_id { 0 } else { *vote };
                    token = *token_id;

                    for peer_id in self.peers.peers_for(&token_id, self.time) {
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
                        })
                    }
                }
                MessageRequest::Parent(block_id, parent_id) => {
                    // TODO a work around. Should be handled in mem_pool
                    let backend = self.backend.borrow();
                    if let Some(parent) = self.mem_pool.query(&parent_id, &*backend) {
                        self.mem_pool.validate_with(&parent, &block_id);
                    } else {
                        let peer_id = self.peers.peer_for(&parent_id, self.time);

                        responses.push(self.request_block(&peer_id, &block_id, 0))
                    }
                }
                MessageRequest::MissingParent(block_id) => {
                    let peer_id = self.peers.peer_for(&block_id, self.time);

                    responses.push(self.request_block(
                        &peer_id,
                        &block_id,
                        self.parent_block_req_ticket,
                    ))
                }
            }
        }

        // Phase 4: Commit chain sync
        // Periodically query nearby peers to keep our commit chain up to date
        let sync_actions = {
            let mut backend = self.backend.borrow_mut();
            backend.commit_chain_tick(&self.peers, self.time)
        };

        // Convert commit chain actions to message envelopes
        for action in sync_actions {
            use crate::ec_commit_chain::CommitChainAction;
            match action {
                CommitChainAction::QueryBlock {
                    block_id,
                    ticket,
                } => {
                    // Use ec_peers to find a host for the block instead of the receiver
                    // This spreads load across the network and helps with discovery
                    let target = self.peers.peer_for(&block_id, self.time);
                    responses.push(MessageEnvelope {
                        sender: self.peer_id,
                        receiver: target,
                        ticket,
                        time: self.time,
                        message: Message::QueryBlock {
                            block_id,
                            target: self.peer_id, // We're the target for the response
                            ticket,
                        },
                    });
                }
                CommitChainAction::QueryCommitBlock {
                    receiver,
                    block_id,
                    ticket,
                } => {
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
    }

    /*
    Vote cases:

        Block in mem-pool (or previously committed)
            IF block is blocked -> reply negative vote
            ELSE IF block is committed -> reply positive vote
            ELSE IF trusted peer -> vote

        Block not in mem-pool
            IF trusted peer - start voting AND request block
            ELSE IF subscribed peer -> request block
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
            log::warn!("Identity-block rejected: insufficient tokens (need at least 2, got {})", block.used);
            return false;
        }

        // 2. GENESIS REQUIREMENT: Must be new peer-id
        if block.parts[0].last != 0 {
            log::warn!("Identity-block rejected: not genesis (last = {})", block.parts[0].last);
            return false;
        }

        // 3. In test mode, we just accept it if it has the right structure
        // Production mode will add PoW validation here
        true
    }

    pub fn handle_message(&mut self, msg: &MessageEnvelope, responses: &mut Vec<MessageEnvelope>) {
        match &msg.message {
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

                let backend = self.backend.borrow();
                match (
                    self.mem_pool.status(block, &*backend),
                    self.peers.trusted_peer(&msg.sender),
                ) {
                    (Some(BlockState::Pending), Some(_)) => {
                        self.mem_pool.vote(block, *vote, &msg.sender, msg.time);
                    }
                    (Some(BlockState::Commit), _) => {
                        // TODO if its a trusted peer - add to tick-pool?
                        if *reply {
                            responses.push(self.reply_direct(&msg.sender, block, false));
                        }
                    }
                    (Some(BlockState::Blocked), _) => {
                        // TODO if its a trusted peer - add to tick-pool?
                        if *reply {
                            // TODO send linked blockers with this one
                            responses.push(self.reply_direct(&msg.sender, block, true));
                        }
                    }
                    (None, Some(_)) => {
                        // TODO check load-balancing count for this peer
                        self.mem_pool.vote(block, *vote, &msg.sender, msg.time);
                        // better ask the sender for it - while propagating towards the "witness"
                        responses.push(self.request_block(
                            &msg.sender,
                            block,
                            self.block_req_ticket,
                        ))
                    }
                    (None, None) => {
                        // TODO test ticket is from subscribed client + DOS protection
                        if msg.ticket > 0 {
                            responses.push(self.request_block(
                                &msg.sender,
                                block,
                                self.block_req_ticket,
                            ))
                        }

                        // TODO this should be handled by "introduction" messages - linking peers
                        // but 2-way relations improve transaction-success alot
                        self.peers.update_peer(&msg.sender, self.time);
                    }
                    _ => {} // discard - do nothing
                }
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
                            let head_of_chain = self.backend.borrow().get_commit_chain_head().unwrap_or(0);
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
                let mut block_was_useful = false;
                let mut is_identity_block = false;

                // Special case: Identity-block (zero-ticket)
                if msg.ticket == 0 {
                    if self.validate_identity_block_test(&block, msg.sender) {
                        // Submit to mempool like normal blocks
                        if self.mem_pool.block(block, self.time) {
                            block_was_useful = true;
                            is_identity_block = true;

                            self.event_sink.log(
                                self.time,
                                self.peer_id,
                                Event::IdentityBlockReceived {
                                    peer_id: block.parts[0].token,
                                    sender: msg.sender,
                                },
                            );
                        }
                    } else {
                        log::warn!("Invalid identity-block received from peer {}", msg.sender);
                    }
                } else if msg.ticket == self.block_req_ticket ^ block.id
                    || msg.ticket == self.parent_block_req_ticket ^ block.id
                {
                    // Block request from mempool
                    if self.mem_pool.block(block, self.time) {
                        // TODO DOS-protection: Balance creations of entries from peers/clients
                        block_was_useful = true;

                        self.event_sink.log(
                            self.time,
                            self.peer_id,
                            Event::BlockReceived {
                                block_id: block.id,
                                peer: msg.sender,
                                size: block.used,
                            },
                        );
                    }
                } else {
                    // Try commit chain first (it verifies ticket internally)
                    let mut backend = self.backend.borrow_mut();
                    if backend.handle_block(block.clone(), msg.ticket) {
                        // Block was accepted by commit chain - ticket was valid
                        block_was_useful = true;
                    } else {
                        // Not a commit chain block - try validate_with for other requests
                        drop(backend); // Release borrow before calling mempool
                        self.mem_pool.validate_with(block, &msg.ticket)
                    }
                }

                // If the peer provided us with a useful block, add them as Identified
                // EXCEPT for identity-blocks (ticket=0), which should not grant trust
                // This prevents abuse where nodes spam identity-blocks to gain Identified status
                if block_was_useful && !is_identity_block {
                    self.peers.add_identified_peer(msg.sender, self.time);
                }
            }
            Message::Referral { token, high, low } => {
                // TODO basic common block-validation (like SHA of content match block.id)
                if msg.ticket == self.block_req_ticket ^ token
                    || msg.ticket == self.parent_block_req_ticket ^ token
                {
                    // TODO psudo random - inject common random
                    let receiver = if (msg.ticket ^ msg.time) & 1 == 0 {low} else {high};

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
                } else if let Some(_peer_action) = self.peers
                            .handle_referral(msg.ticket, *token, [*high, *low], msg.sender, self.time) {
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
                if let Some(request) = backend.handle_commit_block(block.clone(), msg.sender, msg.ticket, self.time) {
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

    fn reply_direct(&self, target: &PeerId, block: &BlockId, blocked: bool) -> MessageEnvelope {
        MessageEnvelope {
            sender: self.peer_id,
            receiver: *target,
            ticket: 0,
            time: self.time,
            message: Message::Vote {
                block_id: *block,
                vote: if blocked { 0 } else { 0xFF },
                reply: false,
            },
        }
    }

    fn request_block(
        &self,
        receiver: &PeerId,
        block: &BlockId,
        ticket: MessageTicket,
    ) -> MessageEnvelope {
        MessageEnvelope {
            sender: self.peer_id,
            receiver: *receiver,
            ticket: 0,
            time: self.time,
            message: Message::QueryBlock {
                block_id: *block,
                target: 0,
                ticket: ticket ^ block, // TODO calc ticket with SHA
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
