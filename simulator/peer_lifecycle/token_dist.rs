// Token Distribution for Peer Lifecycle Simulator
//
// Creates a global token mapping and provides views to individual peers.
// Each peer gets a "view" based on their position on the ring and quality parameters.

use ec_rust::ec_interface::{BlockId, PeerId, TokenId};
use rand::rngs::StdRng;
use rand::Rng;
use std::collections::HashMap;

/// Configuration for token distribution
#[derive(Debug, Clone)]
pub struct TokenDistributionConfig {
    /// Total number of tokens in the global mapping (excluding peer IDs)
    pub total_tokens: usize,

    /// How many neighbors on each side should peers overlap with (±neighbors)
    /// This determines view_width to ensure sufficient overlap for elections
    pub neighbor_overlap: usize,

    /// Fraction of tokens within view_width that peer knows (0.0-1.0)
    /// This is the "quality" parameter
    pub coverage_fraction: f64,
}

impl Default for TokenDistributionConfig {
    fn default() -> Self {
        Self {
            total_tokens: 10_000,
            neighbor_overlap: 5,  // Overlap with 5 neighbors on each side
            coverage_fraction: 0.8,  // Know 80% of nearby tokens
        }
    }
}

/// Global token mapping (the canonical truth)
pub struct GlobalTokenMapping {
    mappings: HashMap<TokenId, BlockId>,
    peer_ids: Vec<PeerId>,
    rng: StdRng,
}

impl GlobalTokenMapping {
    /// Create a new global token mapping with peer IDs and random tokens
    ///
    /// Peer IDs are injected as tokens first (for peer discovery), then random
    /// tokens are added to reach total_tokens count.
    pub fn new(mut rng: StdRng, peer_ids: Vec<PeerId>, total_tokens: usize) -> Self {
        let mut mappings = HashMap::with_capacity(total_tokens + peer_ids.len());

        // First: Add all peer IDs as tokens with RANDOM block IDs
        // (NOT block 0, because proof-of-storage needs real blocks with signature tokens)
        for &peer_id in &peer_ids {
            let block: BlockId = rng.gen();
            mappings.insert(peer_id, block);
        }

        // Second: Generate additional random token→block mappings
        for _ in 0..total_tokens {
            let token: TokenId = rng.gen();
            let block: BlockId = rng.gen();
            mappings.insert(token, block);
        }

        Self { mappings, peer_ids, rng }
    }

    /// Calculate view_width needed for neighbors to overlap
    ///
    /// Given N peers uniformly distributed on ring and desired overlap of K neighbors,
    /// calculate the width that ensures each peer's view includes K neighbors on each side.
    pub fn calculate_view_width(num_peers: usize, neighbor_overlap: usize) -> u64 {
        if num_peers <= 1 {
            return u64::MAX / 2; // Single peer sees everything
        }

        // Average distance between peers on ring
        let ring_size = u64::MAX;
        let avg_peer_distance = ring_size / num_peers as u64;

        // Width should cover neighbor_overlap peers on each side
        // Add 20% margin to ensure coverage despite uniform distribution variance
        // Use saturating_mul to prevent overflow
        let base_width = avg_peer_distance.saturating_mul(neighbor_overlap as u64);
        let width = base_width.saturating_mul(12) / 10;

        width.min(u64::MAX / 2) // Cap at half the ring
    }

    /// Get a view of tokens for a specific peer as a HashMap
    ///
    /// Returns tokens within ±view_width of peer_id, with coverage_fraction sampling.
    /// The peer's own ID is always included (for discovery).
    pub fn get_peer_view(
        &mut self,
        peer_id: PeerId,
        view_width: u64,
        coverage_fraction: f64,
    ) -> HashMap<TokenId, BlockId> {
        let mut view = HashMap::new();

        // IMPORTANT: Add peer_id itself as token (for peer discovery)
        // Every peer knows their own ID mapping (with whatever block it maps to)
        if let Some(&block) = self.mappings.get(&peer_id) {
            view.insert(peer_id, block);
        }

        // DEBUG
        let added_self = view.contains_key(&peer_id);
        if cfg!(debug_assertions) && !added_self {
            eprintln!("[TOKEN_DIST] WARNING: Failed to add peer's own ID {:016x}", peer_id);
        }

        // Filter tokens within range and sample by coverage fraction
        for (&token, &block) in &self.mappings {
            if token == peer_id {
                continue; // Already added above
            }

            if self.is_in_range(peer_id, token, view_width) {
                // Probabilistically include based on coverage fraction
                if self.rng.gen_bool(coverage_fraction) {
                    view.insert(token, block);
                }
            }
        }

        // DEBUG: Check if peer's own ID is still there
        if cfg!(debug_assertions) && !view.contains_key(&peer_id) {
            eprintln!("[TOKEN_DIST] ERROR: Peer {:016x}'s own ID was removed from view! view.len()={}",
                peer_id, view.len());
        }

        view
    }

