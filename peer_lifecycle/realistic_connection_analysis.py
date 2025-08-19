#!/usr/bin/env python3
"""
More realistic analysis of connection achievement with network constraints.
"""

from peer_lifecycle_simulator_fixed import *
import random


def create_realistic_network_config():
    """Create network with more realistic constraints."""
    
    class RealisticNetworkSimulator(NetworkSimulator):
        """Enhanced simulator with realistic network constraints."""
        
        def __init__(self, config):
            super().__init__(config)
            # Add realistic constraints
            self.connection_failure_rate = 0.05  # 5% of connection attempts fail
            self.message_loss_rate = 0.02        # 2% of messages lost
            self.max_invites_per_round = 1       # Limit aggressive invitation sending
            
        def _initialize_network(self):
            """Initialize with more realistic initial connectivity."""
            # Generate connected set with LIMITED bidirectional relationships
            connected_ids = self._generate_peer_ids(self.config.connected_set_size)
            self.connected_set = set(connected_ids)
            
            for peer_id in connected_ids:
                peer = Peer(peer_id, self.config.max_connections_per_peer)
                
                # Instead of full connectivity, each peer starts with subset of connections
                # Simulate a more realistic initial topology
                num_initial_connections = min(
                    self.config.max_connections_per_peer - 2,  # Leave room for growth
                    max(3, len(connected_ids) // 4)           # At least 3, up to 1/4 of network
                )
                
                # Select random peers to connect to (not all peers)
                available_peers = [pid for pid in connected_ids if pid != peer_id]
                if len(available_peers) >= num_initial_connections:
                    initial_connections = random.sample(available_peers, num_initial_connections)
                    for other_id in initial_connections:
                        peer.add_peer(other_id, PeerState.CONNECTED, 0)
                
                self.peers[peer_id] = peer
            
            # Make connections bidirectional (but not necessarily symmetric counts)
            for peer_id, peer in self.peers.items():
                for connected_id in peer.get_connected_peers():
                    if connected_id in self.peers:
                        other_peer = self.peers[connected_id]
                        if other_peer.get_peer_state(peer_id) != PeerState.CONNECTED:
                            # Add reverse connection if space available
                            if len(other_peer.get_connected_peers()) < other_peer.max_connections:
                                other_peer.add_peer(peer_id, PeerState.CONNECTED, 0)
            
            # Generate candidate set with limited identified peers
            candidate_ids = self._generate_peer_ids(self.config.candidate_set_size, exclude=connected_ids)
            self.candidate_set = set(candidate_ids)
            
            for peer_id in candidate_ids:
                peer = Peer(peer_id, self.config.max_connections_per_peer)
                # Candidates start with very limited knowledge
                num_identified = min(self.config.initial_identified_peers, len(connected_ids))
                identified_peers = random.sample(connected_ids, num_identified)
                for identified_id in identified_peers:
                    peer.add_peer(identified_id, PeerState.IDENTIFIED, -1)
                self.peers[peer_id] = peer
            
            # Update metrics tracking
            self.metrics.set_candidate_ids(self.candidate_set)
        
        def simulate_round(self):
            """Simulate with realistic constraints."""
            invites_to_send = []
            
            # Phase 1: Limited mapping requests (not every peer every round)
            active_peers = random.sample(list(self.peers.keys()), 
                                       max(1, len(self.peers) // 2))  # Only half do requests
            
            for peer_id in active_peers:
                peer = self.peers[peer_id]
                
                # Simulate message loss
                if random.random() < self.message_loss_rate:
                    continue
                
                discovered_peer_id = self.simulate_mapping_request(peer)
                
                if discovered_peer_id and discovered_peer_id != peer.id and discovered_peer_id in self.peers:
                    current_state = peer.get_peer_state(discovered_peer_id)
                    
                    # Simulate connection failures
                    if random.random() < self.connection_failure_rate:
                        continue
                    
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
            
            # Phase 2: Send invites (with message loss)
            for sender_id, receiver_id in invites_to_send:
                if receiver_id in self.peers and random.random() >= self.message_loss_rate:
                    invite = Invite(sender_id, self.round_number)
                    self.peers[receiver_id].receive_invite(invite)
            
            # Phase 3: Process invites and limited maintenance
            for peer in self.peers.values():
                # Process received invites
                peer.process_invites(self.round_number)
                
                # LIMITED maintenance invites (not always 2 per round)
                connected_peers = peer.get_connected_peers()
                if len(connected_peers) >= 2 and random.random() < 0.3:  # Only 30% chance
                    num_maintenance = min(self.max_invites_per_round, len(connected_peers))
                    selected_peers = random.sample(connected_peers, num_maintenance)
                    for target_id in selected_peers:
                        if target_id in self.peers and random.random() >= self.message_loss_rate:
                            invite = Invite(peer.id, self.round_number)
                            self.peers[target_id].receive_invite(invite)
                
                # Remove excess connections
                peer.remove_excess_connections()
            
            # Record metrics for this round
            self.metrics.record_round(self.round_number, self.peers)
            self.round_number += 1
    
    return RealisticNetworkSimulator


def test_realistic_constraints():
    """Test network with realistic constraints."""
    print("Testing Realistic Network Constraints")
    print("=" * 50)
    
    RealisticSimulator = create_realistic_network_config()
    
    # Test multiple scenarios
    scenarios = [
        {"name": "Medium Network", "connected": 40, "candidates": 20, "max_conn": 8},
        {"name": "Large Network", "connected": 80, "candidates": 40, "max_conn": 6},
        {"name": "Limited Connections", "connected": 30, "candidates": 15, "max_conn": 4},
    ]
    
    for scenario in scenarios:
        print(f"\n--- {scenario['name']} ---")
        
        config = SimulationConfig(
            connected_set_size=scenario['connected'],
            candidate_set_size=scenario['candidates'],
            max_connections_per_peer=scenario['max_conn'],
            initial_identified_peers=3,
            simulation_rounds=400,
            random_seed=555
        )
        
        simulator = RealisticSimulator(config)
        
        # Show initial connectivity
        initial_connections = []
        for peer in simulator.peers.values():
            initial_connections.append(len(peer.get_connected_peers()))
        
        print(f"Initial connectivity: avg={statistics.mean(initial_connections):.1f}, range={min(initial_connections)}-{max(initial_connections)}")
        
        # Track progress
        measurements = []
        
        for round_num in range(config.simulation_rounds):
            simulator.simulate_round()
            
            if round_num % 50 == 0:
                connected_counts = []
                candidate_counts = []
                
                for peer_id, peer in simulator.peers.items():
                    connection_count = len(peer.get_connected_peers())
                    
                    if peer_id in simulator.connected_set:
                        connected_counts.append(connection_count)
                    elif peer_id in simulator.candidate_set:
                        candidate_counts.append(connection_count)
                
                measurements.append({
                    'round': round_num,
                    'connected_avg': statistics.mean(connected_counts),
                    'connected_at_max': sum(1 for c in connected_counts if c == config.max_connections_per_peer),
                    'connected_total': len(connected_counts),
                    'candidate_avg': statistics.mean(candidate_counts) if candidate_counts else 0,
                    'candidate_at_max': sum(1 for c in candidate_counts if c == config.max_connections_per_peer),
                    'candidate_total': len(candidate_counts)
                })
        
        # Results
        final = measurements[-1]
        print(f"Final results:")
        print(f"  Connected peers: avg={final['connected_avg']:.1f}, at_max={final['connected_at_max']}/{final['connected_total']} ({final['connected_at_max']/final['connected_total']:.1%})")
        print(f"  Candidate peers: avg={final['candidate_avg']:.1f}, at_max={final['candidate_at_max']}/{final['candidate_total']} ({final['candidate_at_max']/final['candidate_total']:.1%})")
        
        # Show evolution
        if len(measurements) >= 3:
            mid_idx = len(measurements) // 2
            print(f"  Evolution over time:")
            print(f"    Connected avg: {measurements[0]['connected_avg']:.1f} -> {measurements[mid_idx]['connected_avg']:.1f} -> {measurements[-1]['connected_avg']:.1f}")
            print(f"    Candidate avg: {measurements[0]['candidate_avg']:.1f} -> {measurements[mid_idx]['candidate_avg']:.1f} -> {measurements[-1]['candidate_avg']:.1f}")


def compare_realistic_vs_idealized():
    """Compare realistic vs idealized network behavior."""
    print(f"\nComparing Realistic vs Idealized Networks")
    print("=" * 50)
    
    config = SimulationConfig(
        connected_set_size=40,
        candidate_set_size=20,
        max_connections_per_peer=6,
        initial_identified_peers=3,
        simulation_rounds=300,
        random_seed=777
    )
    
    # Test idealized network
    print("Idealized Network:")
    idealized = NetworkSimulator(config)
    for _ in range(config.simulation_rounds):
        idealized.simulate_round()
    
    ideal_connected = []
    ideal_candidates = []
    for peer_id, peer in idealized.peers.items():
        connections = len(peer.get_connected_peers())
        if peer_id in idealized.connected_set:
            ideal_connected.append(connections)
        else:
            ideal_candidates.append(connections)
    
    print(f"  Connected avg: {statistics.mean(ideal_connected):.1f}, at_max: {sum(1 for c in ideal_connected if c == 6)}/{len(ideal_connected)}")
    print(f"  Candidates avg: {statistics.mean(ideal_candidates):.1f}, at_max: {sum(1 for c in ideal_candidates if c == 6)}/{len(ideal_candidates)}")
    
    # Test realistic network
    print("Realistic Network:")
    RealisticSimulator = create_realistic_network_config()
    realistic = RealisticSimulator(config)
    for _ in range(config.simulation_rounds):
        realistic.simulate_round()
    
    real_connected = []
    real_candidates = []
    for peer_id, peer in realistic.peers.items():
        connections = len(peer.get_connected_peers())
        if peer_id in realistic.connected_set:
            real_connected.append(connections)
        else:
            real_candidates.append(connections)
    
    print(f"  Connected avg: {statistics.mean(real_connected):.1f}, at_max: {sum(1 for c in real_connected if c == 6)}/{len(real_connected)}")
    print(f"  Candidates avg: {statistics.mean(real_candidates):.1f}, at_max: {sum(1 for c in real_candidates if c == 6)}/{len(real_candidates)}")


def main():
    """Run realistic connection analysis."""
    test_realistic_constraints()
    compare_realistic_vs_idealized()
    
    print(f"\n" + "=" * 60)
    print("Realistic Network Analysis Summary:")
    print("- Initial network is not fully connected")
    print("- Message loss and connection failures reduce efficiency")
    print("- Limited maintenance activity prevents aggressive connection building")
    print("- Achievement rates should be more realistic (70-90% instead of 100%)")


if __name__ == "__main__":
    main()