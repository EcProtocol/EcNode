#!/usr/bin/env python3
"""
Analyze how well peers achieve their max_connections target.
"""

from peer_lifecycle_simulator_fixed import *
import statistics


def analyze_connection_achievement():
    """Analyze how close peers come to their max_connections setting."""
    print("Analyzing Connection Achievement")
    print("=" * 50)
    
    config = SimulationConfig(
        connected_set_size=40,
        candidate_set_size=20,
        max_connections_per_peer=8,
        initial_identified_peers=4,
        simulation_rounds=300,
        random_seed=42
    )
    
    simulator = NetworkSimulator(config)
    
    # Track connection counts over time
    connection_tracking = {
        'connected_set': [],
        'candidate_set': []
    }
    
    # Run simulation and track every 10 rounds
    for round_num in range(config.simulation_rounds):
        simulator.simulate_round()
        
        if round_num % 10 == 0:
            connected_counts = []
            candidate_counts = []
            
            for peer_id, peer in simulator.peers.items():
                connection_count = len(peer.get_connected_peers())
                
                if peer_id in simulator.connected_set:
                    connected_counts.append(connection_count)
                elif peer_id in simulator.candidate_set:
                    candidate_counts.append(connection_count)
            
            connection_tracking['connected_set'].append({
                'round': round_num,
                'counts': connected_counts.copy(),
                'avg': statistics.mean(connected_counts) if connected_counts else 0,
                'min': min(connected_counts) if connected_counts else 0,
                'max': max(connected_counts) if connected_counts else 0,
                'at_max': sum(1 for c in connected_counts if c == config.max_connections_per_peer),
                'below_max': sum(1 for c in connected_counts if c < config.max_connections_per_peer)
            })
            
            connection_tracking['candidate_set'].append({
                'round': round_num,
                'counts': candidate_counts.copy(),
                'avg': statistics.mean(candidate_counts) if candidate_counts else 0,
                'min': min(candidate_counts) if candidate_counts else 0,
                'max': max(candidate_counts) if candidate_counts else 0,
                'at_max': sum(1 for c in candidate_counts if c == config.max_connections_per_peer),
                'below_max': sum(1 for c in candidate_counts if c < config.max_connections_per_peer)
            })
        
        if round_num % 50 == 0:
            print(f"Round {round_num}/{config.simulation_rounds}")
    
    return connection_tracking, config


def analyze_final_state(tracking, config):
    """Analyze the final connection state."""
    print(f"\nFinal State Analysis (max_connections = {config.max_connections_per_peer})")
    print("=" * 60)
    
    # Get final measurements
    final_connected = tracking['connected_set'][-1]
    final_candidates = tracking['candidate_set'][-1]
    
    print(f"Connected Set (originally {config.connected_set_size} peers):")
    print(f"  Average connections: {final_connected['avg']:.2f}")
    print(f"  Range: {final_connected['min']} - {final_connected['max']}")
    print(f"  Peers at max ({config.max_connections_per_peer}): {final_connected['at_max']}/{len(final_connected['counts'])}")
    print(f"  Peers below max: {final_connected['below_max']}/{len(final_connected['counts'])}")
    print(f"  Achievement rate: {final_connected['at_max']/len(final_connected['counts']):.1%}")
    
    print(f"\nCandidate Set (originally {config.candidate_set_size} peers):")
    print(f"  Average connections: {final_candidates['avg']:.2f}")
    print(f"  Range: {final_candidates['min']} - {final_candidates['max']}")
    print(f"  Peers at max ({config.max_connections_per_peer}): {final_candidates['at_max']}/{len(final_candidates['counts'])}")
    print(f"  Peers below max: {final_candidates['below_max']}/{len(final_candidates['counts'])}")
    print(f"  Achievement rate: {final_candidates['at_max']/len(final_candidates['counts']):.1%}")
    
    # Distribution analysis
    print(f"\nConnection Distribution:")
    print(f"Connected Set: {sorted(final_connected['counts'])}")
    print(f"Candidate Set: {sorted(final_candidates['counts'])}")


def analyze_evolution_over_time(tracking, config):
    """Analyze how connection counts evolve over time."""
    print(f"\nEvolution Over Time")
    print("=" * 40)
    
    print(f"{'Round':<8} {'Connected Avg':<14} {'Connected @Max':<14} {'Candidate Avg':<14} {'Candidate @Max':<14}")
    print("-" * 70)
    
    # Show every 5th measurement
    for i in range(0, len(tracking['connected_set']), 5):
        conn = tracking['connected_set'][i]
        cand = tracking['candidate_set'][i]
        
        conn_at_max_pct = (conn['at_max'] / len(conn['counts']) * 100) if conn['counts'] else 0
        cand_at_max_pct = (cand['at_max'] / len(cand['counts']) * 100) if cand['counts'] else 0
        
        print(f"{conn['round']:<8} {conn['avg']:<14.2f} {conn_at_max_pct:<14.1f}% {cand['avg']:<14.2f} {cand_at_max_pct:<14.1f}%")


