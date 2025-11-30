// Peer Lifecycle Simulator Statistics

use ec_rust::ec_interface::PeerId;

// ============================================================================
// Simulation Result
// ============================================================================

/// Complete simulation result
#[derive(Debug, Clone)]
pub struct SimulationResult {
    /// Configuration summary
    pub config_summary: String,

    /// Random seed used
    pub seed_used: [u8; 32],

    /// Total rounds executed
    pub total_rounds: usize,

    /// Final metrics at end of simulation
    pub final_metrics: RoundMetrics,

    /// Historical metrics (sampled at intervals)
    pub metrics_history: Vec<RoundMetrics>,

    /// Event log (what happened and when)
    pub event_log: Vec<EventOutcome>,

    /// Convergence analysis
    pub convergence: ConvergenceAnalysis,

    /// Message overhead statistics
    pub message_overhead: MessageOverhead,
}

// ============================================================================
// Round Metrics
// ============================================================================

/// Metrics collected at a single round
#[derive(Debug, Clone)]
pub struct RoundMetrics {
    /// Round number
    pub round: usize,

    /// Simulated timestamp (ms)
    pub timestamp: u64,

    /// Peer state distribution
    pub peer_counts: PeerCounts,

    /// Election statistics
    pub election_stats: ElectionStats,

    /// Network health indicators
    pub network_health: NetworkHealth,

    /// Quality metrics
    pub quality_metrics: QualityMetrics,
}

/// Peer state counts
#[derive(Debug, Clone)]
pub struct PeerCounts {
    /// Total peers in simulation
    pub total_peers: usize,

    /// Active (online) peers
    pub active_peers: usize,

    /// Peers in Identified state
    pub identified: usize,

    /// Peers in Pending state
    pub pending: usize,

    /// Peers in Connected state
    pub connected: usize,
}

/// Election performance statistics
#[derive(Debug, Clone)]
pub struct ElectionStats {
    /// Elections started this round
    pub elections_started: usize,

    /// Elections completed successfully
    pub elections_completed: usize,

    /// Elections that timed out
    pub elections_timed_out: usize,

    /// Split-brain scenarios detected
    pub split_brain_detected: usize,

    /// Average channels spawned per election
    pub avg_channels_per_election: f64,

    /// Average completion time (rounds)
    pub avg_completion_time: f64,

    /// Cumulative totals
    pub total_elections_started: usize,
    pub total_elections_completed: usize,
    pub total_elections_timed_out: usize,
    pub total_split_brain_detected: usize,
}

/// Network health metrics
#[derive(Debug, Clone)]
pub struct NetworkHealth {
    /// Minimum connected peers (across all active nodes)
    pub min_connected_peers: usize,

    /// Maximum connected peers
    pub max_connected_peers: usize,

    /// Average connected peers
    pub avg_connected_peers: f64,

    /// Standard deviation of connected peer counts
    pub stddev_connected_peers: f64,

    /// Ring coverage percentage (0.0 to 100.0)
    pub ring_coverage_percent: f64,

    /// Network partition detected
    pub partition_detected: bool,
}

/// Quality score metrics
#[derive(Debug, Clone)]
pub struct QualityMetrics {
    /// Minimum quality score
    pub min_quality: f64,

    /// Maximum quality score
    pub max_quality: f64,

    /// Average quality score
    pub avg_quality: f64,

    /// Standard deviation of quality scores
    pub stddev_quality: f64,
}

// ============================================================================
// Event Outcomes
// ============================================================================

/// Log entry for a network event
#[derive(Debug, Clone)]
pub struct EventOutcome {
    /// Round when event occurred
    pub round: usize,

    /// Event type description
    pub event_type: String,

    /// Number of peers affected
    pub peers_affected: usize,

    /// Specific peer IDs (if applicable)
    pub affected_peers: Vec<PeerId>,

    /// Outcome description
    pub outcome: String,
}

// ============================================================================
// Convergence Analysis
// ============================================================================

/// Analysis of network convergence behavior
#[derive(Debug, Clone)]
pub struct ConvergenceAnalysis {
    /// Rounds to reach initial stability
    pub bootstrap_convergence_time: Option<usize>,

    /// Recovery times after churn events (round → recovery_time)
    pub post_churn_recovery_times: Vec<(usize, usize)>,

    /// Target peer count per node
    pub target_peer_count: usize,

    /// Achieved peer count (average across nodes)
    pub achieved_peer_count: usize,

    /// Convergence achieved
    pub converged: bool,
}

// ============================================================================
// Message Overhead
// ============================================================================

