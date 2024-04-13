// track the state of transactions

use std::cell::RefCell;
use std::rc::Rc;

use hashbrown::HashMap;

use crate::ec_interface::{
    Block, BlockId, EcBlocks, EcTime, EcTokens, PeerId, PublicKeyReference, Signature,
    SOME_STEPS_INTO_THE_FUTURE, TOKENS_PER_BLOCK,
};
use crate::ec_mempool::BlockState::Pending;
use crate::ec_peers::{EcPeers, PeerRange};

#[derive(PartialEq)]
pub enum BlockState {
    Pending,
    Commit,
    Blocked,
}

struct PoolVote {
    time: EcTime,
    vote: u8,
}

struct PoolBlockState {
    votes: HashMap<PeerId, PoolVote>,
    state: BlockState,
    block: Option<Block>,
    time: EcTime,
    updated: bool,
    // for each bit: was the token ref validated
    validated: u8,
    // for each bit: is the back ref matching
    matching: u8,
    // for each bit: are we done collecting votes?
    remaining: u8,
}

impl PoolBlockState {
    fn new(time: EcTime) -> Self {
        Self {
            votes: HashMap::new(),
            state: Pending,
            block: None,
            time,
            updated: false,
            validated: 0,
            matching: 0,
            remaining: 0,
        }
    }

    fn vote(&mut self, peer: &PeerId, vote: u8, time: EcTime) {
        let pv = self
            .votes
            .entry(*peer)
            // only update if its newer
            .and_modify(|v| {
                if v.time < time {
                    v.vote = vote;
                    v.time = time;
                }
            })
            .or_insert_with(|| PoolVote { time, vote });

        // did we change anything ? then it might be time to check the votes again
        if pv.time == time {
            self.updated = true;
        }
    }

    fn put_block<F>(&mut self, block: &Block, validate: F)
    where
        F: Fn() -> (bool, u8, u8),
    {
        if self.block.is_none() {
            let (valid, matching, validated) = validate();

            // if not - a "better" block could show up (correct signature etc)
            if valid {
                self.updated = true;
                self.block = Some(*block);
                self.matching = matching;
                self.validated = validated;
            } else {
                // TODO check
                self.state = BlockState::Blocked
            }
        }
    }
}

pub struct EcMemPool {
    pool: HashMap<BlockId, PoolBlockState>,

    tokens: Rc<RefCell<dyn EcTokens>>,
    blocks: Rc<RefCell<dyn EcBlocks>>,
}

fn validate_signature(key: &PublicKeyReference, signature: &Signature) -> bool {
    // TODO real validation of signature <-> public-key-hash
    key == signature
}

fn valid_child(parent: &Block, block: &Block, i: usize) -> bool {
    let mut valid = true;
    if parent.time >= block.time {
        // block MUST come after parents
        valid = false
    } else if let Some(sig) = &block.signatures[i] {
        valid = false;
        for j in 0..parent.used as usize {
            // find the matching ref
            if parent.parts[j].token == block.parts[i].token {
                if validate_signature(&parent.parts[j].key, sig) {
                    valid = true;
                    break;
                }
            }
        }
    } else if block.parts[i].last != 0 {
        // missing signature
        valid = false
    }

    return valid;
}

impl EcMemPool {
    pub fn new(tokens: Rc<RefCell<dyn EcTokens>>, blocks: Rc<RefCell<dyn EcBlocks>>) -> Self {
        Self {
            pool: HashMap::new(),
            tokens,
            blocks,
        }
    }

    pub(crate) fn status(&self, block: &BlockId) -> Option<BlockState> {
        self.pool
            .get(block)
            .map(|b| b.state)
            // else check if its already committed
            .or_else(|| {
                self.blocks
                    .borrow()
                    .lookup(block)
                    .map_or(None, |_| Some(BlockState::Commit))
            })
    }

    pub(crate) fn vote(&mut self, block: &BlockId, vote: u8, msg_sender: &PeerId, time: EcTime) {
        // TODO allow override if already voted
        // DOS protect - do not allow unbounded votes
        if self.pool.len() < 100 {
            self.pool
                .entry(*block)
                .or_insert_with(|| PoolBlockState::new(time))
                .vote(msg_sender, vote, time);
        }
    }

    pub(crate) fn query(&self, block: &BlockId) -> Option<Block> {
        if *block == 0 {
            return None;
        }
        return self
            .pool
            .get(block)
            .and_then(|b| b.block)
            .or_else(|| self.blocks.borrow().lookup(block));
    }
    
