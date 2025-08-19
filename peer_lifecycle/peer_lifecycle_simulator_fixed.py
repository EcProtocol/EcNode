#!/usr/bin/env python3
"""
Peer Life-Cycle Simulator - Fixed Version

Simulates the dynamic peer swapping mechanisms described in the Enhanced 
Synchronization with Block Chains document for the ecRust protocol.
"""

import random
import statistics
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Set, Optional, Tuple
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


class Peer:
    """Represents a peer in the network simulation."""
    
    def __init__(self, peer_id: int, max_connections: int = 10):
        self.id = peer_id
        self.max_connections = max_connections
        self.known_peers: Dict[int, PeerInfo] = {}
        self.pending_invites: List[Invite] = []
        
    def add_peer(self, peer_id: int, state: PeerState, last_heard_from: int = -1):
        """Add or update information about another peer."""
        self.known_peers[peer_id] = PeerInfo(state, last_heard_from)
    
    def get_peer_state(self, peer_id: int) -> Optional[PeerState]:
        """Get the current state of a known peer."""
        peer_info = self.known_peers.get(peer_id)
        return peer_info.state if peer_info else None
    
    def set_peer_state(self, peer_id: int, state: PeerState, round_num: int = -1):
        """Update the state of a peer."""
        if peer_id in self.known_peers:
            self.known_peers[peer_id].state = state
            if round_num >= 0:
                self.known_peers[peer_id].last_heard_from = round_num
        else:
            self.known_peers[peer_id] = PeerInfo(state, round_num)
    
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
        """Find the closest known peer to a target ID."""
        if exclude_states is None:
            exclude_states = set()
        
        candidates = [
            pid for pid, info in self.known_peers.items() 
            if info.state not in exclude_states
        ]
        
        if not candidates:
            return None
        
        return min(candidates, key=lambda pid: self._xor_distance(pid, target_id))
    
    def get_random_known_peer(self) -> Optional[int]:
        """Get a random known peer regardless of state."""
        if not self.known_peers:
            return None
        return random.choice(list(self.known_peers.keys()))
    
    def send_invite(self, target_peer_id: int, round_num: int):
        """Send an invitation to another peer (recorded for processing)."""
        # In real implementation, this would send a network message
        # For simulation, we'll handle this in the main loop
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
                # Unknown sender - add as prospect
                self.set_peer_state(invite.sender_id, PeerState.PROSPECT, round_num)
                state_changes.append((invite.sender_id, PeerState.PROSPECT))
        
        self.pending_invites.clear()
        return state_changes
    
    def remove_excess_connections(self) -> List[int]:
        """Remove random connected peers if over the limit."""
        connected = self.get_connected_peers()
        if len(connected) <= self.max_connections:
            return []
        
        excess_count = len(connected) - self.max_connections
        to_remove = random.sample(connected, excess_count)
        
        for peer_id in to_remove:
            del self.known_peers[peer_id]
        
        return to_remove
    
    def _xor_distance(self, peer1_id: int, peer2_id: int) -> int:
        """Calculate XOR distance between two peer IDs."""
        return peer1_id ^ peer2_id


class SimulationMetrics:
    """Collects and analyzes simulation metrics."""
    
    def __init__(self):
        self.round_data = []
        self.peer_connection_history = {}
        self.entry_times = {}  # candidate_id -> round when first connected
        self.connection_changes = []  # (round, peer_id, old_count, new_count)
        self.candidate_ids = set()
    
    def set_candidate_ids(self, candidate_ids: Set[int]):
        """Set which peers are candidates for tracking purposes."""
        self.candidate_ids = candidate_ids
    
    def record_round(self, round_num: int, peers: Dict[int, Peer]):
        """Record metrics for a single round."""
        round_metrics = {
            'round': round_num,
            'total_connections': 0,
            'avg_connections': 0,
            'connection_distribution': [],
            'state_counts': {state.value: 0 for state in PeerState},
            'candidates_connected': 0
        }
        
        connection_counts = []
        for peer in peers.values():
            connected_count = len(peer.get_connected_peers())
            connection_counts.append(connected_count)
            round_metrics['total_connections'] += connected_count
            
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
            'final_candidates_connected': final_round['candidates_connected']
        }


