# Integrated Long-Run Report

This report documents a longer integrated simulator run intended to exercise the combined protocol parts under sustained load and moderate churn.

## Scenario

Code:
- `simulator/integrated_long_run.rs`

Run mode:
- `cargo run --release --example integrated_long_run`

Measured runtime on this machine:
- `499.47` seconds real time
- about `8m 19s`

Configuration:
- `2400` rounds
- fixed seed
- genesis-backed bootstrap
- `96` initial peers
- `24` joining peers at round `480`
- `12` crashes at round `1200`
- `8` returns at round `1400`
- `16` joining peers at round `1680`
- `10` crashes at round `2000`
- `50_000` genesis blocks
- `NetworkConfig::cross_dc_normal()`
- `3` submitted transaction-blocks per round
- transaction source policy: `connected-only`

Final population:
- `136` total peers created during the run
- `122` active at the end

## Checkpoints

Early baseline at round `400`:
- `96` active peers
- `1154` committed, `46` pending
- recent throughput `3.17` commits/round
- commit latency `p50 16`, `p95 21` rounds

After first growth wave at round `800`:
- `120` active peers
- `2062` committed, `338` pending
- recent throughput `1.50` commits/round
- commit latency `p50 17`, `p95 25` rounds

Late-stage checkpoint at round `2160`:
- `122` active peers
- `4272` committed, `2208` pending
- recent throughput `0.75` commits/round
- commit latency `p50 19`, `p95 606` rounds
- in-flight messages `163120`

## Final Metrics

Core outcome:
- `7200` attempts
- `7200` submitted
- `4664` committed
- `2536` still pending at the end
- commit completion ratio: `64.8%`

Latency:
- average commit latency: `113.1` rounds
- `p50`: `20` rounds
- `p95`: `697` rounds
- min/max: `7` / `1202` rounds

Network transit:
- average sampled transit delay: `1.8` rounds
- `p50`: `2` rounds
- `p95`: `3` rounds
- min/max: `1` / `13` rounds

Throughput and overhead:
- recent throughput at end: `1.83` commits/round
- peak active traces: `528`
- peak active elections: `4769`
- delivered messages: `221,697,893`
- approximate delivered messages per committed block: `47,534`

Onboarding:
- late-join time to connected: average `26.6` rounds, `p95 30`
- late-join time to first known head: average `2.5` rounds
- late-join time to first sync trace: average `11.4` rounds
- late-join connected before head/sync: `0 / 0`

Rejoiners:
- time to connected: average `0.0` rounds
- time to first known head: average `2.2` rounds
- time to first sync trace: average `3.5` rounds
- rejoin connected before head/sync: `8 / 8`

## Interpretation

What worked:
- the combined system stayed connected throughout the run
- joiners integrated quickly even late in the scenario
- commit-chain sync remained active during growth and churn
- the simulated network itself stayed modest: sampled transit delay remained around `2` rounds with a short `p95`

What failed:
- the run was not throughput-stable at this offered load
- committed throughput stayed below the offered `3` transaction-blocks per round
- pending work accumulated over time
- the median stayed reasonably low, but the tail became very large late in the run

The most important reading is this:
- the network model is not the main reason for the long tail
- transport delay remained short
- the long tail came from queue growth and protocol/runtime saturation under sustained offered load

In other words, this run demonstrates functional viability of the combined design, but not stable performance at this load point.

## Rough Wall-Clock Translation

The simulator reports latency in rounds, not milliseconds. To make the numbers easier to read, here is a rough translation for several plausible scheduler cadences.

Overall final latency distribution:

| Metric | Rounds | 25 ms/round | 50 ms/round | 100 ms/round |
| --- | ---: | ---: | ---: | ---: |
| p50 | 20 | 0.50 s | 1.00 s | 2.00 s |
| avg | 113.1 | 2.83 s | 5.66 s | 11.31 s |
| p95 | 697 | 17.43 s | 34.85 s | 69.70 s |

Early healthy baseline at round `400`:

| Metric | Rounds | 25 ms/round | 50 ms/round | 100 ms/round |
| --- | ---: | ---: | ---: | ---: |
| p50 | 16 | 0.40 s | 0.80 s | 1.60 s |
| p95 | 21 | 0.53 s | 1.05 s | 2.10 s |

Late-stage stressed checkpoint at round `2160`:

| Metric | Rounds | 25 ms/round | 50 ms/round | 100 ms/round |
| --- | ---: | ---: | ---: | ---: |
| p50 | 19 | 0.48 s | 0.95 s | 1.90 s |
| p95 | 606 | 15.15 s | 30.30 s | 60.60 s |

These are only rough translations. They assume a future implementation chooses a stable service loop in that range. They are not measured wall-clock protocol latencies.

## Comparison to Other Distributed System Designs

Compared with leader-based replication systems such as Raft or Multi-Paxos:
- this run is clearly slower and much more message-heavy
- a healthy Raft-style system in controlled datacenter deployments usually commits in one or two quorum RTTs
- the fair trade is that Raft-class systems assume managed membership and do not target the same open churn and sync problem

Compared with permissioned BFT systems:
- the healthy early-phase latency here is in a similar rough order of magnitude if rounds map to tens of milliseconds
- the tail behavior here is currently worse under sustained load
- again, those systems usually operate with a fixed validator set and tighter admission control

Compared with open blockchain-style systems:
- the healthy median here is much better than minute-scale or multi-second finality systems
- but the current implementation still shows heavy long-tail degradation once backlog builds
- this makes the current result closer to "interactive when healthy, but not yet predictably bounded under sustained stress"

The honest conclusion is:
- the design looks more promising than slow-finality open systems on median responsiveness
- but it is not yet competitive with mature closed-membership replication systems on efficiency or latency stability

## Limits and Waivers

This report should not be read as a production benchmark.

Important limits:
- rounds are abstract and not yet calibrated to a production scheduler
- no real transport stack, bandwidth limit, kernel queueing, or retransmission behavior is modeled
- no database latency is included
- crypto cost is not fully represented in deployment form
- this is one fixed-seed scenario, not a statistical sweep
- stale rejoin admission is still optimistic because rejoiners can become `Connected` before full catch-up

## Assessment

This long run supports two conclusions:

1. The combined design is now testable in a meaningful integrated way.
   The simulator exercises genesis bootstrap, growth, churn, stale return, transaction flow, peer discovery, and commit-chain sync together.

2. The current implementation is not yet throughput-stable at this load point.
   It continues to function, but backlog growth causes the tail to become too large for a production-quality alternative.

That is still a valuable result. It means the next engineering question is no longer "does the combined design work at all?" but "what sustainable operating region can the current implementation support, and which subsystem is the dominant bottleneck?"
