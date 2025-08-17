#!/usr/bin/env python3
"""
Vote Propagation Simulation

Simulates the core voting protocol from ecRust's commit mechanism.
Based on the voting logic in src/ec_mempool.rs.
"""

import random
import argparse
from typing import Dict, Set, List, Optional
from enum import Enum


class PartitionMode(Enum):
    NONE = "none"           # No partitions - normal operation
    BINARY = "binary"       # Two partitions that can't communicate
    BRIDGE = "bridge"       # Three partitions: A and B isolated, C can reach both


class Node:
    """
    Represents a node in the voting network.
    
    Each node maintains:
    - connections: set of peer node IDs
    - votes_received: most recent vote from each peer
    - committed: whether this node has committed
    - active: whether this node is participating in voting
    """
    
    def __init__(self, node_id: int, partition: int = 0, is_byzantine: bool = False, byzantine_testing: bool = False, reverse_byzantine: bool = False, illegal_resistance: bool = False):
        self.node_id = node_id
        self.partition = partition  # Which partition this node belongs to
        self.is_byzantine = is_byzantine  # True if this is a bad actor
        self.byzantine_testing = byzantine_testing  # True if Byzantine testing is enabled
        self.reverse_byzantine = reverse_byzantine  # True if Byzantine nodes should vote -1 (blocking), False if +1 (forcing)
        self.illegal_resistance = illegal_resistance  # True if honest nodes resist illegal behavior by never committing
        self.connections: Set[int] = set()
        self.votes_received: Dict[int, int] = {}  # peer_id -> vote (+1 or -1)
        self.committed = False
        self.active = False
    
    def add_connection(self, peer_id: int):
        """Add a connection to another node"""
        self.connections.add(peer_id)
    
    def receive_vote(self, from_peer: int, vote: int):
        """
        Receive a vote from a peer. Store the most recent vote.
        If this node is committed, it no longer stores new votes.
        Only accept votes from connected nodes.
        """
        if not self.committed and from_peer in self.connections:
            self.votes_received[from_peer] = vote
            if not self.active:
                self.active = True
    
    def calculate_vote_balance(self) -> int:
        """Calculate the sum of all received votes"""
        return sum(self.votes_received.values())
    
    def should_commit(self) -> bool:
        """Check if vote balance > 2 and node should commit"""
        if self.committed:
            return False
        
        # If illegal resistance is enabled and this is an honest node, never commit
        if self.illegal_resistance and not self.is_byzantine:
            return False
        
        return self.calculate_vote_balance() > 2
    
    def commit(self):
        """Commit this node - it stops participating in voting"""
        self.committed = True
        self.active = False
    
    def get_outgoing_votes(self, all_nodes: List[int] = None) -> List[int]:
        """
        Select peers to send votes to.
        Byzantine nodes vote to ALL nodes, honest nodes follow normal rules.
        """
        if self.is_byzantine:
            # Bad actors vote to ALL nodes in the network
            if all_nodes is not None:
                return [node_id for node_id in all_nodes if node_id != self.node_id]
            else:
                return []
        
        # Honest nodes follow normal rules
        if not self.active or self.committed or len(self.connections) == 0:
            return []
        
        # Select up to 2 random connections
        available_peers = list(self.connections)
        num_votes = min(2, len(available_peers))
        return random.sample(available_peers, num_votes)
    
    def generate_vote(self) -> int:
        """Generate a vote based on node type and commitment status"""
        if self.committed:
            return 1  # All committed nodes vote +1
        elif self.byzantine_testing:
            if self.is_byzantine:
                if self.reverse_byzantine:
                    return -1  # Byzantine nodes vote -1 (trying to block consensus)
                else:
                    return 1  # Byzantine nodes vote +1 (trying to force consensus)
            else:
                if self.reverse_byzantine:
                    return 1  # Honest nodes vote +1 (trying to achieve consensus)
                else:
                    return -1  # Honest nodes vote -1 (defensive against forcing attack)
        else:
            return 1  # Original protocol: all nodes vote +1
    
    def respond_to_vote(self, requesting_peer: int) -> Optional[int]:
        """
        If committed, respond with +1 to non-committed peers.
        Otherwise, no response.
        """
        if self.committed:
            return 1
        return None


