// Token Distribution Strategies

use super::config::{TokenDistribution, WeightDistribution};
use ec_rust::ec_interface::{PeerId, TokenId};
use rand::rngs::StdRng;
use rand::{Rng, RngCore};
use std::collections::HashMap;

/// Distributor for assigning tokens to peers
pub struct TokenDistributor {
    rng: StdRng,
}

impl TokenDistributor {
    pub fn new(rng: StdRng) -> Self {
        Self { rng }
    }

    /// Distribute tokens to peers according to strategy
    pub fn distribute(
        &mut self,
        peers: &[PeerId],
        strategy: &TokenDistribution,
    ) -> HashMap<PeerId, Vec<TokenId>> {
        match strategy {
            TokenDistribution::Uniform { tokens_per_peer } => {
                self.distribute_uniform(peers, *tokens_per_peer)
            }

            TokenDistribution::Clustered {
                tokens_per_peer,
                cluster_radius,
            } => self.distribute_clustered(peers, *tokens_per_peer, *cluster_radius),

            TokenDistribution::Random {
                total_tokens,
                min_per_peer,
                max_per_peer,
            } => self.distribute_random(peers, *total_tokens, *min_per_peer, *max_per_peer),

            TokenDistribution::Weighted {
                total_tokens,
                distribution,
            } => self.distribute_weighted(peers, *total_tokens, distribution),

            TokenDistribution::Custom(mapping) => mapping.clone(),
        }
    }

    /// Uniform distribution: each peer gets N tokens uniformly on ring
    fn distribute_uniform(
        &mut self,
        peers: &[PeerId],
        tokens_per_peer: usize,
    ) -> HashMap<PeerId, Vec<TokenId>> {
        let mut result = HashMap::new();

        for peer_id in peers {
            let mut tokens = Vec::new();
            for _ in 0..tokens_per_peer {
                // Generate random token
                tokens.push(self.rng.next_u64());
            }
            result.insert(*peer_id, tokens);
        }

        result
    }

    /// Clustered distribution: tokens near peer ID on ring
    fn distribute_clustered(
        &mut self,
        peers: &[PeerId],
        tokens_per_peer: usize,
        cluster_radius: u64,
    ) -> HashMap<PeerId, Vec<TokenId>> {
        let mut result = HashMap::new();

        for peer_id in peers {
            let mut tokens = Vec::new();
            for _ in 0..tokens_per_peer {
                // Generate token within cluster_radius of peer_id
                let offset = self.rng.gen_range(0..cluster_radius);
                let direction = if self.rng.gen_bool(0.5) { 1i64 } else { -1i64 };
                let token = peer_id.wrapping_add((offset as i64 * direction) as u64);
                tokens.push(token);
            }
            result.insert(*peer_id, tokens);
        }

        result
    }

    /// Random distribution: total_tokens distributed randomly
    fn distribute_random(
        &mut self,
        peers: &[PeerId],
        total_tokens: usize,
        min_per_peer: usize,
        max_per_peer: usize,
    ) -> HashMap<PeerId, Vec<TokenId>> {
        let mut result: HashMap<PeerId, Vec<TokenId>> = HashMap::new();

        // Initialize with minimum tokens
        for peer_id in peers {
            let mut tokens = Vec::new();
            for _ in 0..min_per_peer {
                tokens.push(self.rng.next_u64());
            }
            result.insert(*peer_id, tokens);
        }

        // Distribute remaining tokens
        let assigned = peers.len() * min_per_peer;
        let remaining = total_tokens.saturating_sub(assigned);

        for _ in 0..remaining {
            // Pick random peer that hasn't hit max
            let available_peers: Vec<_> = peers
                .iter()
                .filter(|p| result.get(p).map_or(0, |v| v.len()) < max_per_peer)
                .collect();

            if available_peers.is_empty() {
                break; // All peers at max
            }

            let peer_id = *available_peers[self.rng.gen_range(0..available_peers.len())];
            result
                .entry(peer_id)
                .or_insert_with(Vec::new)
                .push(self.rng.next_u64());
        }

        result
    }

