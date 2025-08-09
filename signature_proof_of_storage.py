import random
import bisect
from collections import defaultdict

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
    w = 0
    while i < 5 and x < len(sorted_tokens):
        if sorted_tokens[x] & 0x3FF == signature_chunks[i]:
            result.append(sorted_tokens[x])
            i = i + 1
        x = x + 1
        w = w + 1
    
    # Get 5 tokens below using last 5 signature chunks
    x = pos - 1
    while i < 10 and x >= 0:
        if sorted_tokens[x] & 0x3FF == signature_chunks[i]:
            result.append(sorted_tokens[x])
            i = i + 1
        x = x - 1
        w = w + 1

    return result, w

def run_signature_proof_test():
    print("Signature-Based Proof of Storage Analysis")
    print("=" * 50)
    
    generate = 10_000_000
    # Generate random 256-bit tokens
    print(f"Generating {generate} random 256-bit tokens...")
    tokens = [generate_256_bit_token() for _ in range(generate)]
    
    # Generate and display a random signature
    signature, signature_chunks = generate_100_bit_signature()
    print(f"\nGenerated 100-bit signature: {signature:025x}")
    print(f"Signature chunks (10 bits each): {signature_chunks}")
    
    # Test parameters
    densities = [0.99, 0.95, 0.90, 0.80, 0.50, 0.40]
    num_scenarios = 100

    # Generate random lookup token
    lookup_token = generate_256_bit_token()

    token_frequency_all = {}

    print(f"\nRunning {num_scenarios} test scenarios...")
    
    for density in densities:
        print(f"\nTesting density: {density*100:.0f}%")
        
        token_frequency = {}
        width_dist = []
        steps_dist = []
        for _ in range(num_scenarios):
            # Sample tokens at current density
            sample_size = int(len(tokens) * density)
            sampled_tokens = random.sample(tokens, sample_size) + [lookup_token]
            sorted_sampled = sorted(sampled_tokens)
            
            # Find tokens using signature-based method
            extracted_tokens, steps = find_tokens_by_signature(sorted_sampled, lookup_token, signature_chunks) 

            # width
            width_dist.append(extracted_tokens[4] - extracted_tokens[9])
            # steps
            steps_dist.append(steps)

            # track frequency across extracts
            for t in extracted_tokens:
                f = token_frequency.get(t, 0)
                token_frequency[t] = f + 1

                f = token_frequency_all.get(t, 0)
                token_frequency_all[t] = f + 1

        top_freq = sorted(token_frequency.values(), reverse=True)
        print(f"  freq: {top_freq[0:10]}")

        width_dist = sorted(width_dist)
        print(f"  median width: {width_dist[49]:e} q80: {width_dist[80]:e} q90: {width_dist[90]:e}")

        steps_dist = sorted(steps_dist)
        print(f"  median steps: {steps_dist[49]} q80: {steps_dist[80]} q90: {steps_dist[90]}")

        top_freq = sorted(token_frequency_all.values(), reverse=True)
        print(f"  freq ALL: {top_freq[0:20]}")


if __name__ == "__main__":
    # random.seed(42)  # For reproducible results
    run_signature_proof_test()