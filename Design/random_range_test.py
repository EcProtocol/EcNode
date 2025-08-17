#!/usr/bin/env python3
"""
Simple program to demonstrate random number generation within a range.
Generates 100,000 sorted 64-bit integers, picks a range from the middle,
then finds a random number within that range.
-----4c670a7061b69e17de179c4c72a458502bf509c4061728c95d76750fef2
8000493b75426032fcfbcbc0fdd4804160da883205cfe65bf06cbd8743020831

-----3e2aaccf8dfa2cd31a217f27bb0736e394bdf7b85cc004d701a7be5a325
"""

import random
import sys

def main():
    to_generate = 5_000_000
    print(f"Generating {to_generate:,} random integers...")
    
    max_64bit = 2**256 - 1
    numbers = []
    
    for _ in range(to_generate):
        numbers.append(random.randint(0, max_64bit))
    
    # Sort the array
    numbers.sort()
    print(f"Generated and sorted {len(numbers)} numbers")
    
    # Find middle index and get elements at middle-4 and middle+4
    middle = len(numbers) // 2
    low_index = middle - 10
    high_index = middle + 10
    
    low = numbers[low_index]
    high = numbers[high_index]
    
    print(f"\nMiddle index: {middle}")
    print(f"Low (index {low_index}): {low:x}")
    print(f"High (index {high_index}): {high:x}")
    
    distance = high - low
    print(f"Distance: {distance:e}")
    
    # Generate random numbers until we find one in the range [low, high]
    print(f"\nSearching for random number...")
    iterations = 0
    
    while True:
        iterations += 1
        candidate = random.randint(0, max_64bit)
        
        if low <= candidate <= high:
            print(f"\nFound number in range!")
            print(f"Number as hex: 0x{candidate:x}")
            print(f"Iterations taken: {iterations}")
            break
        
        # Progress indicator every 1000000 iterations
        if iterations % 1000000 == 0:
            print(f"  ... {iterations:,} iterations so far")
    
    # Calculate expected iterations based on probability
    range_size = distance + 1  # +1 because range is inclusive
    total_space = max_64bit + 1
    probability = range_size / total_space
    expected_iterations = 1 / probability
    
    print(f"\nStatistical analysis:")
    print(f"Range 2**40: {2**40:,}")
    print(f"Probability: {probability:.2e}")
    print(f"Expected iterations: {expected_iterations:,.0f}")
    print(f"Actual iterations: {iterations:,}")
    
    if iterations < expected_iterations * 2:
        print("Lucky! Found it faster than expected.")
    elif iterations > expected_iterations * 10:
        print("Unlucky! Took much longer than expected.")
    else:
        print("About what we'd expect statistically.")

if __name__ == "__main__":
    main()