#!/usr/bin/env python3
"""
Stress test connection limits with more challenging scenarios.
"""

from peer_lifecycle_simulator_fixed import *
import statistics


def test_oversized_network():
    """Test with many peers relative to max connections."""
    print("Testing Oversized Network")
    print("=" * 40)
    
    # Create scenario where max_connections is much smaller than network size
    config = SimulationConfig(
        connected_set_size=100,  # Large network
        candidate_set_size=50,   # Many candidates
        max_connections_per_peer=5,  # Very limited connections
        initial_identified_peers=3,
        simulation_rounds=400,
        random_seed=789
    )
    
    simulator = NetworkSimulator(config)
    
    print(f"Network: {config.connected_set_size} connected + {config.candidate_set_size} candidates")
    print(f"Max connections per peer: {config.max_connections_per_peer}")
    print(f"Theoretical max connections if all peers maxed: {(config.connected_set_size + config.candidate_set_size) * config.max_connections_per_peer // 2}")
    
    # Track evolution
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
            
            print(f"Round {round_num}: Connected avg={measurements[-1]['connected_avg']:.2f}, Candidate avg={measurements[-1]['candidate_avg']:.2f}")
    
    # Final analysis
    final = measurements[-1]
    print(f"\nFinal Results:")
    print(f"Connected peers at max: {final['connected_at_max']}/{final['connected_total']} ({final['connected_at_max']/final['connected_total']:.1%})")
    print(f"Candidate peers at max: {final['candidate_at_max']}/{final['candidate_total']} ({final['candidate_at_max']/final['candidate_total']:.1%})")
    
    return measurements


def test_limited_discovery():
    """Test with very limited initial peer knowledge."""
    print("\nTesting Limited Discovery")
    print("=" * 40)
    
    config = SimulationConfig(
        connected_set_size=50,
        candidate_set_size=30,
        max_connections_per_peer=10,
        initial_identified_peers=1,  # Very limited initial knowledge
        simulation_rounds=300,
        random_seed=456
    )
    
    simulator = NetworkSimulator(config)
    
    # Track how long it takes candidates to discover enough peers
    discovery_stats = []
    
    for round_num in range(config.simulation_rounds):
        simulator.simulate_round()
        
        if round_num % 25 == 0:
            candidate_known_counts = []
            candidate_connection_counts = []
            
            for peer_id in simulator.candidate_set:
                if peer_id in simulator.peers:
                    peer = simulator.peers[peer_id]
                    known_count = len(peer.known_peers)
                    connection_count = len(peer.get_connected_peers())
                    candidate_known_counts.append(known_count)
                    candidate_connection_counts.append(connection_count)
            
            discovery_stats.append({
                'round': round_num,
                'avg_known': statistics.mean(candidate_known_counts) if candidate_known_counts else 0,
                'avg_connections': statistics.mean(candidate_connection_counts) if candidate_connection_counts else 0,
                'at_max_connections': sum(1 for c in candidate_connection_counts if c == config.max_connections_per_peer)
            })
            
            print(f"Round {round_num}: Candidates know avg={discovery_stats[-1]['avg_known']:.1f} peers, connected to avg={discovery_stats[-1]['avg_connections']:.1f}")
    
    return discovery_stats


