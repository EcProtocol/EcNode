#!/usr/bin/env python3
"""
Reactive vote / commit simulation.

This is a stripped-down reference model of the Rust implementation's reactive
vote flow. It intentionally omits block fetch, batching, churn, and retries so
we can isolate the commit mechanics on top of a chosen peer topology.

Core simplifications:
- perfect one-round message delay
- one-shot vote seeding on first learn
- two message types only: Vote and Commit
- per-role counters only
- a node commits when every role counter reaches the threshold
"""

from __future__ import annotations

import argparse
import bisect
import math
import random
import statistics
from collections import Counter, defaultdict, deque
from dataclasses import dataclass
from typing import Counter as CounterType
from typing import DefaultDict, Dict, List, Optional, Sequence, Set, Tuple


def percentile(values: Sequence[float], q: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    idx = int(math.ceil((len(ordered) - 1) * q))
    return float(ordered[idx])


def format_metric(values: Sequence[float]) -> str:
    if not values:
        return "n/a"
    return (
        f"avg {statistics.mean(values):.2f}, "
        f"p50 {percentile(values, 0.50):.2f}, "
        f"p95 {percentile(values, 0.95):.2f}"
    )


@dataclass(frozen=True)
class Node:
    node_id: int
    address: int


@dataclass(frozen=True)
class Message:
    kind: str  # "vote" or "commit"
    src: int
    dst: int
    roles: Tuple[bool, ...]


@dataclass
class NodeTxState:
    aware_round: Optional[int]
    commit_round: Optional[int]
    counters: List[int]
    vote_senders: Set[int]
    sender_latest_roles: Dict[int, Tuple[bool, ...]]
    seeded: bool


@dataclass
class TxRoundSpread:
    round_num: int
    aware_nodes: int
    counted_nodes: int
    committed_nodes: int
    vote_messages: int
    commit_messages: int


@dataclass
class TxResult:
    origin: int
    role_centers: List[int]
    origin_commit_round: Optional[int]
    aware_node_ids: Set[int]
    committed_node_ids: Set[int]
    final_aware_nodes: int
    final_committed_nodes: int
    final_round: int
    quiesced: bool
    sent_messages: CounterType[str]
    spread_by_round: List[TxRoundSpread]


@dataclass
class RunSummary:
    topology: str
    nodes: int
    transaction_count: int
    roles_per_tx: int
    degree_avg: float
    degree_p95: float
    components: int
    largest_component: int
    isolated_nodes: int
    origin_commit_success_rate: float
    origin_commit_rounds: List[int]
    quiesce_rate: float
    quiesce_rounds: List[int]
    final_aware_fraction: List[float]
    final_committed_fraction: List[float]
    sent_messages: CounterType[str]
    round_aware_fraction: Dict[int, List[float]]
    round_counted_fraction: Dict[int, List[float]]
    round_committed_fraction: Dict[int, List[float]]
    round_vote_messages: Dict[int, List[float]]
    round_commit_messages: Dict[int, List[float]]
    checkpoint_rounds: List[int]
    checkpoint_aware_fraction: Dict[int, List[float]]
    checkpoint_counted_fraction: Dict[int, List[float]]
    checkpoint_committed_fraction: Dict[int, List[float]]
    checkpoint_vote_messages: Dict[int, List[float]]
    checkpoint_commit_messages: Dict[int, List[float]]
    node_aware_tx_counts: List[int]
    node_committed_tx_counts: List[int]
    sample_transaction: Optional[TxResult]


def generate_unique_addresses(count: int, rng: random.Random) -> List[int]:
    seen: Set[int] = set()
    while len(seen) < count:
        seen.add(rng.getrandbits(64))
    return list(seen)


def build_ring_gradient_topology(
    node_ids_by_address: List[int],
    neighbors: int,
    rng: random.Random,
) -> Dict[int, List[int]]:
    adjacency: Dict[int, Set[int]] = {node_id: set() for node_id in node_ids_by_address}
    size = len(node_ids_by_address)
    if size < 2 or neighbors <= 0:
        return {node_id: [] for node_id in node_ids_by_address}

    max_step = size // 2
    guaranteed_steps = min(neighbors, max(max_step, 1))
    fade_steps = min(guaranteed_steps * 2, max(max_step, guaranteed_steps))

    for i in range(size):
        for j in range(i + 1, size):
            clockwise = j - i
            counter = size - clockwise
            rank_distance = min(clockwise, counter)

            connect = False
            if rank_distance <= guaranteed_steps:
                connect = True
            elif rank_distance < fade_steps and fade_steps > guaranteed_steps:
                span = float(fade_steps - guaranteed_steps)
                remaining = float(fade_steps - rank_distance)
                connect = rng.random() < max(0.0, min(remaining / span, 1.0))

            if connect:
                left = node_ids_by_address[i]
                right = node_ids_by_address[j]
                adjacency[left].add(right)
                adjacency[right].add(left)

    return {node_id: sorted(peers) for node_id, peers in adjacency.items()}


def evenly_spaced_offsets(
    start_step: int,
    end_step: int,
    count: int,
) -> List[int]:
    if count <= 0 or end_step < start_step:
        return []

    span = end_step - start_step
    offsets: Set[int] = set()
    for slot in range(count):
        numerator = slot + 1
        denominator = count + 1
        offset = start_step + (span * numerator) // denominator
        offsets.add(max(start_step, min(offset, end_step)))
    return sorted(offsets)


def sampled_band_offsets_per_side(
    start_step: int,
    end_step: int,
    count_per_side: int,
    rng: random.Random,
) -> List[int]:
    if count_per_side <= 0 or end_step < start_step:
        return []

    band_size = end_step - start_step + 1
    if count_per_side >= band_size:
        return list(range(start_step, end_step + 1))

    return sorted(rng.sample(range(start_step, end_step + 1), count_per_side))


def evenly_spaced_tail_offsets(
    fade_steps: int,
    max_step: int,
    tail_peers_per_side: int,
) -> List[int]:
    if tail_peers_per_side <= 0 or max_step <= fade_steps:
        return []

    return evenly_spaced_offsets(fade_steps + 1, max_step, tail_peers_per_side)


def build_ring_core_tail_topology(
    node_ids_by_address: List[int],
    neighbors: int,
    tail_peers_per_side: int,
    rng: random.Random,
) -> Dict[int, List[int]]:
    adjacency = {
        node_id: set(peers)
        for node_id, peers in build_ring_gradient_topology(
            node_ids_by_address,
            neighbors,
            rng,
        ).items()
    }
    size = len(node_ids_by_address)
    if size < 2:
        return {node_id: sorted(peers) for node_id, peers in adjacency.items()}

    max_step = size // 2
    guaranteed_steps = min(neighbors, max(max_step, 1))
    fade_steps = min(guaranteed_steps * 2, max(max_step, guaranteed_steps))
    offsets = evenly_spaced_tail_offsets(fade_steps, max_step, tail_peers_per_side)

    for idx, node_id in enumerate(node_ids_by_address):
        for step in offsets:
            right = node_ids_by_address[(idx + step) % size]
            left = node_ids_by_address[(idx - step) % size]
            adjacency[node_id].add(right)
            adjacency[node_id].add(left)
            adjacency[right].add(node_id)
            adjacency[left].add(node_id)

    return {node_id: sorted(peers) for node_id, peers in adjacency.items()}


def build_ring_stepwise_sample_topology(
    node_ids_by_address: List[int],
    core_steps: int,
    mid_steps: int,
    mid_peers_per_side: int,
    far_peers_per_side: int,
    rng: random.Random,
) -> Dict[int, List[int]]:
    adjacency: Dict[int, Set[int]] = {node_id: set() for node_id in node_ids_by_address}
    size = len(node_ids_by_address)
    if size < 2:
        return {node_id: [] for node_id in node_ids_by_address}

    max_step = size // 2
    core_end = min(max(core_steps, 0), max_step)
    mid_end = min(max(mid_steps, core_end), max_step)

    for idx, node_id in enumerate(node_ids_by_address):
        for step in range(1, core_end + 1):
            right = node_ids_by_address[(idx + step) % size]
            left = node_ids_by_address[(idx - step) % size]
            adjacency[node_id].add(right)
            adjacency[node_id].add(left)
            adjacency[right].add(node_id)
            adjacency[left].add(node_id)

        for step in sampled_band_offsets_per_side(core_end + 1, mid_end, mid_peers_per_side, rng):
            right = node_ids_by_address[(idx + step) % size]
            left = node_ids_by_address[(idx - step) % size]
            adjacency[node_id].add(right)
            adjacency[node_id].add(left)
            adjacency[right].add(node_id)
            adjacency[left].add(node_id)

        for step in sampled_band_offsets_per_side(mid_end + 1, max_step, far_peers_per_side, rng):
            right = node_ids_by_address[(idx + step) % size]
            left = node_ids_by_address[(idx - step) % size]
            adjacency[node_id].add(right)
            adjacency[node_id].add(left)
            adjacency[right].add(node_id)
            adjacency[left].add(node_id)

    return {node_id: sorted(peers) for node_id, peers in adjacency.items()}


def build_ring_core_midband_topology(
    node_ids_by_address: List[int],
    neighbors: int,
    midband_peers_per_side: int,
    rng: random.Random,
) -> Dict[int, List[int]]:
    adjacency = {
        node_id: set(peers)
        for node_id, peers in build_ring_gradient_topology(
            node_ids_by_address,
            neighbors,
            rng,
        ).items()
    }
    size = len(node_ids_by_address)
    if size < 2:
        return {node_id: sorted(peers) for node_id, peers in adjacency.items()}

    max_step = size // 2
    guaranteed_steps = min(neighbors, max(max_step, 1))
    fade_steps = min(guaranteed_steps * 2, max(max_step, guaranteed_steps))
    midband_start = fade_steps + 1
    midband_end = min(max_step, fade_steps * 4)
    offsets = evenly_spaced_offsets(midband_start, midband_end, midband_peers_per_side)

    for idx, node_id in enumerate(node_ids_by_address):
        for step in offsets:
            right = node_ids_by_address[(idx + step) % size]
            left = node_ids_by_address[(idx - step) % size]
            adjacency[node_id].add(right)
            adjacency[node_id].add(left)
            adjacency[right].add(node_id)
            adjacency[left].add(node_id)

    return {node_id: sorted(peers) for node_id, peers in adjacency.items()}


def build_pairwise_probability_topology(
    node_ids_by_address: List[int],
    probability_for_distance,
    rng: random.Random,
) -> Dict[int, List[int]]:
    adjacency: Dict[int, Set[int]] = {node_id: set() for node_id in node_ids_by_address}
    size = len(node_ids_by_address)
    if size < 2:
        return {node_id: [] for node_id in node_ids_by_address}

    for i in range(size):
        for j in range(i + 1, size):
            clockwise = j - i
            counter = size - clockwise
            rank_distance = min(clockwise, counter)
            probability = max(0.0, min(1.0, probability_for_distance(rank_distance)))
            if rng.random() < probability:
                left = node_ids_by_address[i]
                right = node_ids_by_address[j]
                adjacency[left].add(right)
                adjacency[right].add(left)

    return {node_id: sorted(peers) for node_id, peers in adjacency.items()}


def build_random_uniform_topology(
    node_ids_by_address: List[int],
    target_degree: int,
    rng: random.Random,
) -> Dict[int, List[int]]:
    size = len(node_ids_by_address)
    if size < 2:
        return {node_id: [] for node_id in node_ids_by_address}

    probability = min(max(target_degree / max(size - 1, 1), 0.0), 1.0)
    return build_pairwise_probability_topology(
        node_ids_by_address,
        lambda _distance: probability,
        rng,
    )


def build_linear_probability_topology(
    node_ids_by_address: List[int],
    target_degree: int,
    rng: random.Random,
) -> Dict[int, List[int]]:
    size = len(node_ids_by_address)
    if size < 2:
        return {node_id: [] for node_id in node_ids_by_address}

    max_step = size // 2
    base_degree = 0.0
    for distance in range(1, max_step + 1):
        multiplicity = 1.0 if size % 2 == 0 and distance == max_step else 2.0
        base_degree += multiplicity * max(0.0, 1.0 - (distance / max_step))

    scale = min(max(target_degree / max(base_degree, 1e-9), 0.0), 1.0)
    return build_pairwise_probability_topology(
        node_ids_by_address,
        lambda distance: scale * max(0.0, 1.0 - (distance / max_step)),
        rng,
    )


def add_guaranteed_ring_neighbors(
    adjacency: Dict[int, List[int]],
    node_ids_by_address: List[int],
    neighbors: int,
) -> Dict[int, List[int]]:
    if neighbors <= 0 or len(node_ids_by_address) < 2:
        return adjacency

    merged: Dict[int, Set[int]] = {
        node_id: set(peers)
        for node_id, peers in adjacency.items()
    }
    size = len(node_ids_by_address)
    max_step = size // 2
    guaranteed_steps = min(neighbors, max(max_step, 1))

    for idx, node_id in enumerate(node_ids_by_address):
        for step in range(1, guaranteed_steps + 1):
            right = node_ids_by_address[(idx + step) % size]
            left = node_ids_by_address[(idx - step) % size]
            merged[node_id].add(right)
            merged[node_id].add(left)
            merged[right].add(node_id)
            merged[left].add(node_id)

    return {node_id: sorted(peers) for node_id, peers in merged.items()}


def build_linear_probability_with_core_topology(
    node_ids_by_address: List[int],
    target_degree: int,
    guaranteed_neighbors: int,
    rng: random.Random,
) -> Dict[int, List[int]]:
    adjacency = build_linear_probability_topology(
        node_ids_by_address,
        target_degree,
        rng,
    )
    return add_guaranteed_ring_neighbors(
        adjacency,
        node_ids_by_address,
        guaranteed_neighbors,
    )


def probability_between_endpoints(
    rank_distance: int,
    max_step: int,
    center_prob: float,
    far_prob: float,
) -> float:
    if max_step <= 0:
        return max(0.0, min(1.0, center_prob))
    distance_fraction = min(max(rank_distance / max_step, 0.0), 1.0)
    probability = center_prob + ((far_prob - center_prob) * distance_fraction)
    return max(0.0, min(1.0, probability))


def build_heterogeneous_linear_slope_topology(
    node_ids_by_address: List[int],
    center_prob_min: float,
    center_prob_max: float,
    far_prob_min: float,
    far_prob_max: float,
    rng: random.Random,
) -> Dict[int, List[int]]:
    adjacency: Dict[int, Set[int]] = {node_id: set() for node_id in node_ids_by_address}
    size = len(node_ids_by_address)
    if size < 2:
        return {node_id: [] for node_id in node_ids_by_address}

    center_low = max(0.0, min(center_prob_min, 1.0))
    center_high = max(center_low, min(center_prob_max, 1.0))
    far_low = max(0.0, min(far_prob_min, 1.0))
    far_high = max(far_low, min(far_prob_max, 1.0))

    node_slopes: Dict[int, Tuple[float, float]] = {}
    for node_id in node_ids_by_address:
        center_prob = rng.uniform(center_low, center_high)
        bounded_far_high = min(far_high, center_prob)
        bounded_far_low = min(far_low, bounded_far_high)
        far_prob = rng.uniform(bounded_far_low, bounded_far_high)
        node_slopes[node_id] = (center_prob, far_prob)

    max_step = size // 2
    for i in range(size):
        for j in range(i + 1, size):
            clockwise = j - i
            counter = size - clockwise
            rank_distance = min(clockwise, counter)
            left = node_ids_by_address[i]
            right = node_ids_by_address[j]
            left_center, left_far = node_slopes[left]
            right_center, right_far = node_slopes[right]
            left_prob = probability_between_endpoints(
                rank_distance,
                max_step,
                left_center,
                left_far,
            )
            right_prob = probability_between_endpoints(
                rank_distance,
                max_step,
                right_center,
                right_far,
            )
            pair_prob = max(0.0, min(1.0, (left_prob + right_prob) / 2.0))
            if rng.random() < pair_prob:
                adjacency[left].add(right)
                adjacency[right].add(left)

    return {node_id: sorted(peers) for node_id, peers in adjacency.items()}


def build_full_table_topology(
    node_ids_by_address: List[int],
) -> Dict[int, List[int]]:
    return {
        node_id: [peer_id for peer_id in node_ids_by_address if peer_id != node_id]
        for node_id in node_ids_by_address
    }


def connected_components(adjacency: Dict[int, List[int]]) -> List[List[int]]:
    seen: Set[int] = set()
    components: List[List[int]] = []
    for start in adjacency:
        if start in seen:
            continue
        queue = deque([start])
        seen.add(start)
        component: List[int] = []
        while queue:
            node = queue.popleft()
            component.append(node)
            for peer in adjacency[node]:
                if peer not in seen:
                    seen.add(peer)
                    queue.append(peer)
        components.append(component)
    return components


class ReactiveVoteSimulation:
    def __init__(
        self,
        num_nodes: int,
        topology: str,
        neighbors: int,
        tail_peers_per_side: int,
        target_degree: int,
        target_degree_percent: Optional[float],
        hetero_center_prob_min: float,
        hetero_center_prob_max: float,
        hetero_far_prob_min: float,
        hetero_far_prob_max: float,
        stepwise_mid_steps: int,
        stepwise_mid_peers_per_side: int,
        stepwise_far_peers_per_side: int,
        roles_per_tx: int,
        transaction_count: int,
        threshold: int,
        targets_per_side: int,
        origin_targets_per_side: Optional[int],
        range_width: int,
        max_rounds: int,
        checkpoints: List[int],
        seed: Optional[int],
    ) -> None:
        self.num_nodes = num_nodes
        self.topology = topology
        self.neighbors = neighbors
        self.tail_peers_per_side = tail_peers_per_side
        self.target_degree = target_degree
        self.target_degree_percent = target_degree_percent
        self.hetero_center_prob_min = hetero_center_prob_min
        self.hetero_center_prob_max = hetero_center_prob_max
        self.hetero_far_prob_min = hetero_far_prob_min
        self.hetero_far_prob_max = hetero_far_prob_max
        self.stepwise_mid_steps = stepwise_mid_steps
        self.stepwise_mid_peers_per_side = stepwise_mid_peers_per_side
        self.stepwise_far_peers_per_side = stepwise_far_peers_per_side
        self.roles_per_tx = roles_per_tx
        self.transaction_count = transaction_count
        self.threshold = threshold
        self.targets_per_side = targets_per_side
        self.origin_targets_per_side = (
            origin_targets_per_side if origin_targets_per_side is not None else targets_per_side
        )
        self.range_width = range_width
        self.max_rounds = max_rounds
        self.checkpoints = checkpoints
        self.rng = random.Random(seed)

        addresses = generate_unique_addresses(num_nodes, self.rng)
        self.nodes: List[Node] = [Node(i, address) for i, address in enumerate(addresses)]
        self.nodes_by_id: Dict[int, Node] = {node.node_id: node for node in self.nodes}
        self.node_ids_by_address = sorted(
            (node.node_id for node in self.nodes),
            key=lambda node_id: self.nodes_by_id[node_id].address,
        )

        effective_target_degree = target_degree
        if target_degree_percent is not None:
            effective_target_degree = int(
                round((max(num_nodes - 1, 0) * target_degree_percent) / 100.0)
            )
        self.effective_target_degree = max(0, min(effective_target_degree, max(num_nodes - 1, 0)))

        if topology == "ring_gradient":
            self.adjacency = build_ring_gradient_topology(
                self.node_ids_by_address,
                neighbors,
                self.rng,
            )
        elif topology == "ring_core_tail":
            self.adjacency = build_ring_core_tail_topology(
                self.node_ids_by_address,
                neighbors,
                tail_peers_per_side,
                self.rng,
            )
        elif topology == "ring_core_midband":
            self.adjacency = build_ring_core_midband_topology(
                self.node_ids_by_address,
                neighbors,
                tail_peers_per_side,
                self.rng,
            )
        elif topology == "ring_stepwise_sample":
            self.adjacency = build_ring_stepwise_sample_topology(
                self.node_ids_by_address,
                neighbors,
                stepwise_mid_steps,
                stepwise_mid_peers_per_side,
                stepwise_far_peers_per_side,
                self.rng,
            )
        elif topology == "random_uniform":
            self.adjacency = build_random_uniform_topology(
                self.node_ids_by_address,
                self.effective_target_degree,
                self.rng,
            )
        elif topology == "linear_probability":
            self.adjacency = build_linear_probability_topology(
                self.node_ids_by_address,
                self.effective_target_degree,
                self.rng,
            )
        elif topology == "linear_probability_with_core":
            self.adjacency = build_linear_probability_with_core_topology(
                self.node_ids_by_address,
                self.effective_target_degree,
                neighbors,
                self.rng,
            )
        elif topology == "heterogeneous_linear_slope":
            self.adjacency = build_heterogeneous_linear_slope_topology(
                self.node_ids_by_address,
                hetero_center_prob_min,
                hetero_center_prob_max,
                hetero_far_prob_min,
                hetero_far_prob_max,
                self.rng,
            )
        elif topology == "full_table":
            self.adjacency = build_full_table_topology(self.node_ids_by_address)
        else:
            raise ValueError(f"unsupported topology: {topology}")

        self.connected_by_address: Dict[int, List[int]] = {
            node_id: sorted(peers, key=lambda peer_id: self.nodes_by_id[peer_id].address)
            for node_id, peers in self.adjacency.items()
        }
        self.connected_addresses: Dict[int, List[int]] = {
            node_id: [self.nodes_by_id[peer_id].address for peer_id in peers]
            for node_id, peers in self.connected_by_address.items()
        }

    def peers_around_center(self, node_id: int, center: int, per_side: int) -> Set[int]:
        peers = self.connected_by_address[node_id]
        if not peers or per_side <= 0:
            return set()

        addresses = self.connected_addresses[node_id]
        idx = bisect.bisect_left(addresses, center)
        seen: Set[int] = set()

        for offset in range(per_side):
            left = peers[(idx - offset - 1) % len(peers)]
            right = peers[(idx + offset) % len(peers)]
            seen.add(left)
            seen.add(right)

        return seen

    def sender_counts_for_role(self, receiver: int, sender: int, center: int) -> bool:
        return sender in self.peers_around_center(receiver, center, self.range_width)

    def emit_seed_votes(
        self,
        src: int,
        role_centers: Sequence[int],
        targets_per_side: int,
        outbound: DefaultDict[int, List[Message]],
        deliver_round: int,
        sent_messages: CounterType[str],
        round_messages: CounterType[str],
    ) -> None:
        targets: Dict[int, List[bool]] = {}
        for role_idx, center in enumerate(role_centers):
            for peer_id in self.peers_around_center(src, center, targets_per_side):
                role_mask = targets.setdefault(peer_id, [True] * len(role_centers))

        for peer_id, role_mask in targets.items():
            outbound[deliver_round].append(
                Message("vote", src, peer_id, tuple(role_mask))
            )
            sent_messages["vote"] += 1
            round_messages["vote"] += 1

    def emit_commit_messages(
        self,
        src: int,
        destinations: Sequence[int],
        role_count: int,
        outbound: DefaultDict[int, List[Message]],
        deliver_round: int,
        sent_messages: CounterType[str],
        round_messages: CounterType[str],
    ) -> None:
        if not destinations:
            return

        full_mask = tuple(True for _ in range(role_count))
        for peer_id in destinations:
            outbound[deliver_round].append(Message("commit", src, peer_id, full_mask))
            sent_messages["commit"] += 1
            round_messages["commit"] += 1

    def maybe_commit(
        self,
        node_id: int,
        state: NodeTxState,
        role_count: int,
        outbound: DefaultDict[int, List[Message]],
        deliver_round: int,
        sent_messages: CounterType[str],
        round_messages: CounterType[str],
        current_round: int,
    ) -> None:
        if state.commit_round is not None:
            return
        if any(count < self.threshold for count in state.counters):
            return

        state.commit_round = current_round
        self.emit_commit_messages(
            node_id,
            sorted(state.vote_senders),
            role_count,
            outbound,
            deliver_round,
            sent_messages,
            round_messages,
        )

    def recompute_counters(
        self,
        node_id: int,
        state: NodeTxState,
        role_centers: Sequence[int],
    ) -> None:
        counters = [0] * len(role_centers)
        for sender in state.vote_senders:
            for role_idx, center in enumerate(role_centers):
                if self.sender_counts_for_role(node_id, sender, center):
                    counters[role_idx] += 1
        state.counters = counters

    def run_transaction_case(
        self,
        origin: int,
        role_centers: Sequence[int],
    ) -> TxResult:
        role_count = len(role_centers)
        states: Dict[int, NodeTxState] = {
            origin: NodeTxState(
                aware_round=0,
                commit_round=None,
                counters=[0] * role_count,
                vote_senders=set(),
                sender_latest_roles={},
                seeded=True,
            )
        }
        outbound: DefaultDict[int, List[Message]] = defaultdict(list)
        sent_messages: CounterType[str] = Counter()
        spread_by_round: List[TxRoundSpread] = []
        final_round = 0
        quiesced = False

        round_messages: CounterType[str] = Counter()
        self.emit_seed_votes(
            origin,
            role_centers,
            self.origin_targets_per_side,
            outbound,
            1,
            sent_messages,
            round_messages,
        )
        spread_by_round.append(
            TxRoundSpread(
                round_num=0,
                aware_nodes=1,
                counted_nodes=0,
                committed_nodes=0,
                vote_messages=round_messages["vote"],
                commit_messages=round_messages["commit"],
            )
        )

        for round_num in range(1, self.max_rounds + 1):
            if round_num not in outbound:
                if not any(future_round > round_num for future_round in outbound):
                    final_round = round_num
                    quiesced = True
                    break
                spread_by_round.append(
                    TxRoundSpread(
                        round_num=round_num,
                        aware_nodes=sum(1 for state in states.values() if state.aware_round is not None),
                        counted_nodes=sum(
                            1 for state in states.values() if any(count > 0 for count in state.counters)
                        ),
                        committed_nodes=sum(1 for state in states.values() if state.commit_round is not None),
                        vote_messages=0,
                        commit_messages=0,
                    )
                )
                continue

            incoming = outbound.pop(round_num)
            round_messages = Counter()

            if len(incoming) == 0:
                final_round = round_num
                quiesced = True
                break

            for message in incoming:
                state = states.get(message.dst)
                first_awareness = state is None
                if state is None:
                    state = NodeTxState(
                        aware_round=round_num,
                        commit_round=None,
                        counters=[0] * role_count,
                        vote_senders=set(),
                        sender_latest_roles={},
                        seeded=False,
                    )
                    states[message.dst] = state

                if message.kind == "vote":
                    if state.commit_round is not None:
                        self.emit_commit_messages(
                            message.dst,
                            [message.src],
                            role_count,
                            outbound,
                            round_num + 1,
                            sent_messages,
                            round_messages,
                        )
                        continue

                    if first_awareness and not state.seeded:
                        state.seeded = True
                        self.emit_seed_votes(
                            message.dst,
                            role_centers,
                            self.targets_per_side,
                            outbound,
                            round_num + 1,
                            sent_messages,
                            round_messages,
                        )

                state.vote_senders.add(message.src)
                self.recompute_counters(message.dst, state, role_centers)

                self.maybe_commit(
                    message.dst,
                    state,
                    role_count,
                    outbound,
                    round_num + 1,
                    sent_messages,
                    round_messages,
                    round_num,
                )

            spread_by_round.append(
                TxRoundSpread(
                    round_num=round_num,
                    aware_nodes=sum(1 for state in states.values() if state.aware_round is not None),
                    counted_nodes=sum(
                        1 for state in states.values() if any(count > 0 for count in state.counters)
                    ),
                    committed_nodes=sum(1 for state in states.values() if state.commit_round is not None),
                    vote_messages=round_messages["vote"],
                    commit_messages=round_messages["commit"],
                )
            )
            final_round = round_num
        else:
            quiesced = False

        origin_commit_round = states[origin].commit_round
        aware_node_ids = {
            node_id for node_id, state in states.items() if state.aware_round is not None
        }
        committed_node_ids = {
            node_id for node_id, state in states.items() if state.commit_round is not None
        }
        return TxResult(
            origin=origin,
            role_centers=list(role_centers),
            origin_commit_round=origin_commit_round,
            aware_node_ids=aware_node_ids,
            committed_node_ids=committed_node_ids,
            final_aware_nodes=len(aware_node_ids),
            final_committed_nodes=len(committed_node_ids),
            final_round=final_round,
            quiesced=quiesced,
            sent_messages=sent_messages,
            spread_by_round=spread_by_round,
        )

    def run_transaction(self, tx_seed: int) -> TxResult:
        local_rng = random.Random(tx_seed)
        origin = local_rng.randrange(self.num_nodes)
        role_centers = [local_rng.getrandbits(64) for _ in range(self.roles_per_tx)]
        return self.run_transaction_case(origin, role_centers)

    def run(self) -> RunSummary:
        degrees = [len(peers) for peers in self.adjacency.values()]
        components = connected_components(self.adjacency)
        checkpoint_aware_fraction: Dict[int, List[float]] = defaultdict(list)
        checkpoint_counted_fraction: Dict[int, List[float]] = defaultdict(list)
        checkpoint_committed_fraction: Dict[int, List[float]] = defaultdict(list)
        checkpoint_vote_messages: Dict[int, List[float]] = defaultdict(list)
        checkpoint_commit_messages: Dict[int, List[float]] = defaultdict(list)
        round_aware_fraction: Dict[int, List[float]] = defaultdict(list)
        round_counted_fraction: Dict[int, List[float]] = defaultdict(list)
        round_committed_fraction: Dict[int, List[float]] = defaultdict(list)
        round_vote_messages: Dict[int, List[float]] = defaultdict(list)
        round_commit_messages: Dict[int, List[float]] = defaultdict(list)
        origin_commit_rounds: List[int] = []
        sample_transaction: Optional[TxResult] = None
        sent_messages: CounterType[str] = Counter()
        quiesce_rounds: List[int] = []
        final_aware_fraction: List[float] = []
        final_committed_fraction: List[float] = []
        node_aware_tx_counts = [0] * self.num_nodes
        node_committed_tx_counts = [0] * self.num_nodes

        for tx_index in range(self.transaction_count):
            tx_seed = self.rng.getrandbits(64)
            result = self.run_transaction(tx_seed)
            if sample_transaction is None:
                sample_transaction = result
            if result.origin_commit_round is not None:
                origin_commit_rounds.append(result.origin_commit_round)
            if result.quiesced:
                quiesce_rounds.append(result.final_round)
            sent_messages.update(result.sent_messages)
            final_aware_fraction.append(result.final_aware_nodes / self.num_nodes)
            final_committed_fraction.append(result.final_committed_nodes / self.num_nodes)
            for node_id in result.aware_node_ids:
                node_aware_tx_counts[node_id] += 1
            for node_id in result.committed_node_ids:
                node_committed_tx_counts[node_id] += 1

            spread_lookup = {spread.round_num: spread for spread in result.spread_by_round}
            last_spread = result.spread_by_round[-1]
            for round_num in range(self.max_rounds + 1):
                spread = spread_lookup.get(round_num, last_spread)
                round_aware_fraction[round_num].append(spread.aware_nodes / self.num_nodes)
                round_counted_fraction[round_num].append(spread.counted_nodes / self.num_nodes)
                round_committed_fraction[round_num].append(spread.committed_nodes / self.num_nodes)
                round_vote_messages[round_num].append(float(spread.vote_messages))
                round_commit_messages[round_num].append(float(spread.commit_messages))

            for checkpoint in self.checkpoints:
                spread = spread_lookup.get(checkpoint, last_spread)
                checkpoint_aware_fraction[checkpoint].append(
                    spread.aware_nodes / self.num_nodes
                )
                checkpoint_counted_fraction[checkpoint].append(
                    spread.counted_nodes / self.num_nodes
                )
                checkpoint_committed_fraction[checkpoint].append(
                    spread.committed_nodes / self.num_nodes
                )
                checkpoint_vote_messages[checkpoint].append(float(spread.vote_messages))
                checkpoint_commit_messages[checkpoint].append(float(spread.commit_messages))

        return RunSummary(
            topology=self.topology,
            nodes=self.num_nodes,
            transaction_count=self.transaction_count,
            roles_per_tx=self.roles_per_tx,
            degree_avg=statistics.mean(degrees) if degrees else 0.0,
            degree_p95=percentile(degrees, 0.95) if degrees else 0.0,
            components=len(components),
            largest_component=max((len(component) for component in components), default=0),
            isolated_nodes=sum(1 for degree in degrees if degree == 0),
            origin_commit_success_rate=len(origin_commit_rounds) / self.transaction_count
            if self.transaction_count
            else 0.0,
            origin_commit_rounds=origin_commit_rounds,
            quiesce_rate=len(quiesce_rounds) / self.transaction_count if self.transaction_count else 0.0,
            quiesce_rounds=quiesce_rounds,
            final_aware_fraction=final_aware_fraction,
            final_committed_fraction=final_committed_fraction,
            sent_messages=sent_messages,
            round_aware_fraction=dict(round_aware_fraction),
            round_counted_fraction=dict(round_counted_fraction),
            round_committed_fraction=dict(round_committed_fraction),
            round_vote_messages=dict(round_vote_messages),
            round_commit_messages=dict(round_commit_messages),
            checkpoint_rounds=self.checkpoints,
            checkpoint_aware_fraction=dict(checkpoint_aware_fraction),
            checkpoint_counted_fraction=dict(checkpoint_counted_fraction),
            checkpoint_committed_fraction=dict(checkpoint_committed_fraction),
            checkpoint_vote_messages=dict(checkpoint_vote_messages),
            checkpoint_commit_messages=dict(checkpoint_commit_messages),
            node_aware_tx_counts=node_aware_tx_counts,
            node_committed_tx_counts=node_committed_tx_counts,
            sample_transaction=sample_transaction,
        )


def print_summary(summary: RunSummary, show_sample: bool) -> None:
    degree_pct = (
        (summary.degree_avg / max(summary.nodes - 1, 1)) * 100.0
        if summary.nodes > 1
        else 0.0
    )
    print(f"topology: {summary.topology}")
    print(f"nodes: {summary.nodes}")
    print(f"transactions: {summary.transaction_count}")
    print(f"roles per transaction: {summary.roles_per_tx}")
    print(
        "graph: "
        f"avg degree {summary.degree_avg:.2f}, "
        f"avg degree pct {degree_pct:.2f}%, "
        f"p95 degree {summary.degree_p95:.2f}, "
        f"components {summary.components}, "
        f"largest {summary.largest_component}, "
        f"isolated {summary.isolated_nodes}"
    )
    print(
        "origin commit success: "
        f"{summary.origin_commit_success_rate * 100:.1f}%"
    )
    print(f"origin commit rounds: {format_metric(summary.origin_commit_rounds)}")
    print(f"quiesce rate: {summary.quiesce_rate * 100:.1f}%")
    print(f"quiesce rounds: {format_metric(summary.quiesce_rounds)}")
    print(
        "final fractions: "
        f"aware {statistics.mean(summary.final_aware_fraction) * 100:.2f}% | "
        f"committed {statistics.mean(summary.final_committed_fraction) * 100:.2f}%"
    )
    print(
        "messages / tx: "
        f"votes {summary.sent_messages['vote'] / max(summary.transaction_count, 1):.1f}, "
        f"commits {summary.sent_messages['commit'] / max(summary.transaction_count, 1):.1f}"
    )
    print(
        "node participation / tx-set: "
        f"aware {format_metric(summary.node_aware_tx_counts)}, "
        f"committed {format_metric(summary.node_committed_tx_counts)}"
    )
    print()
    print("checkpoint spread:")
    for checkpoint in summary.checkpoint_rounds:
        aware = summary.checkpoint_aware_fraction.get(checkpoint, [])
        counted = summary.checkpoint_counted_fraction.get(checkpoint, [])
        committed = summary.checkpoint_committed_fraction.get(checkpoint, [])
        votes = summary.checkpoint_vote_messages.get(checkpoint, [])
        commits = summary.checkpoint_commit_messages.get(checkpoint, [])
        print(
            f"  round {checkpoint:>3}: "
            f"aware {statistics.mean(aware) * 100:6.2f}% | "
            f"counted {statistics.mean(counted) * 100:6.2f}% | "
            f"committed {statistics.mean(committed) * 100:6.2f}% | "
            f"vote msgs {statistics.mean(votes):7.2f} | "
            f"commit msgs {statistics.mean(commits):7.2f}"
        )

    print()
    print("average round trace:")
    for round_num in sorted(summary.round_aware_fraction):
        aware = summary.round_aware_fraction.get(round_num, [])
        counted = summary.round_counted_fraction.get(round_num, [])
        committed = summary.round_committed_fraction.get(round_num, [])
        votes = summary.round_vote_messages.get(round_num, [])
        commits = summary.round_commit_messages.get(round_num, [])
        print(
            f"  round {round_num:>3}: "
            f"aware {statistics.mean(aware) * 100:6.2f}% | "
            f"counted {statistics.mean(counted) * 100:6.2f}% | "
            f"committed {statistics.mean(committed) * 100:6.2f}% | "
            f"vote msgs {statistics.mean(votes):7.2f} | "
            f"commit msgs {statistics.mean(commits):7.2f}"
        )

    if show_sample and summary.sample_transaction is not None:
        sample = summary.sample_transaction
        print()
        print("sample transaction:")
        print(f"  origin: {sample.origin}")
        print(f"  origin commit round: {sample.origin_commit_round}")
        print(f"  final aware nodes: {sample.final_aware_nodes}")
        print(f"  final committed nodes: {sample.final_committed_nodes}")
        print(f"  final round: {sample.final_round}")
        print(f"  quiesced: {sample.quiesced}")
        print(
            "  sent messages: "
            f"votes {sample.sent_messages['vote']}, "
            f"commits {sample.sent_messages['commit']}"
        )
        print("  round trace:")
        for spread in sample.spread_by_round:
            print(
                f"    round {spread.round_num:>3}: "
                f"aware {spread.aware_nodes:>4}, "
                f"counted {spread.counted_nodes:>4}, "
                f"committed {spread.committed_nodes:>4}, "
                f"vote msgs {spread.vote_messages:>4}, "
                f"commit msgs {spread.commit_messages:>4}"
            )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nodes", type=int, default=512)
    parser.add_argument(
        "--topology",
        choices=(
            "ring_gradient",
            "ring_core_tail",
            "ring_core_midband",
            "ring_stepwise_sample",
            "random_uniform",
            "linear_probability",
            "linear_probability_with_core",
            "heterogeneous_linear_slope",
            "full_table",
        ),
        default="ring_gradient",
    )
    parser.add_argument("--neighbors", type=int, default=8)
    parser.add_argument("--tail-peers-per-side", type=int, default=4)
    parser.add_argument("--target-degree", type=int, default=24)
    parser.add_argument("--target-degree-percent", type=float, default=None)
    parser.add_argument("--hetero-center-prob-min", type=float, default=0.90)
    parser.add_argument("--hetero-center-prob-max", type=float, default=1.00)
    parser.add_argument("--hetero-far-prob-min", type=float, default=0.00)
    parser.add_argument("--hetero-far-prob-max", type=float, default=0.10)
    parser.add_argument("--stepwise-mid-steps", type=int, default=32)
    parser.add_argument("--stepwise-mid-peers-per-side", type=int, default=4)
    parser.add_argument("--stepwise-far-peers-per-side", type=int, default=1)
    parser.add_argument("--transactions", type=int, default=20)
    parser.add_argument("--tokens-per-tx", type=int, default=2)
    parser.add_argument("--threshold", type=int, default=2)
    parser.add_argument("--targets-per-side", type=int, default=2)
    parser.add_argument("--origin-targets-per-side", type=int, default=None)
    parser.add_argument("--range-width", type=int, default=8)
    parser.add_argument("--max-rounds", type=int, default=64)
    parser.add_argument(
        "--checkpoints",
        type=int,
        nargs="*",
        default=[0, 1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64],
    )
    parser.add_argument("--seed", type=int, default=11)
    parser.add_argument("--show-sample", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    simulation = ReactiveVoteSimulation(
        num_nodes=args.nodes,
        topology=args.topology,
        neighbors=args.neighbors,
        tail_peers_per_side=args.tail_peers_per_side,
        target_degree=args.target_degree,
        target_degree_percent=args.target_degree_percent,
        hetero_center_prob_min=args.hetero_center_prob_min,
        hetero_center_prob_max=args.hetero_center_prob_max,
        hetero_far_prob_min=args.hetero_far_prob_min,
        hetero_far_prob_max=args.hetero_far_prob_max,
        stepwise_mid_steps=args.stepwise_mid_steps,
        stepwise_mid_peers_per_side=args.stepwise_mid_peers_per_side,
        stepwise_far_peers_per_side=args.stepwise_far_peers_per_side,
        roles_per_tx=args.tokens_per_tx,
        transaction_count=args.transactions,
        threshold=args.threshold,
        targets_per_side=args.targets_per_side,
        origin_targets_per_side=args.origin_targets_per_side,
        range_width=args.range_width,
        max_rounds=args.max_rounds,
        checkpoints=sorted(set(args.checkpoints)),
        seed=args.seed,
    )
    summary = simulation.run()
    print_summary(summary, show_sample=args.show_sample)


if __name__ == "__main__":
    main()
