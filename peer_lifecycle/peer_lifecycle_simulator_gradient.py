#!/usr/bin/env python3
"""
Peer Life-Cycle Simulator - Gradient Distance Version

Implements the gradient clustering approach with arithmetic distance instead of XOR.
Uses density gradient around each peer's own ID for better local knowledge and routing.
"""

import random
import statistics
import math
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Set, Optional, Tuple
from collections import defaultdict
import json


class PeerState(Enum):
    """States that peers can have in relation to other peers."""
    CONNECTED = "Connected"
    PROSPECT = "Prospect"
    PENDING = "Pending"
    IDENTIFIED = "Identified"


@dataclass
class PeerInfo:
    """Information stored about another peer."""
    state: PeerState
    last_heard_from: int = -1  # Round number, -1 for never contacted
    distance_class: int = -1   # Which distance class this peer belongs to


@dataclass
class Invite:
    """Represents an invitation to become connected."""
    sender_id: int
    round_sent: int


@dataclass
class SimulationConfig:
    """Configuration parameters for the simulation."""
    connected_set_size: int = 100
    candidate_set_size: int = 50
    max_connections_per_peer: int = 10
    initial_identified_peers: int = 4
    simulation_rounds: int = 1000
    address_space_bits: int = 32
    random_seed: Optional[int] = None
    gradient_decay_factor: float = 0.5  # How much to reduce connections per distance class


