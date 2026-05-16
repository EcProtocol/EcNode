use std::collections::{BTreeSet, HashMap};

use ec_rust::ec_interface::PeerId;
use rand::rngs::StdRng;
use rand::Rng;

/// Build a symmetric ring-gradient connectivity graph.
///
/// The model is:
/// - peers are ordered by ring position (sorted peer ids)
/// - the closest `neighbors` peers on each side are always connected
/// - the next `neighbors` peers on each side fade out linearly to zero
/// - anything further away is not connected initially
///
/// This gives us a dense local neighborhood plus sparse longer-range overlap,
/// while keeping the startup graph explicitly two-way.
pub fn build_ring_gradient_topology(
    peer_ids: &[PeerId],
    neighbors: usize,
    rng: &mut StdRng,
) -> HashMap<PeerId, Vec<PeerId>> {
    let mut sorted_peer_ids = peer_ids.to_vec();
    sorted_peer_ids.sort_unstable();

    let mut adjacency: HashMap<PeerId, BTreeSet<PeerId>> = sorted_peer_ids
        .iter()
        .copied()
        .map(|peer_id| (peer_id, BTreeSet::new()))
        .collect();

    if sorted_peer_ids.len() < 2 || neighbors == 0 {
        return adjacency
            .into_iter()
            .map(|(peer_id, peers)| (peer_id, peers.into_iter().collect()))
            .collect();
    }

    let max_step = sorted_peer_ids.len() / 2;
    let guaranteed_steps = neighbors.min(max_step.max(1));
    let fade_steps = (guaranteed_steps * 2).min(max_step.max(guaranteed_steps));

    for i in 0..sorted_peer_ids.len() {
        for j in (i + 1)..sorted_peer_ids.len() {
            let clockwise_steps = j - i;
            let counter_clockwise_steps = sorted_peer_ids.len() - clockwise_steps;
            let rank_distance = clockwise_steps.min(counter_clockwise_steps);

            let connect = if rank_distance <= guaranteed_steps {
                true
            } else if rank_distance < fade_steps && fade_steps > guaranteed_steps {
                let span = (fade_steps - guaranteed_steps) as f64;
                let remaining = (fade_steps - rank_distance) as f64;
                rng.gen_bool((remaining / span).clamp(0.0, 1.0))
            } else {
                false
            };

            if connect {
                adjacency
                    .get_mut(&sorted_peer_ids[i])
                    .expect("peer should exist")
                    .insert(sorted_peer_ids[j]);
                adjacency
                    .get_mut(&sorted_peer_ids[j])
                    .expect("peer should exist")
                    .insert(sorted_peer_ids[i]);
            }
        }
    }

    adjacency
        .into_iter()
        .map(|(peer_id, peers)| (peer_id, peers.into_iter().collect()))
        .collect()
}

/// Build a symmetric ring topology with a steep local core plus an evenly spaced long-range tail.
///
/// The model is:
/// - the closest `neighbors` peers on each side are always connected
/// - the next `neighbors` peers on each side fade out linearly
/// - beyond the fade band, keep a small fixed number of evenly spaced tail peers on each side
///
/// This gives a "spiky" local neighborhood while preserving a low, flat routing base across the
/// whole ring. It is intended as a steady-state ideal for routing experiments, not as a security
/// policy for dynamic peer selection.
pub fn build_ring_core_tail_topology(
    peer_ids: &[PeerId],
    neighbors: usize,
    tail_peers_per_side: usize,
    rng: &mut StdRng,
) -> HashMap<PeerId, Vec<PeerId>> {
    let mut sorted_peer_ids = peer_ids.to_vec();
    sorted_peer_ids.sort_unstable();

    let mut adjacency: HashMap<PeerId, BTreeSet<PeerId>> = sorted_peer_ids
        .iter()
        .copied()
        .map(|peer_id| (peer_id, BTreeSet::new()))
        .collect();

    if sorted_peer_ids.len() < 2 {
        return adjacency
            .into_iter()
            .map(|(peer_id, peers)| (peer_id, peers.into_iter().collect()))
            .collect();
    }

    let len = sorted_peer_ids.len();
    let max_step = len / 2;
    let guaranteed_steps = neighbors.min(max_step.max(1));
    let fade_steps = (guaranteed_steps * 2).min(max_step.max(guaranteed_steps));

    for i in 0..sorted_peer_ids.len() {
        for j in (i + 1)..sorted_peer_ids.len() {
            let clockwise_steps = j - i;
            let counter_clockwise_steps = sorted_peer_ids.len() - clockwise_steps;
            let rank_distance = clockwise_steps.min(counter_clockwise_steps);

            let connect = if rank_distance <= guaranteed_steps {
                true
            } else if rank_distance < fade_steps && fade_steps > guaranteed_steps {
                let span = (fade_steps - guaranteed_steps) as f64;
                let remaining = (fade_steps - rank_distance) as f64;
                rng.gen_bool((remaining / span).clamp(0.0, 1.0))
            } else {
                false
            };

            if connect {
                adjacency
                    .get_mut(&sorted_peer_ids[i])
                    .expect("peer should exist")
                    .insert(sorted_peer_ids[j]);
                adjacency
                    .get_mut(&sorted_peer_ids[j])
                    .expect("peer should exist")
                    .insert(sorted_peer_ids[i]);
            }
        }
    }

    if tail_peers_per_side > 0 && max_step > fade_steps {
        let tail_offsets = evenly_spaced_tail_offsets(fade_steps, max_step, tail_peers_per_side);
        for (idx, peer_id) in sorted_peer_ids.iter().copied().enumerate() {
            for step in &tail_offsets {
                let right = sorted_peer_ids[(idx + step) % len];
                let left = sorted_peer_ids[(idx + len - (step % len)) % len];

                adjacency
                    .get_mut(&peer_id)
                    .expect("peer should exist")
                    .insert(right);
                adjacency
                    .get_mut(&right)
                    .expect("peer should exist")
                    .insert(peer_id);

                adjacency
                    .get_mut(&peer_id)
                    .expect("peer should exist")
                    .insert(left);
                adjacency
                    .get_mut(&left)
                    .expect("peer should exist")
                    .insert(peer_id);
            }
        }
    }

    adjacency
        .into_iter()
        .map(|(peer_id, peers)| (peer_id, peers.into_iter().collect()))
        .collect()
}

