// track the state of transactions

use std::cell::RefCell;
use std::io::SeekFrom;
use std::rc::Rc;
use std::thread::sleep;

use hashbrown::HashMap;

use crate::ec_interface::{
    Block, BlockId, EcBlocks, EcTime, EcTokens, Event, EventSink, PeerId, PublicKeyReference,
    Signature, TokenId, SOME_STEPS_INTO_THE_FUTURE, TOKENS_PER_BLOCK, VOTE_THRESHOLD,
};
use crate::ec_mempool::BlockState::Pending;
use crate::ec_peers::{EcPeers, PeerRange};

#[derive(PartialEq, Clone)]
pub enum BlockState {
    Pending,
    Commit,
    Blocked,
}

pub enum MessageRequest {
    VOTE(BlockId, TokenId, u8, bool),
    PARENT(BlockId, BlockId),
    PARENTCOMMIT(BlockId),
}

impl MessageRequest {
    pub fn sort_key(&self) -> (TokenId, bool) {
        match self {
            // group equal token_id and get possitive votes first
            MessageRequest::VOTE(_, token_id, _, possitive) => (*token_id, *possitive),
            MessageRequest::PARENT(_, parent_id) => (*parent_id, false),
            MessageRequest::PARENTCOMMIT(block_id) => (*block_id, false),
        }
    }
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
    validate: u8,
    // for each bit: is the back ref matching
    vote: u8,
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
            validate: 0xFF,
            vote: 0,
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