    pub(crate) fn validate_with(&mut self, parent: &Block, block_id: &BlockId) {
        if let Some(state) = self.pool.get_mut(block_id) {
            if let (Some(block), Pending) = (state.block, state.state) {
                for i in 0..block.used as usize {
                    if block.parts[i].last == parent.id {
                        valid_child(parent, &block, i);
                    }
                }
            }
        }
    }

    pub(crate) fn block(&mut self, block: &Block, time: EcTime) {
        self.pool
            .entry(block.id)
            .or_insert_with(|| PoolBlockState::new(time))
            .put_block(block, || {
                let mut vote: u8 = 0;
                let mut validated: u8 = 0;

                // out-of-bounds or too-far-into-the-future
                if block.used as usize >= TOKENS_PER_BLOCK
                    || block.time > time + SOME_STEPS_INTO_THE_FUTURE
                {
                    return (false, 0, 0);
                }

                // TODO same token only once

                // TODO verify that block-id is the SHA of block content (INCL (?) /not(?) signatures)

                let tokens = self.tokens.borrow();
                for i in 0..block.used as usize {
                    if let Some(parent) = self.query(&block.parts[i].last) {
                        if valid_child(&parent, block, i) {
                            validated |= 1 << i;
                        } else {
                            // break now - this block is invalid
                            return (false, 0, 0);
                        }
                    }

                    if tokens.lookup(&block.parts[i].token).map_or(0, |t| t.block)
                        == block.parts[i].last
                    {
                        vote |= 1 << i
                    }
                }

                (true, vote, validated)
            });
    }

    pub(crate) fn tick(
        &mut self,
        peers: &EcPeers,
        time: EcTime,
    ) -> Vec<(usize, BlockId, [bool; TOKENS_PER_BLOCK])> {
        let mut requests = Vec::new();

        // TODO clean out expired elements - how old?
        self.pool.retain(|_, s| time - s.time < 20);

        let mut tokens = self.tokens.borrow_mut();

        for (block_id, block_state) in self
            .pool
            .iter_mut()
            .filter(|(_, state)| state.state == Pending)
        {
            if let Some(block) = block_state.block {
                // only check for Commit if updated
                if block_state.updated {
                    let ranges: Vec<PeerRange> = (0..block.used as usize)
                        // balance voting on all tokens
                        .map(|i| peers.peer_range(&block.parts[i].token))
                        .collect();
                    let mut balance = [0; TOKENS_PER_BLOCK];

                    let witness = peers.peer_range(block_id);
                    let mut witness_balance = 0;

                    // test all votes for range and sum up
                    for (peer_id, peer_vote) in block_state.votes {
                        let mut effect = false;

                        for (i, range) in ranges.iter().enumerate() {
                            if range.in_range(&peer_id) {
                                balance[i] += if peer_vote.vote & 1 << i == 0 { -1 } else { 1 };
                                effect = true
                            }
                        }
                        if witness.in_range(&peer_id) {
                            witness_balance += 1;
                            effect = true
                        }

                        if !effect {
                            // TODO will probably not work ... but the idea
                            block_state.votes.remove(&peer_id);
                        }
                    }

                    block_state.remaining = if witness_balance <= 2 {
                        1 << TOKENS_PER_BLOCK
                    } else {
                        0
                    };

                    for i in 0..ranges.len() {
                        if balance[i] <= 2 && balance[i] >= -2 {
                            block_state.remaining |= 1 << i
                        }
                    }

                    // if no remaining votes - commit
                    if block_state.remaining == 0 {
                        // TODO should it be kept in mempool for a while - can it be rolled back ever?

                        // commit / update
                        for i in 0..block.used as usize {
                            // TODO announce for next iteration to update vote
                            tokens.set(&block.parts[i].token, &block.id, block.time);
                        }

                        // save block in permanent store
                        self.blocks.borrow_mut().save(&block);

                        block_state.state = BlockState::Commit
                    }

                    block_state.updated = false
                }

                if block_state.state == Pending {
                    for i in 0..block.used as usize {
                        if (block_state.validated & 1 << i) == 0 {
                            // fetch
                            block.parts[i].last;
                        }

                        if (block_state.remaining & 1 << i) != 0 {
                            // request vote
                            block.parts[i].token;
                        }
                    }
                }
            } else {
                // fetch
                block_id;
            }
        }

        return requests;
    }
}
