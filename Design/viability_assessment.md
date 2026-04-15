# Viability Assessment

This note steps back from individual protocol experiments and asks a broader question:

> Given the current design and current simulator evidence, how close is this system to being a viable alternative to other distributed state systems?

The answer is not "ready" and it is not "speculative". It is somewhere in between:

- the core design now looks plausible
- the fixed-network steady-state path now looks genuinely strong
- the open-network lifecycle path still has material work left

This note is meant to keep that assessment grounded in current evidence rather than intuition.

## Scope

This note draws mainly on:

- [response_driven_commit_flow.md](./response_driven_commit_flow.md)
- [vote_flow_and_batching.md](./vote_flow_and_batching.md)
- [routing_depth_scaling.md](./routing_depth_scaling.md)
- [INTEGRATED_SIMULATION.md](../simulator/INTEGRATED_SIMULATION.md)
- [STEADY_STATE_REPORT.md](../simulator/STEADY_STATE_REPORT.md)
- [DENSE_LINEAR_TOPOLOGY_REPORT.md](../simulator/DENSE_LINEAR_TOPOLOGY_REPORT.md)
- [CHURN_GRAPH_CONTROL_REPORT.md](../simulator/CHURN_GRAPH_CONTROL_REPORT.md)
- [ADVERSARIAL_CONFLICT_REPORT.md](../simulator/ADVERSARIAL_CONFLICT_REPORT.md)

It is deliberately comparative. The question is not only "does this protocol run?" but:

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
- enough churn tolerance that the system stays useful while the network continues to change

That is a harder target than:

- leader-based replication among a fixed set of replicas
- committee-based BFT among a fixed validator set
- eventually consistent partitioned storage

It is also a different target from:

- global mempool + global block production systems that expect good user latency to come from a second layer

So the fair comparison is not "is this simpler than Raft?" It is not.

The fair comparison is:

- can an open, local, self-forming network get close enough to human-timescale behavior that it becomes a realistic base-layer alternative for global transactional state?

That question now has a more positive answer than it did earlier in the project.

## Current Thesis

The working thesis of the design is:

1. Each token is effectively hosted by a local neighborhood rather than by the entire network.
2. The peer graph is intentionally steep and local, with only a sparse far tail.
3. Transactions should route toward the relevant token and witness neighborhoods.
4. Once the host core knows the block and local state, response traffic should dominate over blind re-polling.
5. Therefore message cost should depend more on neighborhood depth and spread than on total network size.

If that thesis holds, then the system can potentially offer:

- better base-layer latency than global-gossip blockchains
- broader openness than fixed-committee systems
- stronger conflict visibility than plain eventually consistent systems

The simulator work so far gives evidence for some parts of that thesis, and clear gaps on others.

## What Has Been Demonstrated

### 1. Formed steady-state can already be fast

The strongest evidence now comes from the fixed formed-network dense-linear branch in
[DENSE_LINEAR_TOPOLOGY_REPORT.md](../simulator/DENSE_LINEAR_TOPOLOGY_REPORT.md).

Representative result:

- `1024` peers, fixed connected graph
- `cross_dc_normal`
- dense linear topology
- `InitialVote` on the first reactive wave
- `far_prob = 0.4`
- commit latency about:
  - p50 `6` rounds
  - p95 `8` rounds

At rough wall-clock mappings:

- `25 ms/round` -> about `0.15s` p50, `0.20s` p95
- `50 ms/round` -> about `0.30s` p50, `0.40s` p95

That matters because it says the base transaction path is not fundamentally too slow,
and that the more reactive first wave was worth doing.

This is a noticeably stronger steady-state claim than the project could make
earlier.

### 2. Fixed-network transaction scope now looks more local

The current fixed-network branch no longer only says "commit can be fast". It
also says "a transaction does not obviously have to touch most of the network".

Representative result on the same `1024`-peer dense-linear setup:

- settled peer spread about `38.6`
- roughly `4%` of the population
- while still holding `5.8 / 8` round commit latency

