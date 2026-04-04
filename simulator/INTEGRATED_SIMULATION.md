# Integrated Simulator

This simulator is the current proof-of-concept harness for exercising the combined protocol parts together.

It is intended to answer:
- Can a population of nodes form, take transactions, churn, and recover while commit-chain sync is active?
- What transaction latency and throughput do we see under changing network conditions?
- How long do joiners and rejoiners need before they become useful again?

## What It Exercises

The integrated runner uses full `EcNode` instances. Each simulated round includes:

1. scheduled lifecycle events
2. observation of newly committed transaction blocks
3. new transaction injection
4. network scheduling and delivery
5. node ticks

Each node tick includes:
- mempool cleanup, validation, voting, and local commits
- peer discovery and elections through `EcPeers`
- commit-chain head exchange and block sync

This means the simulator is not only testing one subsystem at a time. It is testing the interaction between:
- peer formation
- transaction flow
- commit-chain sync
- churn and return
- network stress

## Lifecycle Modes

Two example entrypoints are provided:

- `integrated_simulation`
  - initial cohort starts from an already formed population
  - useful for stressing steady-state churn and sync

- `integrated_genesis_simulation`
  - initial cohort is bootstrapped from deterministic genesis-backed state
  - late joiners still enter cold and must sync from neighbors

- `integrated_long_run`
  - fixed-seed, release-mode scenario intended for longer baseline measurements
  - starts from genesis and applies moderate join/crash/return churn under `cross_dc_normal`

Late joiners are not inserted as already-connected peers. They start with bootstrap hints and have to discover peers, learn commit-chain heads, and begin sync traces.

Rejoiners keep their persisted backend state but restart their volatile runtime state. That makes stale return behavior visible in metrics.

## Network Model

The integrated simulator now uses an explicit in-flight message queue.

Each outbound message is sampled once when it enters the network:
- `loss_fraction`
  - probability that the message is dropped permanently
- `base_delay_rounds`
  - fixed additional queueing delay
- `jitter_rounds`
  - uniform random extra delay from `0..=jitter_rounds`
- `delay_fraction`
  - probability of adding one more round of delay, sampled repeatedly
  - this creates a geometric tail distribution

Important detail:
- messages already have an implicit minimum of one round because node output generated during a round is only scheduled for delivery on a later round

This model is still simple, but it is closer to a datacenter-style asynchronous network than the old “maybe delayed one round again” behavior because delay is now an explicit sampled transit time.

## Transaction Origination

The default operator policy is `connected-only`.

That means new transactions are only injected at nodes that currently consider themselves connected. This is intended to model a well-behaved operator who avoids originating load from obviously unhealthy nodes.

This is an operator policy, not a protocol guarantee.

## Reported Metrics

The simulator reports:

- transaction commit latency
  - avg, p50, p95, min, max
- recent commit throughput
  - commits per round over a moving window
- sampled network transit delay
  - avg, p50, p95, min, max
- total messages delivered
- active commit traces
  - current and peak
- active elections
  - current and peak
- eligible transaction sources
  - current and average
- skipped transaction submissions
  - if no eligible source exists
- late-join onboarding
  - time to connected
  - time to first known commit-chain head
  - time to first sync trace
  - whether connected happened before head/sync
- rejoin onboarding
  - same metrics as late joiners
- recovery watches
  - time until rolling commit rate returns near the pre-event baseline

These metrics are emitted both at checkpoints during the run and in the final summary.

## Why This Is Reasonably Realistic

This simulator is intended to be production-adjacent for control-plane behavior, not a full production benchmark.

It is reasonably close because it uses:
- full `EcNode` execution
- the real peer manager
- the real mempool logic
- the real commit-chain sync path
- deterministic churn and restart events
- explicit queueing, loss, and tail latency in the network model

It is not yet a full production proxy because it still omits:
- real transport stacks and bandwidth limits
- persistent database latency
- correlated network partitions and routing asymmetry
- heterogeneous hardware
- deployment-level clock and scheduler effects

## Current Realism Gap

The largest remaining behavior gap is stale rejoin admission.

Today, a rejoining node can still become `Connected` before catch-up is complete if elections and invitations succeed quickly enough. That means:
- onboarding metrics for rejoiners are meaningful
- but they are still slightly optimistic for a deployment that would require sync-readiness before full service

In deployment terms, a well-behaved implementation would likely wait for stronger sync evidence before treating itself as fully live. A practical rule would be something like:
- do not consider the node fully live until sync has clearly engaged
- or require at least two opposing traces / equivalent catch-up evidence

That rule is not yet enforced in the simulator.

## How To Read Results

The safest way to interpret results is:

- commit latency and throughput
  - good proxy for steady-state usefulness of the combined design
- join and rejoin timing
  - good proxy for churn tolerance and operator recovery windows
- network transit delay
  - describes the simulated network profile actually exercised during the run
- recovery watches
  - rough indicator of service recovery, not a formal liveness proof

## Recommended Profiles

The integrated config exposes named starting points:

- `NetworkConfig::same_dc()`
- `NetworkConfig::cross_dc_normal()`
- `NetworkConfig::cross_dc_stressed()`

These are relative profiles in simulation rounds, not absolute wall-clock promises. They are intended to make scenario comparisons consistent before calibrating to a specific deployment cadence.

## Commands

```bash
cargo run --example integrated_simulation
cargo run --example integrated_genesis_simulation
cargo run --release --example integrated_long_run
```
