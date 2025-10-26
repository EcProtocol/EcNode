use std::cell::RefCell;
use std::rc::Rc;

use hashbrown::HashSet;

use crate::ec_interface::{
    Block, BlockId, EcBlocks, EcTime, EcTokens, Event, EventSink, Message, MessageEnvelope,
    MessageTicket, NoOpSink, PeerId,
};
use crate::ec_mempool::{BlockState, EcMemPool};
use crate::ec_peers::EcPeers;

use crate::ec_mempool::MessageRequest;

pub struct EcNode {
    tokens: Rc<RefCell<dyn EcTokens>>,
    blocks: Rc<RefCell<dyn EcBlocks>>,
    peers: EcPeers,
    mem_pool: EcMemPool,
    peer_id: PeerId,
    time: EcTime,
    block_req_ticket: MessageTicket,
    parent_block_req_ticket: MessageTicket,
    event_sink: Box<dyn EventSink>,
}

impl EcNode {
    /// Create a new node with default NoOpSink (zero overhead)
    pub fn new(
        tokens: Rc<RefCell<dyn EcTokens>>,
        blocks: Rc<RefCell<dyn EcBlocks>>,
        id: PeerId,
        time: EcTime,
    ) -> Self {
        Self::new_with_sink(tokens, blocks, id, time, Box::new(NoOpSink))
    }

    /// Create a new node with a custom event sink for debugging/analysis
    pub fn new_with_sink(
        tokens: Rc<RefCell<dyn EcTokens>>,
        blocks: Rc<RefCell<dyn EcBlocks>>,
        id: PeerId,
        time: EcTime,
        event_sink: Box<dyn EventSink>,
    ) -> Self {
        Self {
            tokens: tokens.clone(),
            blocks: blocks.clone(),
            peers: EcPeers::new(id),
            mem_pool: EcMemPool::new(tokens.clone(), blocks.clone()),
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
        self.blocks.borrow().lookup(block_id)
    }

    pub fn tick(&mut self, responses: &mut Vec<MessageEnvelope>) {
        self.time += 1;
        // TODO pack messages in Message2 style
        // - idea: the oldest transaction (longest in mempool) "sucks" all overlapping into message - sync. on roll.
        // (when commited or timeout - this schedule of a neighborhood is then "freed" of the next oldest etc)
        // TODO could e.g make sure vote msg. for all trxs go to same peers - such that all detect the conflict
        let mut messages = self
            .mem_pool
            .tick(&self.peers, self.time, self.peer_id, &mut *self.event_sink);
        messages.sort_unstable_by_key(MessageRequest::sort_key);

        // loop through and find any conflicting votes (true, true) - and block them.
        let mut token = 0;
        let mut blocked = HashSet::new();
        for request in &messages {
            if let MessageRequest::VOTE(block_id, token_id, _, true) = request {
                if token == *token_id {
                    // TODO mark as blocked

                    blocked.insert(block_id);
                    self.event_sink.log(
                        self.time,
                        self.peer_id,
                        Event::BlockStateChange {
                            block_id: *block_id,
                            from_state: "pending",
                            to_state: "blocked",
                        },
                    );
                }
                token = *token_id
            }
        }

        // TODO check - and also applied to parent - oldest ref first
        for request in &messages {
            match request {
                MessageRequest::VOTE(block_id, token_id, vote, _) => {
                    // blocked then all negative
                    let vote = if blocked.contains(&block_id) {0} else {*vote};

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
                MessageRequest::PARENT(block_id, parent_id) => {
                    // TODO a work around. Should be handled in mem_pool
                    if let Some(parent) = self.mem_pool.query(&parent_id) {
                        self.mem_pool.validate_with(&parent, &block_id);
                    } else {
                        let peer_id = self.peers.peer_for(&parent_id, self.time);

                        responses.push(self.request_block(&peer_id, &block_id, 0))
                    }
                }
                MessageRequest::PARENTCOMMIT(block_id) => {
                        let peer_id = self.peers.peer_for(&block_id, self.time);

                        responses.push(self.request_block(&peer_id, &block_id, self.parent_block_req_ticket))
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
    pub fn handle_message(&mut self, msg: &MessageEnvelope, responses: &mut Vec<MessageEnvelope>) {
        match &msg.message {
            Message::Vote {
                block_id: block,
                vote,
                reply,
            } => {
                self.event_sink.log(self.time, self.peer_id, Event::VoteReceived { 
                    block_id: *block, 
                    from_peer: msg.sender 
                } );

                match (
                    self.mem_pool.status(block),
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
                        responses.push(self.request_block(&msg.sender, block, self.block_req_ticket))
                    }
                    (None, None) => {
                        // TODO test ticket is from subscribed client + DOS protection
                        if msg.ticket > 0 {
                            responses.push(self.request_block(&msg.sender, block, self.block_req_ticket))
                        }

                        // TODO this should be handled by "introduction" messages - linking peers
                        // but 2-way relations improve transaction-success alot
                        self.peers.update_peer(&msg.sender, self.time);
                    }
                    _ => {} // discard - do nothing
                }
            }
            Message::Query {
                token,
                target,
                ticket,
            } => {
                let respond_to = if *target == 0 { msg.sender } else { *target };

                // TODO also for me ? And forwarding
                if let Some(me) = self.mem_pool.query(token).map(|block| MessageEnvelope {
                    sender: self.peer_id,
                    receiver: respond_to,
                    ticket: *ticket,
                    time: self.time,
                    message: Message::Block { block },
                }) {
                    responses.push(me)
                } else if (token ^ self.time) & 0x3 == 0  {
                    // TODO P(forwarding)
                    let peer_id = self.peers.peer_for(token, self.time);

                    responses.push(MessageEnvelope {
                        sender: self.peer_id,
                        receiver: peer_id,
                        ticket: 0,
                        time: self.time,
                        message: Message::Query {
                            token: *token,
                            target: respond_to,
                            ticket: *ticket,
                        },
                    });

                    self.event_sink.log(
                        self.time,
                        self.peer_id,
                        Event::BlockNotFound {
                            block_id: *token,
                            peer: self.peer_id,
                            from_peer: respond_to,
                        },
                    );
                }
            }
            Message::Answer { answer, signature } => {
                self.peers
                    .handle_answer(answer, signature, msg.ticket, msg.sender);
            }
            Message::Block { block } => {
                // TODO basic common block-validation (like SHA of content match block.id)
                if msg.ticket == self.block_req_ticket ^ block.id || 
                // TODO in this case the block must have "commit-at-history-id"
                msg.ticket == self.parent_block_req_ticket ^ block.id  {
                    // TODO DOS-protection

                    if self.mem_pool.block(block, self.time) {
                        let _is_reorg = msg.ticket == self.parent_block_req_ticket ^ block.id;
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

    fn request_block(&self, receiver: &PeerId, block: &BlockId, ticket: MessageTicket) -> MessageEnvelope {
        MessageEnvelope {
            sender: self.peer_id,
            receiver: *receiver,
            ticket: 0,
            time: self.time,
            message: Message::Query {
                token: *block,
                target: 0,
                ticket: ticket ^ block, // TODO calc ticket with SHA
            },
        }
    }
}
