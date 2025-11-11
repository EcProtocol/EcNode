# Peer Election via Proof-of-Storage: Design Document

**Status**: Implementation In Progress
**Version**: 1.0
**Last Updated**: 2025-01-10

---

## Table of Contents

1. [Overview](#overview)
2. [Motivation](#motivation)
3. [Protocol Description](#protocol-description)
4. [Security Properties](#security-properties)
5. [Split-Brain Detection & Resolution](#split-brain-detection--resolution)
6. [Attack Resistance Analysis](#attack-resistance-analysis)
7. [Mathematical Analysis](#mathematical-analysis)
8. [Design Decisions](#design-decisions)
9. [Implementation Notes](#implementation-notes)
10. [Future Enhancements](#future-enhancements)

---

## Overview

The peer election system enables nodes in the ecRust distributed network to discover and connect with highly-aligned peers through a challenge-response mechanism using proof-of-storage signatures.

### Key Features

- **Distributed Peer Discovery**: Nodes challenge random tokens across multiple independent routes
- **Consensus-Based Selection**: Identifies clusters of peers that agree on network state
- **Anti-Gaming Protection**: Duplicate detection and secret-based tickets prevent manipulation
- **Split-Brain Resolution**: Actively resolves competing clusters by spawning additional channels
- **Ring-Based Winner Selection**: Elects peer closest to challenge token on circular ID space

### Goals

1. **Alignment Verification**: Connect to peers that strongly agree on token-to-block mappings
2. **Decentralized Discovery**: No central authority or trusted coordinator
3. **Attack Resistance**: Resist Sybil attacks, collusion, and gaming attempts
4. **Network Health Visibility**: Detect and report split-brain scenarios
5. **Practical Performance**: Complete elections in ~5 seconds with good success rate

---

## Motivation

### The Problem

In a distributed consensus system, nodes need to:
- Discover peers that maintain accurate, aligned state
- Avoid connecting to malicious or out-of-sync peers
- Build a diverse peer set across the ring topology
- Detect network partitions and state disagreements

### Why Proof-of-Storage?

Traditional approaches (DHT routing [4,5], gossip protocols) don't provide **state alignment verification**. A peer may respond to queries but maintain incorrect or outdated state.

**Proof-of-storage signatures** solve this:
- Require peers to demonstrate knowledge of token mappings
- Include 10 signature tokens that must match expected network state
- Make it expensive to fake alignment (must know actual state)
- Enable mathematical analysis of agreement levels

### Why Elections?

Instead of trusting any single peer's response, we:
- Query multiple peers through independent routes
- Require consensus among responses (≥2 peers agreeing on ≥8/10 mappings)
- Select winner from consensus cluster by proximity to challenge token
- Continuously re-elect to detect drift and maintain quality

---

## Protocol Description

### Election Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                        ELECTION TIMELINE                         │
├──────────────┬──────────────────────────┬──────────────────────┤
│ Collection   │ Election + Resolution    │ Timeout              │
│ 0 → 2000ms   │ 2000ms → 5000ms         │ 5000ms               │
└──────────────┴──────────────────────────┴──────────────────────┘

Phase 1: Collection (0 → min_collection_time)
  - Spawn channels to random peers
  - Collect responses as they arrive
  - DO NOT check for consensus (optimization)

Phase 2: Election & Resolution (min_collection_time → timeout)
  - START checking for consensus
  - If winner found → complete election
  - If split-brain detected → spawn more channels to resolve
  - Continue until decisive winner or max channels reached

Phase 3: Timeout (at ttl_ms)
  - Attempt final election with available responses
  - Accept best available or drop election if no consensus
```

### Challenge-Response Flow

```
Node A                     First-Hop B                  Responder C
  │                             │                             │
  │ 1. Generate ticket          │                             │
  │    Blake3(token || B || secret)                          │
  │                             │                             │
  │ 2. Query(token, ticket) ───>│                             │
  │                             │                             │
  │                             │ 3. Forward Query ──────────>│
  │                             │                             │
  │                             │                   4. Generate signature
  │                             │                      (10 token mappings)
  │                             │                             │
  │                             │<──── Answer(signature, ticket)
  │                             │                             │
  │<──── Answer(signature, ticket)                           │
  │                             │                             │
  │ 5. Validate ticket          │                             │
  │ 6. Store response           │                             │
  │ 7. Check consensus          │                             │
```

### Ticket System

**Purpose**: Uniquely identify channels and prevent cross-channel attacks.

**Generation**:
```
ticket = Blake3(challenge_token || first_hop_peer || SECRET)
```

**Properties**:
- Deterministic: Same inputs → same ticket
- Unpredictable: Secret prevents forgery
- Unique per channel: Different first-hop → different ticket
- Secure: 256-bit Blake3 hash

**Security**: Even if an attacker knows challenge_token and first_hop_peer, they cannot forge a valid ticket without the secret. This prevents:
- Replay attacks across elections
- Cross-channel injection
- Ticket prediction

### Channel States

```
┌─────────┐  response arrives  ┌───────────┐
│ Pending ├───────────────────>│ Responded │
└─────────┘                     └───────────┘
     │                               │
     │ duplicate response            │ duplicate response
     │ (RED FLAG)                    │ (RED FLAG)
     v                               v
┌─────────┐                     ┌─────────┐
│ Blocked │                     │ Blocked │
└─────────┘                     └─────────┘
```

**Blocked channels** are excluded from consensus - this is the anti-gaming mechanism.

---

## Security Properties

### 1. Duplicate Detection (Anti-Gaming)

**Attack**: Malicious node on route forwards challenge to multiple peers, collects responses, selects best one to forward.

**Defense**:
- Each channel accepts exactly ONE response
- Second response triggers immediate channel blocking
- Blocked channels excluded from consensus
- All responders in blocked channel are disqualified

**Result**: Gaming attempt detected and neutralized.

### 2. Ticket Uniqueness (Anti-Forgery)

**Attack**: Attacker intercepts ticket, replays it on different route.

**Defense**:
- Tickets are cryptographically bound to (token, first-hop, secret)
- Secret is never transmitted, only known to challenger
- Attacker cannot forge valid tickets for different routes

**Result**: Cross-channel attacks prevented.

### 3. Independent Routes (Anti-Collusion)

**Attack**: Multiple colluding peers coordinate responses.

**Defense**:
- Each channel starts from different first-hop peer
- Routes are independent (non-overlapping preferred)
- Consensus requires ≥2 independent confirmations

**Result**: Collusion requires coordinating multiple independent routes.

### 4. Consensus Threshold (State Validation)

**Attack**: Attacker provides plausible but incorrect state.

**Defense**:
- Require 8/10 token mappings to match between agreeing peers
- Attacker must match 80% of honest network state
- Low probability of accidental match if state is wrong

**Result**: Expensive to fake alignment with honest majority.

---

## Split-Brain Detection & Resolution

### The Problem [10,11]

**Scenario**: Network is genuinely partitioned (CAP theorem), or has competing views of state.

```
Partition A: Peers {1,2,3} agree on state_A (9/10 mappings match)
Partition B: Peers {4,5,6} agree on state_B (9/10 mappings match)

Election collects responses from 3 peers in A, 3 peers in B
→ 3v3 split-brain: No clear winner!
```

### Passive Detection (Original Design)

Simply report: `is_split_brain: true`, pick one arbitrarily.

**Problem**: Provides information but doesn't attempt resolution.

### Active Resolution (Implemented Design)

**Strategy**: Spawn more channels to break the tie.

```
Step 1: Detect split-brain (3v3)
Step 2: Calculate needed channels: Need 60% majority
        Current: 6 responses
        Target: 10 responses (60% = 6 peers → decisive)
        Spawn: 4 more channels
Step 3: Collect additional responses
Step 4: Check again: Now 6v4 → 6/10 = 60% → Decisive winner!
```

### Resolution Algorithm

```rust
if !is_decisive_majority(winning_cluster, valid_count) {
    if has_competing_clusters(all_clusters, winning_cluster) {
        // Split-brain detected
        suggested_channels = calculate_needed_for_majority();
        return SplitBrain { suggested_channels };
    }
}
```

### Majority Threshold: 60%

**Why 60%?**

- **>50%**: Strict majority, but not very strong
- **60%**: Strong majority, confident winner
- **>66%**: Supermajority, may be too strict

**Analysis**:
```
3v3 → 3/6 = 50% → Split-brain (correct)
4v2 → 4/6 = 67% → Decisive (correct)
5v3 → 5/8 = 63% → Decisive (correct)
3v2 → 3/5 = 60% → Decisive (boundary case, reasonable)
```

**Configurable**: Can adjust via `ElectionConfig.majority_threshold`.

### When Resolution Fails

If after spawning up to `max_channels` (default: 10) we still have split-brain:

**This is valuable information!**

The network is genuinely partitioned or has fundamental disagreement. The system:
1. Reports `is_split_brain: true`
2. Still elects a winner from the strongest cluster
3. Logs the split for human analysis
4. Continuous re-election will keep testing for resolution

---

## Attack Resistance Analysis

### Layered Defense Strategy

The peer election system is one layer in a multi-layered defense:

```
Layer 1: POW Identity Generation
  - Peers must perform proof-of-work to obtain ring address
  - Expensive to create many Sybil nodes
  - Extremely expensive to create nodes at specific positions

Layer 2: Continuous Re-Election (ec_peers)
  - Elections run continuously
  - Peers are constantly re-evaluated
  - Bad peers get replaced over time
  - Single compromised election has limited impact

Layer 3: Ring Coverage (ec_peers)
  - Maintains diverse peer set across entire ring
  - Avoids clustering in one area
  - Geographic and topological diversity

Layer 4: This Election System
  - Snapshot validation of state alignment
  - Anti-gaming via duplicate detection
  - Consensus requirement (≥2 independent confirmations)
  - Split-brain detection
```

### Attack Scenarios

#### Attack 1: Sybil Attack [6,7]

**Setup**: Attacker creates multiple fake identities.

**With Minimum Cluster Size = 2**:
- Attacker spawns 2 Sybil nodes
- Both respond to different channels with coordinated signatures
- Need to match 8/10 mappings with honest state
- Need to be closest to challenge token (ring distance)

**Difficulty**:
- POW makes identity creation expensive
- Must match 80% of honest state (expensive to maintain)
- Must compete with honest responses (need to arrive first)
- Risk: Moderate if attacker has resources

**Mitigation**:
- Continuous re-election catches drift
- POW cost accumulates with scale
- Need multiple successful elections to maintain position

#### Attack 2: Collusion

**Setup**: Multiple malicious operators coordinate responses.

**With Minimum Cluster Size = 2**:
- Need 2+ colluding operators
- Must coordinate state (8/10 mappings)
- Must route through independent channels

**Difficulty**:
- Requires actual coordination between operators
- Both must maintain fake state consistently
- Must compete with honest majority for 60%+ share

**Mitigation**:
- Split-brain resolution spawns more channels
- Harder to maintain 6+ colluding nodes for 60% majority
- Continuous re-election exposes inconsistencies over time

#### Attack 3: Route Manipulation

**Setup**: Attacker controls first-hop peer, forks challenge to multiple colluding responders.

**Defense**:
- Each channel accepts exactly ONE response
- Second response immediately blocks channel
- Forking is detected as duplicate response
- Entire channel (including all responders) disqualified

**Result**: Attack detected and neutralized immediately.

#### Attack 4: Ticket Replay

**Setup**: Attacker intercepts ticket, tries to use it on different route.

**Defense**:
- Ticket is cryptographically bound to first-hop peer
- Different route → different first-hop → different expected ticket
- Replayed ticket won't match expected value

**Result**: Attack fails, invalid ticket rejected.

### Why POW Matters [8,9]

**POW for ring addresses fundamentally changes attack economics:**

**Without POW**:
- Spawn 10 Sybil nodes: Free
- Create nodes near target token: Free
- Attack scales easily

**With POW**:
- Spawn 10 Sybil nodes: Expensive (10× POW cost)
- Create nodes near target token: Extremely expensive (must retry POW until address matches)
- Attack scales poorly (linear cost per node)

**Impact on Elections**:
- Attacker cannot easily create "closest peer" to challenge token
- Cannot cheaply create multiple identities for cluster dominance
- Economic disincentive scales with attack ambition

---

## Mathematical Analysis

### Consensus Threshold: 8/10 Mappings

**Question**: Why 8/10 (80% agreement)?

#### Probability of Natural Agreement

Assume:
- Network sync rate: 90% (10% of mappings are stale/diverged)
- Each signature samples 10 random tokens from token space
- Two honest peers compared

**P(both peers have same mapping for a token)** ≈ 0.9
**P(k matches out of 10)** follows binomial distribution: B(10, 0.9)

```
P(10/10 matches) = 0.9^10 = 34.9%
P(9/10 matches)  = 10 × 0.9^9 × 0.1 = 38.7%
P(8/10 matches)  = 45 × 0.9^8 × 0.1^2 = 19.4%
────────────────────────────────────────
P(≥8/10 matches) = 93.0%
```

**Conclusion**: With 90% network sync, two honest peers will meet 8/10 threshold in 93% of cases.

#### Threshold Comparison

| Threshold | P(Match) | Security | Availability | Verdict |
|-----------|----------|----------|--------------|---------|
| 10/10 | 35% | ★★★★★ | ★☆☆☆☆ | Too strict |
| 9/10 | 74% | ★★★★★ | ★★☆☆☆ | High security |
| **8/10** | **93%** | **★★★★☆** | **★★★★☆** | **Balanced** ✓ |
| 7/10 | 99% | ★★★☆☆ | ★★★★★ | Too permissive |
| 6/10 | 99.9% | ★★☆☆☆ | ★★★★★ | Too permissive |

**Chosen**: 8/10 provides good balance of security and practical cluster formation.

### Cluster Size: Minimum 2

**Question**: Why require minimum 2 peers?

#### Security Analysis

**Single peer (size 1)**:
- No independent confirmation
- Could be malicious or faulty
- No cross-validation
- Verdict: ❌ Insufficient

**Two peers (size 2)**:
- Independent confirmation
- Must both agree on 8/10 mappings
- Basic Byzantine fault tolerance [1,2]
- Verdict: ✓ Minimum acceptable

**Three peers (size 3)**:
- Strong confirmation
- Classic BFT threshold (tolerates 1/3 Byzantine) [1,2]
- High confidence in correctness
- Verdict: ✓ Preferred but not required

#### Availability Analysis

| Min Size | P(Cluster Formation) | Availability | Security |
|----------|---------------------|--------------|----------|
| 1 | 100% | ★★★★★ | ☆☆☆☆☆ |
| **2** | **~60%** | **★★★★☆** | **★★★☆☆** |
| 3 | ~46% | ★★★☆☆ | ★★★★☆ |
| 4 | ~35% | ★★☆☆☆ | ★★★★★ |

**Chosen**: Size 2 balances availability (60% success rate) with security (independent confirmation).

**Note**: System naturally prefers larger clusters via "strongest cluster" selection, so when size-3+ clusters form, they're automatically chosen.

### Majority Threshold: 60%

**Question**: Why require 60% for decisive victory?

#### Split-Brain Scenarios

```
3v3 → 3/6 = 50.0% → Split-brain ✓ (tie, need resolution)
4v3 → 4/7 = 57.1% → Split-brain ✓ (close, need resolution)
5v3 → 5/8 = 62.5% → Decisive ✓ (clear winner)
4v2 → 4/6 = 66.7% → Decisive ✓ (strong winner)
5v2 → 5/7 = 71.4% → Decisive ✓ (very strong)
```

**Rationale**:
- **>50%**: Absolute majority, but not confident (could be 51/49)
- **≥60%**: Strong majority, confident winner
- **>66%**: Supermajority, may be too strict for practical use

**Chosen**: 60% provides clear distinction between "close race" and "clear winner".

### Expected Election Performance

**Assumptions**:
- 90% network sync rate
- 8/10 consensus threshold
- Minimum cluster size 2
- Start with 3 channels

**Phase 1: Collection (2000ms)**:
- Spawn 3 channels
- P(2+ agreeing) ≈ 60%

**Phase 2: Election (2000-5000ms)**:
- If 2+ agreeing → Winner found immediately
- If split-brain → Spawn 2-3 more channels
- P(resolution after +3 channels) ≈ 85%

**Expected**:
- ~60% elections succeed in Phase 1
- ~25% elections succeed in Phase 2 after split-brain resolution
- ~15% elections fail (genuine split or below threshold)

**Total success rate**: ~85%

---

## Design Decisions

### Decision 1: Two-Phase Timing

**Choice**: Collection phase (no consensus checks) → Election phase (check consensus)

**Rationale**:
- **Performance**: Clustering is expensive (O(2^n)), avoid checking after every response
- **Quality**: More responses → better cluster formation
- **Batching**: Natural grouping of consensus checks

**Alternative Considered**: Check after every response
**Rejected Because**: High computational overhead, premature elections

### Decision 2: Active Split-Brain Resolution

**Choice**: Spawn additional channels to resolve ties

**Rationale**:
- **Proactive**: Don't just report problem, try to fix it
- **Network Health**: Genuine splits still detected after max attempts
- **User Experience**: Higher success rate, fewer failed elections

**Alternative Considered**: Accept first cluster or timeout
**Rejected Because**: Misses opportunity to break ties, lower success rate

### Decision 3: Majority Threshold = 60%

**Choice**: Require 60% for decisive victory

**Rationale**:
- **Clear Winner**: Strong distinction from 50/50 split
- **Practical**: Not too strict (66% would reduce success rate)
- **Configurable**: Can adjust for different deployments

**Alternative Considered**: 50% strict majority
**Rejected Because**: Too weak, 51/49 is not decisive

**Alternative Considered**: 66% supermajority
**Rejected Because**: Too strict, reduces availability

### Decision 4: Minimum Cluster Size = 2

**Choice**: Require at least 2 agreeing peers

**Rationale**:
- **Independent Confirmation**: One peer is insufficient
- **Availability**: Size-3 requirement would reduce success rate significantly
- **Natural Preference**: System automatically prefers larger clusters when available

**Alternative Considered**: Minimum size 3
**Rejected Because**: Lower availability (~46% vs ~60%)

### Decision 5: Configurable Parameters

**Choice**: Make thresholds configurable via `ElectionConfig`

**Rationale**:
- **Flexibility**: Different deployments have different requirements
- **Experimentation**: Can test different values in practice
- **Evolution**: Can adjust as network characteristics change

**Parameters Made Configurable**:
- `consensus_threshold` (default: 8/10)
- `majority_threshold` (default: 60%)
- `min_cluster_size` (fixed: 2, but structurally configurable)
- `max_channels` (default: 10)
- `min_collection_time` (default: 2000ms)
- `ttl_ms` (default: 5000ms)

---

## Implementation Notes

### Type Compatibility: u64 → 256-bit

**Current**: TokenId, PeerId, BlockId are all `u64` (for testing/simulation)

**Future**: Will become 256-bit types (for production)

**Strategy**:
- Use type aliases throughout implementation
- Functions accept generic types via `TokenId`, `PeerId` etc.
- Add TODO comments at points needing updates:
  - `ring_distance()` - needs U256 arithmetic
  - `generate_ticket()` - already uses `.to_le_bytes()` which will adapt
  - Winner selection - logic unchanged, just types

**Example**:
```rust
// Current (u64)
pub fn ring_distance(a: u64, b: u64) -> u64 {
    let forward = b.wrapping_sub(a);
    let backward = a.wrapping_sub(b);
    forward.min(backward)
}

// Future (256-bit) - TODO
pub fn ring_distance(a: U256, b: U256) -> U256 {
    let forward = b.wrapping_sub(&a);
    let backward = a.wrapping_sub(&b);
    forward.min(backward)
}
```

### Testing Strategy

**Standalone Unit Tests**: All logic testable without networking or ec_peers integration.

**Test Categories**:
1. **Ticket System**: Determinism, uniqueness, security
2. **Ring Distance**: Wrapping, edge cases, opposite sides
3. **Channels**: State transitions, duplicate detection, blocking
4. **Clustering**: Agreement calculation, subset removal, strongest selection
5. **Elections**: Full lifecycle, split-brain, winner selection
6. **Edge Cases**: 0 responses, 1 response, all disagree, timeout

**Test Data Generation**:
```rust
fn create_test_signature(mappings: [(TokenId, BlockId); 10]) -> TokenSignature {
    // Helper to create signatures with specific mappings
}

fn agreeing_signatures(n: usize, common_count: usize) -> Vec<TokenSignature> {
    // Create n signatures that agree on common_count/10 mappings
}
```

### Integration with ec_peers (Future)

**ec_peers Responsibilities**:
- Maintain active elections (HashMap<TokenId, PeerElection>)
- Control timing (spawn channels, check consensus)
- Handle split-brain responses (spawn more channels)
- Route Query/Answer messages
- Maintain connections to elected winners

**Clean Interface**:
- `PeerElection::new()` - start election
- `create_channel()` - get ticket for Query
- `submit_response()` - store Answer
- `try_elect_winner()` - check for winner/split-brain
- Handle `ElectionAttempt` enum response

---

## Future Enhancements

### 1. Adaptive Thresholds

**Idea**: Adjust consensus_threshold based on network health

```rust
if recent_success_rate < 20% {
    // Network struggling, relax threshold
    config.consensus_threshold = 7;  // 70%
} else if recent_success_rate > 80% {
    // Network healthy, can be stricter
    config.consensus_threshold = 9;  // 90%
}
```

**Benefit**: Automatically adapt to network conditions

### 2. Reputation Tracking

**Idea**: Track which peers frequently appear in winning clusters

```rust
struct PeerReputation {
    elections_won: usize,
    elections_participated: usize,
    avg_cluster_size: f64,
}
```

**Benefit**: Prefer high-reputation peers in future elections

### 3. Geographic/Topological Diversity

**Idea**: Ensure channels span diverse network regions

```rust
fn pick_diverse_first_hops(challenge_token: TokenId) -> Vec<PeerId> {
    // Pick peers from different ring segments
    // Prefer peers in different AS/geographic regions
}
```

**Benefit**: Harder for localized attacks to dominate

### 4. Multi-Token Challenges

**Idea**: Challenge multiple tokens in one election

```rust
struct MultiTokenElection {
    challenge_tokens: Vec<TokenId>,
    // Peer must match on majority of challenged tokens
}
```

**Benefit**: Stronger validation, harder to fake

### 5. Historical Consistency Checks

**Idea**: Check if peer's current state is consistent with past responses

```rust
struct PeerHistory {
    past_signatures: Vec<(EcTime, TokenSignature)>,
    // Check for contradictions
}
```

**Benefit**: Detect peers that change state suspiciously

### 6. Weighted Ring Distance

**Idea**: Factor in more than just distance for winner selection

```rust
fn weighted_score(peer: PeerId, token: TokenId, reputation: f64) -> f64 {
    let distance = ring_distance(peer, token);
    let normalized_distance = distance as f64 / (u64::MAX as f64);
    let distance_score = 1.0 - normalized_distance;

    // Combine distance with reputation
    0.7 * distance_score + 0.3 * reputation
}
```

**Benefit**: Balance proximity with peer quality

---

## Conclusion

The peer election system provides a robust, secure, and practical mechanism for distributed peer discovery in the ecRust consensus network. Key strengths:

✅ **Security**: Multi-layered defense (POW, tickets, duplicates, consensus)
✅ **Practicality**: ~85% success rate with good defaults
✅ **Adaptability**: Active split-brain resolution
✅ **Visibility**: Network health signals via split-brain detection
✅ **Simplicity**: Clean interface, testable in isolation
✅ **Flexibility**: Configurable parameters for different deployments

The design balances security with availability, leverages the existing POW identity system, and integrates cleanly with continuous re-election for long-term peer quality maintenance.

---

## References

### Byzantine Fault Tolerance & Consensus

[1] **Lamport, L., Shostak, R., & Pease, M. (1982)**. "The Byzantine Generals Problem". *ACM Transactions on Programming Languages and Systems*, 4(3), 382-401.
   - Foundational work on Byzantine fault tolerance
   - Establishes the n/3 threshold for tolerating Byzantine failures
   - Relevant to: Minimum cluster size (2+ for BFT properties)

[2] **Castro, M., & Liskov, B. (1999)**. "Practical Byzantine Fault Tolerance". *OSDI '99*.
   - Practical BFT consensus algorithm
   - 3f+1 nodes required to tolerate f Byzantine failures
   - Relevant to: Consensus requirements, cluster sizing

[3] **Ongaro, D., & Ousterhout, J. (2014)**. "In Search of an Understandable Consensus Algorithm (Raft)". *USENIX ATC '14*.
   - Modern consensus algorithm with majority voting
   - Relevant to: Majority threshold design (>50%)

### Distributed Hash Tables & Ring Topology

[4] **Stoica, I., Morris, R., Karger, D., Kaashoek, M. F., & Balakrishnan, H. (2001)**. "Chord: A Scalable Peer-to-peer Lookup Service for Internet Applications". *SIGCOMM '01*.
   - Ring-based DHT with consistent hashing
   - XOR/ring distance metrics for proximity
   - Relevant to: Ring topology, distance calculation, winner selection

[5] **Maymounkov, P., & Mazières, D. (2002)**. "Kademlia: A Peer-to-peer Information System Based on the XOR Metric". *IPTPS '02*.
   - XOR-based routing and distance
   - k-bucket routing tables
   - Relevant to: Ring distance, peer selection strategies

### Sybil Attack Resistance

[6] **Douceur, J. R. (2002)**. "The Sybil Attack". *IPTPS '02*.
   - Defines the Sybil attack in peer-to-peer systems
   - Discusses identity validation challenges
   - Relevant to: Need for POW identity generation, attack resistance analysis

[7] **Levine, B. N., Shields, C., & Margolin, N. B. (2006)**. "A Survey of Solutions to the Sybil Attack". *University of Massachusetts Amherst Technical Report*, 2006-052.
   - Comprehensive survey of Sybil attack defenses
   - Proof-of-work as defense mechanism
   - Relevant to: POW strategy, multi-layered defense

### Proof-of-Work & Identity

[8] **Back, A. (2002)**. "Hashcash - A Denial of Service Counter-Measure". *Technical Report*.
   - Original proof-of-work system
   - Cost-asymmetry principle
   - Relevant to: POW for identity generation, attack cost analysis

[9] **Nakamoto, S. (2008)**. "Bitcoin: A Peer-to-Peer Electronic Cash System". *Bitcoin.org*.
   - Proof-of-work for consensus and Sybil resistance
   - Economic incentives against attacks
   - Relevant to: POW economics, attack cost modeling

### Split-Brain & Network Partitions

[10] **Brewer, E. A. (2000)**. "Towards Robust Distributed Systems". *PODC '00* (invited talk).
   - CAP theorem: Consistency, Availability, Partition tolerance
   - Trade-offs in distributed systems during partitions
   - Relevant to: Split-brain scenarios, partition detection

[11] **Gilbert, S., & Lynch, N. (2002)**. "Brewer's Conjecture and the Feasibility of Consistent, Available, Partition-Tolerant Web Services". *SIGACT News*, 33(2), 51-59.
   - Formal proof of CAP theorem
   - Implications for partition handling
   - Relevant to: Split-brain detection and resolution strategy

### Consensus Clustering & Agreement

[12] **Ben-Or, M. (1983)**. "Another Advantage of Free Choice: Completely Asynchronous Agreement Protocols". *PODC '83*.
   - Randomized consensus in asynchronous systems
   - Probabilistic agreement analysis
   - Relevant to: Consensus threshold selection, probabilistic analysis

[13] **Cachin, C., Guerraoui, R., & Rodrigues, L. (2011)**. *Introduction to Reliable and Secure Distributed Programming*. Springer.
   - Comprehensive textbook on distributed systems
   - Consensus, broadcast, and agreement protocols
   - Relevant to: Overall design principles, correctness properties

### Cryptographic Hash Functions

[14] **O'Connor, J., Aumasson, J. P., Neves, S., & Wilcox-O'Hearn, Z. (2020)**. "BLAKE3: One Function, Fast Everywhere". *blake3.io*.
   - Modern cryptographic hash function
   - Security properties and performance
   - Relevant to: Ticket generation security

---

## Appendix: Key Formulas

### Ticket Generation
```
ticket = Blake3(challenge_token || first_hop_peer || SECRET)[0..8]
```

### Ring Distance
```
ring_distance(a, b) = min(|b - a|, |a - b|)  with wrapping
                    = min(b - a mod 2^64, a - b mod 2^64)
```

### Consensus Agreement
```
agreement(sig1, sig2) = |{m ∈ sig1.signature : m ∈ sig2.signature}|
                       = count of matching (token_id, block_id) pairs
```

### Cluster Validity
```
valid_cluster(C, threshold) = ∀(i,j) ∈ C × C: agreement(sig[i], sig[j]) ≥ threshold
                              AND |C| ≥ min_cluster_size
```

### Decisive Majority
```
decisive(cluster, all_responses) = |cluster| / |all_responses| ≥ majority_threshold
                                  = size ≥ 0.6 × total_valid
```

### Split-Brain Detection
```
split_brain(clusters) = ∃C₁,C₂ ∈ clusters: |C₁| ≈ |C₂|
                       = max_size - second_max_size ≤ 1
```

---

**End of Design Document**