class VotingSimulation:
    """
    Main simulation class that manages the network and voting process.
    """
    
    def __init__(self, num_nodes: int, connections_per_node: int, 
                 bidirectional_prob: float, starting_nodes: int,
                 partition_mode: PartitionMode = PartitionMode.NONE,
                 partition_ratio: float = 0.5,
                 byzantine_ratio: float = 0.0,
                 reverse_byzantine: bool = False,
                 illegal_resistance: bool = False):
        self.num_nodes = num_nodes
        self.connections_per_node = connections_per_node
        self.bidirectional_prob = bidirectional_prob
        self.starting_nodes = starting_nodes
        self.partition_mode = partition_mode
        self.partition_ratio = partition_ratio
        self.byzantine_ratio = byzantine_ratio
        self.reverse_byzantine = reverse_byzantine
        self.illegal_resistance = illegal_resistance
        
        self.nodes: Dict[int, Node] = {}
        self.round_num = 0
        self.total_votes_sent = 0
        
        self._setup_network()
        self._setup_partitions()
        self._setup_byzantine_nodes()
        self._activate_starting_nodes()
    
    def _setup_byzantine_nodes(self):
        """Assign Byzantine (bad actor) status to a percentage of nodes"""
        if self.byzantine_ratio <= 0:
            return
        
        num_byzantine = int(self.num_nodes * self.byzantine_ratio)
        node_ids = list(range(self.num_nodes))
        random.shuffle(node_ids)
        
        for i in range(num_byzantine):
            node_id = node_ids[i]
            # Recreate node as Byzantine
            old_node = self.nodes[node_id]
            self.nodes[node_id] = Node(node_id, old_node.partition, is_byzantine=True, byzantine_testing=True, reverse_byzantine=self.reverse_byzantine, illegal_resistance=self.illegal_resistance)
            # Copy connections
            self.nodes[node_id].connections = old_node.connections
    
    def _setup_network(self):
        """Create nodes and establish random connections"""
        # Create all nodes (Byzantine status assigned later)
        byzantine_testing_enabled = self.byzantine_ratio > 0
        for i in range(self.num_nodes):
            self.nodes[i] = Node(i, byzantine_testing=byzantine_testing_enabled, reverse_byzantine=self.reverse_byzantine, illegal_resistance=self.illegal_resistance)
        
        # Establish connections
        for node_id in range(self.num_nodes):
            node = self.nodes[node_id]
            
            # Select random peers for connections
            available_peers = [i for i in range(self.num_nodes) if i != node_id]
            num_connections = min(self.connections_per_node, len(available_peers))
            
            if num_connections > 0:
                peers = random.sample(available_peers, num_connections)
                
                for peer_id in peers:
                    # Add forward connection
                    node.add_connection(peer_id)
                    
                    # Add reverse connection with probability
                    if random.random() < self.bidirectional_prob:
                        self.nodes[peer_id].add_connection(node_id)
    
    def _setup_partitions(self):
        """Assign nodes to partitions based on partition mode"""
        if self.partition_mode == PartitionMode.NONE:
            # All nodes in partition 0 (no partitioning)
            for node in self.nodes.values():
                node.partition = 0
        
        elif self.partition_mode == PartitionMode.BINARY:
            # Split nodes into two partitions
            partition_size = int(self.num_nodes * self.partition_ratio)
            node_ids = list(range(self.num_nodes))
            random.shuffle(node_ids)
            
            for i, node_id in enumerate(node_ids):
                if i < partition_size:
                    self.nodes[node_id].partition = 0
                else:
                    self.nodes[node_id].partition = 1
        
        elif self.partition_mode == PartitionMode.BRIDGE:
            # Split into three partitions: A, B (isolated), C (bridge)
            bridge_size = max(1, int(self.num_nodes * 0.2))  # 20% for bridge
            partition_a_size = int((self.num_nodes - bridge_size) * self.partition_ratio)
            
            node_ids = list(range(self.num_nodes))
            random.shuffle(node_ids)
            
            for i, node_id in enumerate(node_ids):
                if i < partition_a_size:
                    self.nodes[node_id].partition = 0  # Partition A
                elif i < partition_a_size + (self.num_nodes - bridge_size - partition_a_size):
                    self.nodes[node_id].partition = 1  # Partition B
                else:
                    self.nodes[node_id].partition = 2  # Bridge partition C
    
    def _can_communicate(self, from_node: int, to_node: int) -> bool:
        """Check if two nodes can communicate based on partition rules"""
        from_partition = self.nodes[from_node].partition
        to_partition = self.nodes[to_node].partition
        
        if self.partition_mode == PartitionMode.NONE:
            return True
        
        elif self.partition_mode == PartitionMode.BINARY:
            # Only nodes in same partition can communicate
            return from_partition == to_partition
        
        elif self.partition_mode == PartitionMode.BRIDGE:
            # Partition 2 (bridge) can communicate with all
            # Partitions 0 and 1 can only communicate within themselves
            if from_partition == 2 or to_partition == 2:
                return True
            return from_partition == to_partition
        
        return False
    
    def _activate_starting_nodes(self):
        """Activate the initial set of nodes to start voting"""
        starting_node_ids = random.sample(range(self.num_nodes), self.starting_nodes)
        for node_id in starting_node_ids:
            self.nodes[node_id].active = True
    
    def run_simulation_step(self) -> tuple[int, int]:
        """
        Run one step of the simulation.
        Returns (votes_sent_this_round, committed_nodes_total)
        """
        self.round_num += 1
        votes_this_round = 0
        
        # Collect all outgoing votes from active nodes
        vote_messages = []  # (from_node, to_node, vote)
        
        all_node_ids = list(self.nodes.keys())
        
        for node_id, node in self.nodes.items():
            # Byzantine nodes are always active
            if node.is_byzantine or (node.active and not node.committed):
                target_peers = node.get_outgoing_votes(all_node_ids)
                for peer_id in target_peers:
                    vote = node.generate_vote()
                    vote_messages.append((node_id, peer_id, vote))
                    votes_this_round += 1
        
        # Process all vote messages (respecting partition boundaries for honest nodes)
        for from_node, to_node, vote in vote_messages:
            # Byzantine nodes ignore partition boundaries, honest nodes respect them
            #can_communicate = (self.nodes[from_node].is_byzantine or 
            #                 self._can_communicate(from_node, to_node))

            # network partitions are physical
            can_communicate = self._can_communicate(from_node, to_node)
            
            if can_communicate:
                # Only honest nodes check connections, Byzantine votes always go through
                if (self.nodes[from_node].is_byzantine or 
                    from_node in self.nodes[to_node].connections):
                    self.nodes[to_node].receive_vote(from_node, vote)
                
                # Check if receiving node should respond (if committed and honest)
                if not self.nodes[to_node].is_byzantine:
                    response = self.nodes[to_node].respond_to_vote(from_node)
                    if (response is not None and 
                        self._can_communicate(to_node, from_node) and
                        to_node in self.nodes[from_node].connections):
                        self.nodes[from_node].receive_vote(to_node, response)
        
        # Check for new commits
        for node in self.nodes.values():
            if node.should_commit():
                node.commit()
        
        self.total_votes_sent += votes_this_round
        committed_count = sum(1 for node in self.nodes.values() if node.committed)
        active_count = sum(1 for node in self.nodes.values() if node.active)
        
        return votes_this_round, committed_count, active_count
    
    def run_full_simulation(self, max_rounds: int = 1000) -> List[tuple[int, int, int]]:
        """
        Run the complete simulation until no more votes or max rounds.
        Returns list of (round, votes_sent, committed_nodes) tuples.
        """
        results = []
        
        for round_num in range(max_rounds):
            votes_this_round, committed_count, active_count = self.run_simulation_step()
            results.append((round_num + 1, votes_this_round, committed_count))
            
            # Show per-partition stats if partitions exist
            if self.partition_mode != PartitionMode.NONE:
                partition_stats = self._get_partition_stats()
                print(f"Round {round_num + 1:3d}: {votes_this_round:4d} votes, "
                      f"{committed_count:4d} committed nodes, {active_count:4d} active nodes")
                print(f"    Partitions - {partition_stats}")
            else:
                print(f"Round {round_num + 1:3d}: {votes_this_round:4d} votes, "
                      f"{committed_count:4d} committed nodes, {active_count:4d} active nodes")
            
            # Stop if no votes were sent
            if votes_this_round == 0 or active_count == 0:
                print(f"\nSimulation ended after {round_num + 1} rounds - no more votes")
                break
        
        return results
    
    def _get_partition_stats(self) -> str:
        """Get per-partition statistics as a formatted string"""
        partition_counts = {}
        partition_committed = {}
        partition_active = {}
        partition_byzantine = {}
        
        for node in self.nodes.values():
            p = node.partition
            if p not in partition_counts:
                partition_counts[p] = 0
                partition_committed[p] = 0
                partition_active[p] = 0
                partition_byzantine[p] = 0
            
            partition_counts[p] += 1
            if node.committed:
                partition_committed[p] += 1
            if node.active:
                partition_active[p] += 1
            if node.is_byzantine:
                partition_byzantine[p] += 1
        
        stats = []
        for p in sorted(partition_counts.keys()):
            byz_str = f"/{partition_byzantine[p]}B" if self.byzantine_ratio > 0 else ""
            stats.append(f"P{p}: {partition_committed[p]}/{partition_counts[p]}{byz_str} committed, "
                        f"{partition_active[p]} active")
        
        return " | ".join(stats)
    
    def print_network_stats(self):
        """Print statistics about the network setup"""
        total_connections = sum(len(node.connections) for node in self.nodes.values())
        avg_connections = total_connections / self.num_nodes
        
        active_nodes = sum(1 for node in self.nodes.values() if node.active)
        committed_nodes = sum(1 for node in self.nodes.values() if node.committed)
        
        byzantine_nodes = sum(1 for node in self.nodes.values() if node.is_byzantine)
        
        print(f"Network Statistics:")
        print(f"  Total nodes: {self.num_nodes}")
        print(f"  Average connections per node: {avg_connections:.2f}")
        print(f"  Partition mode: {self.partition_mode.value}")
        if self.partition_mode != PartitionMode.NONE:
            print(f"  Partition ratio: {self.partition_ratio:.2f}")
            print(f"  Partition distribution: {self._get_partition_stats()}")
        if self.byzantine_ratio > 0:
            print(f"  Byzantine nodes: {byzantine_nodes}/{self.num_nodes} ({self.byzantine_ratio:.1%})")
        print(f"  Starting active nodes: {self.starting_nodes}")
        print(f"  Currently active nodes: {active_nodes}")
        print(f"  Committed nodes: {committed_nodes}")
        print()


