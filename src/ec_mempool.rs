// track the state of transactions
use hashbrown::HashMap;
use std::cmp::Reverse;

use crate::ec_interface::{
    Block, BlockId, EcBlocks, EcTime, EcTokensV2, Event, EventSink, PeerId, PublicKeyReference,
    Signature, TokenId, SOME_STEPS_INTO_THE_FUTURE, TOKENS_PER_BLOCK, VOTE_THRESHOLD,
};
use crate::ec_mempool::BlockState::Pending;
use crate::ec_peers::{EcPeers, PeerRange};

#[derive(PartialEq, Clone, Debug)]
pub enum BlockState {
    Pending,
    Commit,
    Blocked,
}

pub enum MessageRequest {
    Vote(BlockId, TokenId, u8, bool, u8),
    Parent(BlockId, BlockId),
    MissingParent(BlockId),
}

const PAUSED_VOTE_SEQUENCE: u8 = u8::MAX;

impl MessageRequest {
    pub fn sort_key(&self) -> (TokenId, Reverse<BlockId>, Reverse<bool>) {
        match self {
            // Group equal token_id together, prefer highest block_id first, and for equal block_id
            // let positive votes win the tie. This keeps conflicting token updates deterministic.
            MessageRequest::Vote(block_id, token_id, _, positive, _) => {
                (*token_id, Reverse(*block_id), Reverse(*positive))
            }
            MessageRequest::Parent(_, parent_id) => (*parent_id, Reverse(0), Reverse(false)),
            MessageRequest::MissingParent(block_id) => (*block_id, Reverse(0), Reverse(false)),
        }
    }
}