At `192` peers the same branch settles at about `34-35` peers, so the absolute
settled set is in the same general band while the fraction shrinks as the
population grows.

That is not yet a proof of large-scale sharding, but it is much closer to the
intended scaling story:

- fast commit
- local role cores
- limited transaction reach
- no obvious need for graph-wide participation per transaction

### 3. The integrated lifecycle path stays live under churn

The integrated simulator in [INTEGRATED_SIMULATION.md](../simulator/INTEGRATED_SIMULATION.md) now exercises:

- peer discovery
- churn
- crash / return
- commit-chain sync
- transaction injection
- explicit network delay / loss

The system remains live under those conditions. It degrades, but it does not collapse.

That is a major milestone. It means the combined design is not obviously unstable.

### 4. Response-driven vote flow helped materially

The protocol no longer depends as heavily on pure tick-pumping.

The response-driven work and batching work documented in:

- [response_driven_commit_flow.md](./response_driven_commit_flow.md)
- [vote_flow_and_batching.md](./vote_flow_and_batching.md)

improved:

- wire message count
- late-stage queue pressure
- commit latency
- conflict signaling quality

The latest fixed-network results add a concrete version of that story:

- `InitialVote` cut a large part of the repeated `Vote -> QueryBlock -> Block`
  round-trip cost on the first useful wave
- dense-linear topologies brought role-reaching graph depth close to `1`
- commit latency then moved closer to what those short paths actually suggested

This is important because it moved the protocol from "repeated blind polling"
toward something more structurally efficient.

### 5. Fixed-network conflict handling is much healthier on the denser linear graph

The fixed-network conflict results improved materially on the same branch.

Representative `1024`-peer dense-linear result at `far_prob = 0.4`:

- `highest-majority = 53`
- `stalled = 56`
- `lower-owner commits = 0`
- signal coverage about `0.73`

Compared with the older sparse / corrected-ring steady-state baselines, that is
a real step forward in contested-state behavior.

It is not a final convergence story, but it means the protocol is no longer
only attractive in the conflict-free case.

### 6. The churned graph was over-connected, not under-connected

This is one of the most encouraging findings.

From [CHURN_GRAPH_CONTROL_REPORT.md](../simulator/CHURN_GRAPH_CONTROL_REPORT.md):

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

Even with better graph control, the churn path does not yet preserve enough of
the steady-state envelope.

In other words:

- the protocol can now be both fast and relatively local on a good graph
- but the open network still spends too much time paying for formation, repair,
  sync, and topology churn

This is the main reason the system is not yet competitive with mature production systems.

### 2. Conflict convergence is still too weak

The conflict experiments show that the system now:

- spreads conflict knowledge better
- reduces some bad outcomes
- can warn participants that conflict exists
- can behave much better on the improved fixed-network topology

But it still does not strongly enough guarantee that the highest contender wins locally or globally.

Too many families still:

- stall
- leave lower contenders visible
- or commit lower contenders locally before being eroded away

This is the biggest remaining correctness / usability gap in contested state.

### 3. Message complexity is improved, but still above the ideal lower bound

The project has improved this substantially, but in the harder scenarios the actual cost is still far above the local lower bound.

That means:

- the routing / neighborhood thesis is plausible
- but the implementation is still leaving efficiency on the table

This is especially visible under churn and conflict, where repair, block fetch,
commit-chain sync, and repeated polling still dominate too much of the work.

It is also visible in fixed-network runs where latency is already good but
`InitialVote`, ordinary `Vote`, and block traffic are still high enough that
the batching and fetch rules deserve another round of tightening.

### 4. The design is still harder to reason about than fixed-committee systems

This is not a small issue.

Compared with leader-based or committee-based systems, this protocol has more moving parts:

- organic graph formation
- local routing slope
- delayed response paths
- conflict signaling
- churn and return
- state sync

That complexity is not automatically disqualifying, but it raises the bar for proof, argument, and implementation discipline.

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