def main():
    parser = argparse.ArgumentParser(description='Voting Simulation based on ecRust commit protocol')
    
    parser.add_argument('--nodes', type=int, default=50,
                        help='Number of nodes in the network (default: 50)')
    parser.add_argument('--connections', type=int, default=10,
                        help='Number of connections per node (default: 10)')
    parser.add_argument('--bidirectional-prob', type=float, default=0.7,
                        help='Probability of bidirectional connections (default: 0.7)')
    parser.add_argument('--starting-nodes', type=int, default=2,
                        help='Number of nodes to start voting (default: 2)')
    parser.add_argument('--max-rounds', type=int, default=100,
                        help='Maximum simulation rounds (default: 100)')
    parser.add_argument('--seed', type=int, default=None,
                        help='Random seed for reproducible results')
    
    # Partition configuration arguments
    parser.add_argument('--partition-mode', type=str, choices=['none', 'binary', 'bridge'], 
                        default='none', help='Partition mode: none (default), binary, or bridge')
    parser.add_argument('--partition-ratio', type=float, default=0.5,
                        help='Split ratio for partitions (default: 0.5)')
    
    # Byzantine testing arguments
    parser.add_argument('--byzantine-ratio', type=float, default=0.0,
                        help='Fraction of nodes that are Byzantine bad actors (default: 0.0)')
    parser.add_argument('--reverse-byzantine', action='store_true',
                        help='Reverse Byzantine scenario: bad actors vote -1 (blocking) instead of +1 (forcing)')
    parser.add_argument('--illegal-resistance', action='store_true',
                        help='Honest nodes detect illegal behavior and never commit (prevents amplification)')
    
    args = parser.parse_args()
    
    if args.seed is not None:
        random.seed(args.seed)
        print(f"Using random seed: {args.seed}")
    
    # Parse partition mode
    partition_mode = PartitionMode(args.partition_mode)
    
    print(f"Starting vote propagation simulation...")
    print(f"Parameters: {args.nodes} nodes, {args.connections} connections per node, "
          f"{args.bidirectional_prob:.2f} bidirectional probability, "
          f"{args.starting_nodes} starting nodes")
    if partition_mode != PartitionMode.NONE:
        print(f"Partition mode: {partition_mode.value} (ratio: {args.partition_ratio:.2f})")
    if args.byzantine_ratio > 0:
        attack_type = "blocking attack (-1 votes)" if args.reverse_byzantine else "forcing attack (+1 votes)"
        resistance_mode = " with illegal resistance" if args.illegal_resistance else ""
        print(f"Byzantine testing: {args.byzantine_ratio:.1%} bad actors ({attack_type}){resistance_mode}")
    print()
    
    # Create and run simulation
    sim = VotingSimulation(
        num_nodes=args.nodes,
        connections_per_node=args.connections,
        bidirectional_prob=args.bidirectional_prob,
        starting_nodes=args.starting_nodes,
        partition_mode=partition_mode,
        partition_ratio=args.partition_ratio,
        byzantine_ratio=args.byzantine_ratio,
        reverse_byzantine=args.reverse_byzantine,
        illegal_resistance=args.illegal_resistance
    )
    
    sim.print_network_stats()
    results = sim.run_full_simulation(args.max_rounds)
    
    # Print final statistics
    print(f"\nFinal Results:")
    print(f"  Total rounds: {len(results)}")
    print(f"  Total votes sent: {sim.total_votes_sent}")
    print(f"  Final committed nodes: {results[-1][2] if results else 0}")
    print(f"  Commitment rate: {(results[-1][2] / args.nodes * 100):.1f}%" if results else "0%")
    if partition_mode != PartitionMode.NONE:
        print(f"  Final partition stats: {sim._get_partition_stats()}")
    if args.byzantine_ratio > 0:
        honest_nodes = sum(1 for node in sim.nodes.values() if not node.is_byzantine)
        honest_committed = sum(1 for node in sim.nodes.values() if not node.is_byzantine and node.committed)
        byzantine_committed = sum(1 for node in sim.nodes.values() if node.is_byzantine and node.committed)
        print(f"  Honest nodes committed: {honest_committed}/{honest_nodes} ({honest_committed/honest_nodes*100:.1f}%)")
        print(f"  Byzantine nodes committed: {byzantine_committed}/{sim.num_nodes - honest_nodes}")
        print(f"  Attack success rate: {byzantine_committed/args.nodes*100:.1f}%")


if __name__ == "__main__":
    main()