/// Build a symmetric probabilistic ring-gradient connectivity graph.
///
/// Every pair is considered once. Connection probability falls linearly with
/// actual 64-bit ring distance, so nearby peers are much more likely to connect
/// than far-away peers, but there are no guaranteed neighbors.
pub fn build_probabilistic_ring_gradient_topology(
    peer_ids: &[PeerId],
    rng: &mut StdRng,
) -> HashMap<PeerId, Vec<PeerId>> {
    build_linear_probability_ring_topology(peer_ids, 1.0, 0.0, 0, rng)
}

/// Build a symmetric full-ring linear-probability connectivity graph.
///
/// The model is:
/// - peers are ordered by ring position (sorted peer ids)
/// - optional `guaranteed_neighbors` on each side are always connected
/// - beyond that core, every pair is considered once
/// - connection probability falls linearly from `center_prob` at distance 0
///   to `far_prob` at the far side of the ring
///
/// This expresses a dense "high core" probabilistic family directly. It is
/// useful for comparing how broad, dense linear slope profiles behave in the
/// steady-state harness.
pub fn build_linear_probability_ring_topology(
    peer_ids: &[PeerId],
    center_prob: f64,
    far_prob: f64,
    guaranteed_neighbors: usize,
    rng: &mut StdRng,
) -> HashMap<PeerId, Vec<PeerId>> {
    let mut sorted_peer_ids = peer_ids.to_vec();
    sorted_peer_ids.sort_unstable();

    let mut adjacency: HashMap<PeerId, BTreeSet<PeerId>> = sorted_peer_ids
        .iter()
        .copied()
        .map(|peer_id| (peer_id, BTreeSet::new()))
        .collect();

    if sorted_peer_ids.len() < 2 {
        return adjacency
            .into_iter()
            .map(|(peer_id, peers)| (peer_id, peers.into_iter().collect()))
            .collect();
    }

    let center_prob = center_prob.clamp(0.0, 1.0);
    let far_prob = far_prob.clamp(0.0, center_prob);
    let max_step = sorted_peer_ids.len() / 2;
    let guaranteed_steps = guaranteed_neighbors.min(max_step.max(1));

    for i in 0..sorted_peer_ids.len() {
        for j in (i + 1)..sorted_peer_ids.len() {
            let clockwise_steps = j - i;
            let counter_clockwise_steps = sorted_peer_ids.len() - clockwise_steps;
            let rank_distance = clockwise_steps.min(counter_clockwise_steps);

            let connect = if rank_distance <= guaranteed_steps {
                true
            } else {
                let distance_fraction = if max_step == 0 {
                    0.0
                } else {
                    (rank_distance as f64 / max_step as f64).clamp(0.0, 1.0)
                };
                let probability =
                    (center_prob + ((far_prob - center_prob) * distance_fraction)).clamp(0.0, 1.0);
                rng.gen_bool(probability)
            };

            if connect {
                adjacency
                    .get_mut(&sorted_peer_ids[i])
                    .expect("peer should exist")
                    .insert(sorted_peer_ids[j]);
                adjacency
                    .get_mut(&sorted_peer_ids[j])
                    .expect("peer should exist")
                    .insert(sorted_peer_ids[i]);
            }
        }
    }

    adjacency
        .into_iter()
        .map(|(peer_id, peers)| (peer_id, peers.into_iter().collect()))
        .collect()
}

