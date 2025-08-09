import random
import bisect
from collections import Counter, defaultdict
from statistics import mean, median

def generate_256_bit_token():
    """Generate a random 256-bit token as an integer."""
    return random.getrandbits(256)

def generate_100_bit_signature():
    """Generate a random 100-bit signature and split into 10 chunks of 10 bits each."""
    signature = random.getrandbits(100)
    chunks = []
    for i in range(10):
        chunk = (signature >> (i * 10)) & 0x3FF  # Extract 10 bits (0x3FF = 1023)
        chunks.append(chunk)
    return signature, chunks

def find_tokens_by_signature(sorted_tokens, lookup_token, signature_chunks):
    """Find tokens based on signature-directed search."""
    # Find insertion point (closest position)
    pos = bisect.bisect_left(sorted_tokens, lookup_token)
    
    result = []

    # Get 5 tokens above using first 5 signature chunks
    x = pos + 1
    i = 0
    steps = 0
    while i < 5 and x < len(sorted_tokens):
        if sorted_tokens[x] & 0x3FF == signature_chunks[i]:
            result.append(sorted_tokens[x])
            i = i + 1
        x = x + 1
        steps = steps + 1
    
    # Get 5 tokens below using last 5 signature chunks
    x = pos - 1
    while i < 10 and x >= 0:
        if sorted_tokens[x] & 0x3FF == signature_chunks[i]:
            result.append(sorted_tokens[x])
            i = i + 1
        x = x - 1
        steps = steps + 1

    return result, steps

class StorageNode:
    """Represents a storage node with a specific density of the total token set."""
    
    def __init__(self, node_id, density, total_tokens):
        self.node_id = node_id
        self.density = density
        self.total_tokens = total_tokens
        # Sample tokens based on density
        sample_size = int(len(total_tokens) * density)
        self.stored_tokens = random.sample(total_tokens, sample_size)
        
    def respond_to_lookup(self, lookup_token, signature_chunks):
        """Generate a response to a lookup request using signature-based proof."""
        # Add lookup token to ensure it's in the sorted list
        tokens_with_lookup = self.stored_tokens + [lookup_token]
        sorted_tokens = sorted(tokens_with_lookup)
        
        response_tokens, steps = find_tokens_by_signature(sorted_tokens, lookup_token, signature_chunks)
        return response_tokens, steps

def select_winning_responses(all_responses, selection_ratio=0.3):
    """Select winning responses based on most common tokens across all responses."""
    # Count frequency of each token across all responses
    token_frequency = Counter()
    for node_id, (response_tokens, _) in all_responses.items():
        for token in response_tokens:
            token_frequency[token] += 1
    
    # Calculate commonality score for each response
    response_scores = {}
    for node_id, (response_tokens, _) in all_responses.items():
        # Score is sum of frequencies of tokens in this response
        score = sum(token_frequency[token] for token in response_tokens)
        response_scores[node_id] = score
    
    # Select top responses based on selection ratio
    num_winners = max(1, int(len(all_responses) * selection_ratio))
    sorted_responses = sorted(response_scores.items(), key=lambda x: x[1], reverse=True)
    winning_node_ids = [node_id for node_id, _ in sorted_responses[:num_winners]]
    
    return winning_node_ids, response_scores

