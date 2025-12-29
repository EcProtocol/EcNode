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
}

impl<B: BatchedBackend + EcTokens + EcBlocks + EcCommitChainAccess + 'static, T: TokenStorageBackend>
    EcNode<B, T>
{
    /// Create a new node with default NoOpSink (zero overhead)
    pub fn new(backend: Rc<RefCell<B>>, id: PeerId, time: EcTime, token_storage: T) -> Self {
        Self::new_with_sink(backend, id, time, token_storage, Box::new(NoOpSink))
    }

    /// Create a new node with a custom event sink for debugging/analysis
    pub fn new_with_sink(
        backend: Rc<RefCell<B>>,
        id: PeerId,
        time: EcTime,
        token_storage: T,
        event_sink: Box<dyn EventSink>,
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
        let sync_messages = {
            let mut backend = self.backend.borrow_mut();
            backend.commit_chain_tick(&self.peers, self.time)
        };
        responses.extend(sync_messages);
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
                    // this is not a trusted peer
                    let peers = self.peers.peers_for(block_id, self.time);

                    responses.push(MessageEnvelope {
                        sender: self.peer_id,
                        receiver: msg.sender,
                        ticket: *ticket,
                        time: self.time,
                        message: Message::Referral {
                            token: *block_id,
                            high: peers[1],
                            low: peers[0],
                        },
                    });
                } else if (block_id ^ self.time) & 0x3 == 0 {
                    // forwarding for trusted peers
                    let peer_id = self.peers.peer_for(block_id, self.time);

                    responses.push(MessageEnvelope {
                        sender: self.peer_id,
                        receiver: peer_id,
                        ticket: 0,
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
                        PeerAction::SendAnswer {
                            answer,
                            signature,
                            ticket,
                        } => {
                            let head_of_chain = self.backend.borrow().get_commit_chain_head().unwrap_or(0);
                            responses.push(MessageEnvelope {
                                sender: self.peer_id,
                                receiver,
                                ticket,
                                time: self.time,
                                message: Message::Answer {
                                    answer,
                                    signature,
                                    head_of_chain,
                                },
                            });
                        }
                        PeerAction::SendReferral {
                            token,
                            ticket,
                            suggested_peers,
                        } => {
                            responses.push(MessageEnvelope {
                                sender: self.peer_id,
                                receiver,
                                ticket,
                                time: self.time,
                                message: Message::Referral {
                                    token,
                                    high: suggested_peers[1],
                                    low: suggested_peers[0],
                                },
                            });
                        }
                        PeerAction::SendQuery {
                            receiver,
                            token,
                            ticket,
                        } => {
                            // Forward
                            responses.push(MessageEnvelope {
                                sender: self.peer_id,
                                receiver,
                                ticket,
                                time: self.time,
                                message: Message::QueryToken {
                                    token_id: token,
                                    target: 0,
                                    ticket,
                                },
                            });
                        }
                        PeerAction::SendInvitation { .. } => {
                            // Ignore SendInvitation (not relevant here)
                        }
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

                // Process returned actions (e.g., Invitations)
                for action in actions {
                    match action {
                        PeerAction::SendInvitation {
                            receiver,
                            answer,
                            signature,
                        } => {
                            let head_of_chain = self.backend.borrow().get_commit_chain_head().unwrap_or(0);
                            responses.push(MessageEnvelope {
                                receiver,
                                sender: self.peer_id,
                                ticket: 0, // Invitation uses ticket=0
                                time: self.time,
                                message: Message::Answer {
                                    answer,
                                    signature,
                                    head_of_chain,
                                },
                            });
                        }
                        PeerAction::SendQuery { receiver, token, ticket } => {
                            responses.push(MessageEnvelope {
                                receiver,
                                sender: self.peer_id,
                                ticket,
                                time: self.time,
                                message: Message::QueryToken {
                                    token_id: token,
                                    target: 0,
                                    ticket,
                                },
                            });
                        }
                        _ => {
                            // Ignore other action types
                        }
                    }
                }
            }
            Message::Block { block } => {
                // TODO basic common block-validation (like SHA of content match block.id)
                if msg.ticket == self.block_req_ticket ^ block.id
                    || msg.ticket == self.parent_block_req_ticket ^ block.id
                {
                    if self.mem_pool.block(block, self.time) {
                        // TODO DOS-protection: Balance creations of entries from peers/clients
                 
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
                    // else other req for blocks - or discard
                    self.mem_pool.validate_with(block, &msg.ticket)
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
                } else if let Some(peer_action) = self.peers
                            .handle_referral(msg.ticket, *token, [*high, *low], msg.sender, self.time) {

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
                // If we don't have it, ignore the query (could forward in the future)
            }
            Message::CommitBlock { block } => {
                // Handle incoming commit block from peer
                let mut backend = self.backend.borrow_mut();
                if let Some(mut parent_request) = backend.handle_commit_block(block.clone(), msg.sender, msg.ticket) {
                    // Block didn't connect - need to request parent
                    parent_request.time = self.time;
                    responses.push(parent_request);
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
}