    fn put_block<F>(&mut self, block: &Block, validate: F) -> bool
    where
        F: Fn() -> (bool, u8, u8),
    {
        if self.block.is_none() {
            let (valid, matching, validated) = validate();

            // TODO if not - a "better" block could show up (correct signature etc) ?
            if valid {
                self.updated = true;
                self.block = Some(*block);
                self.vote = matching;
                self.validate = validated;
            } else {
                // TODO check
                self.state = BlockState::Blocked
            }

            return true;
        }

        return false;
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

fn validate_with_parent(parent: &Block, block: &Block, i: usize) -> bool {
    if parent.time >= block.time {
        // block MUST come after parents
        false
    } else if let Some(sig) = &block.signatures[i] {
        for j in 0..parent.used as usize {
            // find the matching ref
            if parent.parts[j].token == block.parts[i].token {
                if validate_signature(&parent.parts[j].key, sig) {
                    return true;
                }
            }
        }
        false
    } else if block.parts[i].last != 0 {
        // missing signature
        false
    } else {
        true
    }
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
            .map(|b| b.state.clone())
            // else check if its already committed
            .or_else(|| {
                self.blocks
                    .borrow()
                    .exists(block)
                    .then_some(BlockState::Commit)
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

    /// Validates a block in the memory pool against its parent block.
    ///
    /// This function is called when a parent block becomes available, allowing for
    /// validation of pending blocks that were waiting for their parent.
    ///
    /// # Arguments
    ///
    /// * `parent` - The parent block used for validation.
    /// * `block_id` - The ID of the block to be validated.
    ///
    /// # Behavior
    ///
    /// 1. Retrieves the block state from the memory pool.
    /// 2. If the block exists and is in a pending state:
    ///    a. Iterates through the block's parts.
    ///    b. Finds the part that references the parent block.
    ///    c. If that part needs validation, calls `validate_with_parent`.
    ///    d. If validation succeeds, marks that part as validated.
    ///
    /// # Note
    ///
    /// TODO: Implement a check to ensure the block_id is the SHA of the parent ("true parent").
    pub(crate) fn validate_with(&mut self, parent: &Block, block_id: &BlockId) {
        if let Some(state) = self.pool.get_mut(block_id) {
            if let (Some(block), BlockState::Pending) = (&state.block, &state.state) {
                for i in 0..block.used as usize {
                    if block.parts[i].last == parent.id {
                        if state.validate & (1 << i) != 0 && validate_with_parent(parent, block, i)
                        {
                            state.validate ^= 1 << i;
                        }
                    }
                    // can be parent for more than one token
                }
            }
        }
    }

    pub(crate) fn block(&mut self, block: &Block, time: EcTime) -> bool {
        self.pool
            .entry(block.id)
            .or_insert_with(|| PoolBlockState::new(time))
            .put_block(block, || {
                // out-of-bounds or too-far-into-the-future
                if block.used as usize >= TOKENS_PER_BLOCK
                    || block.time > time + SOME_STEPS_INTO_THE_FUTURE
                {
                    return (false, 0, 1);
                }

                // TODO same token only once

                // TODO verify that block-id is the SHA of block content (INCL (?) /not(?) signatures)

                (true, 0, 0)
            })
    }

    pub(crate) fn tick(
        &mut self,
        peers: &EcPeers,
        time: EcTime,
        id: PeerId,
        event_sink: &mut dyn EventSink,
    ) -> Vec<MessageRequest> {
        let mut messages = Vec::new();

        // TODO clean out expired elements - how old?
        self.pool.retain(|_, s| time - s.time < 200);

        let mut tokens = self.tokens.borrow_mut();

        let my_range = peers.peer_range(&id);

        for (block_id, block_state) in self
            .pool
            .iter_mut()
            .filter(|(_, state)| state.state == Pending)
        {
            // only consider blocks we hold. Requesting blocks is left to the node on vote-request for blank states
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
                    for (peer_id, peer_vote) in &block_state.votes {
                        for (i, range) in ranges.iter().enumerate() {
                            if range.in_range(&peer_id) {
                                balance[i] += if peer_vote.vote & 1 << i == 0 { -1 } else { 1 };
                            }
                        }
                        if witness.in_range(&peer_id) {
                            witness_balance += 1;
                        }
                    }

                    block_state.remaining = if witness_balance <= VOTE_THRESHOLD {
                        1 << TOKENS_PER_BLOCK
                    } else {
                        0
                    };

                    for i in 0..ranges.len() {
                        if balance[i] <= VOTE_THRESHOLD && balance[i] >= -VOTE_THRESHOLD {
                            block_state.remaining |= 1 << i
                        }
                    }

                    block_state.updated = false;
                }

                let mut vote = 0;
                let mut no_skip_or_reorg = true;
                for i in 0..block.used as usize {
                    let current_mapping =
                        tokens.lookup(&block.parts[i].token).map_or(0, |t| t.block);
                    let last_mapping = block.parts[i].last;

                    if current_mapping == last_mapping {
                        vote |= 1 << i
                    } else if current_mapping != 0 {
                        // Never allow missing links or re-orgs on committed tokens
                        no_skip_or_reorg = false;

                        // request predessor block
                        messages.push(MessageRequest::PARENTCOMMIT(last_mapping));

                        event_sink.log(
                            time,
                            id,
                            Event::Reorg {
                                block_id: *block_id,
                                peer: id,
                                from: current_mapping,
                                to: last_mapping,
                            },
                        );
                    }
                }

                // if no remaining votes AND all is validated - commit
                if no_skip_or_reorg && block_state.remaining == 0 && block_state.validate == 0 {
                    // TODO should it be kept in mempool for a while - can it be rolled back ever? -> NO

                    // TODO should the commit-chain be the responsiblity of token-store - or a seperate manager?

                    // save block in permanent store
                    self.blocks.borrow_mut().save(&block);

                    // commit / update
                    for i in 0..block.used as usize {
                        // TODO (ok?) only tokens in my range -> together with "no_skip_or_reorg" lock
                        if my_range.in_range(&block.parts[i].token) {
                                event_sink.log(
                                    time,
                                    id,
                                    Event::BlockCommitted {
                                        block_id: *block_id,
                                        peer: id,
                                        votes: block_state.votes.len(),
                                    },
                                );

                            tokens.set(&block.parts[i].token, &block.id, block.time);
                        }
                    }

                    block_state.state = BlockState::Commit;
                    continue;
                }

                if no_skip_or_reorg
                {
                    for i in 0..block.used as usize {
                        if (block_state.validate & 1 << i) != 0 {
                            // fetch a parent
                            messages.push(MessageRequest::PARENT(*block_id, block.parts[i].last));
                        }

                        if (block_state.remaining & 1 << i) != 0 {
                            // request vote
                            messages.push(MessageRequest::VOTE(
                                *block_id,
                                block.parts[i].token,
                                vote,
                                vote & 1 << i != 0,
                            ));
                        }
                    }

                    // vote witness
                    if (block_state.remaining & 1 << TOKENS_PER_BLOCK) != 0 {
                        messages.push(MessageRequest::VOTE(*block_id, *block_id, vote, false));
                    }
                }
            }
        }

        return messages;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ec_interface::{BlockTime, EcBlocks, EcTokens, Message, TokenId};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    struct MockEcBlocks {
        blocks: HashMap<BlockId, Block>,
    }

    impl EcBlocks for MockEcBlocks {
        fn lookup(&self, block: &BlockId) -> Option<Block> {
            self.blocks.get(block).cloned()
        }

        fn save(&mut self, block: &Block) {
            self.blocks.insert(block.id, *block);
        }

        fn exists(&self, block: &BlockId) -> bool {
            self.blocks.contains_key(block)
        }
    }

    struct MockEcTokens {
        tokens: HashMap<TokenId, BlockTime>,
    }

    impl EcTokens for MockEcTokens {
        fn lookup(&self, token: &TokenId) -> Option<&BlockTime> {
            self.tokens.get(token)
        }

        fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
            self.tokens.insert(
                *token,
                BlockTime {
                    block: *block,
                    time,
                },
            );
        }

        fn tokens_signature(&self, _token: &TokenId, _key: &PeerId) -> Option<Message> {
            // Not needed for this test
            None
        }
    }

    #[test]
    fn test_query() {
        let block_id = 1;
        let block = Block {
            id: block_id,
            time: 0,
            used: 0,
            parts: [Default::default(); TOKENS_PER_BLOCK],
            signatures: [None; TOKENS_PER_BLOCK],
        };

        let blocks = Rc::new(RefCell::new(MockEcBlocks {
            blocks: HashMap::new(),
        }));

        let tokens = Rc::new(RefCell::new(MockEcTokens {
            tokens: HashMap::new(),
        }));

        let mut mem_pool = EcMemPool::new(tokens.clone(), blocks.clone());

        // Test that querying a non-existent block returns None
        assert!(mem_pool.query(&block_id).is_none());

        // Save the block and test that it can be queried
        blocks.borrow_mut().save(&block);
        assert_eq!(mem_pool.query(&block_id), Some(block));

        // Test that querying a block in the mempool also works
        mem_pool.block(&block, 0);
        assert_eq!(mem_pool.query(&block_id), Some(block));
    }
}
