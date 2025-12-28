// Peer Lifecycle Simulator Statistics

use ec_rust::ec_interface::PeerId;
use std::collections::BTreeMap;

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

    /// Connected peer count distribution by quantile
    pub connected_peer_distribution: Option<ConnectedPeerDistribution>,

    /// Gradient steepness distribution (connectivity shape quality)
    pub gradient_distribution: Option<GradientSteepnessDistribution>,
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
// Connected Peer Count Distribution
// ============================================================================

/// Distribution of connected peer counts across nodes
#[derive(Debug, Clone)]
pub struct ConnectedPeerDistribution {
    /// Number of quantiles (typically 4 for quartiles)
    pub num_quantiles: usize,

    /// Quantile boundaries
    pub quantile_boundaries: Vec<usize>,

    /// Number of peers in each quantile bucket
    pub peer_counts_by_quantile: Vec<usize>,

    /// Average connected peer count in each quantile
    pub avg_connected_by_quantile: Vec<f64>,

    /// Connected peer count range for each quantile [min, max]
    pub connected_ranges: Vec<(usize, usize)>,

    /// Overall statistics
    pub min_connected: usize,
    pub max_connected: usize,
    pub avg_connected: f64,
    pub median_connected: f64,
}

impl Default for ConnectedPeerDistribution {
    fn default() -> Self {
        Self {
            num_quantiles: 4,
            quantile_boundaries: vec![],
            peer_counts_by_quantile: vec![],
            avg_connected_by_quantile: vec![],
            connected_ranges: vec![],
            min_connected: 0,
            max_connected: 0,
            avg_connected: 0.0,
            median_connected: 0.0,
        }
    }
}

// ============================================================================
// Locality Gradient Analysis
// ============================================================================

/// Quantile distribution of locality gradient values
/// Measures how well connected peers cluster near the node (locality gradient)
#[derive(Debug, Clone)]
pub struct GradientSteepnessDistribution {
    /// Number of quantiles (typically 4 for quartiles)
    pub num_quantiles: usize,

    /// Quantile boundaries (e.g., [0.25, 0.5, 0.75, 1.0] for quartiles)
    pub quantile_boundaries: Vec<f64>,

    /// Peer count in each quantile bucket
    /// Index 0: peers with locality in [min, boundary[0]]
    /// Index i: peers with locality in (boundary[i-1], boundary[i]]
    pub peer_counts_by_quantile: Vec<usize>,

    /// Average locality coefficient in each quantile bucket
    pub avg_steepness_by_quantile: Vec<f64>,

    /// Locality coefficient range for each quantile [min, max]
    pub steepness_ranges: Vec<(f64, f64)>,

    /// Overall statistics
    /// Locality coefficient ranges from 0.0 (poor - peers far away) to 1.0 (perfect - peers very close)
    pub min_steepness: f64,
    pub max_steepness: f64,
    pub avg_steepness: f64,
    pub median_steepness: f64,

    /// Quality assessment: percentage of peers with strong locality (>= 0.7)
    pub near_ideal_percent: f64,
}

impl Default for GradientSteepnessDistribution {
    fn default() -> Self {
        Self {
            num_quantiles: 4,
            quantile_boundaries: vec![],
            peer_counts_by_quantile: vec![],
            avg_steepness_by_quantile: vec![],
            steepness_ranges: vec![],
            min_steepness: 0.0,
            max_steepness: 0.0,
            avg_steepness: 0.0,
            median_steepness: 0.0,
            near_ideal_percent: 0.0,
        }
    }
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
                connected_peer_distribution: None,
                gradient_distribution: None,
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

        // Connected Peer Count Distribution
        if let Some(ref dist) = self.final_metrics.network_health.connected_peer_distribution {
            println!("═══ Connected Peer Count Distribution ═══");
            println!("  Overall: min={}, max={}, avg={:.1}, median={:.0}",
                dist.min_connected, dist.max_connected,
                dist.avg_connected, dist.median_connected);
            println!("\n  Distribution by Quartile:");
            for (i, &count) in dist.peer_counts_by_quantile.iter().enumerate() {
                if count > 0 {
                    let (min, max) = dist.connected_ranges[i];
                    let avg = dist.avg_connected_by_quantile[i];
                    println!("    Q{}: {:4} peers, connected [{:2}-{:2}], avg={:.1}",
                        i + 1, count, min, max, avg);
                } else {
                    println!("    Q{}: {:4} peers (empty)", i + 1, 0);
                }
            }
            println!();
        }

