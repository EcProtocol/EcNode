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
    pub total_messages_delivered: usize,
    pub commits_this_round: usize,
    pub recent_commit_rate: f64,
    pub skipped_submissions: usize,
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
            "Blocks: {} attempts, {} submitted, {} skipped, {} committed, {} pending",
            self.submission_attempts,
            self.submitted_blocks,
            self.skipped_submissions,
            self.committed_blocks,
            self.pending_blocks
        );
        println!("Messages delivered: {}", self.total_messages_delivered);
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
