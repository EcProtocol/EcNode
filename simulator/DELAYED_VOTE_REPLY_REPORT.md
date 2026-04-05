# Delayed Vote-Reply Report

This report measures the effect of the delayed vote-reply path added in
[`src/ec_node.rs`](../src/ec_node.rs) and [`src/ec_mempool.rs`](../src/ec_mempool.rs):

- when a trusted peer votes for a block before we have the block content, we record that vote
- once the block arrives and we can compute our honest local vote, we send a one-shot reply back
  to those early voters with `reply = false`

The goal is to reduce avoidable extra vote rounds caused by the fetch-before-reply gap.

## Code Path

- early voters are collected from mempool state in
  [`src/ec_mempool.rs`](../src/ec_mempool.rs)
- delayed replies are emitted on useful block arrival in
  [`src/ec_node.rs`](../src/ec_node.rs)
- focused regression coverage is in
  [`src/ec_node.rs`](../src/ec_node.rs)

## Scenario

The measurements reuse the same fixed-seed lifecycle scenario as the long-run profile comparison:

- genesis bootstrap
- `1600` rounds
- `96` initial peers
- moderate join/crash/return churn
- neighborhood width `6`
- vote targets `2`
- existing-token workload target `50%`
- transaction source policy `ConnectedOnly`

Profiles compared:

- `cross_dc_normal`
- `cross_dc_stressed`

## Results

### `cross_dc_normal`

| Metric | Before | After delayed reply | Change |
| --- | ---: | ---: | ---: |
| Committed / pending | `3370 / 1430` | `3491 / 1309` | `+121` commits, `-121` pending |
| Delivered messages | `52,627,037` | `42,094,118` | `-20.0%` |
| Peak in-flight queue | `184,130` | `145,512` | `-21.0%` |
| Commit latency avg | `37.9` | `28.7` | `-24.3%` |
| Commit latency p50 | `16` | `16` | `0` |
| Commit latency p95 | `193` | `106` | `-45.1%` |
| Block messages to settle avg | `3123.8` | `2215.0` | `-29.1%` |
| Total factor vs role-sum ideal | `19.38x` | `13.66x` | `-29.5%` |
| Total factor vs coalesced ideal | `29.14x` | `20.55x` | `-29.5%` |

### `cross_dc_stressed`

| Metric | Before | After delayed reply | Change |
| --- | ---: | ---: | ---: |
| Committed / pending | `3337 / 1463` | `3351 / 1449` | `+14` commits, `-14` pending |
| Delivered messages | `64,878,831` | `59,486,050` | `-8.3%` |
| Peak in-flight queue | `354,666` | `371,743` | `+4.8%` |
| Commit latency avg | `56.9` | `50.0` | `-12.1%` |
| Commit latency p50 | `25` | `25` | `0` |
| Commit latency p95 | `272` | `217` | `-20.2%` |
| Block messages to settle avg | `4982.8` | `3769.6` | `-24.3%` |
| Total factor vs role-sum ideal | `32.66x` | `24.96x` | `-23.6%` |
| Total factor vs coalesced ideal | `48.95x` | `37.28x` | `-23.8%` |

## Assessment

This looks like a real improvement and should be kept.

Why:

- It improves both cost and latency at the same time in the normal profile.
- Under stressed network conditions it still reduces message amplification and tail latency.
- The only clearly worse stressed metric is peak in-flight queue, and that regression is small
  compared to the improvements in total messages and settlement cost.

The main benefit seems to be that blocks spend less time waiting for the first useful response
after the fetch gap. That reduces repeated vote solicitation without adding recursive ping-pong.

## What This Does Not Yet Justify

This result does **not** yet justify a more complex protocol where every later vote change triggers
another reply wave.

That would introduce more stateful behavior:

- remembering who has seen which vote version
- deciding when a vote change is important enough to rebroadcast
- avoiding oscillation and duplicate traffic under churn and delay

That may still be useful later, but the current result suggests a simpler next step:

- prefer vote targets we have not yet recorded a vote from for this block
- keep the normal neighborhood fallback when that preferred set is thin

That keeps the protocol easier to reason about while using the new delayed reply path to make
“silence” a more meaningful signal.
