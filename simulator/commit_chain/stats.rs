//! Statistics and results for commit chain simulator

use ec_rust::ec_interface::{EcTime, PeerId};
use std::collections::BTreeMap;

/// Simulation result
#[derive(Debug)]
pub struct SimResult {
    /// Seed used for the simulation
    pub seed_used: [u8; 32],

    /// Number of rounds completed
    pub rounds_completed: usize,

    /// Commit statistics
    pub commit_stats: CommitStats,

    /// Synchronization statistics
    pub sync_stats: SyncStats,

    /// Message statistics
    pub message_stats: MessageStats,
}

impl SimResult {
    /// Print a summary of the simulation results
    pub fn print_summary(&self) {
        println!("\n╔════════════════════════════════════════════════════════╗");
        println!("║        Commit Chain Simulation Results                ║");
        println!("╚════════════════════════════════════════════════════════╝\n");

        println!("Configuration:");
        println!("  Seed: {:?}", self.seed_used);
        println!("  Rounds: {}\n", self.rounds_completed);

        println!("Commit Statistics:");
        println!("  Total commits: {}", self.commit_stats.total_commits);
        println!(
            "  Commits per peer: min={}, max={}, avg={:.1}",
            self.commit_stats.commits_per_peer.0,
            self.commit_stats.commits_per_peer.1,
            self.commit_stats.commits_per_peer.2
        );
        println!();

        println!("Synchronization Statistics:");
        println!("  Blocks synced: {}", self.sync_stats.blocks_synced);
        if !self.sync_stats.final_watermarks.is_empty() {
            let watermarks: Vec<_> = self.sync_stats.final_watermarks.values().collect();
            let min_watermark = watermarks.iter().min().unwrap();
            let max_watermark = watermarks.iter().max().unwrap();
            let avg_watermark: f64 =
                watermarks.iter().map(|&&w| w as f64).sum::<f64>() / watermarks.len() as f64;
            println!(
                "  Watermarks: min={}, max={}, avg={:.1}",
                min_watermark, max_watermark, avg_watermark
            );
        }
        if let Some(&max_traces) = self.sync_stats.active_traces.iter().max() {
            let avg_traces: f64 =
                self.sync_stats.active_traces.iter().sum::<usize>() as f64
                    / self.sync_stats.active_traces.len() as f64;
            println!(
                "  Active traces: max={}, avg={:.1}",
                max_traces, avg_traces
            );
        }
        println!();

        println!("Message Statistics:");
        println!("  Total messages: {}", self.message_stats.total_messages);
        println!(
            "  QueryCommitBlock: {}",
            self.message_stats.query_commit_block
        );
        println!(
            "  CommitBlock responses: {}",
            self.message_stats.commit_block_response
        );
        println!("  QueryBlock: {}", self.message_stats.query_block);
        println!("  Block responses: {}", self.message_stats.block_response);
        println!();
    }
}

/// Commit statistics
#[derive(Debug, Default)]
pub struct CommitStats {
    /// Total number of commits across all peers
    pub total_commits: usize,

    /// Commits per peer (min, max, average)
    pub commits_per_peer: (usize, usize, f64),
}

/// Synchronization statistics
#[derive(Debug, Default)]
pub struct SyncStats {
    /// Final watermark per peer
    pub final_watermarks: BTreeMap<PeerId, EcTime>,

    /// Active traces count per round
    pub active_traces: Vec<usize>,

    /// Total blocks synced
    pub blocks_synced: usize,
}

/// Message statistics
#[derive(Debug, Default)]
pub struct MessageStats {
    /// Total message count
    pub total_messages: usize,

    /// QueryCommitBlock messages
    pub query_commit_block: usize,

    /// CommitBlock response messages
    pub commit_block_response: usize,

    /// QueryBlock messages
    pub query_block: usize,

    /// Block response messages
    pub block_response: usize,
}

/// Message counters (internal tracking)
#[derive(Debug, Default, Clone)]
pub struct MessageCounts {
    pub query_commit_block: usize,
    pub commit_block: usize,
    pub query_block: usize,
    pub block: usize,
}
