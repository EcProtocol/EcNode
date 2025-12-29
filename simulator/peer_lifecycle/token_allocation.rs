// Token Allocation and Distribution for Peer Lifecycle Simulator
//
// Manages the global token space and allocates peer IDs from that space.
// Provides token views to individual peers based on their position and quality parameters.
//
// Peer IDs are allocated from the token pool, ensuring all peer IDs are valid,
// discoverable tokens that can be found through proof-of-storage elections.

use ec_rust::ec_interface::{BlockId, PeerId, TokenId};
use ec_rust::ec_memory_backend::MemTokens;
use ec_rust::ec_proof_of_storage::{ring_distance, TokenStorageBackend};
use rand::rngs::StdRng;
use rand::Rng;
use std::collections::{HashMap, HashSet};

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

/// Global token mapping and peer ID allocator
///
/// Manages the global token space and allocates peer IDs from that space.
/// Peer IDs are tokens - this ensures all peer IDs are discoverable via elections.
pub struct GlobalTokenMapping {
    /// All token→block mappings (includes both regular tokens and peer ID tokens)
    mappings: HashMap<TokenId, BlockId>,

    /// Set of token IDs that have been allocated as peer IDs
    allocated_peer_ids: HashSet<PeerId>,

    /// Random number generator for token selection and sampling
    rng: StdRng,
}

impl GlobalTokenMapping {
    /// Create a new global token mapping
    ///
    /// Generates a pool of random tokens. Peer IDs will be allocated from this pool
    /// on-demand using allocate_peer_id().
    pub fn new(mut rng: StdRng, total_tokens: usize) -> Self {
        let mut mappings = HashMap::with_capacity(total_tokens);

        // Generate random token→block mappings
        // Peer IDs will be allocated from this pool later
        for _ in 0..total_tokens {
            let token: TokenId = rng.gen();
            let block: BlockId = rng.gen();
            mappings.insert(token, block);
        }

        Self {
            mappings,
            allocated_peer_ids: HashSet::new(),
            rng,
        }
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

    /// Get a view of tokens for a specific peer as MemTokens
    ///
    /// Returns tokens within ±view_width of peer_id, with coverage_fraction sampling.
    /// The peer's own ID is always included (for discovery).
    ///
    /// Returns a ready-to-use MemTokens instance optimized for signature searches.
    pub fn get_peer_view(
        &mut self,
        peer_id: PeerId,
        view_width: u64,
        coverage_fraction: f64,
    ) -> MemTokens {
        let mut mappings = Vec::new();

        use ec_rust::ec_interface::GENESIS_BLOCK_ID;

        // IMPORTANT: Add peer_id itself as token (for peer discovery)
        // Every peer knows their own ID mapping (with whatever block it maps to)
        if let Some(&block) = self.mappings.get(&peer_id) {
            mappings.push((peer_id, block, GENESIS_BLOCK_ID, 0)); // parent=GENESIS for initial allocation, time=0
        } else if cfg!(debug_assertions) {
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
                    mappings.push((token, block, GENESIS_BLOCK_ID, 0)); // parent=GENESIS for initial allocation, time=0
                }
            }
        }