        // Locality Gradient Distribution
        if let Some(ref gradient) = self.final_metrics.network_health.gradient_distribution {
            println!("═══ Locality Gradient Quality ═══");
            println!("  Overall Locality: min={:.3}, max={:.3}, avg={:.3}, median={:.3}",
                gradient.min_steepness, gradient.max_steepness,
                gradient.avg_steepness, gradient.median_steepness);
            println!("  Strong Locality (≥ 0.7): {:.1}%", gradient.near_ideal_percent);
            println!("\n  Distribution by Quartile:");
            println!("  (1.0 = perfect, peers very close; 0.0 = poor, peers far away)");
            for (i, &count) in gradient.peer_counts_by_quantile.iter().enumerate() {
                if count > 0 {
                    let (min, max) = gradient.steepness_ranges[i];
                    let avg = gradient.avg_steepness_by_quantile[i];
                    println!("    Q{}: {:4} peers, locality [{:.3}-{:.3}], avg={:.3}",
                        i + 1, count, min, max, avg);
                } else {
                    println!("    Q{}: {:4} peers (empty)", i + 1, 0);
                }
            }
            println!();
        }
    }
}

// ============================================================================
// Connected Peer Distribution Calculation
// ============================================================================

/// Calculate connected peer count distribution across all peers
pub fn calculate_connected_peer_distribution(
    connected_counts: &[usize],
    num_quantiles: usize,
) -> ConnectedPeerDistribution {
    if connected_counts.is_empty() {
        return ConnectedPeerDistribution::default();
    }

    let mut counts = connected_counts.to_vec();
    counts.sort();

    let num_peers = counts.len();
    let min_connected = counts[0];
    let max_connected = counts[num_peers - 1];
    let avg_connected = counts.iter().sum::<usize>() as f64 / num_peers as f64;
    let median_connected = if num_peers % 2 == 0 {
        (counts[num_peers / 2 - 1] + counts[num_peers / 2]) as f64 / 2.0
    } else {
        counts[num_peers / 2] as f64
    };

    // Calculate quantile boundaries
    let mut quantile_boundaries = Vec::new();
    for i in 1..=num_quantiles {
        let percentile = i as f64 / num_quantiles as f64;
        let index = ((num_peers as f64 * percentile).ceil() as usize).min(num_peers) - 1;
        quantile_boundaries.push(counts[index]);
    }

    // Group peers into quantile buckets
    let mut peer_counts_by_quantile = vec![0; num_quantiles];
    let mut sum_by_quantile = vec![0; num_quantiles];
    let mut connected_ranges = vec![(None, None); num_quantiles];

    for &conn_count in &counts {
        // Find which quantile this value belongs to
        let mut quantile_idx = 0;
        for (i, &boundary) in quantile_boundaries.iter().enumerate() {
            if conn_count <= boundary {
                quantile_idx = i;
                break;
            }
        }

        peer_counts_by_quantile[quantile_idx] += 1;
        sum_by_quantile[quantile_idx] += conn_count;

        // Update range for this quantile
        let (ref mut min_opt, ref mut max_opt) = &mut connected_ranges[quantile_idx];
        *min_opt = Some(min_opt.map_or(conn_count, |min: usize| min.min(conn_count)));
        *max_opt = Some(max_opt.map_or(conn_count, |max: usize| max.max(conn_count)));
    }

    // Convert Option ranges to (usize, usize) with proper defaults
    let final_ranges: Vec<(usize, usize)> = connected_ranges
        .iter()
        .map(|(min_opt, max_opt)| {
            (min_opt.unwrap_or(0), max_opt.unwrap_or(0))
        })
        .collect();

    // Calculate average connected per quantile
    let avg_connected_by_quantile: Vec<f64> = peer_counts_by_quantile
        .iter()
        .zip(sum_by_quantile.iter())
        .map(|(&count, &sum)| if count > 0 { sum as f64 / count as f64 } else { 0.0 })
        .collect();

    ConnectedPeerDistribution {
        num_quantiles,
        quantile_boundaries,
        peer_counts_by_quantile,
        avg_connected_by_quantile,
        connected_ranges: final_ranges,
        min_connected,
        max_connected,
        avg_connected,
        median_connected,
    }
}

// ============================================================================
// Gradient Steepness Calculation Functions
// ============================================================================

/// Calculate ring distance between two peer IDs (shortest path on ring)
fn ring_distance(a: PeerId, b: PeerId) -> u64 {
    let forward = b.wrapping_sub(a);
    let backward = a.wrapping_sub(b);
    forward.min(backward)
}

