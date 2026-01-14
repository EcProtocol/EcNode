# Probabilistic Proof of Aligned Storage via Suffix-Walk Overlap

**Lars Szuwalski**  
https://github.com/EcProtocol/  
© 2026

---

## Abstract

We present a lightweight, probabilistic mechanism for certifying aligned storage between participants in decentralized systems. Participants respond to randomized suffix queries by performing forward scans over their locally stored data and returning short response sequences. A verifier observes only overlap statistics between responses.

We prove that the overlap probability is bounded above by the minimum storage density among participants, ensuring that high observed overlap implies all parties store a large fraction of the underlying dataset. This bound holds regardless of adversarial strategy: a single well-provisioned participant cannot "carry" an under-provisioned partner.

The protocol's walk dynamics introduce pointer desynchronization when parties record different elements, causing naïve Binomial models to overestimate tail probabilities by 2–3×. We establish rigorous security bounds through systematic simulation of Poisson-walk dynamics. For example, observing 10 or more matches out of 12 recorded elements rules out minimum density below 0.6 at the 2.4% significance level. Independent repetition amplifies confidence exponentially.

The mechanism requires no cryptographic commitments per element, no global verifier, and reveals only O(m) randomly-selected elements per interaction. We analyze several natural adversarial strategies—fabrication, selective answering, collusion, Sybil attacks—and show that none can increase overlap probability beyond what storage density allows. From a mechanism-design perspective, repeated suffix-walk interactions induce a game where aligned storage is the dominant strategy, enabling emergent consensus without central coordination.

The protocol serves as a foundation for proof-of-aligned-storage in distributed systems and provides consensus weight based on demonstrated storage rather than computational power or stake.

Throughout this paper, "proof" refers to statistical evidence under a well-validated probabilistic model, not a cryptographic zero-knowledge proof.

---

## 1. Introduction

Decentralized systems frequently require participants to demonstrate possession of large, shared datasets. Blockchain nodes must store transaction history; distributed storage networks must verify that providers retain user data; peer-to-peer systems must distinguish well-provisioned nodes from free-riders. The challenge is to verify storage without trusting participants, without centralized audits, and without excessive overhead.

Classical approaches rely on cryptographic proofs. Proof of Retrievability (PoR) [10, 11] allows a verifier to confirm that a prover can produce a specific file, using challenges tied to preprocessed authenticators. Proof of Replication [12] extends this to verify physically independent copies. These schemes provide strong guarantees but require per-dataset setup, bind verification to specific block identifiers, and assume a client-server relationship between verifier and prover.

In peer-to-peer settings, these assumptions are often unsuitable. Participants interact continuously with many peers; verification must be lightweight and symmetric; and the relevant question is not "can you retrieve block X?" but rather "do we store the same data?"

We propose a different approach: **probabilistic proof of aligned storage** through randomized suffix queries and overlap statistics. The mechanism is simple. Participants respond to random suffix queries by scanning their stored data and returning the first matching element. A verifier—or peer—observes how often responses agree. High agreement implies that all participants must store a large fraction of the shared dataset.

### 1.1 Contributions

This paper makes the following contributions:

**A lightweight verification primitive.** We define a suffix-walk protocol requiring no cryptographic commitments, no per-element preprocessing, and no distinguished verifier. Verification consists entirely of comparing overlap statistics from randomized queries.

**Analytical foundation.** We show that suffix hits in a large ordered universe are well-approximated by a Poisson process (Lemma 1), derive the overlap probability for aligned parties (Lemma 3), and prove the key structural result: the overlap probability is bounded above by the minimum storage density among participants (Theorem 1). This min-density bound is the central theoretical contribution—it is unconditional, independent of walk length or adversarial strategy, and implies that high observed overlap cannot be achieved unless *all* parties store a large fraction of the data. A single well-provisioned participant cannot "carry" an under-provisioned partner, regardless of how the protocol parameters are chosen.

**Simulation-based security bounds.** When parties record different elements during a walk, their scan pointers desynchronize, introducing correlations that resist closed-form analysis. We show that the naïve Binomial model overestimates tail probabilities by a factor of 2–3× (Section 4.2). Through systematic simulation of the Poisson-walk dynamics, we establish conservative bounds on false-positive probabilities: for example, observing 10 or more matches out of 12 recorded elements rules out min(q_A, q_B) ≤ 0.6 at the 2.4% significance level (Section 4.3).

**Amplification mechanisms.** We demonstrate two orthogonal methods for strengthening guarantees: (1) independent repetition, which multiplies evidence exponentially (r batches reduce false-positive probability to α^r), and (2) multi-party verification, where requiring triple overlap among three parties is substantially more stringent than pairwise overlap (Section 4.4).

**Mechanism-design interpretation.** We frame the protocol as a primitive for incentive design in decentralized systems. Repeated suffix-walk interactions induce a game where storing and aligning a large fraction of the shared dataset is the only stable strategy. Poorly-provisioned or misaligned nodes fail verification checks, lose connectivity, and are gradually excluded. Consensus on shared state emerges from local interactions without global coordination (Section 5).

### 1.2 Why Not Existing Approaches?

Several related techniques might appear applicable but differ in important ways.

**Set similarity sketches** (MinHash, HyperLogLog, Bottom-k sampling) estimate similarity or cardinality of sets but assume cooperative parties and do not address adversarial settings. A malicious participant can construct a small set whose sketch collides with a target—sketches are not proofs.

**Private Set Intersection** protocols compute intersections without revealing non-common elements but are designed for one-shot computation between semi-honest or malicious parties, not continuous peer-to-peer verification. They also reveal the intersection itself, whereas our protocol reveals only a random sample.

**Proof of Retrievability** and related schemes provide strong cryptographic guarantees but require setup (computing authenticators), assume a distinguished verifier, and verify possession of *specific* blocks rather than *alignment* with other participants.

**Data Availability Sampling** verifies that data is available somewhere in a network by sampling random pieces. It operates at the network layer and assumes a known data structure (e.g., erasure-coded blocks). Our protocol operates at the peer layer and certifies alignment between participants whose datasets are not globally specified.

The suffix-walk protocol fills a gap: it provides statistical verification of aligned storage in a continuous, symmetric, peer-to-peer setting, using only the data itself—no commitments, no setup, no trusted parties.

### 1.3 Threat Model

We consider a competitive environment where participants may attempt to:

- **Fabricate responses** by guessing elements they do not store
- **Selectively answer** queries that happen to hit their sparse storage
- **Collude** with other under-provisioned participants to simulate alignment

The protocol does not assume an honest majority. Instead, it ensures that *whatever coalition dominates* must actually store the data. Fabrication is detected through inconsistent overlap with honest peers. Selective answering fails because queries are unpredictable and the "+1" rule forces progression through the dataset. Collusion among under-provisioned parties cannot increase their overlap probability beyond what their storage densities allow (Theorem 1).

We do not claim security against computationally unbounded adversaries who can invert hash functions or predict random suffixes. The protocol provides statistical guarantees calibrated by the number of queries and the acceptance threshold.

### 1.4 Limitations

The protocol reveals a small random sample of stored elements—the minimum disclosure necessary for verification. Applications requiring zero-knowledge guarantees must employ cryptographic alternatives.

The simulation-based bounds in Section 4 are empirical rather than closed-form. While we validate the Poisson-walk model extensively, the bounds inherit the limitations of Monte Carlo estimation. For applications requiring formal proofs of specific tail probabilities, additional analytical work or verified computation would be needed.

### 1.5 Paper Organization

Section 2 defines the protocol formally. Section 3 develops the analytical model: Poisson approximation, overlap probability derivation, and the min-density bound. Section 4 presents simulation methodology and results, including conservative bounds for minimum density and amplification via repetition. Section 5 discusses mechanism properties: information disclosure, adversarial strategies, game-theoretic interpretation, and emergent network behavior. Section 6 surveys related work. Section 7 concludes.

---

## 2. Protocol Definition

This section formally defines the suffix-walk protocol. We establish notation, specify the response generation and verification algorithms, and discuss parameter selection.

### 2.1 Setting and Notation

**Universe.** Let U be a totally ordered universe of N elements. In practice, U consists of cryptographic hashes, transaction identifiers, or other uniformly distributed values that can be sorted. We write u_1 < u_2 < ... < u_N for the elements in sorted order.

**Storage.** Each participant P_i maintains a subset A_i ⊆ U. We model storage as independent Bernoulli sampling: each element u ∈ U belongs to A_i with probability q_i, independently across elements and participants. The parameter q_i is the *storage density* of participant P_i.

**Suffixes.** For a b-bit suffix length, define suffix_b(u) as the last b bits of element u. For a target suffix s ∈ {0,1}^b, element u is a *suffix hit* if suffix_b(u) = s. Under uniform distribution of elements, each element is a suffix hit with probability p_b = 2^{-b}, independently.

**Notation summary.**

| Symbol | Meaning |
|--------|---------|
| U | Ordered universe of N elements |
| A_i | Subset stored by participant P_i |
| q_i | Storage density of P_i |
| b | Suffix length in bits |
| s | Target suffix value, s ∈ {0,1}^b |
| m | Number of recorded elements per walk |
| K | Overlap count between response sets |
| t | Acceptance threshold (require K ≥ t) |

### 2.2 Suffix-Walk Response Generation

A participant responds to a sequence of suffix queries by performing a forward scan over their stored elements.

**Input.**
- Stored subset A ⊆ U (implicitly defined by storage indicator S: U → {0,1})
- Suffix sequence σ = (s_1, s_2, ..., s_m), each s_j ∈ {0,1}^b
- Starting position p_0 ∈ {0, ..., N-1}

**Output.**
- Response sequence R = (r_1, r_2, ..., r_m) of recorded elements, or FAILURE

**Algorithm 1: SuffixWalk**

```
procedure SuffixWalk(A, σ, p_0):
    ptr ← p_0
    R ← empty list
    
    for j = 1 to m:
        found ← false
        for i = ptr to N-1:
            u ← U[i]
            if u ∈ A and suffix_b(u) = s_j:
                R.append(u)
                ptr ← i + 1          // advance past match to ensure forward progress
                found ← true
                break
        if not found:
            return FAILURE
    
    return R
```

**Properties.**

1. *Monotonicity.* The recorded elements are strictly increasing: r_1 < r_2 < ... < r_m.

2. *Determinism.* Given (A, σ, p_0), the response R is uniquely determined.

3. *Dependence on storage.* A participant can only record elements they actually store. Fabricating responses requires guessing elements in U that (a) have the correct suffix and (b) fall in the correct position range—an event with negligible probability for large U.

4. *Failure.* If the participant's stored subset is too sparse or the walk advances too far, they may exhaust U before completing m matches. Failure is observable and constitutes verification failure.

### 2.3 Query Generation

Queries may be generated by a verifier, by the peer being verified, or by a deterministic function of shared randomness.

**Algorithm 2: QueryGeneration**

```
procedure GenerateQueries(seed, m, b):
    rng ← InitializeRNG(seed)
    σ ← empty list
    
    for j = 1 to m:
        s_j ← rng.uniform({0,1}^b)
        σ.append(s_j)
    
    return σ
```

**Starting position.** For robustness against precomputation attacks, the starting position p_0 may also be derived from shared randomness:

```
procedure GenerateStartPosition(seed, N):
    rng ← InitializeRNG(seed)
    return rng.uniform({0, ..., N-1})
```

Randomizing the starting position ensures coverage across the entire universe and prevents adversaries from caching responses for a fixed region.

**Seed derivation.** In peer-to-peer settings, the seed may be derived from a recent block hash (in blockchain contexts), a commitment-reveal protocol between participants, or a verifiable random function (VRF) output. The choice depends on the application's trust assumptions and latency requirements.

### 2.4 Overlap Verification

Given responses from multiple participants, a verifier computes overlap statistics.

**Algorithm 3: PairwiseVerification**

```
procedure VerifyPairwise(R_A, R_B, t):
    K ← |R_A ∩ R_B|
    return K ≥ t
```

**Algorithm 4: TripleVerification**

```
procedure VerifyTriple(R_A, R_B, R_C, t):
    K ← |R_A ∩ R_B ∩ R_C|
    return K ≥ t
```

**Verification output.** The verifier observes only whether verification succeeded (K ≥ t) or failed (K < t), and optionally the overlap count K itself. The verifier does *not* need to know U, does not need to store any elements, and does not need to perform any computation beyond set intersection of the O(m)-sized response sets.

### 2.5 Complete Protocol

We now assemble the components into a complete verification protocol.

