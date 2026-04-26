# Viability Assessment

This note steps back from individual protocol experiments and asks a broader
question:

> Given the current design and current simulator evidence, how close is this
> system to being a viable alternative to other distributed state systems?

The answer is now more positive than it was earlier in the project:

- the core design looks plausible
- the fixed-network steady-state path looks genuinely strong
- the main open problems now look more like efficiency and lifecycle control
  than like fundamental liveness failure

That still does not mean "ready". It means the project has crossed from
speculative to credible enough to deserve continued serious work.

## Scope

This note draws mainly on:

- [response_driven_commit_flow.md](./response_driven_commit_flow.md)
- [vote_flow_and_batching.md](./vote_flow_and_batching.md)
- [routing_depth_scaling.md](./routing_depth_scaling.md)
- [INTEGRATED_SIMULATION.md](../simulator/INTEGRATED_SIMULATION.md)
- [DENSE_LINEAR_TOPOLOGY_REPORT.md](../simulator/DENSE_LINEAR_TOPOLOGY_REPORT.md)
- [FIXED_NETWORK_CONFLICT_LINEAGE_REPORT.md](../simulator/FIXED_NETWORK_CONFLICT_LINEAGE_REPORT.md)
- [FIXED_NETWORK_EXTENSION_STEADY_REPORT.md](../simulator/FIXED_NETWORK_EXTENSION_STEADY_REPORT.md)
- [CHURN_GRAPH_CONTROL_REPORT.md](../simulator/CHURN_GRAPH_CONTROL_REPORT.md)
- [ADVERSARIAL_CONFLICT_REPORT.md](../simulator/ADVERSARIAL_CONFLICT_REPORT.md)
- Repair traffic reduction experiments (parent fetch cooldown, smart voter
  selection, non-conflict follow-up interval tuning)

It is deliberately comparative. The question is not only "does this protocol
run?" but:

- what problem is it solving that other systems do not solve in the same way
- what has actually been demonstrated
- what is still missing before a stronger viability claim would be justified

## Target Problem

The target is not "build another closed replication protocol".

The target is:

- open participation
- no fixed global validator set
- locality-driven routing and hosting
- human-timescale transaction settlement
- enough churn tolerance that the system stays useful while the network
  continues to change

That is a harder target than:

- leader-based replication among a fixed set of replicas
- committee-based BFT among a fixed validator set
- eventually consistent partitioned storage

It is also a different target from:

- global mempool + global block production systems that expect good user
  latency to come from a second layer

So the fair comparison is not "is this simpler than Raft?" It is not.

The fair comparison is:

- can an open, local, self-forming network get close enough to
  human-timescale behavior that it becomes a realistic base-layer alternative
  for global transactional state?

That question now has a meaningfully stronger answer than it did earlier in the
project.

## Current Thesis

The working thesis of the design is:

1. Each token is effectively hosted by a local neighborhood rather than by the
   entire network.
2. The peer graph is intentionally steep and local, with only a sparse far
   tail.
3. Transactions should route toward the relevant token and witness
   neighborhoods.
4. Once the host core knows the block and local state, response traffic should
   dominate over blind re-polling.
5. Therefore message cost should depend more on neighborhood depth and spread
   than on total network size.

If that thesis holds, then the system can potentially offer:

- better base-layer latency than global-gossip blockchains
- broader openness than fixed-committee systems
- stronger conflict visibility than plain eventually consistent systems

The simulator work now gives real support for several parts of that thesis. The
largest remaining gaps are around lifecycle preservation and repair efficiency.

## What Has Been Demonstrated

### 1. Formed steady-state can already be fast

The strongest current evidence comes from the fixed dense-linear branch.

Representative results now include:

- `2000` peers, fixed dense-linear graph, no conflicts:
  - multi-token continuous chain extension
  - avg latency `2.7` rounds
  - p50 `2`
  - p95 `5`
- `2000` peers, fixed dense-linear graph, `30%` conflict families:
  - overall commit latency avg `4.3`
  - p50 `3`
  - p95 `5`

At rough wall-clock mappings:

- `25 ms/round` -> about `0.05s` to `0.08s` p50, `0.13s` p95
- `50 ms/round` -> about `0.10s` to `0.15s` p50, `0.25s` p95

