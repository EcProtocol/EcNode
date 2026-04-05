use ec_rust::ec_interface::{BatchRequestItem, Message};

#[derive(Debug, Clone)]
pub struct DistributionSummary {
    pub samples: usize,
    pub min: usize,
    pub avg: f64,
    pub p50: usize,
    pub p95: usize,
    pub max: usize,
}

impl DistributionSummary {
    pub fn from_samples(samples: &[usize]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }

        let mut sorted = samples.to_vec();
        sorted.sort_unstable();

        let len = sorted.len();
        let percentile_index = |numerator: usize| -> usize {
            ((len - 1) * numerator) / 100
        };

        Some(Self {
            samples: len,
            min: sorted[0],
            avg: sorted.iter().sum::<usize>() as f64 / len as f64,
            p50: sorted[percentile_index(50)],
            p95: sorted[percentile_index(95)],
            max: sorted[len - 1],
        })
    }
}

#[derive(Debug, Clone)]
pub struct FloatDistributionSummary {
    pub samples: usize,
    pub min: f64,
    pub avg: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

impl FloatDistributionSummary {
    pub fn from_samples(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }

        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));

        let len = sorted.len();
        let percentile_index = |numerator: usize| -> usize {
            ((len - 1) * numerator) / 100
        };

        Some(Self {
            samples: len,
            min: sorted[0],
            avg: sorted.iter().sum::<f64>() / len as f64,
            p50: sorted[percentile_index(50)],
            p95: sorted[percentile_index(95)],
            max: sorted[len - 1],
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct MessageTypeBreakdown {
    pub vote: usize,
    pub query_block: usize,
    pub query_token: usize,
    pub request_batch: usize,
    pub answer: usize,
    pub block: usize,
    pub referral: usize,
    pub query_commit_block: usize,
    pub commit_block: usize,
    pub batched_request_items: usize,
}

impl MessageTypeBreakdown {
    fn record_request_item(&mut self, item: &BatchRequestItem) {
        self.batched_request_items += 1;
        match item {
            BatchRequestItem::Vote { .. } => self.vote += 1,
            BatchRequestItem::QueryBlock { .. } => self.query_block += 1,
            BatchRequestItem::QueryToken { .. } => self.query_token += 1,
        }
    }

    pub fn record_wire(&mut self, message: &Message) {
        match message {
            Message::Vote { .. } => self.vote += 1,
            Message::QueryBlock { .. } => self.query_block += 1,
            Message::QueryToken { .. } => self.query_token += 1,
            Message::RequestBatch { items } => {
                self.request_batch += 1;
                self.batched_request_items += items.len();
            }
            Message::Answer { .. } => self.answer += 1,
            Message::Block { .. } => self.block += 1,
            Message::Referral { .. } => self.referral += 1,
            Message::QueryCommitBlock { .. } => self.query_commit_block += 1,
            Message::CommitBlock { .. } => self.commit_block += 1,
        }
    }

    pub fn record_logical(&mut self, message: &Message) {
        match message {
            Message::RequestBatch { items } => {
                for item in items {
                    self.record_request_item(item);
                }
            }
            _ => self.record_wire(message),
        }
    }

    pub fn total(&self) -> usize {
        self.vote
            + self.query_block
            + self.query_token
            + self.request_batch
            + self.answer
            + self.block
            + self.referral
            + self.query_commit_block
            + self.commit_block
    }
}

#[derive(Debug, Clone)]
pub struct MempoolPressureSummary {
    pub avg_pending_without_block: f64,
    pub peak_pending_without_block: usize,
    pub avg_pending_no_trusted_votes: f64,
    pub peak_pending_no_trusted_votes: usize,
    pub avg_pending_waiting_validation: f64,
    pub peak_pending_waiting_validation: usize,
    pub avg_pending_waiting_token_votes: f64,
    pub peak_pending_waiting_token_votes: usize,
    pub avg_pending_waiting_witness: f64,
    pub peak_pending_waiting_witness: usize,
    pub avg_pending_age_50_plus: f64,
    pub peak_pending_age_50_plus: usize,
    pub avg_pending_age_200_plus: f64,
    pub peak_pending_age_200_plus: usize,
}

#[derive(Debug, Clone)]
pub struct VoteIngressSummary {
    pub trusted_votes_recorded: usize,
    pub untrusted_votes_received: usize,
    pub block_requests_triggered_by_votes: usize,
}

#[derive(Debug, Clone)]
pub struct RoundMetrics {
    pub round: usize,
    pub active_peers: usize,
    pub eligible_transaction_sources: usize,
    pub in_flight_messages: usize,
    pub avg_known_peers: f64,
    pub avg_connected_peers: f64,
    pub avg_identified_peers: f64,
    pub avg_pending_peers: f64,
    pub avg_known_heads: f64,
    pub active_elections: usize,
    pub active_traces: usize,
    pub submitted_blocks: usize,
    pub committed_blocks: usize,
    pub pending_blocks: usize,
    pub pending_without_block: usize,
    pub pending_no_trusted_votes: usize,
    pub pending_waiting_validation: usize,
    pub pending_waiting_token_votes: usize,
    pub pending_waiting_witness: usize,
    pub pending_age_50_plus: usize,
    pub pending_age_200_plus: usize,
    pub total_messages_delivered: usize,
    pub commits_this_round: usize,
    pub recent_commit_rate: f64,
    pub skipped_submissions: usize,
    pub trusted_votes_recorded: usize,
    pub untrusted_votes_received: usize,
    pub block_requests_triggered_by_votes: usize,
}

#[derive(Debug, Clone)]
pub struct OnboardingSummary {
    pub observed_peers: usize,
    pub bootstrap_seeded_peers: usize,
    pub time_to_connected: Option<DistributionSummary>,
    pub time_to_known_head: Option<DistributionSummary>,
    pub time_to_sync_trace: Option<DistributionSummary>,
    pub connected_before_known_head: usize,
    pub connected_before_sync_trace: usize,
}

#[derive(Debug, Clone)]
pub struct RecoverySummary {
    pub label: String,
    pub start_round: usize,
    pub baseline_commit_rate: f64,
    pub recovered_round: Option<usize>,
}

impl RecoverySummary {
    pub fn rounds_to_recover(&self) -> Option<usize> {
        self.recovered_round
            .map(|round| round.saturating_sub(self.start_round))
    }
}

#[derive(Debug, Clone)]
pub struct NeighborhoodBucketSummary {
    pub label: String,
    pub token_samples: usize,
    pub committed_blocks: usize,
    pub coverage_size: Option<DistributionSummary>,
    pub vote_eligible_size: Option<DistributionSummary>,
    pub entry_hops: Option<DistributionSummary>,
    pub commit_latency: Option<DistributionSummary>,
}

#[derive(Debug, Clone)]
pub struct NeighborhoodSummary {
    pub token_samples: usize,
    pub local_token_samples: usize,
    pub coverage_size: Option<DistributionSummary>,
    pub vote_eligible_size: Option<DistributionSummary>,
    pub entry_hops: Option<DistributionSummary>,
    pub buckets: Vec<NeighborhoodBucketSummary>,
}

#[derive(Debug, Clone)]
pub struct TransactionSpreadSummary {
    pub submitted_blocks: usize,
    pub committed_blocks: usize,
    pub reachable_vote_peers: Option<DistributionSummary>,
    pub reachable_vote_edges: Option<DistributionSummary>,
    pub witness_coverage: Option<DistributionSummary>,
    pub ideal_role_sum_lower_bound_messages: Option<DistributionSummary>,
    pub ideal_coalesced_lower_bound_messages: Option<DistributionSummary>,
    pub settled_peer_spread: Option<DistributionSummary>,
    pub settled_block_messages: Option<DistributionSummary>,
    pub actual_to_role_sum_ratio: Option<FloatDistributionSummary>,
    pub actual_to_coalesced_ratio: Option<FloatDistributionSummary>,
    pub total_actual_block_messages: usize,
    pub total_ideal_role_sum_lower_bound_messages: usize,
    pub total_ideal_coalesced_lower_bound_messages: usize,
}

#[derive(Debug, Clone)]
pub struct TransactionWorkloadSummary {
    pub configured_existing_token_fraction: f64,
    pub existing_token_parts: usize,
    pub new_token_parts: usize,
    pub blocks_with_existing_tokens: usize,
}

impl TransactionWorkloadSummary {
    pub fn actual_existing_token_fraction(&self) -> f64 {
        let total = self.existing_token_parts + self.new_token_parts;
        if total == 0 {
            0.0
        } else {
            self.existing_token_parts as f64 / total as f64
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimResult {
    pub seed_used: [u8; 32],
    pub rounds_completed: usize,
    pub total_peers: usize,
    pub active_peers: usize,
    pub transaction_source_policy: String,
    pub submission_attempts: usize,
    pub submitted_blocks: usize,
    pub skipped_submissions: usize,
    pub committed_blocks: usize,
    pub pending_blocks: usize,
    pub total_messages_delivered: usize,
    pub total_wire_messages_delivered: usize,
    pub peak_in_flight_messages: usize,
    pub peak_active_traces: usize,
    pub peak_active_elections: usize,
    pub final_network_base_delay_rounds: usize,
    pub final_network_jitter_rounds: usize,
    pub final_network_delay_fraction: f64,
    pub final_network_loss_fraction: f64,
    pub final_avg_known_peers: f64,
    pub final_avg_connected_peers: f64,
    pub final_eligible_transaction_sources: usize,
    pub avg_eligible_transaction_sources: f64,
    pub final_recent_commit_rate: f64,
    pub commit_latency: Option<DistributionSummary>,
    pub network_transit_delay: Option<DistributionSummary>,
    pub scheduled_message_types: MessageTypeBreakdown,
    pub delivered_message_types: MessageTypeBreakdown,
    pub scheduled_wire_message_types: MessageTypeBreakdown,
    pub delivered_wire_message_types: MessageTypeBreakdown,
    pub mempool_pressure: MempoolPressureSummary,
    pub vote_ingress: VoteIngressSummary,
    pub neighborhoods: NeighborhoodSummary,
    pub transaction_workload: TransactionWorkloadSummary,
    pub transaction_spread: TransactionSpreadSummary,
    pub late_joiner_onboarding: OnboardingSummary,
    pub rejoin_onboarding: OnboardingSummary,
    pub recoveries: Vec<RecoverySummary>,
    pub round_metrics: Vec<RoundMetrics>,
}

impl SimResult {
    pub fn print_summary(&self) {
        println!("\n╔════════════════════════════════════════════════════════╗");
        println!("║  Integrated Simulation Summary                        ║");
        println!("╚════════════════════════════════════════════════════════╝");
        println!("Seed: {:?}", self.seed_used);
        println!("Rounds completed: {}", self.rounds_completed);
        println!("Peers: {} total, {} active", self.total_peers, self.active_peers);
        println!(
            "Network: base {} rounds, jitter 0..={}, tail {:.1}%, loss {:.2}%",
            self.final_network_base_delay_rounds,
            self.final_network_jitter_rounds,
            self.final_network_delay_fraction * 100.0,
            self.final_network_loss_fraction * 100.0,
        );
        println!(
            "Transaction sources: {}, avg eligible {:.1}, final eligible {}",
            self.transaction_source_policy,
            self.avg_eligible_transaction_sources,
            self.final_eligible_transaction_sources,
        );
        println!(
            "Transaction workload: existing-token target {:.0}%, actual existing-token parts {:.1}% ({} existing / {} new, {} blocks touched existing state)",
            self.transaction_workload.configured_existing_token_fraction * 100.0,
            self.transaction_workload.actual_existing_token_fraction() * 100.0,
            self.transaction_workload.existing_token_parts,
            self.transaction_workload.new_token_parts,
            self.transaction_workload.blocks_with_existing_tokens,
        );
        println!(
            "Blocks: {} attempts, {} submitted, {} skipped, {} committed, {} pending",
            self.submission_attempts,
            self.submitted_blocks,
            self.skipped_submissions,
            self.committed_blocks,
            self.pending_blocks
        );
        println!("Logical messages delivered: {}", self.total_messages_delivered);
        println!("Wire messages delivered: {}", self.total_wire_messages_delivered);
        println!(
            "Peak in-flight queue: {} messages",
            self.peak_in_flight_messages
        );
        println!(
            "Connectivity: avg known {:.1}, avg connected {:.1}, peak traces {}, peak elections {}",
            self.final_avg_known_peers,
            self.final_avg_connected_peers,
            self.peak_active_traces,
            self.peak_active_elections,
        );
        println!(
            "Recent throughput: {:.2} commits/round",
            self.final_recent_commit_rate
        );
        println!(
            "Scheduled logical messages by type: total {}, votes {}, query-block {}, query-token {}, answers {}, blocks {}, referrals {}, query-commit {}, commit-block {}",
            self.scheduled_message_types.total(),
            self.scheduled_message_types.vote,
            self.scheduled_message_types.query_block,
            self.scheduled_message_types.query_token,
            self.scheduled_message_types.answer,
            self.scheduled_message_types.block,
            self.scheduled_message_types.referral,
            self.scheduled_message_types.query_commit_block,
            self.scheduled_message_types.commit_block,
        );
        println!(
            "Delivered logical messages by type: total {}, votes {}, query-block {}, query-token {}, answers {}, blocks {}, referrals {}, query-commit {}, commit-block {}",
            self.delivered_message_types.total(),
            self.delivered_message_types.vote,
            self.delivered_message_types.query_block,
            self.delivered_message_types.query_token,
            self.delivered_message_types.answer,
            self.delivered_message_types.block,
            self.delivered_message_types.referral,
            self.delivered_message_types.query_commit_block,
            self.delivered_message_types.commit_block,
        );
        println!(
            "Scheduled wire messages by type: total {}, request-batches {}, batched-items {}, votes {}, query-block {}, query-token {}, answers {}, blocks {}, referrals {}, query-commit {}, commit-block {}",
            self.scheduled_wire_message_types.total(),
            self.scheduled_wire_message_types.request_batch,
            self.scheduled_wire_message_types.batched_request_items,
            self.scheduled_wire_message_types.vote,
            self.scheduled_wire_message_types.query_block,
            self.scheduled_wire_message_types.query_token,
            self.scheduled_wire_message_types.answer,
            self.scheduled_wire_message_types.block,
            self.scheduled_wire_message_types.referral,
            self.scheduled_wire_message_types.query_commit_block,
            self.scheduled_wire_message_types.commit_block,
        );
        println!(
            "Delivered wire messages by type: total {}, request-batches {}, batched-items {}, votes {}, query-block {}, query-token {}, answers {}, blocks {}, referrals {}, query-commit {}, commit-block {}",
            self.delivered_wire_message_types.total(),
            self.delivered_wire_message_types.request_batch,
            self.delivered_wire_message_types.batched_request_items,
            self.delivered_wire_message_types.vote,
            self.delivered_wire_message_types.query_block,
            self.delivered_wire_message_types.query_token,
            self.delivered_wire_message_types.answer,
            self.delivered_wire_message_types.block,
            self.delivered_wire_message_types.referral,
            self.delivered_wire_message_types.query_commit_block,
            self.delivered_wire_message_types.commit_block,
        );
        println!(
            "Vote ingress: trusted recorded {}, untrusted received {}, block fetches from votes {}",
            self.vote_ingress.trusted_votes_recorded,
            self.vote_ingress.untrusted_votes_received,
            self.vote_ingress.block_requests_triggered_by_votes,
        );
        println!(
            "Neighborhoods: {} token samples, {:.1}% local-entry",
            self.neighborhoods.token_samples,
            if self.neighborhoods.token_samples == 0 {
                0.0
            } else {
                (self.neighborhoods.local_token_samples as f64 * 100.0)
                    / self.neighborhoods.token_samples as f64
            },
        );
        if let Some(coverage) = &self.neighborhoods.coverage_size {
            println!(
                "Neighborhood coverage: avg {:.1} peers, p50 {}, p95 {}, min {}, max {}",
                coverage.avg, coverage.p50, coverage.p95, coverage.min, coverage.max,
            );
        }
        if let Some(eligible) = &self.neighborhoods.vote_eligible_size {
            println!(
                "Vote-eligible set at entry: avg {:.1} peers, p50 {}, p95 {}, min {}, max {}",
                eligible.avg, eligible.p50, eligible.p95, eligible.min, eligible.max,
            );
        }
        if let Some(hops) = &self.neighborhoods.entry_hops {
            println!(
                "Entry distance to token: avg {:.1} hops, p50 {}, p95 {}, min {}, max {}",
                hops.avg, hops.p50, hops.p95, hops.min, hops.max,
            );
        }
        if !self.neighborhoods.buckets.is_empty() {
            println!("Neighborhood buckets:");
            for bucket in &self.neighborhoods.buckets {
                let coverage = bucket
                    .coverage_size
                    .as_ref()
                    .map(|summary| format!("avg {:.1}, p95 {}", summary.avg, summary.p95))
                    .unwrap_or_else(|| "n/a".to_string());
                let eligible = bucket
                    .vote_eligible_size
                    .as_ref()
                    .map(|summary| format!("avg {:.1}, p95 {}", summary.avg, summary.p95))
                    .unwrap_or_else(|| "n/a".to_string());
                let hops = bucket
                    .entry_hops
                    .as_ref()
                    .map(|summary| format!("avg {:.1}, p95 {}", summary.avg, summary.p95))
                    .unwrap_or_else(|| "n/a".to_string());
                let latency = bucket
                    .commit_latency
                    .as_ref()
                    .map(|summary| format!("avg {:.1}, p95 {}", summary.avg, summary.p95))
                    .unwrap_or_else(|| "n/a".to_string());
                println!(
                    "  - {}: {} token samples, {} committed blocks, coverage {}, eligible {}, entry hops {}, commit latency {}",
                    bucket.label,
                    bucket.token_samples,
                    bucket.committed_blocks,
                    coverage,
                    eligible,
                    hops,
                    latency,
                );
            }
        }
        println!(
            "Transaction spread: {} submitted, {} committed blocks analyzed",
            self.transaction_spread.submitted_blocks,
            self.transaction_spread.committed_blocks,
        );
        if let Some(reachable) = &self.transaction_spread.reachable_vote_peers {
            println!(
                "Reachable vote graph: avg {:.1} peers, p50 {}, p95 {}, min {}, max {}",
                reachable.avg, reachable.p50, reachable.p95, reachable.min, reachable.max,
            );
        }
        if let Some(edges) = &self.transaction_spread.reachable_vote_edges {
            println!(
                "Reachable vote graph edges: avg {:.1}, p50 {}, p95 {}, min {}, max {}",
                edges.avg, edges.p50, edges.p95, edges.min, edges.max,
            );
        }
        if let Some(witness) = &self.transaction_spread.witness_coverage {
            println!(
                "Witness neighborhood: avg {:.1} peers, p50 {}, p95 {}, min {}, max {}",
                witness.avg, witness.p50, witness.p95, witness.min, witness.max,
            );
        }
        if let Some(spread) = &self.transaction_spread.settled_peer_spread {
            println!(
                "Settled peer spread: avg {:.1} peers, p50 {}, p95 {}, min {}, max {}",
                spread.avg, spread.p50, spread.p95, spread.min, spread.max,
            );
        }
        if let Some(messages) = &self.transaction_spread.settled_block_messages {
            println!(
                "Block-related messages to settle: avg {:.1}, p50 {}, p95 {}, min {}, max {}",
                messages.avg, messages.p50, messages.p95, messages.min, messages.max,
            );
        }
        if let Some(ideal) = &self.transaction_spread.ideal_role_sum_lower_bound_messages {
            println!(
                "Ideal role-sum lower bound: avg {:.1} block messages, p50 {}, p95 {}, min {}, max {}",
                ideal.avg, ideal.p50, ideal.p95, ideal.min, ideal.max,
            );
        }
        if let Some(ideal) = &self.transaction_spread.ideal_coalesced_lower_bound_messages {
            println!(
                "Ideal coalesced lower bound: avg {:.1} block messages, p50 {}, p95 {}, min {}, max {}",
                ideal.avg, ideal.p50, ideal.p95, ideal.min, ideal.max,
            );
        }
        if let Some(ratio) = &self.transaction_spread.actual_to_role_sum_ratio {
            println!(
                "Actual / role-sum block-message factor: avg {:.2}x, p50 {:.2}x, p95 {:.2}x, min {:.2}x, max {:.2}x",
                ratio.avg, ratio.p50, ratio.p95, ratio.min, ratio.max,
            );
        }
        if let Some(ratio) = &self.transaction_spread.actual_to_coalesced_ratio {
            println!(
                "Actual / coalesced block-message factor: avg {:.2}x, p50 {:.2}x, p95 {:.2}x, min {:.2}x, max {:.2}x",
                ratio.avg, ratio.p50, ratio.p95, ratio.min, ratio.max,
            );
        }
        if self.transaction_spread.total_ideal_role_sum_lower_bound_messages > 0 {
            println!(
                "Total block-message factor vs role-sum ideal: {:.2}x ({} actual / {} ideal)",
                self.transaction_spread.total_actual_block_messages as f64
                    / self.transaction_spread.total_ideal_role_sum_lower_bound_messages as f64,
                self.transaction_spread.total_actual_block_messages,
                self.transaction_spread.total_ideal_role_sum_lower_bound_messages,
            );
        }
        if self.transaction_spread.total_ideal_coalesced_lower_bound_messages > 0 {
            println!(
                "Total block-message factor vs coalesced ideal: {:.2}x ({} actual / {} ideal)",
                self.transaction_spread.total_actual_block_messages as f64
                    / self.transaction_spread.total_ideal_coalesced_lower_bound_messages as f64,
                self.transaction_spread.total_actual_block_messages,
                self.transaction_spread.total_ideal_coalesced_lower_bound_messages,
            );
        }
        println!(
            "Mempool pressure: avg no-block {:.1}, no-trusted-votes {:.1}, wait-validation {:.1}, wait-token-votes {:.1}, wait-witness {:.1}, aged50+ {:.1}, aged200+ {:.1}",
            self.mempool_pressure.avg_pending_without_block,
            self.mempool_pressure.avg_pending_no_trusted_votes,
            self.mempool_pressure.avg_pending_waiting_validation,
            self.mempool_pressure.avg_pending_waiting_token_votes,
            self.mempool_pressure.avg_pending_waiting_witness,
            self.mempool_pressure.avg_pending_age_50_plus,
            self.mempool_pressure.avg_pending_age_200_plus,
        );
        println!(
            "Mempool peaks: no-block {}, no-trusted-votes {}, wait-validation {}, wait-token-votes {}, wait-witness {}, aged50+ {}, aged200+ {}",
            self.mempool_pressure.peak_pending_without_block,
            self.mempool_pressure.peak_pending_no_trusted_votes,
            self.mempool_pressure.peak_pending_waiting_validation,
            self.mempool_pressure.peak_pending_waiting_token_votes,
            self.mempool_pressure.peak_pending_waiting_witness,
            self.mempool_pressure.peak_pending_age_50_plus,
            self.mempool_pressure.peak_pending_age_200_plus,
        );

        if let Some(latency) = &self.commit_latency {
            println!(
                "Commit latency: {} samples, avg {:.1} rounds, p50 {}, p95 {}, min {}, max {}",
                latency.samples,
                latency.avg,
                latency.p50,
                latency.p95,
                latency.min,
                latency.max,
            );
        }

        if let Some(delay) = &self.network_transit_delay {
            println!(
                "Network transit: {} samples, avg {:.1} rounds, p50 {}, p95 {}, min {}, max {}",
                delay.samples,
                delay.avg,
                delay.p50,
                delay.p95,
                delay.min,
                delay.max,
            );
        }

        println!(
            "Late joiners: {} observed ({} bootstrap-seeded)",
            self.late_joiner_onboarding.observed_peers,
            self.late_joiner_onboarding.bootstrap_seeded_peers,
        );

        if let Some(connected) = &self.late_joiner_onboarding.time_to_connected {
            println!(
                "Late-join time to connected: avg {:.1} rounds, p50 {}, p95 {}, max {}",
                connected.avg, connected.p50, connected.p95, connected.max,
            );
        }

        if let Some(head) = &self.late_joiner_onboarding.time_to_known_head {
            println!(
                "Late-join time to first known head: avg {:.1} rounds, p50 {}, p95 {}, max {}",
                head.avg, head.p50, head.p95, head.max,
            );
        }

        if let Some(sync) = &self.late_joiner_onboarding.time_to_sync_trace {
            println!(
                "Late-join time to first sync trace: avg {:.1} rounds, p50 {}, p95 {}, max {}",
                sync.avg, sync.p50, sync.p95, sync.max,
            );
        }

        println!(
            "Late-join connected before head/sync: {}/{}",
            self.late_joiner_onboarding.connected_before_known_head,
            self.late_joiner_onboarding.connected_before_sync_trace,
        );

        println!(
            "Rejoiners: {} observed ({} bootstrap-seeded)",
            self.rejoin_onboarding.observed_peers,
            self.rejoin_onboarding.bootstrap_seeded_peers,
        );

        if let Some(connected) = &self.rejoin_onboarding.time_to_connected {
            println!(
                "Rejoin time to connected: avg {:.1} rounds, p50 {}, p95 {}, max {}",
                connected.avg, connected.p50, connected.p95, connected.max,
            );
        }

        if let Some(head) = &self.rejoin_onboarding.time_to_known_head {
            println!(
                "Rejoin time to first known head: avg {:.1} rounds, p50 {}, p95 {}, max {}",
                head.avg, head.p50, head.p95, head.max,
            );
        }

        if let Some(sync) = &self.rejoin_onboarding.time_to_sync_trace {
            println!(
                "Rejoin time to first sync trace: avg {:.1} rounds, p50 {}, p95 {}, max {}",
                sync.avg, sync.p50, sync.p95, sync.max,
            );
        }

        println!(
            "Rejoin connected before head/sync: {}/{}",
            self.rejoin_onboarding.connected_before_known_head,
            self.rejoin_onboarding.connected_before_sync_trace,
        );

        if !self.recoveries.is_empty() {
            println!("Recovery watches:");
            for recovery in &self.recoveries {
                match recovery.rounds_to_recover() {
                    Some(rounds) => println!(
                        "  - {}: baseline {:.2} commits/round, recovered in {} rounds",
                        recovery.label,
                        recovery.baseline_commit_rate,
                        rounds,
                    ),
                    None => println!(
                        "  - {}: baseline {:.2} commits/round, not recovered during run",
                        recovery.label,
                        recovery.baseline_commit_rate,
                    ),
                }
            }
        }
    }
}