**Protocol: SuffixWalkVerification**

*Participants:* Prover P (to be verified), Verifier V (may be a peer)

*Public parameters:* Universe size N, suffix length b, walk length m, threshold t

1. **Query commitment.** V selects a random seed ρ and sends commitment c = H(ρ) to P.

2. **Prover preparation.** P acknowledges readiness.

3. **Query reveal.** V reveals ρ. Both parties compute:
   - σ ← GenerateQueries(ρ, m, b)
   - p_0 ← GenerateStartPosition(ρ, N)

4. **Response generation.** P computes R_P ← SuffixWalk(A_P, σ, p_0) and sends R_P to V.

5. **Verifier response.** If V is also a storage participant, V computes R_V ← SuffixWalk(A_V, σ, p_0).

6. **Verification.** V computes K = |R_P ∩ R_V| and accepts if K ≥ t.

**Symmetric variant.** In peer-to-peer settings, both participants act simultaneously as prover and verifier. Each computes their own response and verifies the other's. This symmetric execution is the natural mode for continuous peer assessment.

### 2.6 Batched and Repeated Verification

For stronger guarantees, the protocol may be repeated with independent queries.

**Definition (Batch).** A *batch* consists of one execution of SuffixWalkVerification with a fresh random seed. Batches are independent if their seeds are independent.

**Definition (Repeated verification).** A repeated verification with r batches accepts if and only if all r batches accept individually.

**Amplification property.** If a single batch has false-positive probability α (probability of accepting a low-density participant), then r independent batches have false-positive probability α^r.

### 2.7 Parameter Selection

The protocol's discrimination power depends on the choice of parameters.

**Suffix length b.**
- Determines suffix-hit density: expected hits per element = 2^{-b}
- Typical choice: b = 10 (≈ 0.1% of elements are hits for any given suffix)
- Trade-off: smaller b → more hits → faster walks but less randomness per query

**Walk length m.**
- Number of recorded elements per response
- Typical choice: m = 10–12
- Trade-off: larger m → stronger discrimination but higher failure probability for sparse storage

**Threshold t.**
- Minimum overlap count for acceptance
- Typical choice: t ∈ {0.7m, 0.8m} (e.g., 7–8 out of 10, or 9–10 out of 12)
- Trade-off: higher t → fewer false positives but more false negatives for high-density participants

**Universe size N.**
- Must be large enough that walks complete with high probability
- Constraint: N · 2^{-b} · q ≫ m for storage density q
- For b = 10, m = 10, q = 0.8: need N ≫ 12,800; N = 20,000 suffices with margin

**Number of batches r.**
- Provides exponential amplification
- Selected based on desired false-positive bound

**Table 1: Example parameter configurations**

| Configuration | b | m | t | Target guarantee |
|---------------|---|---|---|------------------|
| Light | 10 | 10 | 7 | Quick peer screening |
| Standard | 10 | 10 | 8 | General verification |
| Strong | 10 | 12 | 10 | High-confidence single-batch |
| Repeated | 10 | 10 | 8 | r = 3 batches for α < 10^{-3} |

### 2.8 Relation to Consensus Protocols

The suffix-walk verification primitive serves as a foundation for consensus in decentralized trustless systems. In the EC (Echo Consent) protocol [15], participants use suffix-walk overlap as the mechanism for:

1. **Peer assessment.** Nodes continuously evaluate potential peers by suffix-walk verification, preferentially connecting to those demonstrating high, aligned storage.

2. **Vote weighting.** Influence in consensus decisions may be weighted by demonstrated storage alignment, ensuring that consensus reflects the view of well-provisioned participants.

3. **Fork resolution.** When participants hold divergent state, suffix-walk verification reveals the divergence through reduced overlap. Nodes naturally gravitate toward the majority-aligned fork through repeated peer assessment.

4. **Sybil resistance.** Creating pseudonymous identities provides no advantage without access to sufficient storage. Each identity must independently pass suffix-walk verification to participate. Note that the suffix-walk protocol alone does not prevent multiple identities from sharing the same underlying storage; full Sybil resistance requires additional mechanisms (see Section 5.2).

The key insight is that suffix-walk verification converts *storage* into a verifiable, non-transferable resource. Unlike proof-of-work (which can be rented) or proof-of-stake (which can be delegated), aligned storage requires ongoing commitment of local resources and cannot be simulated without actually possessing the data.

Section 5 elaborates on the game-theoretic properties that make this primitive suitable for consensus applications.

---

## 3. Analytical Model

This section develops the probabilistic foundation for the suffix-walk protocol. We show that suffix hits are well-approximated by a Poisson process, derive the overlap probability for aligned parties, and explain why the naïve binomial model fails for multi-step walks.

### 3.1 Poisson Approximation for Suffix Hits

Let N elements be drawn independently and uniformly, then sorted to impose a total order. Each element is assigned an independent b-bit suffix, uniform over {0,1}^b. For a fixed suffix value s, define the hit indicator H_i = 𝟙{element i has suffix s}.

**Lemma 1 (Poisson limit for suffix hits).**
Let p_b := 2^{-b}. The point process of hit locations converges to a homogeneous Poisson process with rate p_b per index as N → ∞. For any window of L consecutive indices with L ≪ N:

$$\sum_{i=1}^{L} H_i \xrightarrow{d} \text{Poisson}(L \cdot p_b)$$

*Proof sketch.* This follows from the classical Poisson limit theorem for rare independent events. The H_i are i.i.d. Bernoulli(p_b) with p_b small, so counts in disjoint windows are asymptotically independent Poisson. ∎

**Lemma 2 (Renewal property under "+1" advancement).**
Under the Poisson approximation, after a party records a match and advances the scan position, the future suffix-hit process is independent of the past and identically distributed.

*Proof sketch.* Poisson processes have independent increments. Conditioning on the event time T of the last recorded hit, the post-T process is independent of pre-T history. ∎

