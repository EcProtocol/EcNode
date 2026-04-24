use hashbrown::HashSet;
// track the state of transactions
use indexmap::IndexMap;
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

#[derive(Debug, PartialEq)]
pub enum MessageRequest {
    Block(BlockId),
    Vote(BlockId, TokenId, u8, bool, u8),
    Parent(BlockId, BlockId),
    MissingParent(BlockId),
}

const PAUSED_VOTE_SEQUENCE: u8 = u8::MAX;
const VOTE_SCHEDULE_PAUSE_TICKS: u8 = 2;
const CONFLICT_REACTIVE_TARGET_COUNT: usize = 4;

fn vote_schedule_cycle_len(active_ticks: u8) -> u8 {
    active_ticks
        .saturating_mul(VOTE_SCHEDULE_PAUSE_TICKS.saturating_add(1))
        .max(1)
}

fn vote_schedule_pair_start_for_state(
    state: u8,
    active_ticks: u8,
    pairs_per_tick: u8,
) -> Option<u8> {
    if active_ticks == 0 || pairs_per_tick == 0 {
        return None;
    }

    let cycle_len = vote_schedule_cycle_len(active_ticks);
    let normalized = state % cycle_len;
    let schedule_span = VOTE_SCHEDULE_PAUSE_TICKS.saturating_add(1);
    if normalized % schedule_span == 0 {
        Some((normalized / schedule_span).saturating_mul(pairs_per_tick))
    } else {
        None
    }
}

fn vote_schedule_next_state(state: u8, active_ticks: u8) -> u8 {
    if active_ticks == 0 {
        return 0;
    }

    let cycle_len = vote_schedule_cycle_len(active_ticks);
    (state + 1) % cycle_len
}

fn vote_schedule_restart_state(active_ticks: u8) -> u8 {
    if active_ticks == 0 {
        return 0;
    }

    vote_schedule_cycle_len(active_ticks).saturating_sub(VOTE_SCHEDULE_PAUSE_TICKS)
}

