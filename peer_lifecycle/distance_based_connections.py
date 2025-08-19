#!/usr/bin/env python3
"""
Distance-based connection management for optimal routing performance.
"""

from peer_lifecycle_simulator_fixed import *
import math


class DistanceOptimizedPeer(Peer):
    """Peer with distance-based connection management."""
    
    def __init__(self, peer_id: int, max_connections: int = 10):
        super().__init__(peer_id, max_connections)
        # Allow some slack for optimal distribution
        self.connection_slack = max(2, max_connections // 4)  # 25% slack
        self.soft_max_connections = max_connections + self.connection_slack
        
    def get_distance_to_peer(self, peer_id: int) -> int:
        """Get XOR distance to another peer."""
        return self._xor_distance(self.id, peer_id)
    
    def calculate_optimal_distribution(self, all_peers: List[int]) -> Dict[str, int]:
        """Calculate optimal number of connections per distance bucket."""
        if not all_peers:
            return {}
        
        # Calculate distances to all other peers
        distances = [(pid, self.get_distance_to_peer(pid)) for pid in all_peers if pid != self.id]
        distances.sort(key=lambda x: x[1])  # Sort by distance
        
        # Create distance buckets (logarithmic scale)
        max_distance = max(d[1] for d in distances) if distances else 1
        num_buckets = min(8, len(distances))  # Up to 8 distance buckets
        
        buckets = {}
        bucket_ranges = []
        
        for i in range(num_buckets):
            # Logarithmic bucket boundaries
            if i == 0:
                bucket_min = 0
            else:
                bucket_min = int(max_distance * (2 ** (i-num_buckets+1)))
            
            if i == num_buckets - 1:
                bucket_max = max_distance
            else:
                bucket_max = int(max_distance * (2 ** (i-num_buckets+2)))
            
            bucket_ranges.append((bucket_min, bucket_max))
            buckets[f"bucket_{i}"] = []
        
        # Assign peers to buckets
        for peer_id, distance in distances:
            for i, (bucket_min, bucket_max) in enumerate(bucket_ranges):
                if bucket_min <= distance <= bucket_max:
                    buckets[f"bucket_{i}"].append(peer_id)
                    break
        
        # Calculate desired connections per bucket (slope distribution)
        total_budget = self.max_connections
        bucket_targets = {}
        
        for i in range(num_buckets):
            bucket_key = f"bucket_{i}"
            # Exponential decay: closer buckets get more connections
            weight = 2 ** (num_buckets - i - 1)  # 8, 4, 2, 1 for 4 buckets
            bucket_targets[bucket_key] = weight
        
        # Normalize to fit within connection budget
        total_weight = sum(bucket_targets.values())
        for bucket_key in bucket_targets:
            raw_target = (bucket_targets[bucket_key] / total_weight) * total_budget
            bucket_targets[bucket_key] = max(1, int(raw_target))  # At least 1 per bucket
        
        return bucket_targets, buckets, bucket_ranges
    
    def optimize_connections(self, all_peers: List[int]) -> List[int]:
        """Optimize connection distribution based on distance."""
        if len(all_peers) <= 1:
            return []
        
        current_connected = self.get_connected_peers()
        
        # If we're not over the soft limit, don't optimize yet
        if len(current_connected) <= self.max_connections:
            return []
        
        # Calculate optimal distribution
        try:
            bucket_targets, buckets, bucket_ranges = self.calculate_optimal_distribution(all_peers)
        except:
            # Fallback to random removal if calculation fails
            excess = len(current_connected) - self.max_connections
            return random.sample(current_connected, excess)
        
        # Analyze current distribution
        current_distribution = {}
        for bucket_key in buckets:
            current_distribution[bucket_key] = []
        
        for connected_peer in current_connected:
            distance = self.get_distance_to_peer(connected_peer)
            # Find which bucket this peer belongs to
            for i, (bucket_min, bucket_max) in enumerate(bucket_ranges):
                if bucket_min <= distance <= bucket_max:
                    bucket_key = f"bucket_{i}"
                    current_distribution[bucket_key].append(connected_peer)
                    break
        
        # Identify peers to remove
        to_remove = []
        
        for bucket_key, target_count in bucket_targets.items():
            current_peers = current_distribution.get(bucket_key, [])
            current_count = len(current_peers)
            
            if current_count > target_count:
                # Remove excess peers from this bucket (keep the closest ones)
                excess_count = current_count - target_count
                # Sort by distance and remove the furthest ones in this bucket
                bucket_peers_with_dist = [(pid, self.get_distance_to_peer(pid)) for pid in current_peers]
                bucket_peers_with_dist.sort(key=lambda x: x[1])  # Sort by distance
                
                # Remove the furthest peers in this bucket
                for i in range(min(excess_count, len(bucket_peers_with_dist))):
                    furthest_peer = bucket_peers_with_dist[-(i+1)][0]  # Start from furthest
                    to_remove.append(furthest_peer)
        
        # If we still need to remove more peers (shouldn't happen with good bucket design)
        remaining_excess = len(current_connected) - self.max_connections - len(to_remove)
        if remaining_excess > 0:
            remaining_connected = [p for p in current_connected if p not in to_remove]
            additional_removals = random.sample(remaining_connected, min(remaining_excess, len(remaining_connected)))
            to_remove.extend(additional_removals)
        
        return to_remove[:len(current_connected) - self.max_connections]  # Ensure we don't remove too many
    
    def remove_excess_connections(self, all_peers: List[int] = None) -> List[int]:
        """Remove excess connections using distance-based optimization."""
        connected = self.get_connected_peers()
        
        # If under soft limit, no removal needed
        if len(connected) <= self.soft_max_connections:
            return []
        
        if all_peers is None:
            # Fallback to random removal
            excess_count = len(connected) - self.max_connections
            to_remove = random.sample(connected, min(excess_count, len(connected)))
        else:
            # Use distance-based optimization
            to_remove = self.optimize_connections(all_peers)
        
        # Remove the selected peers
        for peer_id in to_remove:
            if peer_id in self.known_peers:
                del self.known_peers[peer_id]
        
        return to_remove
    
    def get_connection_distribution_stats(self, all_peers: List[int] = None) -> Dict:
        """Get statistics about current connection distribution."""
        connected = self.get_connected_peers()
        if not connected or not all_peers:
            return {"total": len(connected), "distribution": {}}
        
        try:
            bucket_targets, buckets, bucket_ranges = self.calculate_optimal_distribution(all_peers)
            
            # Analyze current distribution
            current_distribution = {}
            for i, (bucket_min, bucket_max) in enumerate(bucket_ranges):
                bucket_key = f"bucket_{i}"
                current_distribution[bucket_key] = {
                    "range": f"{bucket_min}-{bucket_max}",
                    "target": bucket_targets.get(bucket_key, 0),
                    "current": 0,
                    "peers": []
                }
            
            for connected_peer in connected:
                distance = self.get_distance_to_peer(connected_peer)
                for i, (bucket_min, bucket_max) in enumerate(bucket_ranges):
                    if bucket_min <= distance <= bucket_max:
                        bucket_key = f"bucket_{i}"
                        current_distribution[bucket_key]["current"] += 1
                        current_distribution[bucket_key]["peers"].append(connected_peer)
                        break
            
            return {
                "total": len(connected),
                "max_allowed": self.max_connections,
                "soft_max": self.soft_max_connections,
                "distribution": current_distribution
            }
        except:
            return {"total": len(connected), "distribution": {}}


class DistanceOptimizedSimulator(NetworkSimulator):
    """Simulator using distance-optimized peers."""
    
    def _initialize_network(self):
        """Initialize network with distance-optimized peers."""
        # Generate connected set
        connected_ids = self._generate_peer_ids(self.config.connected_set_size)
        self.connected_set = set(connected_ids)
        
        for peer_id in connected_ids:
            peer = DistanceOptimizedPeer(peer_id, self.config.max_connections_per_peer)
            # Add all other connected peers as Connected initially
            for other_id in connected_ids:
                if other_id != peer_id:
                    peer.add_peer(other_id, PeerState.CONNECTED, 0)
            self.peers[peer_id] = peer
        
        # Generate candidate set
        candidate_ids = self._generate_peer_ids(self.config.candidate_set_size, exclude=connected_ids)
        self.candidate_set = set(candidate_ids)
        
        for peer_id in candidate_ids:
            peer = DistanceOptimizedPeer(peer_id, self.config.max_connections_per_peer)
            # Add random connected peers as Identified
            num_identified = min(self.config.initial_identified_peers, len(connected_ids))
            identified_peers = random.sample(connected_ids, num_identified)
            for identified_id in identified_peers:
                peer.add_peer(identified_id, PeerState.IDENTIFIED, -1)
            self.peers[peer_id] = peer
        
        # Update metrics tracking
        self.metrics.set_candidate_ids(self.candidate_set)
    
    def simulate_round(self):
        """Simulate round with distance-optimized connection management."""
        invites_to_send = []
        all_peer_ids = list(self.peers.keys())
        
        # Phase 1: Mapping requests (same as before)
        for peer in self.peers.values():
            discovered_peer_id = self.simulate_mapping_request(peer)
            
            if discovered_peer_id and discovered_peer_id != peer.id and discovered_peer_id in self.peers:
                current_state = peer.get_peer_state(discovered_peer_id)
                
                if current_state is None:
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
        
        # Phase 2: Send invites (same as before)
        for sender_id, receiver_id in invites_to_send:
            if receiver_id in self.peers:
                invite = Invite(sender_id, self.round_number)
                self.peers[receiver_id].receive_invite(invite)
        
        # Phase 3: Process invites and distance-optimized maintenance
        for peer in self.peers.values():
            # Process received invites
            peer.process_invites(self.round_number)
            
            # Send maintenance invites to connected peers
            connected_peers = peer.get_connected_peers()
            if len(connected_peers) >= 2:
                selected_peers = random.sample(connected_peers, min(2, len(connected_peers)))
                for target_id in selected_peers:
                    if target_id in self.peers:
                        invite = Invite(peer.id, self.round_number)
                        self.peers[target_id].receive_invite(invite)
            
            # Distance-optimized connection management
            peer.remove_excess_connections(all_peer_ids)
        
        # Record metrics
        self.metrics.record_round(self.round_number, self.peers)
        self.round_number += 1


def test_distance_optimization():
    """Test the distance-based connection optimization."""
    print("Testing Distance-Based Connection Optimization")
    print("=" * 60)
    
    config = SimulationConfig(
        connected_set_size=40,
        candidate_set_size=20,
        max_connections_per_peer=8,
        initial_identified_peers=4,
        simulation_rounds=200,
        random_seed=42
    )
    
    # Test both regular and distance-optimized simulators
    print("Regular Simulator:")
    regular_sim = NetworkSimulator(config)
    for _ in range(config.simulation_rounds):
        regular_sim.simulate_round()
    
    print("Distance-Optimized Simulator:")
    optimized_sim = DistanceOptimizedSimulator(config)
    for _ in range(config.simulation_rounds):
        optimized_sim.simulate_round()
    
    # Analyze connection distributions
    print(f"\nConnection Distribution Analysis:")
    
    # Sample a few peers from each simulator
    sample_peers = list(optimized_sim.connected_set)[:3]
    
    for i, peer_id in enumerate(sample_peers):
        if peer_id in optimized_sim.peers:
            peer = optimized_sim.peers[peer_id]
            all_peers = list(optimized_sim.peers.keys())
            stats = peer.get_connection_distribution_stats(all_peers)
            
            print(f"\nPeer {peer_id} (Optimized):")
            print(f"  Total connections: {stats['total']}/{stats.get('max_allowed', 0)} (soft max: {stats.get('soft_max', 0)})")
            
            if 'distribution' in stats and stats['distribution']:
                for bucket_key, bucket_info in stats['distribution'].items():
                    print(f"  {bucket_key}: {bucket_info['current']}/{bucket_info['target']} (range: {bucket_info['range']})")
    
    return regular_sim, optimized_sim


def analyze_routing_efficiency(simulator, name: str):
    """Analyze routing efficiency of the network."""
    print(f"\nRouting Efficiency Analysis - {name}")
    print("=" + "=" * (35 + len(name)))
    
    # Test routing performance with random queries
    hop_counts = []
    success_rates = []
    
    test_peers = random.sample(list(simulator.peers.keys()), min(10, len(simulator.peers)))
    
    for test_peer_id in test_peers:
        if test_peer_id not in simulator.peers:
            continue
            
        peer = simulator.peers[test_peer_id]
        peer_hop_counts = []
        
        # Test 20 random queries from this peer
        for _ in range(20):
            max_id = (1 << simulator.config.address_space_bits) - 1
            target_id = random.randint(0, max_id)
            
            # Simulate routing with hop counting
            hops = 0
            current_id = test_peer_id
            visited = set()
            
            while hops < 10 and current_id not in visited:
                visited.add(current_id)
                hops += 1
                
                if current_id not in simulator.peers:
                    break
                    
                current_peer = simulator.peers[current_id]
                connected = current_peer.get_connected_peers()
                
                if not connected:
                    break
                
                # Find closest connected peer
                closest = min(connected, key=lambda pid: current_peer._xor_distance(pid, target_id))
                current_distance = current_peer._xor_distance(current_id, target_id)
                closest_distance = current_peer._xor_distance(closest, target_id)
                
                if closest_distance >= current_distance:
                    break  # No improvement possible
                
                current_id = closest
            
            peer_hop_counts.append(hops)
        
        hop_counts.extend(peer_hop_counts)
    
    if hop_counts:
        avg_hops = statistics.mean(hop_counts)
        max_hops = max(hop_counts)
        min_hops = min(hop_counts)
        
        print(f"  Average hops to convergence: {avg_hops:.2f}")
        print(f"  Hop range: {min_hops} - {max_hops}")
        print(f"  Hop distribution: {sorted(hop_counts)[:10]}... (first 10)")


def main():
    """Test distance-based connection optimization."""
    regular_sim, optimized_sim = test_distance_optimization()
    
    # Analyze routing efficiency
    analyze_routing_efficiency(regular_sim, "Regular Network")
    analyze_routing_efficiency(optimized_sim, "Distance-Optimized Network")
    
    print(f"\n" + "=" * 60)
    print("Distance Optimization Summary:")
    print("- Peers maintain connections with distance-based distribution")
    print("- More connections to nearby peers, fewer to distant peers")  
    print("- Slack allowed above max_connections for optimization")
    print("- Should improve routing efficiency and reduce hop counts")


if __name__ == "__main__":
    main()