/// Message overhead statistics
#[derive(Debug, Clone)]
pub struct MessageOverhead {
    /// Total messages sent
    pub total_messages: usize,

    /// Query messages sent
    pub queries_sent: usize,

    /// Answer messages received
    pub answers_received: usize,

    /// Invitation messages sent
    pub invitations_sent: usize,

    /// Referral messages sent
    pub referrals_sent: usize,

    /// Average messages per peer per round
    pub messages_per_peer_per_round: f64,

    /// Average messages per election
    pub messages_per_election: f64,
}

// ============================================================================
// Helper Implementations
// ============================================================================

impl RoundMetrics {
    /// Create initial empty metrics
    pub fn new(round: usize, timestamp: u64) -> Self {
        Self {
            round,
            timestamp,
            peer_counts: PeerCounts {
                total_peers: 0,
                active_peers: 0,
                identified: 0,
                pending: 0,
                connected: 0,
            },
            election_stats: ElectionStats {
                elections_started: 0,
                elections_completed: 0,
                elections_timed_out: 0,
                split_brain_detected: 0,
                avg_channels_per_election: 0.0,
                avg_completion_time: 0.0,
                total_elections_started: 0,
                total_elections_completed: 0,
                total_elections_timed_out: 0,
                total_split_brain_detected: 0,
            },
            network_health: NetworkHealth {
                min_connected_peers: 0,
                max_connected_peers: 0,
                avg_connected_peers: 0.0,
                stddev_connected_peers: 0.0,
                ring_coverage_percent: 0.0,
                partition_detected: false,
            },
            quality_metrics: QualityMetrics {
                min_quality: 0.0,
                max_quality: 0.0,
                avg_quality: 0.0,
                stddev_quality: 0.0,
            },
        }
    }
}

impl SimulationResult {
    /// Print summary to console
    pub fn print_summary(&self) {
        println!("\n╔════════════════════════════════════════════════════════╗");
        println!("║    PEER LIFECYCLE SIMULATION RESULTS                   ║");
        println!("╚════════════════════════════════════════════════════════╝\n");

        println!("Configuration: {}", self.config_summary);
        println!("Rounds: {}", self.total_rounds);
        println!();

        // Final metrics
        let metrics = &self.final_metrics;
        println!("═══ Final State ═══");
        println!("  Peers: {} total, {} active",
            metrics.peer_counts.total_peers,
            metrics.peer_counts.active_peers);
        println!("  States: {} Identified, {} Pending, {} Connected",
            metrics.peer_counts.identified,
            metrics.peer_counts.pending,
            metrics.peer_counts.connected);
        println!();

        // Election stats
        println!("═══ Election Performance ═══");
        println!("  Total Started: {}", metrics.election_stats.total_elections_started);
        println!("  Completed: {}", metrics.election_stats.total_elections_completed);
        println!("  Timed Out: {}", metrics.election_stats.total_elections_timed_out);
        println!("  Split-Brain: {}", metrics.election_stats.total_split_brain_detected);
        if metrics.election_stats.total_elections_started > 0 {
            let success_rate = (metrics.election_stats.total_elections_completed as f64
                / metrics.election_stats.total_elections_started as f64) * 100.0;
            println!("  Success Rate: {:.1}%", success_rate);
        }
        println!();

        // Network health
        println!("═══ Network Health ═══");
        println!("  Connected Peers: min={}, max={}, avg={:.1}",
            metrics.network_health.min_connected_peers,
            metrics.network_health.max_connected_peers,
            metrics.network_health.avg_connected_peers);
        println!("  Ring Coverage: {:.1}%", metrics.network_health.ring_coverage_percent);
        println!();

        // Message overhead
        println!("═══ Message Overhead ═══");
        println!("  Total Messages: {}", self.message_overhead.total_messages);
        println!("  Queries: {}", self.message_overhead.queries_sent);
        println!("  Answers: {}", self.message_overhead.answers_received);
        println!("  Referrals: {}", self.message_overhead.referrals_sent);
        println!("  Per Peer/Round: {:.2}", self.message_overhead.messages_per_peer_per_round);
        println!();

        // Convergence
        if self.convergence.converged {
            println!("═══ Convergence ═══");
            if let Some(bootstrap_time) = self.convergence.bootstrap_convergence_time {
                println!("  Bootstrap Time: {} rounds", bootstrap_time);
            }
            println!("  Target Peers: {}", self.convergence.target_peer_count);
            println!("  Achieved Peers: {}", self.convergence.achieved_peer_count);
            println!();
        }
    }
}
