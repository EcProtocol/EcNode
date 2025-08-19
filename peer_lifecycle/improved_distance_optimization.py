#!/usr/bin/env python3
"""
Improved distance-based connection management with better routing optimization.
"""

from peer_lifecycle_simulator_fixed import *
import math


class ImprovedDistanceOptimizedPeer(Peer):
    """Peer with improved distance-based connection management."""
    
    def __init__(self, peer_id: int, max_connections: int = 10):
        super().__init__(peer_id, max_connections)
        # Allow slack for optimization
        self.connection_slack = max(2, max_connections // 3)  # ~33% slack
        self.soft_max_connections = max_connections + self.connection_slack
        
    def get_distance_to_peer(self, peer_id: int) -> int:
        """Get XOR distance to another peer."""
        return self._xor_distance(self.id, peer_id)
    
    def calculate_routing_distribution(self, all_peers: List[int]) -> Dict:
        """Calculate optimal distribution for efficient routing."""
        if not all_peers:
            return {}
        
        # Filter out self
        other_peers = [pid for pid in all_peers if pid != self.id]
        if not other_peers:
            return {}
        
        # Create distance-based buckets using bit prefixes for better routing
        # This creates a Kademlia-style distribution
        buckets = {}
        max_bits = 32  # Assuming 32-bit address space for simulation
        
        for prefix_len in range(max_bits):
            buckets[prefix_len] = []
        
        # Assign peers to buckets based on common prefix length
        for peer_id in other_peers:
            distance = self.get_distance_to_peer(peer_id)
            if distance == 0:
                continue  # Skip self
            
            # Find the length of common prefix (number of leading zeros in XOR)
            prefix_len = self._count_leading_zeros(distance)
            prefix_len = min(prefix_len, max_bits - 1)  # Cap at max_bits - 1
            buckets[prefix_len].append(peer_id)
        
        # Calculate target connections per bucket (exponential distribution)
        # Closer buckets (higher prefix_len) get more connections
        target_distribution = {}
        total_budget = self.max_connections
        
        # Find non-empty buckets
        non_empty_buckets = [k for k, v in buckets.items() if v]
        if not non_empty_buckets:
            return {}
        
        # Assign more connections to buckets with longer common prefixes (closer peers)
        # But ensure we have some connections to distant peers for routing
        weights = {}
        for bucket_id in non_empty_buckets:
            # Higher weight for longer prefixes (closer peers)
            # But minimum weight for routing to distant areas
            if bucket_id >= 24:  # Very close peers
                weights[bucket_id] = 4
            elif bucket_id >= 16:  # Close peers
                weights[bucket_id] = 3
            elif bucket_id >= 8:   # Medium distance
                weights[bucket_id] = 2
            else:  # Distant peers - still important for routing
                weights[bucket_id] = 1
        
        total_weight = sum(weights.values())
        if total_weight == 0:
            return {}
        
        # Distribute connections based on weights
        for bucket_id in weights:
            raw_target = (weights[bucket_id] / total_weight) * total_budget
            target_distribution[bucket_id] = max(1, int(raw_target))
        
        # Ensure we don't exceed budget
        total_assigned = sum(target_distribution.values())
        if total_assigned > total_budget:
            # Scale down proportionally
            scale_factor = total_budget / total_assigned
            for bucket_id in target_distribution:
                target_distribution[bucket_id] = max(1, int(target_distribution[bucket_id] * scale_factor))
        
        return target_distribution, buckets
    
    def _count_leading_zeros(self, value: int) -> int:
        """Count leading zeros in binary representation."""
        if value == 0:
            return 32  # Assuming 32-bit integers
        
        count = 0
        # Check each bit from most significant
        for i in range(31, -1, -1):
            if value & (1 << i):
                break
            count += 1
        
        return count
    
    def optimize_connections_improved(self, all_peers: List[int]) -> List[int]:
        """Improved connection optimization for better routing."""
        current_connected = self.get_connected_peers()
        
        # If we're not over the soft limit, don't optimize
        if len(current_connected) <= self.max_connections:
            return []
        
        try:
            target_distribution, buckets = self.calculate_routing_distribution(all_peers)
        except:
            # Fallback to random removal
            excess = len(current_connected) - self.max_connections
            return random.sample(current_connected, min(excess, len(current_connected)))
        
        if not target_distribution:
            excess = len(current_connected) - self.max_connections
            return random.sample(current_connected, min(excess, len(current_connected)))
        
        # Analyze current distribution
        current_distribution = {}
        for bucket_id in buckets:
            current_distribution[bucket_id] = []
        
        for connected_peer in current_connected:
            distance = self.get_distance_to_peer(connected_peer)
            if distance == 0:
                continue
            
            prefix_len = self._count_leading_zeros(distance)
            prefix_len = min(prefix_len, 31)
            
            if prefix_len in current_distribution:
                current_distribution[prefix_len].append(connected_peer)
        
        # Identify peers to remove
        to_remove = []
        
        for bucket_id, target_count in target_distribution.items():
            current_peers = current_distribution.get(bucket_id, [])
            current_count = len(current_peers)
            
            if current_count > target_count:
                excess_count = current_count - target_count
                # Within each bucket, keep the closest peers
                peers_with_dist = [(pid, self.get_distance_to_peer(pid)) for pid in current_peers]
                peers_with_dist.sort(key=lambda x: x[1])  # Sort by distance (closest first)
                
                # Remove the furthest peers in this bucket
                for i in range(min(excess_count, len(peers_with_dist))):
                    furthest_peer = peers_with_dist[-(i+1)][0]
                    to_remove.append(furthest_peer)
        
        # If we still need to remove more, remove randomly
        current_after_removal = [p for p in current_connected if p not in to_remove]
        additional_needed = len(current_after_removal) - self.max_connections
        
        if additional_needed > 0:
            additional_removals = random.sample(current_after_removal, 
                                              min(additional_needed, len(current_after_removal)))
            to_remove.extend(additional_removals)
        
        return to_remove
    
    def remove_excess_connections(self, all_peers: List[int] = None) -> List[int]:
        """Remove excess connections using improved distance optimization."""
        connected = self.get_connected_peers()
        
        # If under soft limit, no removal needed
        if len(connected) <= self.soft_max_connections:
            return []
        
        if all_peers is None:
            # Fallback to random removal
            excess_count = len(connected) - self.max_connections
            to_remove = random.sample(connected, min(excess_count, len(connected)))
        else:
            # Use improved distance optimization
            to_remove = self.optimize_connections_improved(all_peers)
        
        # Remove the selected peers
        for peer_id in to_remove:
            if peer_id in self.known_peers:
                del self.known_peers[peer_id]
        
        return to_remove
    
    def get_routing_stats(self, all_peers: List[int] = None) -> Dict:
        """Get routing-focused statistics."""
        connected = self.get_connected_peers()
        if not connected or not all_peers:
            return {"total": len(connected)}
        
        try:
            target_distribution, buckets = self.calculate_routing_distribution(all_peers)
            
            # Analyze current distribution by prefix length
            current_by_prefix = {}
            for prefix_len in range(32):
                current_by_prefix[prefix_len] = 0
            
            for connected_peer in connected:
                distance = self.get_distance_to_peer(connected_peer)
                if distance > 0:
                    prefix_len = self._count_leading_zeros(distance)
                    prefix_len = min(prefix_len, 31)
                    current_by_prefix[prefix_len] += 1
            
            # Filter to non-zero buckets
            active_buckets = {k: v for k, v in current_by_prefix.items() if v > 0}
            
            return {
                "total": len(connected),
                "max_allowed": self.max_connections,
                "soft_max": self.soft_max_connections,
                "prefix_distribution": active_buckets,
                "targets": target_distribution
            }
        except Exception as e:
            return {"total": len(connected), "error": str(e)}


class ImprovedDistanceSimulator(NetworkSimulator):
    """Simulator using improved distance-optimized peers."""
    
    def _initialize_network(self):
        """Initialize network with improved distance-optimized peers."""
        # Generate connected set
        connected_ids = self._generate_peer_ids(self.config.connected_set_size)
        self.connected_set = set(connected_ids)
        
        for peer_id in connected_ids:
            peer = ImprovedDistanceOptimizedPeer(peer_id, self.config.max_connections_per_peer)
            # Add all other connected peers as Connected initially
            for other_id in connected_ids:
                if other_id != peer_id:
                    peer.add_peer(other_id, PeerState.CONNECTED, 0)
            self.peers[peer_id] = peer
        
        # Generate candidate set
        candidate_ids = self._generate_peer_ids(self.config.candidate_set_size, exclude=connected_ids)
        self.candidate_set = set(candidate_ids)
        
        for peer_id in candidate_ids:
            peer = ImprovedDistanceOptimizedPeer(peer_id, self.config.max_connections_per_peer)
            # Add random connected peers as Identified
            num_identified = min(self.config.initial_identified_peers, len(connected_ids))
            identified_peers = random.sample(connected_ids, num_identified)
            for identified_id in identified_peers:
                peer.add_peer(identified_id, PeerState.IDENTIFIED, -1)
            self.peers[peer_id] = peer
        
        # Update metrics tracking
        self.metrics.set_candidate_ids(self.candidate_set)
    
    def simulate_round(self):
        """Simulate round with improved distance optimization."""
        invites_to_send = []
        all_peer_ids = list(self.peers.keys())
        
        # Phase 1: Mapping requests
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
        
        # Phase 2: Send invites
        for sender_id, receiver_id in invites_to_send:
            if receiver_id in self.peers:
                invite = Invite(sender_id, self.round_number)
                self.peers[receiver_id].receive_invite(invite)
        
        # Phase 3: Process invites and improved connection management
        for peer in self.peers.values():
            # Process received invites
            peer.process_invites(self.round_number)
            
            # Send maintenance invites
            connected_peers = peer.get_connected_peers()
            if len(connected_peers) >= 2:
                selected_peers = random.sample(connected_peers, min(2, len(connected_peers)))
                for target_id in selected_peers:
                    if target_id in self.peers:
                        invite = Invite(peer.id, self.round_number)
                        self.peers[target_id].receive_invite(invite)
            
            # Improved connection optimization
            peer.remove_excess_connections(all_peer_ids)
        
        # Record metrics
        self.metrics.record_round(self.round_number, self.peers)
        self.round_number += 1


def test_improved_optimization():
    """Test the improved distance optimization."""
    print("Testing Improved Distance-Based Optimization")
    print("=" * 60)
    
    config = SimulationConfig(
        connected_set_size=50,
        candidate_set_size=25,
        max_connections_per_peer=8,
        initial_identified_peers=4,
        simulation_rounds=200,
        random_seed=12345
    )
    
    # Test multiple approaches
    simulators = {
        "Regular": NetworkSimulator(config),
        "Improved Distance": ImprovedDistanceSimulator(config)
    }
    
    results = {}
    
    for name, sim in simulators.items():
        print(f"\nRunning {name} simulation...")
        for _ in range(config.simulation_rounds):
            sim.simulate_round()
        
        # Analyze routing performance
        hop_counts = []
        test_peers = random.sample(list(sim.peers.keys()), min(10, len(sim.peers)))
        
        for test_peer_id in test_peers:
            if test_peer_id not in sim.peers:
                continue
                
            peer = sim.peers[test_peer_id]
            
            # Test routing performance
            for _ in range(10):  # 10 queries per test peer
                max_id = (1 << sim.config.address_space_bits) - 1
                target_id = random.randint(0, max_id)
                
                hops = 0
                current_id = test_peer_id
                visited = set()
                
                while hops < 15 and current_id not in visited:
                    visited.add(current_id)
                    hops += 1
                    
                    if current_id not in sim.peers:
                        break
                        
                    current_peer = sim.peers[current_id]
                    connected = current_peer.get_connected_peers()
                    
                    if not connected:
                        break
                    
                    # Find closest connected peer
                    closest = min(connected, key=lambda pid: current_peer._xor_distance(pid, target_id))
                    current_distance = current_peer._xor_distance(current_id, target_id)
                    closest_distance = current_peer._xor_distance(closest, target_id)
                    
                    if closest_distance >= current_distance:
                        break
                    
                    current_id = closest
                
                hop_counts.append(hops)
        
        results[name] = {
            "avg_hops": statistics.mean(hop_counts) if hop_counts else 0,
            "max_hops": max(hop_counts) if hop_counts else 0,
            "min_hops": min(hop_counts) if hop_counts else 0,
            "hop_distribution": sorted(hop_counts)
        }
    
    # Compare results
    print(f"\nRouting Performance Comparison:")
    print("=" * 40)
    
    for name, data in results.items():
        print(f"{name}:")
        print(f"  Average hops: {data['avg_hops']:.2f}")
        print(f"  Hop range: {data['min_hops']} - {data['max_hops']}")
        print(f"  Sample distribution: {data['hop_distribution'][:10]}")
    
    # Analyze connection patterns for improved version
    if "Improved Distance" in simulators:
        sim = simulators["Improved Distance"]
        sample_peers = list(sim.connected_set)[:3]
        
        print(f"\nConnection Pattern Analysis (Improved):")
        for peer_id in sample_peers:
            if peer_id in sim.peers and hasattr(sim.peers[peer_id], 'get_routing_stats'):
                peer = sim.peers[peer_id]
                stats = peer.get_routing_stats(list(sim.peers.keys()))
                
                print(f"\nPeer {peer_id}:")
                print(f"  Total connections: {stats['total']}/{stats.get('max_allowed', 0)}")
                
                if 'prefix_distribution' in stats:
                    active_prefixes = {k: v for k, v in stats['prefix_distribution'].items() if v > 0}
                    print(f"  Prefix distribution: {dict(list(active_prefixes.items())[:8])}")
    
    return results


def main():
    """Test improved distance optimization."""
    results = test_improved_optimization()
    
    print(f"\n" + "=" * 60)
    print("Improved Distance Optimization Summary:")
    
    if "Regular" in results and "Improved Distance" in results:
        regular_hops = results["Regular"]["avg_hops"]
        improved_hops = results["Improved Distance"]["avg_hops"]
        
        if improved_hops < regular_hops:
            improvement = (regular_hops - improved_hops) / regular_hops * 100
            print(f"✓ Improved routing: {improvement:.1f}% reduction in average hops")
        else:
            degradation = (improved_hops - regular_hops) / regular_hops * 100
            print(f"⚠ Routing degradation: {degradation:.1f}% increase in average hops")
    
    print("- Uses Kademlia-style prefix buckets for better routing structure")
    print("- Maintains more connections to nearby peers (longer prefixes)")
    print("- Ensures some connections to distant areas for global routing")
    print("- Allows connection slack for optimization flexibility")


if __name__ == "__main__":
    main()