**Handling repeated suffix values.** Repeated suffix queries (e.g., s_t = s_{t'}) do not invalidate the Poisson approximation. For uniformly random suffixes, different suffix values correspond to independent thinnings of the universe. Collisions among suffix queries occur with probability O(m²/2^b) for m queries, which is small for m ≪ 2^{b/2}. For b = 10, m = 5, the collision probability is about 1%.

### 3.2 Overlap Probability Under Alignment

Consider two parties A and B who independently retain each element with probabilities q_A and q_B respectively. When both parties' scan pointers are aligned at the same position, we can compute the probability that the next recorded element is common to both.

**Lemma 3 (Aligned overlap probability).**
Let suffix hits form a Poisson process, independently thinned by parties A and B with retention probabilities q_A, q_B ∈ (0,1]. Starting from aligned positions, the probability that the next visible hit is common to both parties is:

$$p_{AB} = \frac{q_A q_B}{q_A + q_B - q_A q_B}$$

*Proof.* Each suffix hit falls into one of three visible categories:
- Common (both retain): probability q_A q_B
- A-only: probability q_A(1 - q_B)
- B-only: probability (1 - q_A)q_B

By Poisson thinning, the first visible hit is drawn from the union of these categories. The total visible rate is q_A + q_B - q_A q_B, and the conditional probability of being common is:

$$p_{AB} = \frac{q_A q_B}{q_A + q_B - q_A q_B}$$

∎

**Extension to three parties.** For parties A, B, C with densities q_A, q_B, q_C:

$$p_{ABC} = \frac{q_A q_B q_C}{q_A + q_B + q_C - q_A q_B - q_A q_C - q_B q_C + q_A q_B q_C}$$

### 3.3 The Min-Density Bound

The following bound is the key property enabling storage verification.

**Theorem 1 (Min-density bound).**
For any q_A, q_B ∈ (0,1]:

$$p_{AB} \leq \min(q_A, q_B)$$

with equality if and only if max(q_A, q_B) = 1.

*Proof.* Without loss of generality, assume q_A ≤ q_B. Then:

$$p_{AB} = \frac{q_A q_B}{q_A + q_B - q_A q_B} = \frac{q_A q_B}{q_A(1-q_B) + q_B}$$

Since q_A(1-q_B) ≥ 0, the denominator satisfies q_A(1-q_B) + q_B ≥ q_B, hence:

$$p_{AB} \leq \frac{q_A q_B}{q_B} = q_A = \min(q_A, q_B)$$

Equality holds iff q_A(1-q_B) = 0, i.e., q_B = 1. ∎

**Corollary.** High observed overlap implies *all* participating parties must have high density. A single well-provisioned party cannot "carry" a poorly-provisioned partner.

### 3.4 Pointer Desynchronization and Departure from Binomial

The overlap probability p_{AB} applies when parties are *aligned*. However, over multiple steps, parties can become *desynchronized*: if A and B record different elements on step t, their scan pointers advance to different positions.

Note that the "+1" advancement rule itself does not cause desynchronization—it merely ensures forward progress so that walks terminate. When both parties record the *same* element (a match), they both advance past it and remain synchronized. Desynchronization occurs specifically when parties record *different* elements, which happens when one party stores an element the other does not.

This desynchronization introduces negative correlation between consecutive match indicators. After a mismatch, the "behind" party searches a region that partially overlaps with what the "ahead" party will search, slightly increasing the probability of a subsequent match. This correlation concentrates the overlap distribution toward its mean.

**Consequence.** The naïve model K ~ Binomial(m, p_{AB}) overestimates tail probabilities. For rigorous bounds on tail events, we turn to direct simulation of the walk dynamics.

---

## 4. Simulation-Based Security Bounds

Because the desynchronization dynamics make closed-form tail analysis intractable, we establish security guarantees through systematic simulation of the Poisson-walk model.

### 4.1 Methodology

**Poisson-walk simulation.** We simulate three independent Poisson processes representing:
- Common hits: rate λ_C ∝ q_A q_B
- A-only hits: rate λ_A ∝ q_A(1 - q_B)
- B-only hits: rate λ_B ∝ (1 - q_A)q_B

Both parties execute the "+1" walk rule: record the next visible hit, advance past it. We count the overlap K = |Rec_A ∩ Rec_B| over m recorded elements per party.

**Validation.** The Poisson-walk model closely matches discrete-universe simulation (N = 20,000, 64-bit elements, 10-bit suffixes) across all tested parameter ranges, with discrepancies consistent with Monte Carlo error. The aligned-step common-hit probability empirically matches the predicted p_{AB}.

**Conservatism of the model.** The Poisson-walk model is conservative for security analysis because it ignores finite-N effects that would cause sparse-storage adversaries to fail more often (by exhausting the universe before completing walks). In the continuous Poisson model, walks always complete; in reality, low-density participants face additional failure modes. Thus, bounds derived from the Poisson-walk model slightly favor the adversary.

### 4.2 Poisson-Walk vs Naïve Binomial

The following table compares tail probabilities for m = 12 recorded elements (60,000 trials per configuration):

| Configuration | p_thin | Model | P(K≥9) | P(K≥10) |
|--------------|--------|-------|--------|---------|
| q_A = q_B = 0.80 | 0.667 | Binomial | 0.393 | 0.181 |
| | | Poisson-walk | 0.152 | 0.082 |
| q_A = q_B = 0.90 | 0.818 | Binomial | 0.841 | 0.623 |
| | | Poisson-walk | 0.461 | 0.349 |
| q_A = q_B = 0.95 | 0.905 | Binomial | 0.978 | 0.901 |
| | | Poisson-walk | 0.716 | 0.634 |
| q_A = 0.80, q_B = 0.90 | 0.735 | Binomial | 0.601 | 0.345 |
| | | Poisson-walk | 0.256 | 0.161 |

**Observation.** The naïve Binomial model overestimates tail probabilities by a factor of 2–3×. This discrepancy grows with m and with threshold stringency.

### 4.3 Conservative Bounds for Minimum Density

A verifier's goal is to bound the minimum density among participants. We adopt a p-value approach:

> *If min(q_A, q_B) ≤ μ, what is the maximum probability of observing overlap K ≥ t?*

The worst case for the adversary is (q_A, q_B) = (μ, 1), maximizing p_{AB} subject to the constraint. We simulate this configuration directly (80,000 trials):

| min(q) ≤ μ | P(K≥9) | P(K≥10) | P(K≥11) | P(K=12) |
|------------|--------|---------|---------|---------|
| 0.50 | 0.011 | 0.005 | — | — |
| 0.60 | 0.048 | 0.024 | — | — |
| 0.70 | 0.154 | 0.095 | 0.046 | 0.014 |
| 0.80 | 0.366 | 0.271 | 0.168 | 0.067 |
| 0.90 | 0.679 | 0.596 | — | — |

**Interpretation.**
- Observing K ≥ 9 rules out min(q) ≤ 0.5 at the 1.1% level
- Observing K ≥ 10 rules out min(q) ≤ 0.6 at the 2.4% level
- Observing K ≥ 11 rules out min(q) ≤ 0.7 at the 4.6% level

**Model-conditional interpretation.** All reported significance levels are conditional on the Poisson-walk model, which we validate empirically against discrete-universe simulation (Section 4.1). They should be interpreted as conservative model-based p-values, not absolute guarantees. The conservatism arises because the Poisson-walk model, by ignoring finite-N boundary effects, slightly overestimates the adversary's success probability in the parameter ranges we consider.

For stronger guarantees ("near-certain that min(q) ≈ 1"), require either more queries or repeated independent batches.

### 4.4 Multi-Party Extension

For three parties, triple overlap K_{ABC} provides stronger discrimination. Key findings (m = 10 per party, 20,000 trials):

| Configuration | P(any pair ≥ 8) | P(triple ≥ 8) |
|--------------|-----------------|---------------|
| q_A = q_B = q_C = 0.80 | 0.36 | 0.03 |
| q_A = q_B = q_C = 0.90 | 0.78 | 0.24 |
| q_A = q_B = q_C = 0.99 | — | 0.91 |

Triple certificates are substantially stronger than pairwise certificates and sharply penalize any participant with moderate density.

### 4.5 Confidence Amplification via Repetition

Because each batch of suffix queries can be made independent (fresh random suffixes), repeating the protocol multiplies evidence:

- Single-batch false-positive probability: α
- r independent batches all passing: α^r

**Example.** If a single batch with threshold K ≥ 10 has false-positive probability 0.095 against min(q) ≤ 0.7, then:
- 2 batches: 0.009
- 3 batches: 0.0009
- 5 batches: < 10^{-5}

This provides exponential confidence amplification without revealing additional information beyond O(mr) elements.

---

## 5. Mechanism Properties

This section analyzes the suffix-walk protocol as a mechanism-design primitive. We characterize information disclosure, examine adversarial strategies and their detectability, develop the game-theoretic interpretation, and describe the emergent network behavior that arises from repeated local interactions.

**Scope.** The following discussion is interpretive and explores how suffix-walk verification can serve as a building block in larger consensus protocols. We do not claim a complete consensus protocol or formal safety proof. The game-theoretic and emergent-behavior analyses describe plausible dynamics under reasonable assumptions about participant incentives, not proven equilibrium properties.

### 5.1 Information Disclosure

The protocol requires participants to reveal information. We characterize this disclosure and argue it is minimal for the guarantees provided.

**What is revealed.** Each protocol execution reveals:
- The response set R of m recorded elements
- Whether the participant completed the walk (success) or failed

**Properties of disclosure.**

1. *Bounded size.* Exactly O(m) elements are revealed per execution, regardless of storage size or universe size.

2. *Random selection.* The revealed elements are determined by unpredictable suffix queries. A participant cannot choose which elements to reveal (beyond choosing not to participate).

3. *Non-adaptive.* The query sequence is fixed before the participant responds. There is no interactive probing that could extract additional information.

4. *Position-dependent.* Revealed elements come from a random region of the ordered universe (determined by the starting position). Repeated executions with independent seeds sample different regions.

**Disclosure is necessary.** Any protocol that verifies aligned storage must reveal *some* information about stored elements—otherwise, verification reduces to trusting self-reports. The suffix-walk protocol achieves verification with disclosure proportional to the desired confidence (more queries → more revealed elements → stronger guarantees), which is arguably optimal.

**Disclosure does not enable manipulation.** A concern might be that revealed elements help an adversary construct a sparse set that passes future verification. This is mitigated by query unpredictability (future suffixes and starting positions are unknown), the "+1" rule (even knowing some elements, the adversary cannot predict which will be recorded in future walks), and multi-party verification (the adversary would need to align with multiple honest parties simultaneously).

#### 5.1.1 Protocol Variants for Reduced Disclosure

The base protocol reveals O(m) stored elements per execution. Two variants offer different trade-offs:

**Hashed responses.** Participants respond with H(u) rather than u, where H is a collision-resistant hash function. Overlap is computed over hashes: K = |H(R_A) ∩ H(R_B)|. All analytical properties are preserved since suffix hits on H(u) remain approximately Poisson when H behaves as a random oracle. This variant reveals no information about actual element values, only their hashes. The trade-off is that a verifier cannot confirm responses correspond to legitimately stored elements—though the min-density bound still constrains adversarial success probability.

**Hash-query with preimage reveal.** Suffix queries target suffix_b(H(u)), but participants return the preimage u. The verifier confirms correctness by computing H(u) and checking the suffix. This variant provides explicit verifiability and ensures uniform suffix distribution regardless of the underlying element structure, while maintaining the same disclosure as the base protocol. It is particularly useful when elements are not uniformly distributed (e.g., structured transaction data) but hashes are.

| Variant | Query On | Return | Verifiable | Element Privacy |
|---------|----------|--------|------------|-----------------|
| Base protocol | u | u | ✓ (implicit) | ✗ |
| Hashed responses | H(u) | H(u) | ✗ | ✓ |
| Preimage-reveal | H(u) | u | ✓ (explicit) | ✗ |

The choice among variants depends on application requirements: hashed responses prioritize privacy; preimage-reveal prioritizes verifiability and handles non-uniform element distributions. The core analytical results—Theorem 1 (min-density bound), the Poisson approximation, and simulation-based tail bounds—apply to all three variants.

### 5.2 Adversarial Strategies and Detection

We consider several natural strategies an adversary might employ to pass verification without maintaining full storage. For each, we show that the strategy cannot increase overlap probability beyond what the adversary's actual storage density allows. We do not claim this analysis is exhaustive; adaptive adversaries with auxiliary information or control over protocol timing may have additional options not considered here.

**Fabrication.** The adversary guesses elements they do not store, hoping they happen to be valid. For universe U with N elements and suffix length b, the probability a random guess is valid is approximately 2^{-b} · (range size / N). For m independent fabrications to succeed, the probability is approximately 2^{-bm}. With b = 10, m = 10, this is approximately 2^{-100}, which is negligible.

**Selective answering.** The adversary stores only a sparse subset but answers only queries that happen to hit their storage. The "+1" rule forces progression, so the adversary cannot repeatedly answer queries in a favorable region. Sparse storage leads to walk failures. Even if walks complete, overlap with well-provisioned peers will be low (Theorem 1).

**Precomputation and caching.** The adversary precomputes responses for anticipated queries. Query unpredictability defeats this: with b-bit suffixes, there are 2^b possible suffix values. Starting position randomization adds another dimension. Caching all combinations requires storage proportional to the full dataset.

**Collusion.** Multiple under-provisioned adversaries coordinate to simulate alignment. Collusion does not circumvent Theorem 1: if adversaries A and B have storage densities q_A and q_B, their overlap probability satisfies p_{AB} ≤ min(q_A, q_B) regardless of coordination. There is no "free" collusion that improves overlap without increasing storage.

**Sybil attacks.** The adversary creates many pseudonymous identities. Each identity must independently pass suffix-walk verification, so identities without access to sufficient storage are rejected. However, the protocol does not prevent an adversary from using the *same* storage to back multiple identities. An adversary with storage density q could create k identities that all share this storage; each would pass verification with the same overlap probability.

This means the suffix-walk protocol alone provides *per-identity* storage verification, not *unique* storage verification. The min-density bound guarantees that any accepted identity has access to high-density storage, but does not guarantee that storage is exclusive to that identity.

Achieving full Sybil resistance requires additional mechanisms at other protocol layers, such as:
- Binding storage commitments to identity (so reuse is detectable)
- Proof-of-space constructions that enforce uniqueness
- Economic costs for identity creation
- Social or reputational identity systems

This limitation does not undermine the core contribution: the min-density bound remains the fundamental constraint on adversarial overlap probability, and any coalition's *total* verified storage still reflects *actual* storage somewhere in the system.

### 5.3 Game-Theoretic Interpretation

The protocol induces a repeated game among participants with the following structure.

**Players.** Participants P_1, ..., P_n with storage capacities c_1, ..., c_n.

**Actions.** Each player i chooses storage density q_i ∈ [0, c_i] and an alignment target (which version of shared state to align with).

**Signals.** Players observe overlap statistics K_{ij} from pairwise suffix-walk verifications, whether peers pass or fail verification thresholds, and network connectivity.

**Payoffs.** Players derive utility from continued connectivity, participation in consensus decisions (weighted by demonstrated storage), and access to services contingent on verification status.

**Incentive analysis.** Higher storage density leads to higher overlap with well-provisioned peers, higher probability of passing verification thresholds, and greater connectivity and influence. Alignment with the majority leads to overlap with more peers and membership in the dominant cluster. Under-provisioning or misalignment leads to verification failures and reduced connectivity.

**Equilibrium.** Under reasonable assumptions about payoff structure, the strategy profile where all players store at capacity and align with each other constitutes a Nash equilibrium. Unilateral deviation reduces expected overlap and payoff.

### 5.4 Emergent Network Behavior

From local suffix-walk interactions, global network structure emerges without central coordination.

**Cluster formation.** Consider a network where participants perform continuous pairwise verification and preferentially maintain connections to peers that pass verification. Initially, participants connect to random peers. Suffix-walk verification reveals overlap statistics for each connection. Participants retain high-overlap connections and drop low-overlap connections. Over time, clusters form among participants with aligned storage.

**Convergence to shared state.** When participants genuinely seek consensus, the following dynamic emerges: participants observe which cluster has more members and stronger internal overlap; participants in minority clusters update their storage to align with the majority; the network converges toward a single dominant cluster storing shared state. This process requires no global view—each participant makes local decisions based on overlap statistics with their immediate peers.

**Fork resolution.** When the network holds genuinely divergent state, suffix-walk verification provides a natural fork-resolution mechanism. Participants on different forks have low cross-fork overlap. Each participant can observe which fork has more support. Participants preferentially connect to the dominant fork, and the minority fork loses connectivity. Unlike proof-of-work or proof-of-stake, this mechanism resolves forks based on *demonstrated storage*.

### 5.5 Application to Consensus Protocols

The suffix-walk mechanism provides a foundation for consensus in decentralized trustless systems. In the EC (Echo Consent) protocol [15], voting weight derives from demonstrated aligned storage rather than computational power or staked capital.

**Properties of storage-based consensus weight:**
- *Non-transferable.* Unlike stake, storage cannot be delegated. Each participant must maintain their own data.
- *Non-rentable.* Unlike hashpower, storage for suffix-walk verification cannot be temporarily rented—the verification is continuous and unpredictable.
- *Verifiable.* Unlike reputation, storage is objectively measurable through protocol interaction.

**Consensus properties.** A consensus protocol built on suffix-walk verification would plausibly inherit the following properties, though formal proofs would require specifying the complete protocol: liveness (as long as some threshold of storage capacity is honest and online), safety (accepting a fraudulent participant requires overlap statistics to deviate significantly from expected values), consistency (participants who pass mutual verification are storing aligned data), and partial Sybil resistance (each accepted identity must have access to sufficient storage, though additional mechanisms are needed to ensure storage uniqueness across identities).

---

## 6. Related Work

The suffix-walk overlap protocol draws on techniques from several research areas while serving a distinct purpose.

### Set Similarity Sketching and Cardinality Estimation

**Bottom-k and priority sampling** [1] provide unbiased estimators for set operations by retaining elements with smallest hash values. The Theta Sketch framework [2] generalizes these ideas with practical optimizations. Our suffix-walk mechanism differs in that it produces *ordered* samples via forward scans rather than global minimum selection, and overlap statistics serve as certificates rather than estimators.

**HyperLogLog** [3] estimates cardinality using the distribution of leading zeros in hashed values. Flajolet et al. explicitly employ Poissonization in their analysis—the same technique underlying our Lemma 1.

**Consistent weighted sampling** [5, 6] extends min-hash to weighted sets while preserving the property that sample agreement reflects similarity. Our protocol creates a different sampling dynamic—elements are drawn in scan order rather than by hash rank, and parties desynchronize when they record different elements, inducing the correlations analyzed in Section 3.4.

**Bias in sequential sampling.** Belbasi et al. [9] show that minimizer-based Jaccard estimators exhibit systematic bias due to dependencies between adjacent windows. Our observation that the naïve Binomial model overestimates tail probabilities reflects an analogous phenomenon.

### Private Set Intersection

Private Set Intersection (PSI) protocols [7, 8] allow parties to compute set intersections without revealing non-intersecting elements. Our protocol differs fundamentally: PSI computes the intersection privately, while suffix-walk certifies that a large intersection *exists* without computing it exactly. PSI reveals intersection elements to participants; suffix-walk reveals only a random sample. PSI is typically one-shot; suffix-walk operates continuously.

### Proof of Storage and Data Availability

**Proof of Retrievability (PoR)** [10, 11] allows a client to verify that a server can produce a stored file. These schemes bind proofs to *specific* data blocks and require setup.

**Proof of Replication** [12] extends PoR to verify physically independent copies.

Our protocol provides a complementary primitive: no per-element commitments, alignment rather than retrievability, peer-to-peer rather than client-server.

**Data Availability Sampling (DAS)** [13, 14] addresses whether data *exists* in a distributed system by sampling random pieces. Our protocol operates at the peer layer and certifies *alignment* between participants rather than global availability.

### Poissonization and Analytical Techniques

Flajolet et al. [3] use Poissonization extensively in HyperLogLog analysis. Jacquet and Szpankowski [4] develop analytical depoissonization techniques. In our setting, Poissonization provides the foundation for the overlap probability formula, but pointer desynchronization resists closed-form analysis, motivating our simulation approach.

### Summary of Distinction

| Approach | Goal | Method | Assumptions |
|----------|------|--------|-------------|
| Set sketching | Estimate similarity | Hash-based sampling | Cooperative parties |
| PSI | Compute intersection privately | Cryptographic protocols | Semi-honest/malicious |
| PoR/PoRep | Verify specific data storage | Cryptographic commitments | Client-server |
| DAS | Verify data availability | Random sampling | Known data structure |
| **Suffix-walk** | **Certify aligned storage** | **Statistical overlap** | **Competitive peers** |

---

## 7. Conclusion

We have presented a probabilistic mechanism for certifying aligned storage in decentralized systems. The suffix-walk protocol enables participants to demonstrate possession of large, overlapping datasets through randomized queries and overlap statistics, without cryptographic commitments, per-element preprocessing, or trusted verifiers.

### Summary of Contributions

**Analytical foundation.** We established that suffix hits in a large ordered universe are well-approximated by a Poisson process, derived the overlap probability for aligned parties, and proved the key structural result: overlap probability is bounded above by the minimum storage density among participants.

**Simulation-based bounds.** We demonstrated that pointer desynchronization causes the naïve Binomial model to overestimate tail probabilities by a factor of 2–3×. Through systematic simulation, we established conservative bounds enabling verifiers to make rigorous statistical inferences about minimum storage density.

**Amplification mechanisms.** We showed that confidence can be amplified exponentially through independent repetition and strengthened structurally through multi-party verification.

**Mechanism-design perspective.** We framed the protocol as a primitive for incentive design, showing that repeated suffix-walk interactions induce a game where storing and aligning data is the dominant strategy.

### Limitations

The protocol reveals a small random sample of stored elements—the minimum disclosure necessary for verification. Our security bounds are empirically derived through simulation. The game-theoretic analysis assumes participants value network connectivity and consensus participation.

### Future Directions

Several extensions merit investigation: tighter analytical bounds via Markov chain methods, weighted verification for non-uniform element importance, adaptation to dynamic universes, privacy enhancements through commitment schemes, and network simulation for empirical validation of emergent consensus.

### Closing Remarks

The suffix-walk protocol demonstrates that meaningful storage verification is possible without heavyweight cryptographic machinery. As a mechanism-design primitive, it converts storage—a concrete, non-transferable resource—into verifiable consensus weight, providing a foundation for decentralized systems where influence derives from demonstrated commitment to shared state.

---

## References

[1] M. Thorup, "Bottom-k and priority sampling, set similarity and subset sums with minimal independence," in *Proc. 45th ACM Symposium on Theory of Computing (STOC)*, 2013.

[2] A. Dasgupta, K. Lang, L. Rhodes, and J. Thaler, "A framework for estimating stream expression cardinalities," in *Proc. International Conference on Database Theory (ICDT)*, 2015.

[3] P. Flajolet, É. Fusy, O. Gandouet, and F. Meunier, "HyperLogLog: the analysis of a near-optimal cardinality estimation algorithm," in *Proc. Conference on Analysis of Algorithms (AofA)*, 2007.

[4] P. Jacquet and W. Szpankowski, "Analytical depoissonization and its applications," *Theoretical Computer Science*, vol. 201, no. 1–2, pp. 1–62, 1998.

[5] S. Ioffe, "Improved consistent sampling, weighted minhash and L1 sketching," in *Proc. IEEE International Conference on Data Mining (ICDM)*, 2010.

[6] B. Haeupler, M. Manasse, and K. Talwar, "Consistent weighted sampling made more practical," in *Proc. 23rd International Conference on World Wide Web (WWW)*, 2014.

[7] E. De Cristofaro and G. Tsudik, "Practical private set intersection protocols with linear complexity," in *Proc. Financial Cryptography and Data Security (FC)*, 2010.

[8] D. Morales, A. Díaz-Domingo, F. Bourse, and O. Blazy, "Private set intersection: A systematic literature review," *Computer Science Review*, vol. 49, 2023.

[9] S. Belbasi, A. Souza, and R. Patro, "The minimizer Jaccard estimator is biased and inconsistent," *Bioinformatics*, vol. 38, no. Suppl_1, pp. i169–i176, 2022.

[10] A. Juels and B. S. Kaliski Jr., "PORs: Proofs of retrievability for large files," in *Proc. ACM Conference on Computer and Communications Security (CCS)*, 2007.

[11] H. Shacham and B. Waters, "Compact proofs of retrievability," in *Proc. ASIACRYPT*, 2008.

[12] Filecoin, "Proof of replication," Filecoin Research, Technical Report, 2017.

[13] Ethereum Research, "Data availability sampling," 2022. Available: https://ethereum.org/en/roadmap/danksharding/

[14] Celestia, "Data availability sampling," Celestia Documentation, 2023.

[15] L. Szuwalski, "The EC Protocol: Distributed Consensus via Echo Consent," Zenodo, 2025. 10.5281/zenodo.18029971

---

## Appendix A: Proof of Lemma 3

**Lemma 3 (Aligned overlap probability).** Let suffix hits form a Poisson process, independently thinned by parties A and B with retention probabilities q_A, q_B ∈ (0,1]. Starting from aligned positions, the probability that the next visible hit is common to both parties is:

$$p_{AB} = \frac{q_A q_B}{q_A + q_B - q_A q_B}$$

*Proof.* Under independent Bernoulli thinning, each suffix hit belongs to one of four disjoint categories:

- Common (both retain): probability q_A q_B
- A-only (A retains, B does not): probability q_A(1 - q_B)
- B-only (B retains, A does not): probability (1 - q_A)q_B
- Neither (both reject): probability (1 - q_A)(1 - q_B)

By the thinning property of Poisson processes, each category forms an independent Poisson process with rate proportional to the category probability.

The first hit *visible to at least one party* is the first event in the superposition of {common, A-only, B-only}. This superposition is itself a Poisson process with rate proportional to:

$$q_A q_B + q_A(1-q_B) + (1-q_A)q_B = q_A + q_B - q_A q_B$$

By the superposition property, the type of the first event is drawn according to the normalized category probabilities. The probability it is common is:

$$p_{AB} = \frac{q_A q_B}{q_A + q_B - q_A q_B}$$

∎

---

## Appendix B: Reference Implementation

**Algorithm 1: SuffixWalk (Python)**

```python
def suffix_walk(stored: set, universe: list, suffixes: list, b: int, start: int = 0) -> list:
    """
    Perform suffix walk over stored elements.
    
    Args:
        stored: Set of elements the participant stores
        universe: Sorted list of all elements
        suffixes: List of m suffix values to query
        b: Suffix length in bits
        start: Starting position in universe
    
    Returns:
        List of m recorded elements, or None on failure
    """
    mask = (1 << b) - 1
    ptr = start
    responses = []
    
    for suffix in suffixes:
        found = False
        for i in range(ptr, len(universe)):
            elem = universe[i]
            if elem in stored and (elem & mask) == suffix:
                responses.append(elem)
                ptr = i + 1
                found = True
                break
        if not found:
            return None  # FAILURE
    
    return responses


def compute_overlap(responses_a: list, responses_b: list) -> int:
    """Compute overlap count between two response sets."""
    return len(set(responses_a) & set(responses_b))


def verify(responses_a: list, responses_b: list, threshold: int) -> bool:
    """Verify that overlap meets threshold."""
    return compute_overlap(responses_a, responses_b) >= threshold
```

**Poisson-Walk Simulation (Python)**

```python
import random
import math

def poisson_walk_simulation(q_a: float, q_b: float, m: int, trials: int = 10000) -> dict:
    """
    Simulate suffix-walk overlap using Poisson-walk model.
    
    Args:
        q_a: Storage density of party A
        q_b: Storage density of party B
        m: Number of recorded elements per party
        trials: Number of simulation trials
    
    Returns:
        Dictionary mapping overlap count to frequency
    """
    # Rates for the three Poisson processes
    rate_common = q_a * q_b
    rate_a_only = q_a * (1 - q_b)
    rate_b_only = (1 - q_a) * q_b
    rate_total = rate_common + rate_a_only + rate_b_only
    
    # Normalized probabilities
    p_common = rate_common / rate_total
    p_a_only = rate_a_only / rate_total
    # p_b_only = rate_b_only / rate_total (implicit)
    
    overlap_counts = {k: 0 for k in range(m + 1)}
    
    for _ in range(trials):
        # Track how many elements each party has recorded
        count_a = 0
        count_b = 0
        overlap = 0
        
        while count_a < m and count_b < m:
            r = random.random()
            if r < p_common:
                # Common hit: both record
                count_a += 1
                count_b += 1
                overlap += 1
            elif r < p_common + p_a_only:
                # A-only hit: only A records
                count_a += 1
            else:
                # B-only hit: only B records
                count_b += 1
        
        overlap_counts[overlap] += 1
    
    # Convert to frequencies
    return {k: v / trials for k, v in overlap_counts.items()}
```

---

## Appendix C: Extended Simulation Tables

**Table C.1: Pairwise overlap probabilities (m = 10, 20,000 trials)**

| q_A | q_B | P(K≥6) | P(K≥7) | P(K≥8) | P(K≥9) | P(K=10) |
|-----|-----|--------|--------|--------|--------|---------|
| 0.70 | 0.70 | 0.312 | 0.142 | 0.047 | 0.011 | 0.002 |
| 0.80 | 0.80 | 0.576 | 0.355 | 0.168 | 0.054 | 0.011 |
| 0.90 | 0.90 | 0.872 | 0.712 | 0.480 | 0.238 | 0.072 |
| 0.95 | 0.95 | 0.965 | 0.893 | 0.742 | 0.498 | 0.221 |
| 0.99 | 0.99 | 0.998 | 0.992 | 0.970 | 0.906 | 0.736 |

**Table C.2: Conservative bounds (m = 12, 80,000 trials, configuration (μ, 1))**

| min(q) ≤ μ | P(K≥8) | P(K≥9) | P(K≥10) | P(K≥11) | P(K=12) |
|------------|--------|--------|---------|---------|---------|
| 0.40 | 0.008 | 0.003 | 0.001 | <0.001 | <0.001 |
| 0.50 | 0.032 | 0.011 | 0.005 | 0.001 | <0.001 |
| 0.60 | 0.112 | 0.048 | 0.024 | 0.008 | 0.002 |
| 0.70 | 0.287 | 0.154 | 0.095 | 0.046 | 0.014 |
| 0.80 | 0.536 | 0.366 | 0.271 | 0.168 | 0.067 |
| 0.90 | 0.823 | 0.679 | 0.596 | 0.468 | 0.284 |

**Table C.3: Triple overlap (m = 10, 20,000 trials)**

| q_A | q_B | q_C | P(triple ≥ 6) | P(triple ≥ 7) | P(triple ≥ 8) |
|-----|-----|-----|---------------|---------------|---------------|
| 0.80 | 0.80 | 0.80 | 0.18 | 0.07 | 0.03 |
| 0.90 | 0.90 | 0.90 | 0.53 | 0.35 | 0.24 |
| 0.95 | 0.95 | 0.95 | 0.79 | 0.62 | 0.51 |
| 0.99 | 0.99 | 0.99 | 0.98 | 0.95 | 0.91 |
| 0.80 | 0.90 | 0.95 | 0.32 | 0.16 | 0.09 |