/// Build a symmetric topology from low-bit location distance.
///
/// This is intentionally synthetic. It lets fixed-network experiments model
/// multiple dense location cells even when the full peer-id ring is small.
/// Peers with equal low-bit locations connect with `center_prob`; peers on the
/// opposite side of the low-bit location ring connect with `far_prob`.
pub fn build_location_linear_probability_topology(
    peer_ids: &[PeerId],
    location_bits: u8,
    center_prob: f64,
    far_prob: f64,
    rng: &mut StdRng,
) -> HashMap<PeerId, Vec<PeerId>> {
    let mut adjacency: HashMap<PeerId, BTreeSet<PeerId>> = peer_ids
        .iter()
        .copied()
        .map(|peer_id| (peer_id, BTreeSet::new()))
        .collect();

    if peer_ids.len() < 2 {
        return adjacency
            .into_iter()
            .map(|(peer_id, peers)| (peer_id, peers.into_iter().collect()))
            .collect();
    }

    let bits = location_bits.clamp(1, 63);
    let ring = if bits >= 63 { 1u64 << 63 } else { 1u64 << bits };
    let mask = ring - 1;
    let center_prob = center_prob.clamp(0.0, 1.0);
    let far_prob = far_prob.clamp(0.0, center_prob);

    for i in 0..peer_ids.len() {
        for j in (i + 1)..peer_ids.len() {
            let left = peer_ids[i] & mask;
            let right = peer_ids[j] & mask;
            let forward = right.wrapping_sub(left) & mask;
            let backward = left.wrapping_sub(right) & mask;
            let distance = forward.min(backward) as f64;
            let max_distance = (ring / 2).max(1) as f64;
            let distance_fraction = (distance / max_distance).clamp(0.0, 1.0);
            let probability =
                (center_prob + ((far_prob - center_prob) * distance_fraction)).clamp(0.0, 1.0);

            if rng.gen_bool(probability) {
                adjacency
                    .get_mut(&peer_ids[i])
                    .expect("peer should exist")
                    .insert(peer_ids[j]);
                adjacency
                    .get_mut(&peer_ids[j])
                    .expect("peer should exist")
                    .insert(peer_ids[i]);
            }
        }
    }

    adjacency
        .into_iter()
        .map(|(peer_id, peers)| (peer_id, peers.into_iter().collect()))
        .collect()
}