That matters because it says the base transaction path is not fundamentally too
slow. The good-graph regime is fast enough to be operationally interesting.

### 2. Fixed-network transaction scope now looks genuinely local

The current fixed-network branch no longer only says "commit can be fast". It
also says "a transaction does not obviously have to touch most of the network".

Representative results:

- `2000`-peer fixed conflict run:
  - settled peer spread avg `37.1`
  - p50 `31`
  - p95 `78`
- `2000`-peer fixed extension run:
  - dominant clean-extension class settled spread avg `52.2`
  - p95 `117`

Those are still small fractions of the full population. That is not yet a proof
of large-scale sharding, but it is much closer to the intended scaling story:

- fast commit
- local role cores
- limited transaction reach
- no obvious need for graph-wide participation per transaction

### 3. Fixed-network conflict handling is now much healthier

The corrected conflict report is the most important recent change in the
overall assessment.

Representative `2000`-peer fixed dense-linear result:

- `288` conflict families
- `268` highest-majority
- `20` stalled
- `0` lower-majority
- `0` lower-owner commits
- `0` multi-owner commits
- highest-candidate coverer share avg `0.90`, p50 `1.00`, p95 `1.00`

That does not mean conflict is solved. It does mean the earlier conclusion that
conflicts mostly failed to converge was too pessimistic. Under the corrected
committed-candidate measurement:

- minority contenders are being suppressed
- most families do converge on the highest contender
- the main remaining issue is the expensive stalled tail, not obvious safety
  failure

### 4. Clean chain extension is especially promising

The fixed extension report gives a strong view of the ordinary non-conflict
path.

Representative `2000`-peer fixed dense-linear result:

- `896` clean committed-chain extensions submitted
- `753` committed
- avg latency `2.5`
- p95 latency `4`
- settled spread avg `52.2`

Compared with fresh creation in the same run, clean extension is:

- faster
- more local
- materially cheaper in block-message cost

That is encouraging because the ordinary continuing-token path is likely to be
the dominant useful workload if the system ever becomes practical.

### 5. The integrated lifecycle path stays live under churn

The integrated simulator still matters because it exercises:

- peer discovery
- churn
- crash / return
- commit-chain sync
- transaction injection
- explicit network delay / loss

The system remains live under those conditions. It degrades, but it does not
collapse. That is still a major milestone.

### 6. The churned graph was over-connected, not under-connected

This remains one of the most encouraging findings from
[CHURN_GRAPH_CONTROL_REPORT.md](../simulator/CHURN_GRAPH_CONTROL_REPORT.md):

- the main lifecycle graph problem was not lack of peers
- it was too many medium and far peers
- the network stayed broad and flat rather than steep and local

That is good news because:

- inventing missing connectivity is hard
- reducing excess breadth is easier

It means the system is biased toward liveness, not starvation.

## Where It Still Falls Short

### 1. Churn and bootstrap still pull the system too far away from the good fixed-network regime

This is still the largest practical gap.

The protocol can now be both fast and relatively local on a good graph, but the
open network still spends too much time paying for:

- formation
- repair
- sync
- topology churn

This is the main reason the system is not yet competitive with mature
production systems.

### 2. Conflict is now safer than before, but the stalled tail is still too expensive

The conflict results are much better than the earlier exact-head view implied.
But even with that correction, there is still a nontrivial stalled tail,
especially for clean-parent conflicts.

So the remaining conflict issue is no longer mainly:

- "the lower contender keeps winning"

It is more:

- "too many contested families are still expensive and indecisive for too
  long"

That is still important, but it is a narrower and more tractable problem than
the earlier picture suggested.

### 3. Message complexity has improved significantly through policy tuning

Recent experiments confirmed that repair traffic was indeed policy-driven rather
than fundamental. Three targeted optimizations achieved substantial reductions:

**Optimizations applied:**

1. **Parent fetch cooldown** (5 ticks between repeated requests)
   - Prevents flooding when waiting for parent blocks
   - Reduced duplicate requests significantly

2. **Smart voter selection for missing parents**
   - Request parent blocks from peers who voted positive (they have the block)
   - Reduced referrals by 92% (659K → 55K)