def run_winning_set_analysis():
    print("Winning Set Analysis - Density vs Selection Probability")
    print("=" * 60)
    
    # Generate master token set
    total_token_count = 20000  # Reduced for faster execution
    print(f"Generating {total_token_count} total tokens...")
    total_tokens = [generate_256_bit_token() for _ in range(total_token_count)]
    
    # Define node densities to test
    densities = [0.95, 0.90, 0.80, 0.70, 0.60, 0.50, 0.40, 0.30]  # Reduced count
    nodes_per_density = 5   # Reduced for faster execution
    selection_ratio = 0.3   # Top 30% of responses are selected as winners
    
    # Create nodes with different densities
    all_nodes = []
    node_id = 0
    for density in densities:
        for _ in range(nodes_per_density):
            node = StorageNode(node_id, density, total_tokens)
            all_nodes.append(node)
            node_id += 1
    
    print(f"Created {len(all_nodes)} nodes across {len(densities)} density levels")
    print(f"Selection ratio: {selection_ratio*100:.0f}% (top responses win)")
    
    # Run multiple scenarios
    num_scenarios = 50  # Reduced for faster execution
    density_win_counts = defaultdict(int)  # Track wins per density
    density_total_counts = defaultdict(int)  # Track total nodes per density
    
    print(f"\nRunning {num_scenarios} scenarios...")
    
    scenario_results = []
    
    for scenario in range(num_scenarios):
        # Generate random lookup and signature for this scenario
        lookup_token = generate_256_bit_token()
        signature, signature_chunks = generate_100_bit_signature()
        
        # Collect responses from all nodes
        all_responses = {}
        for node in all_nodes:
            response_tokens, steps = node.respond_to_lookup(lookup_token, signature_chunks)
            all_responses[node.node_id] = (response_tokens, steps)
        
        # Select winning responses
        winning_node_ids, response_scores = select_winning_responses(all_responses, selection_ratio)
        
        # Track results by density
        scenario_result = {
            'winning_nodes': [],
            'all_scores': []
        }
        
        for node in all_nodes:
            density_total_counts[node.density] += 1
            is_winner = node.node_id in winning_node_ids
            
            if is_winner:
                density_win_counts[node.density] += 1
            
            scenario_result['all_scores'].append({
                'node_id': node.node_id,
                'density': node.density,
                'score': response_scores[node.node_id],
                'is_winner': is_winner
            })
            
            if is_winner:
                scenario_result['winning_nodes'].append(node.density)
        
        scenario_results.append(scenario_result)
        
        if (scenario + 1) % 20 == 0:
            print(f"  Completed {scenario + 1}/{num_scenarios} scenarios")
    
    # Calculate selection probabilities
    print("\n" + "=" * 60)
    print("SELECTION PROBABILITY BY DENSITY")
    print("=" * 60)
    
    results_summary = []
    for density in sorted(densities, reverse=True):
        total_appearances = density_total_counts[density]
        wins = density_win_counts[density]
        win_probability = wins / total_appearances if total_appearances > 0 else 0
        
        results_summary.append({
            'density': density,
            'win_probability': win_probability,
            'wins': wins,
            'total': total_appearances
        })
        
        print(f"Density {density*100:4.0f}%: {win_probability:6.3f} probability ({wins:3d}/{total_appearances:3d} wins)")
    
    # Statistical analysis
    print("\n" + "=" * 60)
    print("STATISTICAL ANALYSIS")
    print("=" * 60)
    
    # Calculate correlation coefficient
    densities_list = [r['density'] for r in results_summary]
    probabilities_list = [r['win_probability'] for r in results_summary]
    
    # Simple correlation calculation
    n = len(densities_list)
    sum_x = sum(densities_list)
    sum_y = sum(probabilities_list)
    sum_xy = sum(x * y for x, y in zip(densities_list, probabilities_list))
    sum_x2 = sum(x * x for x in densities_list)
    sum_y2 = sum(y * y for y in probabilities_list)
    
    correlation = (n * sum_xy - sum_x * sum_y) / ((n * sum_x2 - sum_x**2) * (n * sum_y2 - sum_y**2))**0.5
    
    print(f"Correlation between density and selection probability: {correlation:.4f}")
    
    # Expected vs actual selection rates
    expected_selection_rate = selection_ratio
    actual_selection_rates = probabilities_list
    
    print(f"\nExpected uniform selection rate: {expected_selection_rate:.3f}")
    print(f"Actual selection rates range: {min(actual_selection_rates):.3f} to {max(actual_selection_rates):.3f}")
    if min(actual_selection_rates) > 0:
        print(f"Selection rate improvement (max/min): {max(actual_selection_rates)/min(actual_selection_rates):.2f}x")
    else:
        print(f"Selection rate improvement: infinite (some densities have 0% selection rate)")
    
    # Analyze score distributions
    print("\n" + "=" * 60)
    print("RESPONSE SCORE ANALYSIS")
    print("=" * 60)
    
    density_scores = defaultdict(list)
    for scenario_result in scenario_results:
        for score_data in scenario_result['all_scores']:
            density_scores[score_data['density']].append(score_data['score'])
    
    for density in sorted(densities, reverse=True):
        scores = density_scores[density]
        avg_score = mean(scores)
        med_score = median(scores)
        print(f"Density {density*100:4.0f}%: avg score {avg_score:6.1f}, median {med_score:6.1f}")
    
    # Incentive analysis
    print("\n" + "=" * 60)
    print("STORAGE INCENTIVE ANALYSIS")
    print("=" * 60)
    
    print("Probability improvement by increasing storage:")
    for i in range(len(results_summary) - 1):
        current = results_summary[i]
        next_level = results_summary[i + 1]
        
        storage_increase = (current['density'] - next_level['density']) * 100
        prob_improvement = current['win_probability'] - next_level['win_probability']
        
        if next_level['win_probability'] > 0:
            relative_improvement = (prob_improvement / next_level['win_probability']) * 100
            print(f"  {next_level['density']*100:.0f}% → {current['density']*100:.0f}% "
                  f"(+{storage_increase:.0f}% storage): "
                  f"+{prob_improvement:.3f} probability ({relative_improvement:+.1f}%)")

if __name__ == "__main__":
    random.seed(42)  # For reproducible results
    run_winning_set_analysis()