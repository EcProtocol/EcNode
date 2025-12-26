// track the state of transactions

use std::cell::RefCell;
use std::rc::Rc;

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
    Vote(BlockId, TokenId, u8, bool),
    Parent(BlockId, BlockId),
    MissingParent(BlockId),
}

impl MessageRequest {
    pub fn sort_key(&self) -> (TokenId, bool, BlockId) {
        match self {
            // group equal token_id and get possitive votes first
            MessageRequest::Vote(block_id, token_id, _, possitive) => (*token_id, *possitive, *block_id),
            MessageRequest::Parent(_, parent_id) => (*parent_id, false, 0),
            MessageRequest::MissingParent(block_id) => (*block_id, false, 0),
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
            // TODO DOS protection? If all known peers vote for a block?
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

/// Result of evaluating a pending block
/// Contains only blocks that passed reorg checks and can proceed to voting/commit
pub struct BlockEvaluation {
    pub block_id: BlockId,  // Include block_id for future flexibility
    pub block: Block,
    pub vote_mask: u8,  // Pre-calculated vote bits (1 = can verify, 0 = cannot verify)
}

pub struct EcMemPool {
    pool: HashMap<BlockId, PoolBlockState>,
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
    pub fn new() -> Self {
        Self {
            pool: HashMap::new(),
        }
    }

    /// Clean up expired blocks from the pool
    ///
    /// Removes blocks that are too old (haven't committed within the timeout period).
    /// Should be called at the start of each tick before evaluation.
    pub(crate) fn cleanup_expired(&mut self, time: EcTime) {
        // TODO: Make timeout configurable? Currently 200 ticks
        self.pool.retain(|_, state| time - state.time < 200);
    }

    /// Evaluate all pending blocks and determine which can proceed to commit
    ///
    /// This phase does all token lookups with an immutable borrow and:
    /// - Calculates vote masks (positive vote only if we can verify the chain)
    /// - Detects reorgs/missing history and generates PARENTCOMMIT requests
    /// - Filters out blocks that cannot commit this tick
    ///
    /// Returns:
    /// - Vec of blocks that can proceed (no reorg detected)
    /// - Vec of messages for reorg parent requests
    pub(crate) fn evaluate_pending_blocks(
        &self,
        tokens: &dyn EcTokens,
        time: EcTime,
        id: PeerId,
        event_sink: &mut dyn EventSink,
    ) -> (Vec<BlockEvaluation>, Vec<MessageRequest>) {
        let mut evaluations = Vec::new();
        let mut messages = Vec::new();

        for (block_id, block_state) in self.pool.iter().filter(|(_, state)| state.state == Pending) {
            if let Some(block) = block_state.block {
                let mut vote = 0;
                let mut can_commit = true;

                // Check each token in the block
                for i in 0..block.used as usize {
                    let token_id = block.parts[i].token;
                    let last_mapping = block.parts[i].last;
                    let current_mapping = tokens.lookup(&token_id).map_or(0, |t| t.block);

                    if current_mapping == last_mapping {
                        // Chain is correct - we can verify this
                        vote |= 1 << i;
                    } else if current_mapping == 0 {
                        // We don't have this token - cannot verify
                        // vote bit stays 0 (negative vote)
                        // This forces the block to find nodes that CAN verify
                    } else {
                        // We have a different mapping!
                        // Either we're missing history (skip) or client is attempting reorg
                        // Request parent to build our history / detect reorg
                        can_commit = false;

                        messages.push(MessageRequest::MissingParent(last_mapping));

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

                // Only add blocks that can potentially commit this tick
                if can_commit {
                    evaluations.push(BlockEvaluation {
                        block_id: *block_id,
                        block,
                        vote_mask: vote,
                    });
                }
                // Blocks with reorg/missing history stay in mempool but won't be processed this tick
            }
        }

        (evaluations, messages)
    }

    pub(crate) fn status(&self, block: &BlockId, blocks: &dyn EcBlocks) -> Option<BlockState> {
        self.pool
            .get(block)
            .map(|b| b.state.clone())
            // else check if its already committed
            .or_else(|| blocks.exists(block).then_some(BlockState::Commit))
    }

    // TODO "equal share" - make sure some peer does not fill up the pool. Also - limit on pool-size needed?
    pub(crate) fn vote(&mut self, block: &BlockId, vote: u8, msg_sender: &PeerId, time: EcTime) {
        self.pool
            .entry(*block)
            .or_insert_with(|| PoolBlockState::new(time))
            .vote(msg_sender, vote, time);
    }

    pub(crate) fn query(&self, block: &BlockId, blocks: &dyn EcBlocks) -> Option<Block> {
        if *block == 0 {
            return None;
        }
        return self
            .pool
            .get(block)
            .and_then(|b| b.block)
            .or_else(|| blocks.lookup(block));
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

    /// Process evaluated blocks for voting and committing
    ///
    /// This phase only receives blocks that passed reorg checks.
    /// It handles voting logic and commits blocks to the batch when ready.
    pub(crate) fn tick_with_evaluations(
        &mut self,
        peers: &EcPeers,
        time: EcTime,
        id: PeerId,
        event_sink: &mut dyn EventSink,
        evaluations: &[BlockEvaluation],
        batch: &mut dyn crate::ec_interface::StorageBatch,
    ) -> Vec<MessageRequest> {
        let mut messages = Vec::new();
        let my_range = peers.peer_range(&id);

        // Only process blocks that passed evaluation (no reorg detected)
        for evaluation in evaluations {
            let block_id = evaluation.block_id;
            let block_state = self.pool.get_mut(&block_id).unwrap();
            let block = &evaluation.block;
            // Check for Commit if updated
            if block_state.updated {
                let ranges: Vec<PeerRange> = (0..block.used as usize)
                    .map(|i| peers.peer_range(&block.parts[i].token))
                    .collect();
                let mut balance = [0; TOKENS_PER_BLOCK];

                let witness = peers.peer_range(&block_id);
                let mut witness_balance = 0;

                // Test all votes for range and sum up
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

            // Check if ready to commit (all votes collected and validated)
            if block_state.remaining == 0 && block_state.validate == 0 {
                // COMMIT!
                batch.save_block(block);

                // Update tokens in batch (only those in our range)
                for i in 0..block.used as usize {
                    if my_range.in_range(&block.parts[i].token) {
                        event_sink.log(
                            time,
                            id,
                            Event::BlockCommitted {
                                block_id,
                                peer: id,
                                votes: block_state.votes.len(),
                            },
                        );

                        batch.update_token(&block.parts[i].token, &block.id, block.time);
                    }
                }

                block_state.state = BlockState::Commit;
                continue;
            }

            // Not ready to commit - request validation or votes
            for i in 0..block.used as usize {
                if (block_state.validate & 1 << i) != 0 {
                    // Fetch parent for signature validation
                    messages.push(MessageRequest::Parent(block_id, block.parts[i].last));
                }

                if (block_state.remaining & 1 << i) != 0 {
                    // Request vote using pre-calculated vote_mask
                    messages.push(MessageRequest::Vote(
                        block_id,
                        block.parts[i].token,
                        evaluation.vote_mask,
                        evaluation.vote_mask & 1 << i != 0,
                    ));
                }
            }

            // Vote witness
            if (block_state.remaining & 1 << TOKENS_PER_BLOCK) != 0 {
                messages.push(MessageRequest::Vote(
                    block_id,
                    block_id,
                    evaluation.vote_mask,
                    false,
                ));
            }
        }

        return messages;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ec_interface::{BlockTime, EcBlocks, EcTokens, TokenId, TokenSignature};
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

        fn tokens_signature(&self, _token: &TokenId, _key: &PeerId) -> Option<TokenSignature> {
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

        let mut mem_pool = EcMemPool::new();

        // Test that querying a non-existent block returns None
        assert!(mem_pool.query(&block_id, &*blocks.borrow()).is_none());

        // Save the block and test that it can be queried
        blocks.borrow_mut().save(&block);
        assert_eq!(mem_pool.query(&block_id, &*blocks.borrow()), Some(block));

        // Test that querying a block in the mempool also works
        mem_pool.block(&block, 0);
        assert_eq!(mem_pool.query(&block_id, &*blocks.borrow()), Some(block));
    }
}