3. **Reduced follow-up intensity for non-conflicting blocks**
   - Non-conflicts only poll every 3rd tick (conflicts get full follow-up)
   - Non-conflicts commit efficiently via reactive InitialVote wave alone

**Results on `2000`-peer fixed dense-linear with 30% conflicts:**

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Total messages | 3.80M | 1.99M | -48% |
| Commits | 868 | 972 | +12% |
| Referrals | 659K | 55K | -92% |
| Stalled conflicts | - | 15 | low |
| Efficiency | ~4,400 msg/commit | 2,048 msg/commit | -53% |

The interval=3 setting for non-conflict follow-up proved optimal. Surprisingly,
it achieved *more* commits than full follow-up (972 vs 967) while using 28% less
traffic. This suggests the reactive flow works better when not interrupted by
aggressive polling.

**Remaining gap:**

Message complexity is now much closer to practical, but still above the
theoretical local lower bound. The block-message factor remains around 11x the
ideal role-sum bound. Further improvements may come from:

- better batching of vote replies
- coalescing parent/block requests
- adaptive scheduling based on network conditions

### 4. The design is still harder to reason about than fixed-committee systems

Compared with leader-based or committee-based systems, this protocol still has
more moving parts:

- organic graph formation
- local routing slope
- reactive dissemination
- conflict signaling
- churn and return
- state sync

That complexity is not automatically disqualifying, but it raises the bar for
proof, argument, and implementation discipline.

## Comparison By System Class

### Leader-Based Replication

Examples:

- Raft
- Multi-Paxos
- replicated SQL / KV systems

These still win clearly on:

- simplicity of agreement structure
- predictability of latency
- operational maturity
- bounded message patterns

This design potentially wins on:

- openness
- no fixed membership authority
- locality-driven scaling instead of whole-group replication per transaction

Current assessment:

- not competitive yet on efficiency or confidence
- potentially competitive only because it is solving a different problem

### Fixed-Committee BFT

Examples:

- PBFT
- Tendermint
- HotStuff-family protocols

These still win on:

- crisp safety model inside a known validator set
- stronger and cleaner convergence semantics

This design potentially wins on:

- avoiding fixed committees
- avoiding one global agreement group for all state
- scaling by neighborhoods rather than global validator participation

Current assessment:

- still behind on proof and lifecycle control
- but the fixed-network conflict results now make the safety side more credible
  than before

### Nakamoto / Global-Gossip Blockchain Systems

Examples:

- Bitcoin-like or Ethereum-like global block production models

These typically pay for:

- global visibility
- slow global convergence
- strong demand for second-layer systems if user experience must be fast

This design appears stronger on:

- base-layer latency potential
- locality of work
- avoiding global participation for every transaction

Current assessment:

- the fixed-network results strongly support the claim that the base layer can
  be much more responsive than global block production
- the churn path still needs work before that advantage can be claimed in
  normal operation

### Layer-2 / Channels / Rollup-First Scaling

This is not a consensus family, but it is the main practical competitor to the
thesis.

The usual argument is:

- open global base layers cannot realistically carry human-timescale
  transactional load directly
- so fast user interactions must live off-chain or in secondary systems

The current evidence pushes back on that argument:

- not enough to disprove it
- but enough to keep the thesis very much alive

Current assessment:

- the project now has a stronger case that an open base layer might carry
  meaningful direct transactional load
- but only if repair traffic and lifecycle overhead come down substantially

### Eventually Consistent Partitioned Systems

Examples:

- DHT / Dynamo-like families
- Cassandra-style local-replica designs

These are easier to scale because they accept weaker semantics.

This design is stronger when it works because it tries to maintain:

- token-local agreement
- conflict visibility
- routing to meaningful host neighborhoods

Current assessment:

- this design is aiming for stronger semantics than plain eventual consistency
- the cost is greater protocol complexity and more demanding convergence
  requirements

## Main Strengths Right Now

The best current strengths are:

1. **Human-timescale fixed-network commit looks real**
   - The project no longer depends on a purely speculative performance story.

2. **A more local transaction scope now looks plausible**
   - The project now has evidence that transactions can stay within a small
     slice of the network on a good graph.