struct PoolVote {
    time: EcTime,
    vote: u8,
    reply: bool,
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
    competing_block: Option<BlockId>,
    vote_sequence: [u8; TOKENS_PER_BLOCK + 1],
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
            competing_block: None,
            vote_sequence: [0; TOKENS_PER_BLOCK + 1],
        }
    }

    fn vote(&mut self, peer: &PeerId, vote: u8, time: EcTime, reply: bool) {
        let pv = self
            .votes
            .entry(*peer)
            // only update if its newer
            .and_modify(|v| {
                if v.time < time {
                    v.vote = vote;
                    v.time = time;
                    v.reply = reply;
                } else if v.time == time {
                    v.reply |= reply;
                }
            })
            // TODO DOS protection? If all known peers vote for a block?
            .or_insert_with(|| PoolVote { time, vote, reply });

        // did we change anything ? then it might be time to check the votes again
        if pv.time == time {
            pv.reply |= reply;
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

pub struct BlockedConflictTransition {
    pub blocked_block_id: BlockId,
    pub higher_block_id: BlockId,
    pub interested_voters: Vec<PeerId>,
}

pub struct CommitTransition {
    pub committed_block_id: BlockId,
    pub competing_block_id: Option<BlockId>,
    pub interested_voters: Vec<PeerId>,
}

pub struct EcMemPool {
    pool: HashMap<BlockId, PoolBlockState>,
    vote_balance_threshold: i64,
    vote_request_active_rounds: u8,
}

#[derive(Debug, Clone, Default)]
pub struct MempoolDiagnostics {
    pub total_entries: usize,
    pub pending_entries: usize,
    pub committed_entries: usize,
    pub blocked_entries: usize,
    pub pending_without_block: usize,
    pub pending_no_trusted_votes: usize,
    pub pending_with_trusted_votes: usize,
    pub pending_waiting_validation: usize,
    pub pending_waiting_token_votes: usize,
    pub pending_waiting_witness: usize,
    pub pending_age_50_plus: usize,
    pub pending_age_200_plus: usize,
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
    fn sorted_interested_voters(state: &PoolBlockState) -> Vec<PeerId> {
        let mut voters: Vec<PeerId> = state
            .votes
            .iter()
            .filter_map(|(peer_id, vote)| vote.reply.then_some(*peer_id))
            .collect();
        voters.sort_unstable();
        voters
    }

    fn sorted_block_ids<F>(&self, mut predicate: F) -> Vec<BlockId>
    where
        F: FnMut(&PoolBlockState) -> bool,
    {
        let mut block_ids: Vec<BlockId> = self
            .pool
            .iter()
            .filter_map(|(block_id, state)| predicate(state).then_some(*block_id))
            .collect();
        block_ids.sort_unstable();
        block_ids
    }

    pub fn new() -> Self {
        Self::with_vote_balance_threshold(VOTE_THRESHOLD)
    }

    pub fn with_vote_balance_threshold(vote_balance_threshold: i64) -> Self {
        Self::with_vote_policy(vote_balance_threshold, 0, 4)
    }

    pub fn with_vote_policy(
        vote_balance_threshold: i64,
        _vote_request_resend_cooldown: EcTime,
        vote_request_active_rounds: u8,
    ) -> Self {
        Self {
            pool: HashMap::new(),
            vote_balance_threshold,
            vote_request_active_rounds: vote_request_active_rounds.max(1),
        }
    }

    pub fn diagnostics(&self, time: EcTime) -> MempoolDiagnostics {
        let mut diagnostics = MempoolDiagnostics {
            total_entries: self.pool.len(),
            ..MempoolDiagnostics::default()
        };

        for state in self.pool.values() {
            match state.state {
                BlockState::Pending => {
                    diagnostics.pending_entries += 1;

                    if state.block.is_none() {
                        diagnostics.pending_without_block += 1;
                    }

                    if state.votes.is_empty() {
                        diagnostics.pending_no_trusted_votes += 1;
                    } else {
                        diagnostics.pending_with_trusted_votes += 1;
                    }

                    if state.validate != 0 {
                        diagnostics.pending_waiting_validation += 1;
                    }

                    if state.remaining & ((1 << TOKENS_PER_BLOCK) - 1) != 0 {
                        diagnostics.pending_waiting_token_votes += 1;
                    }

                    if state.remaining & (1 << TOKENS_PER_BLOCK) != 0 {
                        diagnostics.pending_waiting_witness += 1;
                    }

                    let age = time.saturating_sub(state.time);
                    if age >= 50 {
                        diagnostics.pending_age_50_plus += 1;
                    }
                    if age >= 200 {
                        diagnostics.pending_age_200_plus += 1;
                    }
                }
                BlockState::Commit => diagnostics.committed_entries += 1,
                BlockState::Blocked => diagnostics.blocked_entries += 1,
            }
        }

        diagnostics
    }

    /// Clean up expired blocks from the pool
    ///
    /// Removes blocks that are too old (haven't committed within the timeout period).
    /// Should be called at the start of each tick before evaluation.
    pub(crate) fn cleanup_expired(&mut self, time: EcTime) {
        // TODO: Make timeout configurable? Currently 200 ticks
        self.pool
            .retain(|_, state| time.saturating_sub(state.time) < 200);
    }

    /// Persistently blocks lower direct conflicts before any commit decisions are made.
    ///
    /// A direct conflict is another known block that updates the same token from the same
    /// parent mapping. If we know a higher block ID for any part of a pending block, the
    /// lower block must never commit locally.
    pub(crate) fn block_known_higher_conflicts(&mut self) -> Vec<BlockedConflictTransition> {
        let mut highest_by_conflict = HashMap::new();
        let mut competing_by_block: HashMap<BlockId, BlockId> = HashMap::new();
        let mut transitions = Vec::new();
        let mut family_members: HashMap<(TokenId, BlockId), Vec<BlockId>> = HashMap::new();
        let block_ids_with_blocks = self.sorted_block_ids(|state| state.block.is_some());

        for block_id in &block_ids_with_blocks {
            let Some(state) = self.pool.get(block_id) else {
                continue;
            };
            let Some(block) = state.block else {
                continue;
            };

            for i in 0..block.used as usize {
                let key = (block.parts[i].token, block.parts[i].last);
                family_members.entry(key).or_default().push(*block_id);
                if state.state == BlockState::Blocked {
                    continue;
                }
                highest_by_conflict
                    .entry(key)
                    .and_modify(|highest: &mut BlockId| {
                        if *highest < *block_id {
                            *highest = *block_id;
                        }
                    })
                    .or_insert(*block_id);
            }
        }

        for members in family_members.values() {
            if members.len() < 2 {
                continue;
            }
            let mut sorted_members = members.clone();
            sorted_members.sort_unstable();
            for (idx, block_id) in sorted_members.iter().enumerate() {
                let competing = if idx + 1 < sorted_members.len() {
                    Some(sorted_members[idx + 1])
                } else if idx > 0 {
                    Some(sorted_members[idx - 1])
                } else {
                    None
                };
                if let Some(competing) = competing {
                    competing_by_block
                        .entry(*block_id)
                        .and_modify(|current| {
                            if *current < competing {
                                *current = competing;
                            }
                        })
                        .or_insert(competing);
                }
            }
        }

        for block_id in self.sorted_block_ids(|_| true) {
            if let Some(state) = self.pool.get_mut(&block_id) {
                state.competing_block = competing_by_block.get(&block_id).copied();
            }
        }

        for block_id in self.sorted_block_ids(|state| state.state == BlockState::Pending) {
            let Some(state) = self.pool.get_mut(&block_id) else {
                continue;
            };
            let Some(block) = state.block else {
                continue;
            };

            let higher_conflict = (0..block.used as usize)
                .filter_map(|i| {
                    let key = (block.parts[i].token, block.parts[i].last);
                    highest_by_conflict.get(&key).copied()
                })
                .filter(|highest| *highest > block_id)
                .max();

            if let Some(higher_block_id) = higher_conflict {
                state.state = BlockState::Blocked;
                state.updated = false;
                transitions.push(BlockedConflictTransition {
                    blocked_block_id: block_id,
                    higher_block_id,
                    interested_voters: Self::sorted_interested_voters(state),
                });
            }
        }

        transitions
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
        tokens: &dyn EcTokensV2,
        time: EcTime,
        id: PeerId,
        event_sink: &mut dyn EventSink,
    ) -> (Vec<BlockEvaluation>, Vec<MessageRequest>) {
        let mut evaluations = Vec::new();
        let mut messages = Vec::new();
        for block_id in self.sorted_block_ids(|state| state.state == Pending) {
            let Some(block_state) = self.pool.get(&block_id) else {
                continue;
            };
            if let Some(block) = block_state.block {
                let mut vote = 0;
                let mut can_commit = true;

                // Check each token in the block
                for i in 0..block.used as usize {
                    let token_id = block.parts[i].token;
                    let last_mapping = block.parts[i].last;
                    let current_mapping = tokens.lookup_current(&token_id).map_or(0, |t| t.block);

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
                                block_id,
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
                        block_id,
                        block,
                        vote_mask: vote,
                    });
                }
                // Blocks with reorg/missing history stay in mempool but won't be processed this tick
            }
        }

        (evaluations, messages)
    }

    pub(crate) fn evaluate_pending_block(
        &self,
        block_id: &BlockId,
        tokens: &dyn EcTokensV2,
        time: EcTime,
        id: PeerId,
        event_sink: &mut dyn EventSink,
    ) -> (Option<BlockEvaluation>, Vec<MessageRequest>) {
        let mut messages = Vec::new();
        let Some(block_state) = self.pool.get(block_id).filter(|state| state.state == Pending) else {
            return (None, messages);
        };
        let Some(block) = block_state.block else {
            return (None, messages);
        };

        let mut vote = 0;
        let mut can_commit = true;

        for i in 0..block.used as usize {
            let token_id = block.parts[i].token;
            let last_mapping = block.parts[i].last;
            let current_mapping = tokens.lookup_current(&token_id).map_or(0, |t| t.block);

            if current_mapping == last_mapping {
                vote |= 1 << i;
            } else if current_mapping == 0 {
                // We do not hold this token locally; the role must find a host.
            } else {
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

        if can_commit {
            (
                Some(BlockEvaluation {
                    block_id: *block_id,
                    block,
                    vote_mask: vote,
                }),
                messages,
            )
        } else {
            (None, messages)
        }
    }

    pub(crate) fn status(&self, block: &BlockId, blocks: &dyn EcBlocks) -> Option<BlockState> {
        self.pool
            .get(block)
            .map(|b| b.state.clone())
            // else check if its already committed
            .or_else(|| blocks.exists(block).then_some(BlockState::Commit))
    }

    // TODO "equal share" - make sure some peer does not fill up the pool. Also - limit on pool-size needed?
    pub(crate) fn vote(
        &mut self,
        block: &BlockId,
        vote: u8,
        msg_sender: &PeerId,
        time: EcTime,
        reply: bool,
    ) {
        self.pool
            .entry(*block)
            .or_insert_with(|| PoolBlockState::new(time))
            .vote(msg_sender, vote, time, reply);
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

    pub(crate) fn interested_voters(&self, block: &BlockId) -> Vec<PeerId> {
        self.pool
            .get(block)
            .map(Self::sorted_interested_voters)
            .unwrap_or_default()
    }

    pub(crate) fn competing_block_for(&self, block: &BlockId) -> Option<BlockId> {
        self.pool.get(block).and_then(|state| state.competing_block)
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

                // TODO verify that block-id is the SHA of block content INCL signatures)

                (true, 0, 0)
            })
    }

    fn refresh_pending_vote_state(
        block_state: &mut PoolBlockState,
        block: &Block,
        peers: &EcPeers,
        vote_balance_threshold: i64,
    ) {
        let previous_remaining = block_state.remaining;
        let ranges: Vec<PeerRange> = (0..block.used as usize)
            .map(|i| peers.peer_range(&block.parts[i].token))
            .collect();
        let mut balance = [0; TOKENS_PER_BLOCK];

        let witness = peers.peer_range(&block.id);
        let mut witness_balance = 0;

        for (peer_id, peer_vote) in &block_state.votes {
            for (i, range) in ranges.iter().enumerate() {
                if range.in_range(peer_id) {
                    balance[i] += if peer_vote.vote & 1 << i == 0 { -1 } else { 1 };
                }
            }
            if witness.in_range(peer_id) {
                witness_balance += 1;
            }
        }

        block_state.remaining = if witness_balance <= vote_balance_threshold {
            1 << TOKENS_PER_BLOCK
        } else {
            0
        };

        for i in 0..ranges.len() {
            if balance[i] > vote_balance_threshold {
                continue;
            }

            block_state.remaining |= 1 << i;

            if balance[i] < -vote_balance_threshold {
                block_state.vote_sequence[i] = PAUSED_VOTE_SEQUENCE;
            } else if block_state.vote_sequence[i] == PAUSED_VOTE_SEQUENCE {
                block_state.vote_sequence[i] = 0;
            }
        }

        for bit in 0..=TOKENS_PER_BLOCK {
            let bit_mask = 1 << bit;
            if (block_state.remaining & bit_mask) == 0 {
                block_state.vote_sequence[bit] = 0;
            } else if (previous_remaining & bit_mask) == 0 {
                if block_state.vote_sequence[bit] != PAUSED_VOTE_SEQUENCE {
                    block_state.vote_sequence[bit] = 0;
                }
            }
        }

        block_state.updated = false;
    }

    pub(crate) fn seed_reactive_vote_requests(
        &mut self,
        peers: &EcPeers,
        evaluation: &BlockEvaluation,
        initial_sequence_span: u8,
    ) -> Vec<TokenId> {
        if initial_sequence_span == 0 {
            return Vec::new();
        }

        let Some(block_state) = self.pool.get_mut(&evaluation.block_id) else {
            return Vec::new();
        };
        if block_state.state != BlockState::Pending {
            return Vec::new();
        }

        if block_state.updated {
            Self::refresh_pending_vote_state(
                block_state,
                &evaluation.block,
                peers,
                self.vote_balance_threshold,
            );
        }

        let seed_sequence = initial_sequence_span.min(self.vote_request_active_rounds);
        if seed_sequence == 0 {
            return Vec::new();
        }

        let mut targets = Vec::new();

        for i in 0..evaluation.block.used as usize {
            let bit_mask = 1 << i;
            if (block_state.remaining & bit_mask) == 0 {
                block_state.vote_sequence[i] = 0;
                continue;
            }
            if block_state.vote_sequence[i] == PAUSED_VOTE_SEQUENCE || block_state.vote_sequence[i] != 0 {
                continue;
            }

            block_state.vote_sequence[i] = seed_sequence;
            targets.push(evaluation.block.parts[i].token);
        }

        let witness_idx = TOKENS_PER_BLOCK;
        let witness_mask = 1 << witness_idx;
        if (block_state.remaining & witness_mask) != 0
            && block_state.vote_sequence[witness_idx] != PAUSED_VOTE_SEQUENCE
            && block_state.vote_sequence[witness_idx] == 0
        {
            block_state.vote_sequence[witness_idx] = seed_sequence;
            targets.push(evaluation.block_id);
        }

        targets
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
    ) -> (Vec<MessageRequest>, Vec<CommitTransition>) {
        let mut messages = Vec::new();
        let mut commits = Vec::new();
        let my_range = peers.peer_range(&id);

        // Only process blocks that passed evaluation (no reorg detected)
        for evaluation in evaluations {
            let block_id = evaluation.block_id;
            let block_state = self.pool.get_mut(&block_id).unwrap();
            let block = &evaluation.block;
            // Check for Commit if updated
            if block_state.updated {
                Self::refresh_pending_vote_state(
                    block_state,
                    block,
                    peers,
                    self.vote_balance_threshold,
                );
            }

            // Check if ready to commit (all votes collected and validated)
            if block_state.remaining == 0 && block_state.validate == 0 {
                commits.push(CommitTransition {
                    committed_block_id: block_id,
                    competing_block_id: block_state.competing_block,
                    interested_voters: Self::sorted_interested_voters(block_state),
                });
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

                        // Update token with parent (block.parts[i].last is the parent block ID)
                        batch.update_token(&block.parts[i].token, &block.id, &block.parts[i].last, block.time);
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
                    let sequence = block_state.vote_sequence[i];
                    if sequence == PAUSED_VOTE_SEQUENCE {
                        continue;
                    }
                    if sequence >= self.vote_request_active_rounds {
                        block_state.vote_sequence[i] = 0;
                        continue;
                    }
                    block_state.vote_sequence[i] = sequence + 1;
                    // Request vote using pre-calculated vote_mask
                    messages.push(MessageRequest::Vote(
                        block_id,
                        block.parts[i].token,
                        evaluation.vote_mask,
                        evaluation.vote_mask & 1 << i != 0,
                        sequence,
                    ));
                }
            }

            // Vote witness
            if (block_state.remaining & 1 << TOKENS_PER_BLOCK) != 0 {
                let witness_idx = TOKENS_PER_BLOCK;
                let sequence = block_state.vote_sequence[witness_idx];
                if sequence >= self.vote_request_active_rounds {
                    block_state.vote_sequence[witness_idx] = 0;
                    continue;
                }
                block_state.vote_sequence[witness_idx] = sequence + 1;
                messages.push(MessageRequest::Vote(
                    block_id,
                    block_id,
                    evaluation.vote_mask,
                    false,
                    sequence,
                ));
            }
        }

        (messages, commits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::ec_interface::{
        EcTokens, EcTokensV2, NoOpSink, StorageBatch, TokenBlock, TokenState, TrustedMapping,
        TrustSource,
    };
    use crate::ec_peers::EcPeers;
    use rand::SeedableRng;

    struct MockEcBlocks {
        blocks: hashbrown::HashMap<BlockId, Block>,
    }

    #[derive(Default)]
    struct MockTokens {
        tokens: hashbrown::HashMap<TokenId, TokenState>,
    }

    impl EcTokensV2 for MockTokens {
        fn lookup_state(&self, token: &TokenId) -> Option<TokenState> {
            self.tokens.get(token).cloned()
        }

        fn lookup_current(&self, token: &TokenId) -> Option<TrustedMapping> {
            self.tokens.get(token).and_then(|state| state.current)
        }

        fn is_local(&self, token: &TokenId) -> bool {
            self.tokens
                .get(token)
                .is_some_and(|state| state.is_local())
        }
    }

    impl EcTokens for MockTokens {
        fn lookup(&self, _token: &TokenId) -> Option<&crate::ec_interface::BlockTime> {
            unimplemented!("legacy EcTokens lookup is not used in mempool tests")
        }

        fn set(&mut self, _token: &TokenId, _block: &BlockId, _parent: &BlockId, _time: EcTime) {
            unimplemented!("legacy EcTokens set is not used in mempool tests")
        }

        fn tokens_signature(
            &self,
            _token: &TokenId,
            _peer: &PeerId,
        ) -> Option<crate::ec_interface::TokenSignature> {
            None
        }
    }

    #[derive(Default)]
    struct TestBatch {
        saved_blocks: Vec<BlockId>,
        updated_tokens: Vec<(TokenId, BlockId, BlockId, EcTime)>,
    }

    impl StorageBatch for TestBatch {
        fn save_block(&mut self, block: &Block) {
            self.saved_blocks.push(block.id);
        }

        fn update_token(&mut self, token: &TokenId, block: &BlockId, parent: &BlockId, time: EcTime) {
            self.updated_tokens.push((*token, *block, *parent, time));
        }

        fn update_token_sync(
            &mut self,
            _token: &TokenId,
            _block: &BlockId,
            _parent: &BlockId,
            _time: EcTime,
            _source_peer: PeerId,
        ) {
            panic!("sync updates are not used in mempool tests");
        }

        fn commit(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        fn block_count(&self) -> usize {
            self.saved_blocks.len()
        }
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
            blocks: hashbrown::HashMap::new(),
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

    #[test]
    fn vote_requests_sort_highest_block_id_first_within_token() {
        let mut requests = vec![
            MessageRequest::Vote(10, 77, 0b0000_0001, true, 0),
            MessageRequest::Vote(30, 77, 0b0000_0001, true, 0),
            MessageRequest::Vote(20, 77, 0b0000_0001, true, 0),
            MessageRequest::Vote(15, 66, 0b0000_0001, true, 0),
        ];

        requests.sort_unstable_by_key(MessageRequest::sort_key);

        let ordered: Vec<(BlockId, TokenId)> = requests
            .into_iter()
            .map(|request| match request {
                MessageRequest::Vote(block_id, token_id, _, _, _) => (block_id, token_id),
                other => panic!("unexpected request in sort test: {:?}", std::mem::discriminant(&other)),
            })
            .collect();

        assert_eq!(ordered, vec![(15, 66), (30, 77), (20, 77), (10, 77)]);
    }

    #[test]
    fn higher_direct_conflict_blocks_lower_before_commit() {
        let token = 42;
        let parent = 7;
        let lower_block = Block {
            id: 100,
            time: 10,
            used: 1,
            parts: [TokenBlock {
                token,
                last: parent,
                key: 1,
            }, Default::default(), Default::default(), Default::default(), Default::default(), Default::default()],
            signatures: [None; TOKENS_PER_BLOCK],
        };
        let higher_block = Block {
            id: 200,
            time: 10,
            used: 1,
            parts: [TokenBlock {
                token,
                last: parent,
                key: 2,
            }, Default::default(), Default::default(), Default::default(), Default::default(), Default::default()],
            signatures: [None; TOKENS_PER_BLOCK],
        };

        let mut mem_pool = EcMemPool::new();
        assert!(mem_pool.block(&lower_block, 10));
        assert!(mem_pool.block(&higher_block, 10));

        for peer_id in [11, 12, 13] {
            mem_pool.vote(&lower_block.id, 0b0000_0001, &peer_id, 10, true);
            mem_pool.vote(&higher_block.id, 0b0000_0001, &peer_id, 10, true);
        }

        mem_pool.block_known_higher_conflicts();

        let mut tokens = MockTokens::default();
        tokens.tokens.insert(
            token,
            TokenState {
                current: Some(TrustedMapping {
                    block: parent,
                    parent: 0,
                    time: 0,
                    source: TrustSource::Confirmed,
                }),
                pending: None,
            },
        );

        let mut sink = NoOpSink;
        let (evaluations, _messages) =
            mem_pool.evaluate_pending_blocks(&tokens, 10, 55, &mut sink);

        assert!(matches!(
            mem_pool.status(&lower_block.id, &MockEcBlocks::default_blocks()),
            Some(BlockState::Blocked)
        ));
        assert_eq!(evaluations.len(), 1);
        assert_eq!(evaluations[0].block_id, higher_block.id);

        let peers = EcPeers::with_config_and_rng(
            55,
            crate::ec_peers::PeerManagerConfig::default(),
            rand::rngs::StdRng::from_seed([7u8; 32]),
        );
        let mut batch = TestBatch::default();
        let mut sink = NoOpSink;
        let (_messages, commits) = mem_pool.tick_with_evaluations(
            &peers,
            10,
            55,
            &mut sink,
            &evaluations,
            &mut batch,
        );

        assert_eq!(batch.saved_blocks, vec![higher_block.id]);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].committed_block_id, higher_block.id);
        assert_eq!(commits[0].competing_block_id, Some(lower_block.id));
        assert_eq!(commits[0].interested_voters.len(), 3);
        assert!(matches!(
            mem_pool.status(&higher_block.id, &MockEcBlocks::default_blocks()),
            Some(BlockState::Commit)
        ));
        assert!(matches!(
            mem_pool.status(&lower_block.id, &MockEcBlocks::default_blocks()),
            Some(BlockState::Blocked)
        ));
    }

    #[test]
    fn vote_sequence_cycles_four_steps_then_resets() {
        let block = Block {
            id: 200,
            time: 10,
            used: 1,
            parts: [TokenBlock {
                token: 42,
                last: 7,
                key: 1,
            }, Default::default(), Default::default(), Default::default(), Default::default(), Default::default()],
            signatures: [None; TOKENS_PER_BLOCK],
        };

        let mut mem_pool = EcMemPool::with_vote_policy(2, 0, 4);
        assert!(mem_pool.block(&block, 10));

        let evaluation = BlockEvaluation {
            block_id: block.id,
            block: block.clone(),
            vote_mask: 0b0000_0001,
        };
        let peers = EcPeers::with_config_and_rng(
            55,
            crate::ec_peers::PeerManagerConfig::default(),
            rand::rngs::StdRng::from_seed([8u8; 32]),
        );
        let mut batch = TestBatch::default();
        let mut sink = NoOpSink;

        let mut seen = Vec::new();
        for time in 10..16 {
            let (messages, _) = mem_pool.tick_with_evaluations(
                &peers,
                time,
                55,
                &mut sink,
                std::slice::from_ref(&evaluation),
                &mut batch,
            );
            let token_sequences: Vec<u8> = messages
                .iter()
                .filter_map(|message| match message {
                    MessageRequest::Vote(block_id, token_id, _, true, sequence)
                        if *block_id == block.id && *token_id == 42 =>
                    {
                        Some(*sequence)
                    }
                    _ => None,
                })
                .collect();
            seen.push(token_sequences);
        }

        assert_eq!(seen[0], vec![0]);
        assert_eq!(seen[1], vec![1]);
        assert_eq!(seen[2], vec![2]);
        assert_eq!(seen[3], vec![3]);
        assert!(seen[4].is_empty(), "fifth tick should skip before restarting");
        assert_eq!(seen[5], vec![0]);
    }

    #[test]
    fn strongly_negative_tally_pauses_requests_until_balance_recovers() {
        let block = Block {
            id: 300,
            time: 10,
            used: 1,
            parts: [TokenBlock {
                token: 42,
                last: 7,
                key: 1,
            }, Default::default(), Default::default(), Default::default(), Default::default(), Default::default()],
            signatures: [None; TOKENS_PER_BLOCK],
        };

        let mut mem_pool = EcMemPool::with_vote_policy(2, 0, 4);
        assert!(mem_pool.block(&block, 10));

        for peer_id in [11, 12, 13] {
            mem_pool.vote(&block.id, 0, &peer_id, 10, true);
        }

        let evaluation = BlockEvaluation {
            block_id: block.id,
            block: block.clone(),
            vote_mask: 0b0000_0001,
        };
        let peers = EcPeers::with_config_and_rng(
            55,
            crate::ec_peers::PeerManagerConfig::default(),
            rand::rngs::StdRng::from_seed([18u8; 32]),
        );
        let mut batch = TestBatch::default();
        let mut sink = NoOpSink;

        let (messages_paused, _) = mem_pool.tick_with_evaluations(
            &peers,
            10,
            55,
            &mut sink,
            std::slice::from_ref(&evaluation),
            &mut batch,
        );
        assert!(
            !messages_paused.iter().any(|message| matches!(
                message,
                MessageRequest::Vote(block_id, token_id, _, true, _)
                    if *block_id == block.id && *token_id == 42
            )),
            "negative tally below threshold should pause token vote requests"
        );

        mem_pool.vote(&block.id, 0b0000_0001, &21, 11, true);

        let (messages_resumed, _) = mem_pool.tick_with_evaluations(
            &peers,
            11,
            55,
            &mut sink,
            std::slice::from_ref(&evaluation),
            &mut batch,
        );
        assert!(
            messages_resumed.iter().any(|message| matches!(
                message,
                MessageRequest::Vote(block_id, token_id, _, true, sequence)
                    if *block_id == block.id && *token_id == 42 && *sequence == 0
            )),
            "once balance rises back above the negative pause threshold, polling should resume"
        );
    }

    #[test]
    fn vote_sequence_can_use_three_active_rounds_then_skip() {
        let block = Block {
            id: 301,
            time: 10,
            used: 1,
            parts: [TokenBlock {
                token: 42,
                last: 7,
                key: 1,
            }, Default::default(), Default::default(), Default::default(), Default::default(), Default::default()],
            signatures: [None; TOKENS_PER_BLOCK],
        };

        let mut mem_pool = EcMemPool::with_vote_policy(2, 0, 3);
        assert!(mem_pool.block(&block, 10));

        let evaluation = BlockEvaluation {
            block_id: block.id,
            block: block.clone(),
            vote_mask: 0b0000_0001,
        };
        let peers = EcPeers::with_config_and_rng(
            55,
            crate::ec_peers::PeerManagerConfig::default(),
            rand::rngs::StdRng::from_seed([28u8; 32]),
        );
        let mut batch = TestBatch::default();
        let mut sink = NoOpSink;

        let mut seen = Vec::new();
        for time in 10..15 {
            let (messages, _) = mem_pool.tick_with_evaluations(
                &peers,
                time,
                55,
                &mut sink,
                std::slice::from_ref(&evaluation),
                &mut batch,
            );
            let token_sequences: Vec<u8> = messages
                .iter()
                .filter_map(|message| match message {
                    MessageRequest::Vote(block_id, token_id, _, true, sequence)
                        if *block_id == block.id && *token_id == 42 =>
                    {
                        Some(*sequence)
                    }
                    _ => None,
                })
                .collect();
            seen.push(token_sequences);
        }

        assert_eq!(seen[0], vec![0]);
        assert_eq!(seen[1], vec![1]);
        assert_eq!(seen[2], vec![2]);
        assert!(seen[3].is_empty(), "fourth tick should skip before restarting");
        assert_eq!(seen[4], vec![0]);
    }

    #[test]
    fn reactive_seed_advances_sequence_to_next_unsent_pair() {
        let block = Block {
            id: 401,
            time: 10,
            used: 1,
            parts: [TokenBlock {
                token: 42,
                last: 7,
                key: 1,
            }, Default::default(), Default::default(), Default::default(), Default::default(), Default::default()],
            signatures: [None; TOKENS_PER_BLOCK],
        };

        let mut mem_pool = EcMemPool::with_vote_policy(2, 0, 4);
        assert!(mem_pool.block(&block, 10));

        let evaluation = BlockEvaluation {
            block_id: block.id,
            block: block.clone(),
            vote_mask: 0b0000_0001,
        };
        let peers = EcPeers::with_config_and_rng(
            55,
            crate::ec_peers::PeerManagerConfig::default(),
            rand::rngs::StdRng::from_seed([38u8; 32]),
        );

        let reactive_targets = mem_pool.seed_reactive_vote_requests(&peers, &evaluation, 2);
        assert_eq!(reactive_targets, vec![42, block.id]);

        let mut batch = TestBatch::default();
        let mut sink = NoOpSink;
        let (messages, _) = mem_pool.tick_with_evaluations(
            &peers,
            11,
            55,
            &mut sink,
            std::slice::from_ref(&evaluation),
            &mut batch,
        );

        let token_sequences: Vec<u8> = messages
            .iter()
            .filter_map(|message| match message {
                MessageRequest::Vote(block_id, token_id, _, true, sequence)
                    if *block_id == block.id && *token_id == 42 =>
                {
                    Some(*sequence)
                }
                _ => None,
            })
            .collect();

        assert_eq!(
            token_sequences,
            vec![2],
            "after the reactive seed consumes the first two pairs, periodic polling should continue at sequence 2",
        );
    }

    impl MockEcBlocks {
        fn default_blocks() -> Self {
            Self {
                blocks: hashbrown::HashMap::new(),
            }
        }
    }
}