class NetworkSimulator:
    """Main simulation engine for peer lifecycle dynamics."""
    
    def __init__(self, config: SimulationConfig):
        self.config = config
        self.peers: Dict[int, Peer] = {}
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
            peer = Peer(peer_id, self.config.max_connections_per_peer)
            # Add all other connected peers as Connected
            for other_id in connected_ids:
                if other_id != peer_id:
                    peer.add_peer(other_id, PeerState.CONNECTED, 0)
            self.peers[peer_id] = peer
        
        # Generate candidate set with limited identified peers
        candidate_ids = self._generate_peer_ids(self.config.candidate_set_size, exclude=connected_ids)
        self.candidate_set = set(candidate_ids)
        
        for peer_id in candidate_ids:
            peer = Peer(peer_id, self.config.max_connections_per_peer)
            # Add random connected peers as Identified
            num_identified = min(self.config.initial_identified_peers, len(connected_ids))
            identified_peers = random.sample(connected_ids, num_identified)
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
    
    def simulate_mapping_request(self, peer: Peer) -> Optional[int]:
        """Simulate a peer performing a mapping request."""
        # Generate random target ID
        max_id = (1 << self.config.address_space_bits) - 1
        target_id = random.randint(0, max_id)
        
        # Collect starting points for the search
        starting_points = []
        
        # 1. Add closest known peer as starting point
        closest_peer_id = peer.find_closest_peer_to_target(target_id)
        if closest_peer_id and closest_peer_id in self.peers:
            starting_points.append(closest_peer_id)
        
        # 2. Add 2 random known peers as starting points
        all_known = peer.get_all_known_peers()
        available_randoms = [pid for pid in all_known if pid != closest_peer_id and pid in self.peers]
        
        num_random = min(2, len(available_randoms))
        if num_random > 0:
            random_starts = random.sample(available_randoms, num_random)
            starting_points.extend(random_starts)
        
        if not starting_points:
            return None
        
        # Perform recursive search from each starting point
        responses = []
        for start_id in starting_points:
            response = self._recursive_search_with_hop_limit(start_id, target_id, max_hops=10)
            if response:
                responses.append(response)
        
        if not responses:
            return None
        
        # Select the response closest to target
        best_response = min(responses, 
                           key=lambda pid: peer._xor_distance(pid, target_id))
        
        return best_response
    
    def _recursive_search_with_hop_limit(self, start_peer_id: int, target_id: int, max_hops: int) -> Optional[int]:
        """Recursive search with hop limit to prevent infinite loops."""
        if max_hops <= 0 or start_peer_id not in self.peers:
            return start_peer_id
            
        current_peer = self.peers[start_peer_id]
        connected_peers = current_peer.get_connected_peers()
        
        # If no connected peers, respond with own ID
        if not connected_peers:
            return start_peer_id
        
        # Find closest connected peer
        closest_connected = min(connected_peers,
                               key=lambda pid: current_peer._xor_distance(pid, target_id))
        
        # If current peer is closer or equal, respond with own ID
        current_distance = current_peer._xor_distance(start_peer_id, target_id)
        closest_distance = current_peer._xor_distance(closest_connected, target_id)
        
        if current_distance <= closest_distance:
            return start_peer_id
        
        # Continue search with reduced hop count
        return self._recursive_search_with_hop_limit(closest_connected, target_id, max_hops - 1)
    
    def simulate_round(self):
        """Simulate one round of the peer lifecycle."""
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
            
            # Send maintenance invites to 2 connected peers
            connected_peers = peer.get_connected_peers()
            if len(connected_peers) >= 2:
                selected_peers = random.sample(connected_peers, 2)
                for target_id in selected_peers:
                    if target_id in self.peers:
                        invite = Invite(peer.id, self.round_number)
                        self.peers[target_id].receive_invite(invite)
            
            # Remove excess connections
            peer.remove_excess_connections()
        
        # Record metrics for this round
        self.metrics.record_round(self.round_number, self.peers)
        self.round_number += 1
    
    def run_simulation(self) -> SimulationMetrics:
        """Run the complete simulation."""
        print(f"Starting simulation with {len(self.connected_set)} connected peers and {len(self.candidate_set)} candidates")
        
        for round_num in range(self.config.simulation_rounds):
            if round_num % 100 == 0:
                print(f"Round {round_num}/{self.config.simulation_rounds}")
            
            self.simulate_round()
        
        print("Simulation completed")
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
            state['peer_states'][peer_id] = {
                'type': peer_type,
                'connected_count': len(peer.get_connected_peers()),
                'total_known': len(peer.known_peers),
                'state_breakdown': {
                    state.value: len(peer.get_peers_by_state(state))
                    for state in PeerState
                }
            }
        
        return state


def run_detailed_analysis():
    """Run simulation with detailed tracking and analysis."""
    config = SimulationConfig(
        connected_set_size=30,
        candidate_set_size=15,
        max_connections_per_peer=6,
        initial_identified_peers=3,
        simulation_rounds=200,
        random_seed=42
    )
    
    simulator = NetworkSimulator(config)
    
    # Track initial state
    print("=== Initial Network State ===")
    initial_state = simulator.get_network_state()
    
    # Show some candidate initial states
    candidate_count = 0
    for peer_id, peer_data in initial_state['peer_states'].items():
        if peer_data['type'] == 'candidate' and candidate_count < 5:
            print(f"Candidate {peer_id}: {peer_data['state_breakdown']}")
            candidate_count += 1
    
    metrics = simulator.run_simulation()
    
    # Print summary statistics
    stats = metrics.get_summary_stats()
    print("\n=== Simulation Results ===")
    print(f"Total rounds: {stats['total_rounds']}")
    print(f"Total candidates: {stats['total_candidates']}")
    print(f"Candidates that achieved connection: {stats['candidates_that_connected']}")
    print(f"Entry success rate: {stats['entry_success_rate']:.2%}")
    print(f"Final candidates with connections: {stats['final_candidates_connected']}")
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


def main():
    """Run a sample simulation with default parameters."""
    run_detailed_analysis()


if __name__ == "__main__":
    main()