3. **Fixed-network conflict safety looks materially better**
   - The corrected conflict report shows suppression of lower contenders and
     majority formation on the highest contender in most families.

4. **Open-network liveness looks real**
   - The combined system survives churn and keeps processing work.

5. **The graph problem still looks tractable**
   - Over-connection is a better starting point than under-connection.

6. **A large share of remaining cost is now proven schedulable** ✓
   - This hypothesis has been validated: targeted policy changes (parent fetch
     cooldown, smart voter selection, non-conflict follow-up tuning) achieved
     48% traffic reduction with no regression in commits or latency.
   - The interval=3 setting for non-conflict follow-up proved optimal,
     achieving more commits (972 vs 967) with 28% less traffic than full
     follow-up.

## Main Risks

The major risks are:

1. **Open-network overhead may remain too high**
   - If the system cannot form and preserve dense-linear-like peer sets under
     realistic churn, the design will remain interesting but operationally
     weak.

2. **Repair traffic may remain too high even when correctness is acceptable**
   - *Partially mitigated*: Recent experiments cut repair traffic by 48% through
     policy tuning. The block-message factor is now ~11x (down from higher), but
     still above the theoretical ~1x local lower bound.
   - Remaining gap is narrower and more tractable than before.

3. **Complexity may outrun verifiability**
   - A protocol can fail not because it is impossible, but because it becomes
     too subtle for teams to implement consistently and safely.

4. **Graph tuning alone may have diminishing returns**
   - Topology helps unlock the good regime, but repair policy, parent fetch
     behavior, batching, and peer management still decide whether that regime
     remains efficient.

## What Would Count As The Next Proof Point

The next meaningful evidence bar is not "another small win".

It is something stronger:

1. **A churn/bootstrap baseline that forms something close to the current good fixed-network graph**
   - not just any connected graph
   - specifically a graph that preserves fast local settlement and limited
     spread after joins, crashes, and returns

2. **Commit-chain sync and peer management that fit inside the same efficiency envelope**
   - the sync path and the peer-maintenance path need to cooperate with
     batching rather than silently becoming the dominant traffic source

3. **Repair-policy improvements on top of that graph** ✓ *partially achieved*
   - materially less `QueryBlock` ✓ (reduced via smart voter selection)
   - materially less missing-parent repair ✓ (cooldown + targeting)
   - no regression in fixed-network latency ✓ (p50 stayed at 4, some improved to 3)
   - no regression in conflict safety ✓ (stalled conflicts at 15, down from 18)

4. **A clearer efficiency story** ✓ *partially achieved*
   - message-load factors moving closer to the local lower bound ✓ (48% reduction)
   - especially in the non-conflict and moderate-conflict cases ✓
   - evidence that schedule and fetch-policy improvements can cut a large share
     of current repair traffic ✓ (demonstrated with interval tuning)

5. **A stronger design argument**
   - not just simulator success, but a clearer explanation of why the protocol
     should converge and remain efficient under the intended operating
     conditions

## Bottom Line

The project has crossed an important threshold.

It no longer looks like:

- a speculative alternative consensus idea with no convincing operational path

It now looks like:

- a plausible open, locality-driven transactional system with a real
  fixed-network story
- a real churn/liveness story
- and a **significantly improved** lifecycle-efficiency story

**Recent progress on repair traffic:**

The hypothesis that message complexity was policy-driven has been validated.
Three targeted optimizations (parent fetch cooldown, smart voter selection,
reduced non-conflict follow-up) achieved:

- 48% reduction in total messages
- 92% reduction in referrals
- 12% increase in commits
- 53% improvement in messages-per-commit efficiency

This moves the system from "over-chatty" toward "reasonably efficient" for
fixed-network operation.

So the current bottom-line assessment is:

- **potential**: strong
- **viability today**: closer than before, but not yet
- **most encouraging evidence**: fast fixed-network commit, local transaction
  scope, strong fixed-network conflict safety, open-network liveness, and
  **demonstrated policy-level traffic reduction**
- **most important missing piece**: forming and preserving the right graph
  under churn; the repair/batching story is now much stronger

If the remaining churn/bootstrap gaps close, the system could become a serious
alternative in a part of the design space that current mainstream systems do
not cover cleanly.