class GradientPeer:
    """Represents a peer using gradient distance-based clustering."""
    
    def __init__(self, peer_id: int, max_connections: int = 10, address_space_bits: int = 32):
        self.id = peer_id
        self.max_connections = max_connections
        self.address_space_bits = address_space_bits
        self.address_space_size = 1 << address_space_bits
        self.known_peers: Dict[int, PeerInfo] = {}
        self.pending_invites: List[Invite] = []
        
        # Gradient clustering: organize connections by distance classes
        self.distance_classes: Dict[int, List[int]] = defaultdict(list)
        self.max_distance_classes = int(math.log2(self.address_space_size)) + 1
        
        # Allocate connection budget across distance classes with exponential decay
        self.distance_class_budgets = self._calculate_distance_budgets()
    
    def _calculate_distance_budgets(self) -> Dict[int, int]:
        """Calculate how many connections to allocate per distance class."""
        budgets = {}
        remaining_budget = self.max_connections * 5  # Allow more total for gradient effect
        
        for distance_class in range(self.max_distance_classes):
            # Exponential decay: closer classes get more connections
            if distance_class == 0:
                allocation = max(1, remaining_budget // 3)  # Local area gets most
            else:
                allocation = max(1, remaining_budget // (2 ** distance_class))
            
            budgets[distance_class] = min(allocation, remaining_budget)
            remaining_budget = max(0, remaining_budget - allocation)
            
            if remaining_budget == 0:
                break
        
        return budgets
    
    def _arithmetic_distance(self, peer1_id: int, peer2_id: int) -> int:
        """Calculate arithmetic distance between two peer IDs."""
        return abs(peer1_id - peer2_id)
    
    def _ring_distance(self, peer1_id: int, peer2_id: int) -> int:
        """Calculate ring distance (shortest path around the ring)."""
        direct_distance = abs(peer1_id - peer2_id)
        wrap_distance = self.address_space_size - direct_distance
        return min(direct_distance, wrap_distance)
    
    def _get_distance_class(self, distance: int) -> int:
        """Determine which distance class a given distance belongs to."""
        if distance == 0:
            return 0
        return min(int(math.log2(distance)) + 1, self.max_distance_classes - 1)
    
    def add_peer(self, peer_id: int, state: PeerState, last_heard_from: int = -1):
        """Add or update information about another peer."""
        distance = self._ring_distance(self.id, peer_id)
        distance_class = self._get_distance_class(distance)
        
        peer_info = PeerInfo(state, last_heard_from, distance_class)
        self.known_peers[peer_id] = peer_info
        
        # Update distance class tracking
        self._update_distance_class_membership()
    
    def _update_distance_class_membership(self):
        """Update which peers belong to which distance classes."""
        self.distance_classes.clear()
        
        for peer_id, peer_info in self.known_peers.items():
            if peer_info.state == PeerState.CONNECTED:
                self.distance_classes[peer_info.distance_class].append(peer_id)
    
    def get_peer_state(self, peer_id: int) -> Optional[PeerState]:
        """Get the current state of a known peer."""
        peer_info = self.known_peers.get(peer_id)
        return peer_info.state if peer_info else None
    
    def set_peer_state(self, peer_id: int, state: PeerState, round_num: int = -1):
        """Update the state of a peer."""
        distance = self._ring_distance(self.id, peer_id)
        distance_class = self._get_distance_class(distance)
        
        if peer_id in self.known_peers:
            self.known_peers[peer_id].state = state
            self.known_peers[peer_id].distance_class = distance_class
            if round_num >= 0:
                self.known_peers[peer_id].last_heard_from = round_num
        else:
            self.known_peers[peer_id] = PeerInfo(state, round_num, distance_class)
        
        self._update_distance_class_membership()
    
    def update_last_heard_from(self, peer_id: int, round_num: int):
        """Update when we last heard from a peer."""
        if peer_id in self.known_peers:
            self.known_peers[peer_id].last_heard_from = round_num
    
    def get_peers_by_state(self, state: PeerState) -> List[int]:
        """Get all peers in a specific state."""
        return [pid for pid, info in self.known_peers.items() if info.state == state]
    
    def get_connected_peers(self) -> List[int]:
        """Get all connected peers."""
        return self.get_peers_by_state(PeerState.CONNECTED)
    
    def get_all_known_peers(self) -> List[int]:
        """Get all known peers regardless of state."""
        return list(self.known_peers.keys())
    
    def find_closest_peer_to_target(self, target_id: int, exclude_states: Set[PeerState] = None) -> Optional[int]:
        """Find the closest known peer to a target ID using gradient approach."""
        if exclude_states is None:
            exclude_states = set()
        
        # Collect candidates from appropriate distance classes
        target_distance = self._ring_distance(self.id, target_id)
        target_distance_class = self._get_distance_class(target_distance)
        
        candidates = []
        
        # First try the exact distance class
        for peer_id, peer_info in self.known_peers.items():
            if (peer_info.state not in exclude_states and 
                peer_info.distance_class == target_distance_class):
                candidates.append(peer_id)
        
        # If no candidates in exact class, try adjacent classes
        if not candidates:
            for offset in [1, -1, 2, -2]:
                check_class = target_distance_class + offset
                if 0 <= check_class < self.max_distance_classes:
                    for peer_id, peer_info in self.known_peers.items():
                        if (peer_info.state not in exclude_states and 
                            peer_info.distance_class == check_class):
                            candidates.append(peer_id)
                    if candidates:
                        break
        
        # If still no candidates, use any available peer
        if not candidates:
            candidates = [
                pid for pid, info in self.known_peers.items() 
                if info.state not in exclude_states
            ]
        
        if not candidates:
            return None
        
        # Return the closest candidate to the target
        return min(candidates, key=lambda pid: self._ring_distance(pid, target_id))
    
    def get_random_known_peer(self) -> Optional[int]:
        """Get a random known peer, preferring closer peers."""
        if not self.known_peers:
            return None
        
        # Weight selection by inverse distance class (closer = higher probability)
        weighted_candidates = []
        for peer_id, peer_info in self.known_peers.items():
            weight = 1.0 / (peer_info.distance_class + 1)  # +1 to avoid division by zero
            weighted_candidates.extend([peer_id] * int(weight * 10))
        
        if weighted_candidates:
            return random.choice(weighted_candidates)
        return random.choice(list(self.known_peers.keys()))
    
    def send_invite(self, target_peer_id: int, round_num: int):
        """Send an invitation to another peer."""
        pass
    
    def receive_invite(self, invite: Invite):
        """Receive an invitation from another peer."""
        self.pending_invites.append(invite)
    
    def process_invites(self, round_num: int) -> List[Tuple[int, PeerState]]:
        """Process all pending invites and return state changes."""
        state_changes = []
        
        for invite in self.pending_invites:
            sender_state = self.get_peer_state(invite.sender_id)
            
            if sender_state == PeerState.PENDING:
                self.set_peer_state(invite.sender_id, PeerState.CONNECTED, round_num)
                state_changes.append((invite.sender_id, PeerState.CONNECTED))
                
            elif sender_state == PeerState.IDENTIFIED:
                self.set_peer_state(invite.sender_id, PeerState.PROSPECT, round_num)
                state_changes.append((invite.sender_id, PeerState.PROSPECT))
                
            elif sender_state == PeerState.CONNECTED:
                self.update_last_heard_from(invite.sender_id, round_num)
                state_changes.append((invite.sender_id, PeerState.CONNECTED))
            
            elif sender_state is None:
                self.set_peer_state(invite.sender_id, PeerState.PROSPECT, round_num)
                state_changes.append((invite.sender_id, PeerState.PROSPECT))
        
        self.pending_invites.clear()
        return state_changes
    
    def remove_excess_connections(self) -> List[int]:
        """Remove excess connections using gradient-aware strategy."""
        connected = self.get_connected_peers()
        if len(connected) <= self.max_connections:
            return []
        
        # Group by distance class
        class_groups = defaultdict(list)
        for peer_id in connected:
            peer_info = self.known_peers[peer_id]
            class_groups[peer_info.distance_class].append(peer_id)
        
        to_remove = []
        excess_count = len(connected) - self.max_connections
        
        # Remove from furthest classes first, but maintain some representation
        for distance_class in sorted(class_groups.keys(), reverse=True):
            if excess_count <= 0:
                break
            
            peers_in_class = class_groups[distance_class]
            budget_for_class = self.distance_class_budgets.get(distance_class, 1)
            
            if len(peers_in_class) > budget_for_class:
                excess_in_class = len(peers_in_class) - budget_for_class
                remove_count = min(excess_in_class, excess_count)
                
                # Remove random peers from this class
                to_remove.extend(random.sample(peers_in_class, remove_count))
                excess_count -= remove_count
        
        # Remove the selected peers
        for peer_id in to_remove:
            del self.known_peers[peer_id]
        
        self._update_distance_class_membership()
        return to_remove


class SimulationMetrics:
    """Collects and analyzes simulation metrics."""
    
    def __init__(self):
        self.round_data = []
        self.peer_connection_history = {}
        self.entry_times = {}
        self.connection_changes = []
        self.candidate_ids = set()
        self.gradient_metrics = []  # Track gradient-specific metrics
    
    def set_candidate_ids(self, candidate_ids: Set[int]):
        """Set which peers are candidates for tracking purposes."""
        self.candidate_ids = candidate_ids
    
    def record_round(self, round_num: int, peers: Dict[int, GradientPeer]):
        """Record metrics for a single round."""
        round_metrics = {
            'round': round_num,
            'total_connections': 0,
            'avg_connections': 0,
            'connection_distribution': [],
            'state_counts': {state.value: 0 for state in PeerState},
            'candidates_connected': 0,
            'avg_distance_classes_used': 0,
            'gradient_efficiency': 0
        }
        
        connection_counts = []
        distance_classes_used = []
        
        for peer in peers.values():
            connected_count = len(peer.get_connected_peers())
            connection_counts.append(connected_count)
            round_metrics['total_connections'] += connected_count
            
            # Track distance class usage
            classes_used = len([c for c, peers_in_class in peer.distance_classes.items() if peers_in_class])
            distance_classes_used.append(classes_used)
            
            # Track individual peer connection history
            if peer.id not in self.peer_connection_history:
                self.peer_connection_history[peer.id] = []
            self.peer_connection_history[peer.id].append(connected_count)
            
            # Count states
            for peer_info in peer.known_peers.values():
                round_metrics['state_counts'][peer_info.state.value] += 1
            
            # Track candidates that have connections
            if peer.id in self.candidate_ids and connected_count > 0:
                round_metrics['candidates_connected'] += 1
                if peer.id not in self.entry_times:
                    self.entry_times[peer.id] = round_num
        
        if connection_counts:
            round_metrics['avg_connections'] = statistics.mean(connection_counts)
            round_metrics['connection_distribution'] = connection_counts.copy()
        
        if distance_classes_used:
            round_metrics['avg_distance_classes_used'] = statistics.mean(distance_classes_used)
        
        self.round_data.append(round_metrics)
    
    def record_entry(self, candidate_id: int, round_num: int):
        """Record when a candidate peer first achieves connected status."""
        if candidate_id not in self.entry_times:
            self.entry_times[candidate_id] = round_num
    
    def get_summary_stats(self) -> Dict:
        """Generate summary statistics for the simulation."""
        if not self.round_data:
            return {}
        
        final_round = self.round_data[-1]
        entry_times_list = list(self.entry_times.values())
        
        return {
            'total_rounds': len(self.round_data),
            'final_total_connections': final_round['total_connections'],
            'final_avg_connections': final_round['avg_connections'],
            'candidates_that_connected': len(entry_times_list),
            'total_candidates': len(self.candidate_ids),
            'avg_entry_time': statistics.mean(entry_times_list) if entry_times_list else 0,
            'median_entry_time': statistics.median(entry_times_list) if entry_times_list else 0,
            'max_entry_time': max(entry_times_list) if entry_times_list else 0,
            'entry_success_rate': len(entry_times_list) / len(self.candidate_ids) if self.candidate_ids else 0,
            'final_candidates_connected': final_round['candidates_connected'],
            'avg_distance_classes_used': final_round['avg_distance_classes_used']
        }


class GradientNetworkSimulator:
    """Network simulator using gradient distance-based clustering."""
    
    def __init__(self, config: SimulationConfig):
        self.config = config
        self.peers: Dict[int, GradientPeer] = {}
        self.connected_set: Set[int] = set()
        self.candidate_set: Set[int] = set()
        self.round_number = 0
        self.metrics = SimulationMetrics()
        
        if config.random_seed is not None:
            random.seed(config.random_seed)
        
        self._initialize_network()
    
    def _initialize_network(self):
        """Initialize the network with connected and candidate peers."""
        # Generate connected set with bidirectional relationships
        connected_ids = self._generate_peer_ids(self.config.connected_set_size)
        self.connected_set = set(connected_ids)
        
        for peer_id in connected_ids:
            peer = GradientPeer(peer_id, self.config.max_connections_per_peer, self.config.address_space_bits)
            # Add all other connected peers as Connected, respecting distance classes
            for other_id in connected_ids:
                if other_id != peer_id:
                    peer.add_peer(other_id, PeerState.CONNECTED, 0)
            self.peers[peer_id] = peer
        
        # Generate candidate set with limited identified peers
        candidate_ids = self._generate_peer_ids(self.config.candidate_set_size, exclude=connected_ids)
        self.candidate_set = set(candidate_ids)
        
        for peer_id in candidate_ids:
            peer = GradientPeer(peer_id, self.config.max_connections_per_peer, self.config.address_space_bits)
            # Add random connected peers as Identified, preferring closer ones
            num_identified = min(self.config.initial_identified_peers, len(connected_ids))
            
            # Sort connected peers by distance and take a mix of close and distant
            connected_by_distance = sorted(connected_ids, 
                                         key=lambda cid: peer._ring_distance(peer_id, cid))
            
            # Take mostly close peers but include some distant ones
            close_count = max(1, num_identified * 2 // 3)
            distant_count = num_identified - close_count
            
            identified_peers = (connected_by_distance[:close_count] + 
                              random.sample(connected_by_distance[close_count:], 
                                          min(distant_count, len(connected_by_distance[close_count:]))))
            
            for identified_id in identified_peers:
                peer.add_peer(identified_id, PeerState.IDENTIFIED, -1)
            self.peers[peer_id] = peer
        
        # Update metrics tracking
        self.metrics.set_candidate_ids(self.candidate_set)
    
    def _generate_peer_ids(self, count: int, exclude: List[int] = None) -> List[int]:
        """Generate unique random peer IDs."""
        if exclude is None:
            exclude = []
        exclude_set = set(exclude)
        
        max_id = (1 << self.config.address_space_bits) - 1
        generated = set()
        
        while len(generated) < count:
            peer_id = random.randint(0, max_id)
            if peer_id not in exclude_set and peer_id not in generated:
                generated.add(peer_id)
        
        return list(generated)
    
    def simulate_mapping_request(self, peer: GradientPeer) -> Optional[int]:
        """Simulate a peer performing a mapping request using gradient routing."""
        # Generate random target ID
        max_id = (1 << self.config.address_space_bits) - 1
        target_id = random.randint(0, max_id)
        
        # Collect starting points using gradient approach
        starting_points = []
        
        # 1. Add closest known peer as starting point
        closest_peer_id = peer.find_closest_peer_to_target(target_id)
        if closest_peer_id and closest_peer_id in self.peers:
            starting_points.append(closest_peer_id)
        
        # 2. Add peers from appropriate distance class
        target_distance = peer._ring_distance(peer.id, target_id)
        target_distance_class = peer._get_distance_class(target_distance)
        
        # Get peers from the target distance class and adjacent classes
        for offset in [0, 1, -1]:
            check_class = target_distance_class + offset
            if check_class in peer.distance_classes:
                available_peers = [pid for pid in peer.distance_classes[check_class] 
                                 if pid != closest_peer_id and pid in self.peers]
                num_to_add = min(2, len(available_peers))
                if num_to_add > 0:
                    starting_points.extend(random.sample(available_peers, num_to_add))
                    break
        
        if not starting_points:
            return None
        
        # Perform recursive search from each starting point
        responses = []
        for start_id in starting_points:
            response = self._gradient_recursive_search(start_id, target_id, max_hops=10)
            if response:
                responses.append(response)
        
        if not responses:
            return None
        
        # Select the response closest to target using ring distance
        best_response = min(responses, 
                           key=lambda pid: self.peers[peer.id]._ring_distance(pid, target_id))
        
        return best_response
    
    def _gradient_recursive_search(self, start_peer_id: int, target_id: int, max_hops: int) -> Optional[int]:
        """Gradient-aware recursive search with hop limit."""
        if max_hops <= 0 or start_peer_id not in self.peers:
            return start_peer_id
            
        current_peer = self.peers[start_peer_id]
        connected_peers = current_peer.get_connected_peers()
        
        # If no connected peers, respond with own ID
        if not connected_peers:
            return start_peer_id
        
        # Find closest connected peer using ring distance
        closest_connected = min(connected_peers,
                               key=lambda pid: current_peer._ring_distance(pid, target_id))
        
        # If current peer is closer or equal, respond with own ID
        current_distance = current_peer._ring_distance(start_peer_id, target_id)
        closest_distance = current_peer._ring_distance(closest_connected, target_id)
        
        if current_distance <= closest_distance:
            return start_peer_id
        
        # Continue search with reduced hop count
        return self._gradient_recursive_search(closest_connected, target_id, max_hops - 1)
    
    def simulate_round(self):
        """Simulate one round of the peer lifecycle using gradient approach."""
        invites_to_send = []  # (sender_id, receiver_id)
        
        # Phase 1: Each peer performs mapping request and handles response
        for peer in self.peers.values():
            discovered_peer_id = self.simulate_mapping_request(peer)
            
            if discovered_peer_id and discovered_peer_id != peer.id and discovered_peer_id in self.peers:
                current_state = peer.get_peer_state(discovered_peer_id)
                
                if current_state is None:  # Unknown peer
                    peer.set_peer_state(discovered_peer_id, PeerState.PENDING, self.round_number)
                    invites_to_send.append((peer.id, discovered_peer_id))
                    
                elif current_state == PeerState.IDENTIFIED:
                    peer.set_peer_state(discovered_peer_id, PeerState.PENDING, self.round_number)
                    invites_to_send.append((peer.id, discovered_peer_id))
                    
                elif current_state == PeerState.PROSPECT:
                    peer.set_peer_state(discovered_peer_id, PeerState.CONNECTED, self.round_number)
                    invites_to_send.append((peer.id, discovered_peer_id))
                    
                elif current_state == PeerState.CONNECTED:
                    peer.update_last_heard_from(discovered_peer_id, self.round_number)
                    invites_to_send.append((peer.id, discovered_peer_id))
        
        # Phase 2: Send invites
        for sender_id, receiver_id in invites_to_send:
            if receiver_id in self.peers:
                invite = Invite(sender_id, self.round_number)
                self.peers[receiver_id].receive_invite(invite)
        
        # Phase 3: Process invites and maintenance
        for peer in self.peers.values():
            # Process received invites
            state_changes = peer.process_invites(self.round_number)
            
            # Send maintenance invites to connected peers from different distance classes
            connected_peers = peer.get_connected_peers()
            if len(connected_peers) >= 2:
                # Try to select peers from different distance classes for diversity
                selected_peers = []
                used_classes = set()
                
                for peer_id in connected_peers:
                    peer_info = peer.known_peers[peer_id]
                    if peer_info.distance_class not in used_classes and len(selected_peers) < 2:
                        selected_peers.append(peer_id)
                        used_classes.add(peer_info.distance_class)
                
                # Fill remaining slots if needed
                while len(selected_peers) < 2 and len(selected_peers) < len(connected_peers):
                    remaining = [p for p in connected_peers if p not in selected_peers]
                    selected_peers.append(random.choice(remaining))
                
                for target_id in selected_peers:
                    if target_id in self.peers:
                        invite = Invite(peer.id, self.round_number)
                        self.peers[target_id].receive_invite(invite)
            
            # Remove excess connections using gradient-aware strategy
            peer.remove_excess_connections()
        
        # Record metrics for this round
        self.metrics.record_round(self.round_number, self.peers)
        self.round_number += 1
    
    def run_simulation(self) -> SimulationMetrics:
        """Run the complete simulation."""
        print(f"Starting gradient simulation with {len(self.connected_set)} connected peers and {len(self.candidate_set)} candidates")
        
        for round_num in range(self.config.simulation_rounds):
            if round_num % 100 == 0:
                print(f"Round {round_num}/{self.config.simulation_rounds}")
            
            self.simulate_round()
        
        print("Gradient simulation completed")
        return self.metrics
    
    def get_network_state(self) -> Dict:
        """Get current network state for analysis."""
        state = {
            'round': self.round_number,
            'connected_set_size': len(self.connected_set),
            'candidate_set_size': len(self.candidate_set),
            'peer_states': {}
        }
        
        for peer_id, peer in self.peers.items():
            peer_type = 'connected' if peer_id in self.connected_set else 'candidate'
            distance_class_info = {
                cls: len(peers) for cls, peers in peer.distance_classes.items()
            }
            
            state['peer_states'][peer_id] = {
                'type': peer_type,
                'connected_count': len(peer.get_connected_peers()),
                'total_known': len(peer.known_peers),
                'distance_classes': distance_class_info,
                'state_breakdown': {
                    state.value: len(peer.get_peers_by_state(state))
                    for state in PeerState
                }
            }
        
        return state


def run_gradient_analysis():
    """Run simulation with gradient approach and detailed analysis."""
    config = SimulationConfig(
        connected_set_size=30,
        candidate_set_size=15,
        max_connections_per_peer=6,
        initial_identified_peers=3,
        simulation_rounds=200,
        random_seed=42
    )
    
    simulator = GradientNetworkSimulator(config)
    
    # Track initial state
    print("=== Initial Gradient Network State ===")
    initial_state = simulator.get_network_state()
    
    # Show some candidate initial states with distance class info
    candidate_count = 0
    for peer_id, peer_data in initial_state['peer_states'].items():
        if peer_data['type'] == 'candidate' and candidate_count < 5:
            print(f"Candidate {peer_id}: {peer_data['state_breakdown']}")
            print(f"  Distance classes: {peer_data['distance_classes']}")
            candidate_count += 1
    
    metrics = simulator.run_simulation()
    
    # Print summary statistics
    stats = metrics.get_summary_stats()
    print("\n=== Gradient Simulation Results ===")
    print(f"Total rounds: {stats['total_rounds']}")
    print(f"Total candidates: {stats['total_candidates']}")
    print(f"Candidates that achieved connection: {stats['candidates_that_connected']}")
    print(f"Entry success rate: {stats['entry_success_rate']:.2%}")
    print(f"Final candidates with connections: {stats['final_candidates_connected']}")
    print(f"Average distance classes used: {stats['avg_distance_classes_used']:.1f}")
    
    if stats['candidates_that_connected'] > 0:
        print(f"Average time to first connection: {stats['avg_entry_time']:.1f} rounds")
        print(f"Median time to first connection: {stats['median_entry_time']:.1f} rounds")
        print(f"Maximum time to first connection: {stats['max_entry_time']} rounds")
    
    # Show round-by-round progress for candidates
    print("\n=== Candidate Connection Progress ===")
    for i in range(0, min(len(metrics.round_data), 201), 50):
        round_data = metrics.round_data[i]
        print(f"Round {i}: {round_data['candidates_connected']} candidates connected")
    
    return simulator, metrics


def compare_approaches():
    """Compare XOR vs Gradient approaches side by side."""
    print("=== Comparing XOR vs Gradient Approaches ===")
    
    # Import the original simulator
    import sys
    sys.path.append('/workspaces/ecRust/peer_lifecycle')
    from peer_lifecycle_simulator_fixed import NetworkSimulator, SimulationConfig as OriginalConfig
    
    config = OriginalConfig(
        connected_set_size=30,
        candidate_set_size=15,
        max_connections_per_peer=6,
        initial_identified_peers=3,
        simulation_rounds=200,
        random_seed=42
    )
    
    # Run XOR version
    print("\n--- Running XOR Version ---")
    xor_simulator = NetworkSimulator(config)
    xor_metrics = xor_simulator.run_simulation()
    xor_stats = xor_metrics.get_summary_stats()
    
    # Run Gradient version
    print("\n--- Running Gradient Version ---")
    gradient_config = SimulationConfig(
        connected_set_size=30,
        candidate_set_size=15,
        max_connections_per_peer=6,
        initial_identified_peers=3,
        simulation_rounds=200,
        random_seed=42
    )
    gradient_simulator = GradientNetworkSimulator(gradient_config)
    gradient_metrics = gradient_simulator.run_simulation()
    gradient_stats = gradient_metrics.get_summary_stats()
    
    # Compare results
    print("\n=== Comparison Results ===")
    print(f"{'Metric':<30} {'XOR':<15} {'Gradient':<15} {'Difference':<15}")
    print("-" * 75)
    
    metrics_to_compare = [
        ('Entry Success Rate', 'entry_success_rate', '{:.2%}'),
        ('Avg Entry Time', 'avg_entry_time', '{:.1f}'),
        ('Final Connected Candidates', 'final_candidates_connected', '{}'),
        ('Final Avg Connections', 'final_avg_connections', '{:.1f}'),
    ]
    
    for name, key, fmt_str in metrics_to_compare:
        xor_val = xor_stats.get(key, 0)
        grad_val = gradient_stats.get(key, 0)
        
        if 'rate' in key or 'percentage' in key:
            diff = grad_val - xor_val
            diff_str = f"+{diff:.2%}" if diff > 0 else f"{diff:.2%}"
        else:
            diff = grad_val - xor_val
            diff_str = f"+{diff:.1f}" if diff > 0 else f"{diff:.1f}"
        
        print(f"{name:<30} {fmt_str.format(xor_val):<15} {fmt_str.format(grad_val):<15} {diff_str:<15}")
    
    return xor_stats, gradient_stats


def main():
    """Run gradient approach analysis."""
    run_gradient_analysis()
    
    # Run comparison
    compare_approaches()


if __name__ == "__main__":
    main()