/// Calculate locality gradient coefficient for a single peer
/// Measures how close the connected peers are to the node's peer ID
///
/// Returns a locality coefficient from 0.0 to 1.0:
/// - 1.0 = perfect locality (all connected peers very close to node)
/// - 0.5 = moderate locality (connected peers at medium distance)
/// - 0.0 = poor locality (connected peers at opposite side of ring)
pub fn calculate_gradient_steepness(peer_id: PeerId, active_peers: &[PeerId]) -> f64 {
    if active_peers.is_empty() {
        return 1.0; // No peers = assume perfect (neutral)
    }

    // Calculate ring distances from this peer to all connected peers
    let mut distances = Vec::new();
    for &connected_peer in active_peers {
        let distance = ring_distance(peer_id, connected_peer);
        distances.push(distance as f64);
    }

    // Calculate average distance from node to connected peers
    let avg_distance = distances.iter().sum::<f64>() / distances.len() as f64;

    // Maximum possible distance on the ring (half the ring)
    let max_distance = u64::MAX as f64 / 2.0;

    // Locality coefficient: 1.0 - (avg_distance / max_distance)
    // - If avg_distance = 0 (all peers at same location): locality = 1.0
    // - If avg_distance = max_distance (peers at opposite side): locality = 0.0
    let locality = 1.0 - (avg_distance / max_distance);

    // Clamp to [0.0, 1.0] range (should be automatic, but be safe)
    locality.max(0.0).min(1.0)
}

/// Calculate locality gradient distribution across all peers
/// Groups peers into quantiles based on their locality coefficient values
pub fn calculate_gradient_distribution(
    peer_steepness_map: &BTreeMap<PeerId, f64>,
    num_quantiles: usize,
) -> GradientSteepnessDistribution {
    if peer_steepness_map.is_empty() {
        return GradientSteepnessDistribution::default();
    }

    // Collect all locality values
    let mut locality_values: Vec<f64> = peer_steepness_map.values().copied().collect();
    locality_values.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let count = locality_values.len();
    let min_locality = locality_values[0];
    let max_locality = locality_values[count - 1];
    let avg_locality = locality_values.iter().sum::<f64>() / count as f64;
    let median_locality = if count % 2 == 0 {
        (locality_values[count / 2 - 1] + locality_values[count / 2]) / 2.0
    } else {
        locality_values[count / 2]
    };

    // Calculate quantile boundaries (e.g., for quartiles: 25th, 50th, 75th percentiles)
    let mut quantile_boundaries = Vec::new();
    for i in 1..=num_quantiles {
        let percentile = i as f64 / num_quantiles as f64;
        let index = ((count as f64 * percentile).ceil() as usize).min(count) - 1;
        quantile_boundaries.push(locality_values[index]);
    }

    // Group peers into quantile buckets
    let mut peer_counts_by_quantile = vec![0; num_quantiles];
    let mut sum_by_quantile = vec![0.0; num_quantiles];
    let mut locality_ranges = vec![(None, None); num_quantiles];

    for &locality in &locality_values {
        // Find which quantile this value belongs to
        let mut quantile_idx = 0;
        for (i, &boundary) in quantile_boundaries.iter().enumerate() {
            if locality <= boundary {
                quantile_idx = i;
                break;
            }
        }

        peer_counts_by_quantile[quantile_idx] += 1;
        sum_by_quantile[quantile_idx] += locality;

        // Update range for this quantile
        let (ref mut min_opt, ref mut max_opt) = &mut locality_ranges[quantile_idx];
        *min_opt = Some(min_opt.map_or(locality, |min: f64| min.min(locality)));
        *max_opt = Some(max_opt.map_or(locality, |max: f64| max.max(locality)));
    }

    // Convert Option ranges to (f64, f64) with proper defaults
    let steepness_ranges: Vec<(f64, f64)> = locality_ranges
        .iter()
        .map(|(min_opt, max_opt)| {
            (min_opt.unwrap_or(0.0), max_opt.unwrap_or(0.0))
        })
        .collect();

    // Calculate average locality per quantile
    let avg_locality_by_quantile: Vec<f64> = peer_counts_by_quantile
        .iter()
        .zip(sum_by_quantile.iter())
        .map(|(&count, &sum)| if count > 0 { sum / count as f64 } else { 0.0 })
        .collect();

    // Calculate strong-locality percentage (>= 0.7 is considered good)
    let strong_locality_count = locality_values
        .iter()
        .filter(|&&l| l >= 0.7)
        .count();
    let strong_locality_percent = (strong_locality_count as f64 / count as f64) * 100.0;

    GradientSteepnessDistribution {
        num_quantiles,
        quantile_boundaries,
        peer_counts_by_quantile,
        avg_steepness_by_quantile: avg_locality_by_quantile,
        steepness_ranges,
        min_steepness: min_locality,
        max_steepness: max_locality,
        avg_steepness: avg_locality,
        median_steepness: median_locality,
        near_ideal_percent: strong_locality_percent,
    }
}