def analyze_connection_constraints():
    """Analyze why peers might not reach max connections."""
    print(f"\nConnection Constraint Analysis")
    print("=" * 50)
    
    # Test with different max_connections settings
    scenarios = [4, 6, 8, 10, 12, 15]
    
    results = {}
    
    for max_conn in scenarios:
        config = SimulationConfig(
            connected_set_size=30,
            candidate_set_size=15,
            max_connections_per_peer=max_conn,
            initial_identified_peers=4,
            simulation_rounds=200,
            random_seed=123
        )
        
        simulator = NetworkSimulator(config)
        
        # Run to convergence
        for _ in range(config.simulation_rounds):
            simulator.simulate_round()
        
        # Measure final state
        connected_counts = []
        candidate_counts = []
        
        for peer_id, peer in simulator.peers.items():
            connection_count = len(peer.get_connected_peers())
            
            if peer_id in simulator.connected_set:
                connected_counts.append(connection_count)
            elif peer_id in simulator.candidate_set:
                candidate_counts.append(connection_count)
        
        results[max_conn] = {
            'connected_avg': statistics.mean(connected_counts),
            'connected_at_max': sum(1 for c in connected_counts if c == max_conn),
            'connected_total': len(connected_counts),
            'candidate_avg': statistics.mean(candidate_counts) if candidate_counts else 0,
            'candidate_at_max': sum(1 for c in candidate_counts if c == max_conn),
            'candidate_total': len(candidate_counts)
        }
    
    print(f"{'Max':<4} {'Conn Avg':<9} {'Conn @Max':<10} {'Cand Avg':<9} {'Cand @Max':<10}")
    print("-" * 50)
    
    for max_conn, data in results.items():
        conn_pct = (data['connected_at_max'] / data['connected_total'] * 100) if data['connected_total'] > 0 else 0
        cand_pct = (data['candidate_at_max'] / data['candidate_total'] * 100) if data['candidate_total'] > 0 else 0
        
        print(f"{max_conn:<4} {data['connected_avg']:<9.2f} {conn_pct:<10.1f}% {data['candidate_avg']:<9.2f} {cand_pct:<10.1f}%")


def detailed_peer_analysis(config):
    """Analyze individual peer connection patterns."""
    print(f"\nDetailed Peer Analysis")
    print("=" * 30)
    
    simulator = NetworkSimulator(config)
    
    # Run simulation
    for _ in range(200):
        simulator.simulate_round()
    
    # Analyze a few specific peers
    sample_connected = list(simulator.connected_set)[:5]
    sample_candidates = list(simulator.candidate_set)[:5]
    
    print(f"Sample Connected Peers:")
    for peer_id in sample_connected:
        if peer_id in simulator.peers:
            peer = simulator.peers[peer_id]
            connections = len(peer.get_connected_peers())
            known_total = len(peer.known_peers)
            state_breakdown = {
                state.value: len(peer.get_peers_by_state(state))
                for state in PeerState
            }
            print(f"  Peer {peer_id}: {connections}/{config.max_connections_per_peer} connections, {known_total} known total")
            print(f"    States: {state_breakdown}")
    
    print(f"\nSample Candidate Peers:")
    for peer_id in sample_candidates:
        if peer_id in simulator.peers:
            peer = simulator.peers[peer_id]
            connections = len(peer.get_connected_peers())
            known_total = len(peer.known_peers)
            state_breakdown = {
                state.value: len(peer.get_peers_by_state(state))
                for state in PeerState
            }
            print(f"  Peer {peer_id}: {connections}/{config.max_connections_per_peer} connections, {known_total} known total")
            print(f"    States: {state_breakdown}")


def main():
    """Run comprehensive connection achievement analysis."""
    # Main analysis
    tracking, config = analyze_connection_achievement()
    
    # Analyze results
    analyze_final_state(tracking, config)
    analyze_evolution_over_time(tracking, config)
    analyze_connection_constraints()
    detailed_peer_analysis(config)
    
    print(f"\n" + "=" * 60)
    print("Key Insights:")
    print("- Connected set peers generally maintain near-max connections")
    print("- Candidates can achieve max connections but may take time")
    print("- Higher max_connections settings are harder to achieve")
    print("- Peer discovery and churn affect connection achievement")
    print("- Network size and connectivity affect maximum achievable connections")


if __name__ == "__main__":
    main()