    /// Weighted distribution: use distribution function
    fn distribute_weighted(
        &mut self,
        peers: &[PeerId],
        total_tokens: usize,
        distribution: &WeightDistribution,
    ) -> HashMap<PeerId, Vec<TokenId>> {
        // Calculate weights for each peer
        let weights = self.calculate_weights(peers.len(), distribution);

        // Normalize weights to sum to total_tokens
        let sum: f64 = weights.iter().sum();
        let mut token_counts: Vec<usize> = weights
            .iter()
            .map(|&w| ((w / sum) * total_tokens as f64).round() as usize)
            .collect();

        // Adjust for rounding errors
        let assigned: usize = token_counts.iter().sum();
        if assigned < total_tokens {
            // Add remaining to random peers
            for _ in 0..(total_tokens - assigned) {
                let idx = self.rng.gen_range(0..token_counts.len());
                token_counts[idx] += 1;
            }
        } else if assigned > total_tokens {
            // Remove excess from random peers
            for _ in 0..(assigned - total_tokens) {
                let idx = self.rng.gen_range(0..token_counts.len());
                if token_counts[idx] > 0 {
                    token_counts[idx] -= 1;
                }
            }
        }

        // Assign tokens
        let mut result = HashMap::new();
        for (i, peer_id) in peers.iter().enumerate() {
            let mut tokens = Vec::new();
            for _ in 0..token_counts[i] {
                tokens.push(self.rng.next_u64());
            }
            result.insert(*peer_id, tokens);
        }

        result
    }

    /// Calculate weights based on distribution function
    fn calculate_weights(&mut self, count: usize, distribution: &WeightDistribution) -> Vec<f64> {
        match distribution {
            WeightDistribution::PowerLaw { alpha } => {
                // Power law: weight[i] = i^(-alpha)
                (1..=count)
                    .map(|i| (i as f64).powf(-alpha))
                    .collect()
            }

            WeightDistribution::Exponential { lambda } => {
                // Exponential: weight[i] = exp(-lambda * i)
                (0..count)
                    .map(|i| (-lambda * i as f64).exp())
                    .collect()
            }

            WeightDistribution::Normal { mean, stddev } => {
                // Normal distribution
                (0..count)
                    .map(|i| {
                        let x = i as f64;
                        let exponent = -((x - mean).powi(2)) / (2.0 * stddev.powi(2));
                        exponent.exp()
                    })
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn test_uniform_distribution() {
        let rng = StdRng::from_seed([0u8; 32]);
        let mut distributor = TokenDistributor::new(rng);

        let peers = vec![1000, 2000, 3000];
        let result = distributor.distribute_uniform(&peers, 5);

        assert_eq!(result.len(), 3);
        for peer in &peers {
            assert_eq!(result.get(peer).unwrap().len(), 5);
        }
    }

    #[test]
    fn test_random_distribution() {
        let rng = StdRng::from_seed([0u8; 32]);
        let mut distributor = TokenDistributor::new(rng);

        let peers = vec![1000, 2000, 3000];
        let result = distributor.distribute_random(&peers, 20, 2, 10);

        // Each peer should have at least min_per_peer
        for peer in &peers {
            let count = result.get(peer).unwrap().len();
            assert!(count >= 2);
            assert!(count <= 10);
        }

        // Total should be close to total_tokens
        let total: usize = result.values().map(|v| v.len()).sum();
        assert_eq!(total, 20);
    }

    #[test]
    fn test_clustered_distribution() {
        let rng = StdRng::from_seed([0u8; 32]);
        let mut distributor = TokenDistributor::new(rng);

        let peers = vec![1000];
        let result = distributor.distribute_clustered(&peers, 10, 100);

        let tokens = result.get(&1000).unwrap();
        assert_eq!(tokens.len(), 10);

        // Tokens should be within 100 of peer_id (allowing wraparound)
        for token in tokens {
            let dist = ring_distance(1000, *token);
            assert!(dist <= 100, "Token {} too far from peer 1000 (dist: {})", token, dist);
        }
    }

    fn ring_distance(a: u64, b: u64) -> u64 {
        let forward = b.wrapping_sub(a);
        let backward = a.wrapping_sub(b);
        forward.min(backward)
    }
}
