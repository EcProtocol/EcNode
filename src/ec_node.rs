use std::cell::RefCell;
use std::rc::Rc;

use crate::ec_interface::{
    Block, BlockId, EcBlocks, EcTime, EcTokens, Message, MessageEnvelope, MessageTicket, PeerId,
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
}

impl EcNode {
    pub fn new(
        tokens: Rc<RefCell<dyn EcTokens>>,
        blocks: Rc<RefCell<dyn EcBlocks>>,
        id: PeerId,
        time: EcTime,
    ) -> Self {
        Self {
            tokens: tokens.clone(),
            blocks: blocks.clone(),
            peers: EcPeers::new(id),
            mem_pool: EcMemPool::new(tokens.clone(), blocks.clone()),
            peer_id: id,
            time,
            block_req_ticket: 2, // TODO shuffle
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

    pub fn tick(&mut self, responses: &mut Vec<MessageEnvelope>, send: bool) {
        self.time += 1;
        // TODO sort requests - and detect CONFLICT (multiple positive vores for same token)
        // could e.g make sure vote msg. for all trxs go to same peers - such that all detect the conflict
        // also work on earlier blocks before later
        for request in self.mem_pool.tick(&self.peers, self.time, self.peer_id) {
            // TODO pack messages in Message2 style
            match request {
                MessageRequest::VOTE(block_id, token_id, vote, reply) => {
                    for peer_id in self.peers.peers_for(&token_id, self.time) {
                        responses.push(MessageEnvelope {
                            sender: self.peer_id,
                            receiver: peer_id,
                            ticket: 0,
                            time: self.time,
                            message: Message::Vote {
                                block_id,
                                vote,
                                reply,
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

                        responses.push(MessageEnvelope {
                            sender: self.peer_id,
                            receiver: peer_id,
                            ticket: 0,
                            time: self.time,
                            message: Message::Query {
                                token: parent_id,
                                target: 0,
                                ticket: block_id, // TODO calc ticket with SHA
                            },
                        })
                    }
                }
                MessageRequest::COMMIT(block_id, peer_id) => {
                    responses.push(self.reply_direct(&peer_id, &block_id, false))
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
                        self.mem_pool.vote(block, *vote, &msg.sender, msg.time);
                        // better ask the sender for it - while propagating towards the "witness"
                        responses.push(self.request_block(&msg.sender, block))
                    }
                    (None, None) => {
                        // TODO test ticket is from subscribed client + DOS protection
                        if msg.ticket > 0 {
                            responses.push(self.request_block(&msg.sender, block))
                        }

                        // TODO push to a queue of potential peers
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
                // TODO also for me ? And forwarding
                if let Some(me) = self.mem_pool.query(token).map(|block| MessageEnvelope {
                    sender: self.peer_id,
                    receiver: if *target == 0 { msg.sender } else { *target },
                    ticket: *ticket,
                    time: self.time,
                    message: Message::Block { block },
                }) {
                    responses.push(me)
                } else {
                    // TODO P(forwarding)
                    // self.peers.peers_for(token, 1)
                }
            }
            Message::Answer { .. } => {}
            Message::Block { block } => {
                // TODO basic common block-validation (like SHA of content match block.id)
                if msg.ticket == self.block_req_ticket ^ block.id {
                    // TODO DOS-protection
                    self.mem_pool.block(block, self.time)
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

    fn request_block(&self, receiver: &PeerId, block: &BlockId) -> MessageEnvelope {
        MessageEnvelope {
            sender: self.peer_id,
            receiver: *receiver,
            ticket: 0,
            time: self.time,
            message: Message::Query {
                token: *block,
                target: 0,
                ticket: self.block_req_ticket ^ block, // TODO calc ticket with SHA
            },
        }
    }
}