        // Create MemTokens (will sort internally for fast searches)
        MemTokens::from_mappings(mappings)
    }

    /// Get list of peer IDs that should be known by this peer
    ///
    /// Returns peer IDs within ±view_width, useful for initializing topology.
    /// Separate from token view to allow different knowledge vs connectivity parameters.
    pub fn get_nearby_peers(&self, peer_id: PeerId, view_width: u64) -> Vec<PeerId> {
        self.allocated_peer_ids
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

    /// Allocate a new peer ID from the existing token pool
    ///
    /// Efficiently picks a random token that is not already used as a peer ID.
    /// Uses retry strategy: pick random token, check if allocated, retry if needed.
    /// This is O(1) expected time when tokens >> peer_ids (e.g., 10000 tokens, 50 peers).
    /// Returns None if all tokens are allocated.
    pub fn allocate_peer_id(&mut self) -> Option<PeerId> {
        // Check if we've exhausted the token pool
        if self.allocated_peer_ids.len() >= self.mappings.len() {
            return None;
        }

        // Convert keys to Vec for random selection (only once)
        let all_tokens: Vec<TokenId> = self.mappings.keys().copied().collect();

        // Pick random tokens until we find one not already allocated
        // With tokens >> peer_ids, expected number of iterations is very small
        use rand::seq::SliceRandom;
        const MAX_RETRIES: usize = 1000;

        for _ in 0..MAX_RETRIES {
            let peer_id = *all_tokens.choose(&mut self.rng)?;

            // Check if this token is already allocated as a peer ID
            if !self.allocated_peer_ids.contains(&peer_id) {
                // Found an available token - allocate it
                self.allocated_peer_ids.insert(peer_id);
                return Some(peer_id);
            }
        }

        // Fallback: Should never reach here with reasonable token/peer ratio
        // If we do, fall back to exhaustive search
        for &token in &all_tokens {
            if !self.allocated_peer_ids.contains(&token) {
                self.allocated_peer_ids.insert(token);
                return Some(token);
            }
        }

        None
    }

    /// Get all currently allocated peer IDs
    pub fn allocated_peer_ids(&self) -> &HashSet<PeerId> {
        &self.allocated_peer_ids
    }

    /// Get count of allocated peer IDs
    pub fn peer_count(&self) -> usize {
        self.allocated_peer_ids.len()
    }
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
        let mapping = GlobalTokenMapping::new(rng, 1000);
        // Should have 1000 random tokens
        assert_eq!(mapping.total_tokens(), 1000);
        // No peer IDs allocated yet
        assert_eq!(mapping.peer_count(), 0);
    }

    #[test]
    fn test_peer_view_includes_peer_id() {
        let rng = StdRng::seed_from_u64(42);
        let mut mapping = GlobalTokenMapping::new(rng, 100);

        // Allocate a peer ID
        let peer_id = mapping.allocate_peer_id().unwrap();

        let view = mapping.get_peer_view(peer_id, 10000, 1.0);

        // Peer should always know their own ID
        assert!(view.lookup(&peer_id).is_some());
    }

    #[test]
    fn test_coverage_fraction() {
        let rng = StdRng::seed_from_u64(42);
        let mut mapping = GlobalTokenMapping::new(rng, 10000);

        // Allocate a peer ID
        let peer_id = mapping.allocate_peer_id().unwrap();

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
        let expected = u64::MAX / 100 * 24; // (avg_distance * overlap * 1.2) avoiding overflow
        assert_eq!(width, expected);

        // With 100 peers and 5 neighbor overlap
        let width = GlobalTokenMapping::calculate_view_width(100, 5);
        let expected = (u64::MAX / 100) * 5 * 12 / 10;
        assert_eq!(width, expected);
    }

    #[test]
    fn test_allocate_peer_id() {
        let rng = StdRng::seed_from_u64(42);
        let mut mapping = GlobalTokenMapping::new(rng, 100);

        // Allocate 3 peer IDs
        let peer1 = mapping.allocate_peer_id().unwrap();
        let peer2 = mapping.allocate_peer_id().unwrap();
        let peer3 = mapping.allocate_peer_id().unwrap();

        // All should be unique
        assert_ne!(peer1, peer2);
        assert_ne!(peer2, peer3);
        assert_ne!(peer1, peer3);

        // All should be valid tokens from the mapping
        assert!(mapping.mappings.contains_key(&peer1));
        assert!(mapping.mappings.contains_key(&peer2));
        assert!(mapping.mappings.contains_key(&peer3));

        // Should be tracked as allocated
        assert!(mapping.allocated_peer_ids.contains(&peer1));
        assert!(mapping.allocated_peer_ids.contains(&peer2));
        assert!(mapping.allocated_peer_ids.contains(&peer3));

        // Peer count should be 3
        assert_eq!(mapping.peer_count(), 3);
    }
}