impl MessageRequest {
    pub fn sort_key(&self) -> (TokenId, Reverse<BlockId>, Reverse<bool>) {
        match self {
            MessageRequest::Block(block_id) => (*block_id, Reverse(0), Reverse(false)),
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
    votes: IndexMap<PeerId, PoolVote>,
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
            votes: IndexMap::new(),
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
                if v.time <= time {
                    v.vote = vote;
                    v.time = time;
                    v.reply = reply;
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
}

/// Result of evaluating a pending block
/// Contains only blocks that passed reorg checks and can proceed to voting/commit
pub struct BlockEvaluation {
    pub block_id: BlockId, // Include block_id for future flexibility
    pub block: Block,
    pub vote_mask: u8, // Pre-calculated vote bits (1 = can verify, 0 = cannot verify)
}

pub struct CommitTransition {
    pub committed_block_id: BlockId,
    pub competing_block_id: Option<BlockId>,
    pub interested_voters: Vec<PeerId>,
}

/// Request produced when a newly learned block should trigger immediate `InitialVote`
/// handling outside the periodic tick flow.
///
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InitialVoteRequest {
    pub receiver: PeerId,
    pub block: Block,
    pub vote: u8,
}

pub struct EcMemPool {
    pool: IndexMap<BlockId, PoolBlockState>,
    vote_balance_threshold: i64,
    vote_request_active_rounds: u8,
    vote_request_pairs_per_tick: u8,
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
    pub fn new() -> Self {
        Self::with_vote_balance_threshold(VOTE_THRESHOLD)
    }

    pub fn with_vote_balance_threshold(vote_balance_threshold: i64) -> Self {
        Self::with_vote_policy(vote_balance_threshold, 0, 4, 1)
    }

    pub fn with_vote_policy(
        vote_balance_threshold: i64,
        _vote_request_resend_cooldown: EcTime,
        vote_request_active_rounds: u8,
        vote_request_pairs_per_tick: u8,
    ) -> Self {
        Self {
            pool: IndexMap::new(),
            vote_balance_threshold,
            vote_request_active_rounds: vote_request_active_rounds.max(1),
            vote_request_pairs_per_tick: vote_request_pairs_per_tick.max(1),
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

    /// Evaluate all pending blocks and determine which can proceed to commit
    ///
    /// This phase does all token lookups with an immutable borrow and:
    /// - Requests missing blocks for pending entries that only have vote placeholders
    /// - Calculates vote masks (positive vote only if we can verify the chain)
    /// - Detects reorgs/missing history and generates PARENTCOMMIT requests
    /// - Filters out blocks that cannot commit this tick
    ///
    /// Returns:
    /// - Vec of blocks that can proceed (no reorg detected)
    /// - Vec of block/parent/vote-related follow-up requests
    pub(crate) fn evaluate_pending_blocks(
        &self,
        tokens: &dyn EcTokensV2,
        time: EcTime,
        id: PeerId,
        event_sink: &mut dyn EventSink,
    ) -> (Vec<BlockEvaluation>, Vec<MessageRequest>) {
        let mut evaluations = Vec::new();
        let mut messages = Vec::new();
        for (block_id, block_state) in &self.pool {
            if block_state.state != Pending {
                continue;
            }
            let Some(block) = block_state.block else {
                messages.push(MessageRequest::Block(*block_id));
                continue;
            };

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
            .map(|state| {
                state
                    .votes
                    .iter()
                    .filter_map(|(peer_id, vote)| vote.reply.then_some(*peer_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn last_interested_voter(&self, block: &BlockId) -> Option<PeerId> {
        self.pool.get(block).and_then(|state| {
            state
                .votes
                .iter()
                .rev()
                .find_map(|(peer_id, vote)| vote.reply.then_some(*peer_id))
        })
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

    fn add_block(&mut self, block: &Block, time: EcTime) -> bool {
        let state = self
            .pool
            .entry(block.id)
            .or_insert_with(|| PoolBlockState::new(time));

        if state.block.is_some() {
            return false;
        }

        if block.used as usize >= TOKENS_PER_BLOCK || block.time > time + SOME_STEPS_INTO_THE_FUTURE
        {
            // TODO same token only once

            // TODO verify that block-id is the SHA of block content INCL signatures)

            state.state = BlockState::Blocked;

            false
        } else {
            state.state = BlockState::Pending;
            state.block = Some(*block);
            state.time = time;
            state.updated = true;
            state.validate = 0;
            state.vote = 0;
            state.remaining = 0;
            state.competing_block = None;
            state.vote_sequence = [0; TOKENS_PER_BLOCK + 1];

            true
        }
    }

    pub(crate) fn block(&mut self, block: &Block, time: EcTime) -> bool {
        self.add_block(block, time)
    }

    fn blocks_conflict(existing: &Block, new_block: &Block) -> bool {
        for existing_idx in 0..existing.used as usize {
            for new_idx in 0..new_block.used as usize {
                if existing.parts[existing_idx].token == new_block.parts[new_idx].token
                    && existing.parts[existing_idx].last == new_block.parts[new_idx].last
                {
                    return true;
                }
            }
        }

        false
    }

    fn vote_mask_for_block(block: &Block, tokens: &dyn EcTokensV2) -> u8 {
        let mut vote = 0;

        for i in 0..block.used as usize {
            let token_id = block.parts[i].token;
            let last_mapping = block.parts[i].last;
            let current_mapping = tokens.lookup_current(&token_id).map_or(0, |t| t.block);

            if current_mapping == last_mapping {
                vote |= 1 << i;
            }
        }

        vote
    }

    fn pending_vote_mask(block: &Block) -> u8 {
        let mut mask = 1 << TOKENS_PER_BLOCK;
        for i in 0..block.used as usize {
            mask |= 1 << i;
        }
        mask
    }

    fn schedule_closest_peer_requests(
        requests: &mut HashSet<PeerId>,
        peers: &EcPeers,
        block: &Block,
        count: usize,
    ) {
        for i in 0..block.used as usize {
            for receiver in peers.find_closest_active_peers(block.parts[i].token, count) {
                requests.insert(receiver);
            }
        }

        for receiver in peers.find_closest_active_peers(block.id, count) {
            requests.insert(receiver);
        }
    }

    /// Add a newly learned block through the reactive path and describe the
    /// immediate `InitialVote` work that should happen before the next tick.
    ///
    /// Behavior:
    /// - Returns an empty request list if the block already has a concrete
    ///   mempool entry or fails basic shape/time validation.
    /// - Reuses an existing vote-only placeholder entry for `block` when present,
    ///   preserving its votes while filling in the concrete block body.
    /// - Compares `block` against existing concrete blocks in the pool. Two
    ///   blocks conflict when they update the same token from the same parent.
    /// - Tracks the highest competing block ID for both the new state and any
    ///   conflicting existing state, marking whichever side loses the ID race as
    ///   `Blocked`.
    /// - Emits `InitialVoteRequest`s for:
    ///   - each recorded voter of an existing block newly blocked by `block`
    ///   - the 4 closest active peers to each target token and witness of such blocked blocks
    ///   - the configured first-wave peers for each target token and witness of `block`
    ///
    /// The returned requests are deduplicated by receiver because the same peer
    /// can be reached through multiple target tokens.
    pub(crate) fn reactive_add_block(
        &mut self,
        block: &Block,
        peers: &EcPeers,
        tokens: &dyn EcTokensV2,
        time: EcTime,
    ) -> Vec<InitialVoteRequest> {
        let block_id = block.id;

        // Phase 1: Add block, check if already known or invalid
        if !self.add_block(block, time) {
            return Vec::new();
        }

        let mut requests = HashSet::new();
        let mut blocked_by_id: Option<BlockId> = None;

        // Phase 2: Find conflicts and update existing blocks
        for (&existing_block_id, existing_state) in &mut self.pool {
            if existing_block_id == block_id {
                continue;
            }
            let Some(existing_block) = existing_state.block else {
                continue;
            };

            if !Self::blocks_conflict(&existing_block, block) {
                continue;
            }

            // new block loses
            if existing_block_id > block_id {
                blocked_by_id = Some(
                    blocked_by_id
                        .map_or(existing_block_id, |current| current.max(existing_block_id)),
                );
                continue;
            }

            // existing block loses (and new block replaces competing-block)
            if block_id > existing_block_id
                && block_id > existing_state.competing_block.unwrap_or(0)
            {
                existing_state.competing_block = Some(block_id);
                if existing_state.state == Pending {
                    existing_state.state = BlockState::Blocked;
                    existing_state.updated = false;

                    // those that voted for it
                    for (&receiver, vote) in &existing_state.votes {
                        if !vote.reply {
                            continue;
                        }
                        requests.insert(receiver);
                    }

                    // its neighborhoods
                    Self::schedule_closest_peer_requests(
                        &mut requests,
                        peers,
                        &existing_block,
                        CONFLICT_REACTIVE_TARGET_COUNT,
                    );
                }
            }
        }

        let vote = if blocked_by_id.is_some() {
            0
        } else {
            Self::vote_mask_for_block(block, tokens)
        };
        let regular_vote_sequence_state =
            vote_schedule_restart_state(self.vote_request_active_rounds);

        // Phase 3: Update new block's state
        let interested_voters: Vec<PeerId> = {
            let state = self.pool.get_mut(&block_id).unwrap();

            state.state = if blocked_by_id.is_none() {
                BlockState::Pending
            } else {
                BlockState::Blocked
            };
            state.competing_block = blocked_by_id;
            state.vote = vote;
            if state.state == BlockState::Pending {
                state.remaining = Self::pending_vote_mask(block);
                for i in 0..block.used as usize {
                    state.vote_sequence[i] = regular_vote_sequence_state;
                }
                state.vote_sequence[TOKENS_PER_BLOCK] = regular_vote_sequence_state;
            }
            state
                .votes
                .iter()
                .filter_map(|(peer_id, vote)| vote.reply.then_some(*peer_id))
                .collect()
        };

        Self::schedule_closest_peer_requests(
            &mut requests,
            peers,
            block,
            peers.first_vote_target_count(),
        );

        let mut responses: Vec<InitialVoteRequest> = Vec::new();

        // Notify voters of the new block if it got blocked
        if let Some(blocked_by_id) = blocked_by_id {
            let blocker = self.pool.get(&blocked_by_id).unwrap();
            for receiver in interested_voters {
                let b = blocker.block.unwrap();
                responses.push(InitialVoteRequest {
                    receiver,
                    block: b,
                    vote: Self::vote_mask_for_block(&b, tokens),
                });
            }
        }

        for &receiver in &requests {
            responses.push(InitialVoteRequest {
                receiver,
                block: *block,
                vote,
            });
        }

        responses
    }

    fn refresh_pending_vote_state(
        block_state: &mut PoolBlockState,
        block: &Block,
        peers: &EcPeers,
        vote_balance_threshold: i64,
    ) {
        let previous_remaining = block_state.remaining;
        let (balance, witness_balance) = Self::calculate_vote_balances(block_state, block, peers);

        block_state.remaining = if witness_balance <= vote_balance_threshold {
            1 << TOKENS_PER_BLOCK
        } else {
            0
        };

        for i in 0..block.used as usize {
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

    fn calculate_vote_balances(
        block_state: &PoolBlockState,
        block: &Block,
        peers: &EcPeers,
    ) -> ([i64; TOKENS_PER_BLOCK], i64) {
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

        (balance, witness_balance)
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
            let block = &evaluation.block;
            let (competing_block_id, vote_count) = {
                let block_state = self.pool.get_mut(&block_id).unwrap();
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
                    (block_state.competing_block, block_state.votes.len())
                } else {
                    // Not ready to commit - request validation or votes
                    for i in 0..block.used as usize {
                        if (block_state.validate & 1 << i) != 0 {
                            // Fetch parent for signature validation
                            messages.push(MessageRequest::Parent(block_id, block.parts[i].last));
                        }

                        if (block_state.remaining & 1 << i) != 0 {
                            let state = block_state.vote_sequence[i];
                            if state == PAUSED_VOTE_SEQUENCE {
                                continue;
                            }
                            let next_state =
                                vote_schedule_next_state(state, self.vote_request_active_rounds);
                            let Some(sequence_start) = vote_schedule_pair_start_for_state(
                                state,
                                self.vote_request_active_rounds,
                                self.vote_request_pairs_per_tick,
                            ) else {
                                block_state.vote_sequence[i] = next_state;
                                continue;
                            };
                            block_state.vote_sequence[i] = next_state;
                            for offset in 0..self.vote_request_pairs_per_tick {
                                // Request vote using pre-calculated vote_mask
                                messages.push(MessageRequest::Vote(
                                    block_id,
                                    block.parts[i].token,
                                    evaluation.vote_mask,
                                    evaluation.vote_mask & 1 << i != 0,
                                    sequence_start + offset,
                                ));
                            }
                        }
                    }

                    // Vote witness
                    if (block_state.remaining & 1 << TOKENS_PER_BLOCK) != 0 {
                        let witness_idx = TOKENS_PER_BLOCK;
                        let state = block_state.vote_sequence[witness_idx];
                        if state == PAUSED_VOTE_SEQUENCE {
                            continue;
                        }
                        let next_state =
                            vote_schedule_next_state(state, self.vote_request_active_rounds);
                        let Some(sequence_start) = vote_schedule_pair_start_for_state(
                            state,
                            self.vote_request_active_rounds,
                            self.vote_request_pairs_per_tick,
                        ) else {
                            block_state.vote_sequence[witness_idx] = next_state;
                            continue;
                        };
                        block_state.vote_sequence[witness_idx] = next_state;
                        for offset in 0..self.vote_request_pairs_per_tick {
                            messages.push(MessageRequest::Vote(
                                block_id,
                                block_id,
                                evaluation.vote_mask,
                                false,
                                sequence_start + offset,
                            ));
                        }
                    }

                    continue;
                }
            };

            commits.push(CommitTransition {
                committed_block_id: block_id,
                competing_block_id,
                interested_voters: self.interested_voters(&block_id),
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
                            votes: vote_count,
                        },
                    );

                    // Update token with parent (block.parts[i].last is the parent block ID)
                    batch.update_token(
                        &block.parts[i].token,
                        &block.id,
                        &block.parts[i].last,
                        block.time,
                    );
                }
            }

            self.pool.get_mut(&block_id).unwrap().state = BlockState::Commit;
            continue;
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
    };
    use crate::ec_peers::{EcPeers, PeerManagerConfig};
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
            self.tokens.get(token).is_some_and(|state| state.is_local())
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

        fn update_token(
            &mut self,
            token: &TokenId,
            block: &BlockId,
            parent: &BlockId,
            time: EcTime,
        ) {
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

    fn test_block(id: BlockId, token: TokenId, last: BlockId) -> Block {
        Block {
            id,
            time: 10,
            used: 1,
            parts: [
                TokenBlock {
                    token,
                    last,
                    key: 1,
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
            signatures: [None; TOKENS_PER_BLOCK],
        }
    }

    fn test_peers() -> EcPeers {
        let mut peers = EcPeers::with_config_and_rng(
            55,
            PeerManagerConfig::default(),
            rand::rngs::StdRng::from_seed([41u8; 32]),
        );
        for peer_id in [100, 200, 300, 400, 500] {
            peers.update_peer(&peer_id, 0);
        }
        peers
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
                other => panic!(
                    "unexpected request in sort test: {:?}",
                    std::mem::discriminant(&other)
                ),
            })
            .collect();

        assert_eq!(ordered, vec![(15, 66), (30, 77), (20, 77), (10, 77)]);
    }

    #[test]
    fn pending_entry_without_block_requests_block_fetch() {
        let mut mem_pool = EcMemPool::new();
        mem_pool.vote(&77, 0, &11, 10, true);

        let tokens = MockTokens::default();
        let mut sink = NoOpSink;
        let (evaluations, messages) = mem_pool.evaluate_pending_blocks(&tokens, 10, 55, &mut sink);

        assert!(evaluations.is_empty());
        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0], MessageRequest::Block(77)));
    }

    #[test]
    fn block_fills_vote_placeholder_with_concrete_state() {
        let mut mem_pool = EcMemPool::new();
        let block = test_block(77, 250, 7);

        mem_pool.vote(&block.id, 0b0000_0001, &11, 10, true);

        assert!(mem_pool.block(&block, 10));

        let state = mem_pool.pool.get(&block.id).unwrap();
        assert_eq!(state.block, Some(block));
        assert!(state.updated);
        assert_eq!(state.validate, 0);
        assert_eq!(state.vote, 0);
        assert_eq!(state.votes.len(), 1);
    }

    #[test]
    fn reactive_add_block_seeds_new_block_to_innermost_peers() {
        let block = test_block(200, 250, 7);
        let mut mem_pool = EcMemPool::new();
        let peers = test_peers();
        let mut tokens = MockTokens::default();
        tokens.tokens.insert(
            250,
            TokenState {
                current: Some(TrustedMapping {
                    block: 7,
                    parent: 0,
                    time: 0,
                    source: crate::ec_interface::TrustSource::Confirmed,
                }),
                pending: None,
            },
        );

        let requests = mem_pool.reactive_add_block(&block, &peers, &tokens, 10);

        assert_eq!(requests.len(), 4);
        assert!(requests
            .iter()
            .all(|request| { request.block.id == block.id && request.vote == 0b0000_0001 }));
        assert!(matches!(
            mem_pool.pool.get(&block.id).map(|state| &state.state),
            Some(BlockState::Pending)
        ));
    }

    #[test]
    fn reactive_add_block_pauses_before_vote_repair_resumes() {
        let block = test_block(200, 250, 7);
        let mut mem_pool = EcMemPool::with_vote_policy(2, 0, 4, 1);
        let peers = test_peers();
        let mut tokens = MockTokens::default();
        tokens.tokens.insert(
            250,
            TokenState {
                current: Some(TrustedMapping {
                    block: 7,
                    parent: 0,
                    time: 0,
                    source: crate::ec_interface::TrustSource::Confirmed,
                }),
                pending: None,
            },
        );

        let requests = mem_pool.reactive_add_block(&block, &peers, &tokens, 10);
        assert!(!requests.is_empty());

        let state = mem_pool.pool.get(&block.id).unwrap();
        assert_eq!(state.remaining, (1 << 0) | (1 << TOKENS_PER_BLOCK));
        assert_eq!(state.vote_sequence[0], 10);
        assert_eq!(state.vote_sequence[TOKENS_PER_BLOCK], 10);

        let evaluation = BlockEvaluation {
            block_id: block.id,
            block,
            vote_mask: 0b0000_0001,
        };
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
                MessageRequest::Vote(block_id, _, _, _, _) if *block_id == block.id
            )),
            "the first tick after the eager InitialVote fanout should be paused",
        );

        let (messages_still_paused, _) = mem_pool.tick_with_evaluations(
            &peers,
            11,
            55,
            &mut sink,
            std::slice::from_ref(&evaluation),
            &mut batch,
        );
        assert!(
            !messages_still_paused.iter().any(|message| matches!(
                message,
                MessageRequest::Vote(block_id, _, _, _, _) if *block_id == block.id
            )),
            "the second tick after the eager InitialVote fanout should still be paused",
        );

        let (messages_resumed, _) = mem_pool.tick_with_evaluations(
            &peers,
            12,
            55,
            &mut sink,
            std::slice::from_ref(&evaluation),
            &mut batch,
        );
        assert!(
            messages_resumed.iter().any(|message| {
                matches!(
                    message,
                    MessageRequest::Vote(block_id, token_id, _, true, sequence)
                        if *block_id == block.id && *token_id == 250 && *sequence == 0
                )
            }),
            "after two pause ticks, Vote repair should restart from the innermost token ring",
        );
        assert!(
            messages_resumed.iter().any(|message| {
                matches!(
                    message,
                    MessageRequest::Vote(block_id, token_id, _, false, sequence)
                        if *block_id == block.id && *token_id == block.id && *sequence == 0
                )
            }),
            "the witness Vote schedule should restart from the innermost ring as well",
        );
    }

    #[test]
    fn reactive_add_block_fanout_includes_witness_targets() {
        let block = test_block(490, 250, 7);
        let mut mem_pool = EcMemPool::new();
        let peers = test_peers();
        let mut tokens = MockTokens::default();
        tokens.tokens.insert(
            250,
            TokenState {
                current: Some(TrustedMapping {
                    block: 7,
                    parent: 0,
                    time: 0,
                    source: crate::ec_interface::TrustSource::Confirmed,
                }),
                pending: None,
            },
        );

        let requests = mem_pool.reactive_add_block(&block, &peers, &tokens, 10);
        let receivers: HashSet<PeerId> = requests.iter().map(|request| request.receiver).collect();

        assert_eq!(receivers.len(), 5);
        assert!(receivers.contains(&500));
    }

    #[test]
    fn reactive_add_block_deduplicates_receivers_across_targets() {
        let block = Block {
            id: 210,
            time: 10,
            used: 2,
            parts: [
                TokenBlock {
                    token: 250,
                    last: 7,
                    key: 1,
                },
                TokenBlock {
                    token: 260,
                    last: 8,
                    key: 1,
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
            signatures: [None; TOKENS_PER_BLOCK],
        };
        let mut mem_pool = EcMemPool::new();
        let peers = test_peers();
        let mut tokens = MockTokens::default();
        tokens.tokens.insert(
            250,
            TokenState {
                current: Some(TrustedMapping {
                    block: 7,
                    parent: 0,
                    time: 0,
                    source: crate::ec_interface::TrustSource::Confirmed,
                }),
                pending: None,
            },
        );
        tokens.tokens.insert(
            260,
            TokenState {
                current: Some(TrustedMapping {
                    block: 8,
                    parent: 0,
                    time: 0,
                    source: crate::ec_interface::TrustSource::Confirmed,
                }),
                pending: None,
            },
        );

        let requests = mem_pool.reactive_add_block(&block, &peers, &tokens, 10);
        let unique_receivers: HashSet<PeerId> =
            requests.iter().map(|request| request.receiver).collect();

        assert_eq!(requests.len(), unique_receivers.len());
        assert!(requests
            .iter()
            .all(|request| request.block.id == block.id && request.vote == 0b0000_0011));
    }

    #[test]
    fn reactive_add_block_marks_new_block_blocked_by_higher_existing_conflict() {
        let existing = test_block(300, 250, 7);
        let new_block = test_block(200, 250, 7);
        let mut mem_pool = EcMemPool::new();
        let peers = test_peers();
        let mut tokens = MockTokens::default();
        tokens.tokens.insert(
            250,
            TokenState {
                current: Some(TrustedMapping {
                    block: 7,
                    parent: 0,
                    time: 0,
                    source: crate::ec_interface::TrustSource::Confirmed,
                }),
                pending: None,
            },
        );

        assert!(mem_pool.block(&existing, 10));

        let requests = mem_pool.reactive_add_block(&new_block, &peers, &tokens, 10);

        assert_eq!(requests.len(), 4);
        assert!(requests
            .iter()
            .all(|request| { request.block.id == new_block.id && request.vote == 0 }));

        let new_state = mem_pool.pool.get(&new_block.id).unwrap();
        assert_eq!(new_state.state, BlockState::Blocked);
        assert_eq!(new_state.competing_block, Some(existing.id));
    }

    #[test]
    fn reactive_add_block_blocks_lower_existing_conflict_and_notifies_voters() {
        let existing = test_block(100, 250, 7);
        let new_block = test_block(200, 250, 7);
        let mut mem_pool = EcMemPool::new();
        let peers = test_peers();
        let mut tokens = MockTokens::default();
        tokens.tokens.insert(
            250,
            TokenState {
                current: Some(TrustedMapping {
                    block: 7,
                    parent: 0,
                    time: 0,
                    source: crate::ec_interface::TrustSource::Confirmed,
                }),
                pending: None,
            },
        );

        assert!(mem_pool.block(&existing, 10));
        mem_pool.vote(&existing.id, 0b0000_0001, &999, 10, true);
        mem_pool.vote(&existing.id, 0b0000_0001, &998, 11, false);

        let requests = mem_pool.reactive_add_block(&new_block, &peers, &tokens, 10);

        // All requests now contain the new_block (winning block)
        assert!(requests
            .iter()
            .all(|request| request.block.id == new_block.id && request.vote == 0b0000_0001));

        // Voter 999 (who voted for existing) should be notified about the new block
        assert!(requests.iter().any(|request| request.receiver == 999));
        assert!(!requests.iter().any(|request| request.receiver == 998));

        // Should have requests for: existing block voter (999) + 2 closest peers for
        // existing block + 4 closest peers for new block (deduplicated)
        assert!(requests.len() >= 4);

        let existing_state = mem_pool.pool.get(&existing.id).unwrap();
        assert_eq!(existing_state.state, BlockState::Blocked);
        assert_eq!(existing_state.competing_block, Some(new_block.id));
    }

    #[test]
    fn reactive_add_block_does_not_notify_terminal_losing_blocks() {
        for existing_state_kind in [BlockState::Blocked, BlockState::Commit] {
            let existing = test_block(100, 250, 7);
            let new_block = test_block(200, 250, 7);
            let mut mem_pool = EcMemPool::new();
            let peers = test_peers();
            let mut tokens = MockTokens::default();
            tokens.tokens.insert(
                250,
                TokenState {
                    current: Some(TrustedMapping {
                        block: 7,
                        parent: 0,
                        time: 0,
                        source: crate::ec_interface::TrustSource::Confirmed,
                    }),
                    pending: None,
                },
            );

            assert!(mem_pool.block(&existing, 10));
            mem_pool.vote(&existing.id, 0b0000_0001, &999, 10, true);
            let existing_state = mem_pool.pool.get_mut(&existing.id).unwrap();
            existing_state.state = existing_state_kind.clone();

            let requests = mem_pool.reactive_add_block(&new_block, &peers, &tokens, 10);

            assert!(
                requests
                    .iter()
                    .all(|request| request.block.id != existing.id),
                "terminal losing blocks should not emit reactive InitialVote requests",
            );

            let existing_state = mem_pool.pool.get(&existing.id).unwrap();
            assert_eq!(existing_state.state, existing_state_kind);
            assert_eq!(existing_state.competing_block, Some(new_block.id));
        }
    }

    #[test]
    fn strongly_negative_tally_pauses_requests_until_balance_recovers() {
        let block = Block {
            id: 300,
            time: 10,
            used: 1,
            parts: [
                TokenBlock {
                    token: 42,
                    last: 7,
                    key: 1,
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
            signatures: [None; TOKENS_PER_BLOCK],
        };

        let mut mem_pool = EcMemPool::with_vote_policy(2, 0, 4, 1);
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
}