fn evenly_spaced_tail_offsets(
    fade_steps: usize,
    max_step: usize,
    tail_peers_per_side: usize,
) -> Vec<usize> {
    if tail_peers_per_side == 0 || max_step <= fade_steps {
        return Vec::new();
    }

    let tail_start = fade_steps + 1;
    let span = max_step.saturating_sub(tail_start);
    let mut offsets = BTreeSet::new();

    for slot in 0..tail_peers_per_side {
        let numerator = slot + 1;
        let denominator = tail_peers_per_side + 1;
        let offset = tail_start + (span * numerator) / denominator;
        offsets.insert(offset.clamp(tail_start, max_step));
    }

    offsets.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::{
        build_linear_probability_ring_topology, build_location_linear_probability_topology,
        build_probabilistic_ring_gradient_topology, build_ring_core_tail_topology,
        build_ring_gradient_topology,
    };

    #[test]
    fn ring_gradient_topology_is_symmetric_and_keeps_close_neighbors() {
        let peer_ids: Vec<u64> = (0..16).map(|n| n as u64 * 10).collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let adjacency = build_ring_gradient_topology(&peer_ids, 2, &mut rng);

        for (idx, peer_id) in peer_ids.iter().enumerate() {
            let peers = adjacency.get(peer_id).expect("peer should exist");

            let forward_1 = peer_ids[(idx + 1) % peer_ids.len()];
            let forward_2 = peer_ids[(idx + 2) % peer_ids.len()];
            let backward_1 = peer_ids[(idx + peer_ids.len() - 1) % peer_ids.len()];
            let backward_2 = peer_ids[(idx + peer_ids.len() - 2) % peer_ids.len()];

            assert!(peers.contains(&forward_1));
            assert!(peers.contains(&forward_2));
            assert!(peers.contains(&backward_1));
            assert!(peers.contains(&backward_2));
        }

        for (peer_id, peers) in &adjacency {
            for other_id in peers {
                assert!(
                    adjacency
                        .get(other_id)
                        .expect("other peer should exist")
                        .contains(peer_id),
                    "adjacency should be symmetric for {peer_id} <-> {other_id}"
                );
            }
        }
    }

    #[test]
    fn probabilistic_ring_gradient_topology_is_symmetric() {
        let peer_ids: Vec<u64> = (0..16).map(|n| n as u64 * 10).collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        let adjacency = build_probabilistic_ring_gradient_topology(&peer_ids, &mut rng);

        for (peer_id, peers) in &adjacency {
            for other_id in peers {
                assert!(
                    adjacency
                        .get(other_id)
                        .expect("other peer should exist")
                        .contains(peer_id),
                    "adjacency should be symmetric for {peer_id} <-> {other_id}"
                );
            }
        }
    }

    #[test]
    fn linear_probability_ring_topology_keeps_guaranteed_neighbors_and_is_symmetric() {
        let peer_ids: Vec<u64> = (0..16).map(|n| n as u64 * 10).collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(23);
        let adjacency = build_linear_probability_ring_topology(&peer_ids, 1.0, 0.2, 2, &mut rng);

        for (idx, peer_id) in peer_ids.iter().enumerate() {
            let peers = adjacency.get(peer_id).expect("peer should exist");
            let forward_1 = peer_ids[(idx + 1) % peer_ids.len()];
            let forward_2 = peer_ids[(idx + 2) % peer_ids.len()];
            let backward_1 = peer_ids[(idx + peer_ids.len() - 1) % peer_ids.len()];
            let backward_2 = peer_ids[(idx + peer_ids.len() - 2) % peer_ids.len()];

            assert!(peers.contains(&forward_1));
            assert!(peers.contains(&forward_2));
            assert!(peers.contains(&backward_1));
            assert!(peers.contains(&backward_2));
        }

        for (peer_id, peers) in &adjacency {
            for other_id in peers {
                assert!(
                    adjacency
                        .get(other_id)
                        .expect("other peer should exist")
                        .contains(peer_id),
                    "adjacency should be symmetric for {peer_id} <-> {other_id}"
                );
            }
        }
    }

    #[test]
    fn ring_core_tail_topology_is_symmetric_and_has_long_range_tail() {
        let peer_ids: Vec<u64> = (0..32).map(|n| n as u64 * 10).collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(19);
        let adjacency = build_ring_core_tail_topology(&peer_ids, 2, 3, &mut rng);

        for (idx, peer_id) in peer_ids.iter().enumerate() {
            let peers = adjacency.get(peer_id).expect("peer should exist");
            let forward_1 = peer_ids[(idx + 1) % peer_ids.len()];
            let backward_1 = peer_ids[(idx + peer_ids.len() - 1) % peer_ids.len()];

            assert!(peers.contains(&forward_1));
            assert!(peers.contains(&backward_1));

            let has_tail = peers.iter().any(|other_id| {
                let other_idx = peer_ids
                    .iter()
                    .position(|candidate| candidate == other_id)
                    .expect("peer should exist");
                let clockwise_steps = other_idx.abs_diff(idx);
                let rank_distance = clockwise_steps.min(peer_ids.len() - clockwise_steps);
                rank_distance > 4
            });
            assert!(
                has_tail,
                "peer {peer_id} should keep at least one long-range tail peer"
            );
        }

        for (peer_id, peers) in &adjacency {
            for other_id in peers {
                assert!(
                    adjacency
                        .get(other_id)
                        .expect("other peer should exist")
                        .contains(peer_id),
                    "adjacency should be symmetric for {peer_id} <-> {other_id}"
                );
            }
        }
    }

    #[test]
    fn location_linear_probability_topology_groups_low_bit_cells() {
        let peer_ids = vec![0x1000_0000, 0x2000_0000, 0x3000_8000, 0x4000_8000];
        let mut rng = rand::rngs::StdRng::seed_from_u64(29);
        let adjacency =
            build_location_linear_probability_topology(&peer_ids, 16, 1.0, 0.0, &mut rng);

        assert!(adjacency[&0x1000_0000].contains(&0x2000_0000));
        assert!(adjacency[&0x3000_8000].contains(&0x4000_8000));
        assert!(!adjacency[&0x1000_0000].contains(&0x3000_8000));
        assert!(!adjacency[&0x2000_0000].contains(&0x4000_8000));
    }
}