    /// Get list of peer IDs that should be known by this peer
    ///
    /// Returns peer IDs within ±view_width, useful for initializing topology.
    /// Separate from token view to allow different knowledge vs connectivity parameters.
    pub fn get_nearby_peers(&self, peer_id: PeerId, view_width: u64) -> Vec<PeerId> {
        self.peer_ids
            .iter()
            .filter(|&&other_id| {
                other_id != peer_id && self.is_in_range(peer_id, other_id, view_width)
            })
            .copied()
            .collect()
    }

    /// Check if token is within ±view_width of peer_id on the ring
    fn is_in_range(&self, peer_id: PeerId, token: TokenId, view_width: u64) -> bool {
        let distance = ring_distance(peer_id, token);
        distance <= view_width
    }

    /// Get total number of tokens
    pub fn total_tokens(&self) -> usize {
        self.mappings.len()
    }
}

/// Calculate ring distance (shortest path on circular ID space)
fn ring_distance(a: u64, b: u64) -> u64 {
    let forward = b.wrapping_sub(a);
    let backward = a.wrapping_sub(b);
    forward.min(backward)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn test_ring_distance() {
        assert_eq!(ring_distance(0, 100), 100);
        assert_eq!(ring_distance(100, 0), 100);
        assert_eq!(ring_distance(0, 0), 0);

        // Wrapping
        let near_max = u64::MAX - 100;
        assert_eq!(ring_distance(0, near_max), 101);
    }

    #[test]
    fn test_global_mapping_creation() {
        let rng = StdRng::seed_from_u64(42);
        let peer_ids = vec![100, 200, 300];
        let mapping = GlobalTokenMapping::new(rng, peer_ids.clone(), 1000);
        // Should have 1000 random tokens + 3 peer IDs
        assert_eq!(mapping.total_tokens(), 1003);
    }

    #[test]
    fn test_peer_view_includes_peer_id() {
        let rng = StdRng::seed_from_u64(42);
        let peer_id = 12345;
        let peer_ids = vec![peer_id, 50000, 100000];
        let mut mapping = GlobalTokenMapping::new(rng, peer_ids, 100);

        let view = mapping.get_peer_view(peer_id, 10000, 1.0);

        // Peer should always know their own ID
        assert!(view.contains_key(&peer_id));
        assert_eq!(view[&peer_id], 0);
    }

    #[test]
    fn test_coverage_fraction() {
        let rng = StdRng::seed_from_u64(42);
        let peer_id = 0;
        let peer_ids = vec![peer_id];
        let mut mapping = GlobalTokenMapping::new(rng, peer_ids, 10000);

        // Full coverage
        let full_view = mapping.get_peer_view(peer_id, u64::MAX / 2, 1.0);
        let full_count = full_view.len();

        // Half coverage (probabilistic, so approximate)
        let half_view = mapping.get_peer_view(peer_id, u64::MAX / 2, 0.5);
        let half_count = half_view.len();

        // Half coverage should have roughly half the tokens (with some variance)
        assert!(half_count < full_count);
        let half_f64 = half_count as f64;
        let full_f64 = full_count as f64;
        assert!(half_f64 > full_f64 * 0.3); // At least 30%
        assert!(half_f64 < full_f64 * 0.7); // At most 70%
    }

    #[test]
    fn test_view_width_calculation() {
        // With 10 peers and 2 neighbor overlap, should cover ~20% of ring on each side
        let width = GlobalTokenMapping::calculate_view_width(10, 2);
        let expected = (u64::MAX / 10) * 2 * 12 / 10; // avg_distance * overlap * 1.2
        assert_eq!(width, expected);

        // With 100 peers and 5 neighbor overlap
        let width = GlobalTokenMapping::calculate_view_width(100, 5);
        let expected = (u64::MAX / 100) * 5 * 12 / 10;
        assert_eq!(width, expected);
    }

    #[test]
    fn test_nearby_peers() {
        let rng = StdRng::seed_from_u64(42);
        let peer_ids: Vec<PeerId> = vec![100, 500, 1000, 5000, 10000];
        let mapping = GlobalTokenMapping::new(rng, peer_ids.clone(), 100);

        // Get peers near 1000
        let width = 2000; // Should include 100, 500, and exclude far ones
        let nearby = mapping.get_nearby_peers(1000, width);

        // Should include self's neighbors but not self
        assert!(!nearby.contains(&1000)); // Not self
        assert!(nearby.contains(&100));   // Close enough
        assert!(nearby.contains(&500));   // Close enough
        // 5000 and 10000 should be excluded (too far)
        assert!(!nearby.contains(&5000));
        assert!(!nearby.contains(&10000));
    }
}