def test_asymmetric_network():
    """Test with different max_connections for different peer groups."""
    print("\nTesting Asymmetric Network")
    print("=" * 40)
    
    # Create a base network
    config = SimulationConfig(
        connected_set_size=30,
        candidate_set_size=20,
        max_connections_per_peer=8,
        initial_identified_peers=3,
        simulation_rounds=200,
        random_seed=321
    )
    
    simulator = NetworkSimulator(config)
    
    # Modify some peers to have different connection limits
    high_capacity_peers = list(simulator.connected_set)[:10]  # First 10 connected peers
    low_capacity_peers = list(simulator.candidate_set)[:10]   # First 10 candidates
    
    for peer_id in high_capacity_peers:
        if peer_id in simulator.peers:
            simulator.peers[peer_id].max_connections = 12  # Higher capacity
    
    for peer_id in low_capacity_peers:
        if peer_id in simulator.peers:
            simulator.peers[peer_id].max_connections = 4   # Lower capacity
    
    print(f"Network setup:")
    print(f"  {len(high_capacity_peers)} high-capacity peers (max 12 connections)")
    print(f"  {config.connected_set_size - len(high_capacity_peers)} normal connected peers (max 8 connections)")
    print(f"  {len(low_capacity_peers)} low-capacity candidates (max 4 connections)")
    print(f"  {config.candidate_set_size - len(low_capacity_peers)} normal candidates (max 8 connections)")
    
    # Run simulation
    for round_num in range(config.simulation_rounds):
        simulator.simulate_round()
        
        if round_num % 50 == 0:
            high_cap_connections = []
            normal_connections = []
            low_cap_connections = []
            
            for peer_id, peer in simulator.peers.items():
                connection_count = len(peer.get_connected_peers())
                
                if peer_id in high_capacity_peers:
                    high_cap_connections.append(connection_count)
                elif peer_id in low_capacity_peers:
                    low_cap_connections.append(connection_count)
                else:
                    normal_connections.append(connection_count)
            
            print(f"Round {round_num}:")
            print(f"  High-cap (max 12): avg={statistics.mean(high_cap_connections):.1f}")
            print(f"  Normal (max 8): avg={statistics.mean(normal_connections):.1f}")
            print(f"  Low-cap (max 4): avg={statistics.mean(low_cap_connections):.1f}")
    
    # Final analysis
    final_high = [len(simulator.peers[pid].get_connected_peers()) for pid in high_capacity_peers if pid in simulator.peers]
    final_low = [len(simulator.peers[pid].get_connected_peers()) for pid in low_capacity_peers if pid in simulator.peers]
    
    print(f"\nFinal connection distributions:")
    print(f"High-capacity peers: {sorted(final_high)}")
    print(f"Low-capacity peers: {sorted(final_low)}")


def investigate_perfect_achievement():
    """Investigate why we're seeing 100% achievement rates."""
    print("\nInvestigating Perfect Achievement")
    print("=" * 45)
    
    # Test edge cases that should be harder to achieve
    test_cases = [
        {"name": "Tiny network", "connected": 5, "candidates": 3, "max_conn": 4},
        {"name": "Huge max_conn", "connected": 20, "candidates": 10, "max_conn": 25},
        {"name": "Many candidates", "connected": 15, "candidates": 40, "max_conn": 6},
    ]
    
    for case in test_cases:
        print(f"\nTest case: {case['name']}")
        print(f"  {case['connected']} connected + {case['candidates']} candidates, max_conn={case['max_conn']}")
        
        config = SimulationConfig(
            connected_set_size=case['connected'],
            candidate_set_size=case['candidates'],
            max_connections_per_peer=case['max_conn'],
            initial_identified_peers=min(3, case['connected']),
            simulation_rounds=200,
            random_seed=111
        )
        
        simulator = NetworkSimulator(config)
        
        # Run simulation
        for _ in range(config.simulation_rounds):
            simulator.simulate_round()
        
        # Analyze final state
        all_connections = []
        for peer in simulator.peers.values():
            all_connections.append(len(peer.get_connected_peers()))
        
        at_max = sum(1 for c in all_connections if c == case['max_conn'])
        total_peers = len(all_connections)
        
        print(f"  Result: {at_max}/{total_peers} peers at max ({at_max/total_peers:.1%})")
        print(f"  Connection distribution: {sorted(all_connections)}")
        
        # Check if network can even support everyone at max
        total_possible_connections = total_peers * case['max_conn']
        theoretical_max_unique_connections = total_possible_connections // 2
        print(f"  Theoretical analysis:")
        print(f"    Total connection slots: {total_possible_connections}")
        print(f"    Max unique connections: {theoretical_max_unique_connections}")
        print(f"    Network can support all at max: {total_possible_connections <= total_peers * (total_peers - 1)}")


def main():
    """Run stress tests on connection achievement."""
    test_oversized_network()
    test_limited_discovery()
    test_asymmetric_network()
    investigate_perfect_achievement()
    
    print(f"\n" + "=" * 60)
    print("Analysis Summary:")
    print("The simulation appears to achieve 100% connection target rates,")
    print("which suggests either:")
    print("1. The algorithm is very effective at peer discovery")
    print("2. The network constraints are not realistic")
    print("3. There might be implementation issues with connection limits")
    print("4. The removal of excess connections is working too perfectly")


if __name__ == "__main__":
    main()