// Simulation Statistics and Results

/// Complete simulation result
#[derive(Debug, Clone)]
pub struct SimResult {
    pub statistics: SimStatistics,
    pub committed_blocks: usize,
    pub total_messages: usize,
    pub seed_used: [u8; 32],
}

/// Aggregated simulation statistics
#[derive(Debug, Clone)]
pub struct SimStatistics {
    pub message_counts: MessageCounts,
    pub peer_stats: PeerStats,
    pub rounds_per_commit: f64,
    pub messages_per_commit: f64,
}

/// Breakdown of message types
#[derive(Debug, Clone)]
pub struct MessageCounts {
    pub query: usize,
    pub vote: usize,
    pub block: usize,
    pub answer: usize,
}

/// Peer connectivity statistics
#[derive(Debug, Clone)]
pub struct PeerStats {
    pub max_peers: usize,
    pub min_peers: usize,
    pub avg_peers: f64,
}