- still behind on convergence quality
- but the upside remains attractive if open participation is a hard requirement

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

- the steady-state results support the claim that the base layer can be much more responsive than global block production
- the churn path still needs work before that advantage can be claimed in normal operation

### Layer-2 / Channels / Rollup-First Scaling

This is not a consensus family, but it is the main practical competitor to the thesis.

The usual argument is:

- open global base layers cannot realistically carry human-timescale transactional load directly
- so fast user interactions must live off-chain or in secondary systems

The current evidence pushes back on that argument:

- not enough to disprove it
- but enough to keep the thesis alive

Current assessment:

- the project now looks capable of supporting the claim that an open base layer might carry meaningful direct transactional load
- but only if conflict and churn overhead are brought down further

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
- the cost is greater protocol complexity and more demanding convergence requirements

## Main Strengths Right Now

The best current strengths are:

1. **Human-timescale fixed-network commit looks real**
   - The project no longer depends on a purely speculative performance story.
   - The current fixed-network branch is materially stronger than the older steady-state baseline.

2. **A more local transaction scope now looks plausible**
   - The project now has evidence that transactions can stay within a small slice of the network on a good graph.

3. **Open-network liveness looks real**
   - The combined system survives churn and keeps processing work.

4. **The graph problem is tractable**
   - Over-connection is a better starting point than under-connection.

5. **The protocol direction is improving**
   - Response-driven flow and better graph shaping are materially better than the earlier simpler versions.

6. **The scaling model is still distinctive**
   - The project is not merely re-implementing a weaker or noisier version of Raft or HotStuff.

## Main Risks

The major risks are:

1. **Conflict may remain too weakly convergent**
   - If highest-contender convergence stays too weak, the system becomes operationally awkward even if it is live.

2. **Open-network overhead may remain too high**
   - If the system cannot form and preserve dense-linear-like peer sets under realistic churn, the design will remain academically interesting but operationally weak.

3. **Complexity may outrun verifiability**
   - A protocol can fail not because it is impossible, but because it becomes too subtle for teams to implement consistently and safely.

4. **Graph tuning alone may have diminishing returns**
   - The recent results suggest topology can unlock the good regime, but peer management, commit-chain sync, and fetch / batching policy still decide whether that regime survives in the live system.

## What Would Count As The Next Proof Point

The next meaningful evidence bar is not "another small win".

It is something stronger:

1. **A churn/bootstrap baseline that forms something close to the current good fixed-network graph**
   - not just any connected graph
   - specifically a graph that preserves fast local settlement and limited spread after joins, crashes, and returns

2. **Commit-chain sync and peer management that fit inside the same efficiency envelope**
   - the sync path and the peer-maintenance path need to cooperate with batching rather than silently becoming the dominant traffic source

3. **Conflict improvements on top of that graph**
   - materially fewer stalled families
   - materially fewer lower-owner commits
   - no major regression in liveness

4. **A clearer efficiency story**
   - message-load factors moving closer to the local lower bound
   - especially in the non-conflict and moderate-conflict cases

5. **A stronger design argument**
   - not just simulator success, but a clearer explanation of why the protocol should converge and remain efficient under the intended operating conditions

## Bottom Line

The project has crossed an important threshold.

It no longer looks like:

- a speculative alternative consensus idea with no convincing operational path

It now looks like:

- a plausible open, locality-driven transactional system with a real fixed-network story
- a real churn/liveness story
- and a still-incomplete conflict and lifecycle-efficiency story

That is a meaningful change.

So the current bottom-line assessment is:

- **potential**: strong
- **viability today**: not yet
- **most encouraging evidence**: fast fixed-network commit, more local transaction scope, and open-network liveness
- **most important missing piece**: forming and preserving the right graph under churn, together with better conflict convergence and tighter sync / batching behavior

If those gaps close, the system could become a serious alternative in a part of the design space that current mainstream systems do not cover cleanly.
