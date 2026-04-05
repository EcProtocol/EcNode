# Adaptive Neighborhood Report

This report tests the adaptive idea:

- keep a wide local neighborhood near the token
- narrow the neighborhood for far-away tokens
- only fast-reply on pending blocks when the node has prior token state

The purpose was to see whether proxy-path narrowing can reduce message work without losing the benefits of a healthy local neighborhood.

## Configuration

All runs used the same `1600`-round integrated long-run scenario under `cross_dc_normal`, fixed seed `0`.

Compared cases:

- fixed width `4`
- fixed width `6`
- adaptive `6 -> 2` beyond `8` hops
- adaptive `6 -> 2` beyond `16` hops

All runs used the new state-aware pending fast-reply rule:

- pending fast-reply now only happens if the node has prior state for every token in the block
- proxy-only nodes do not emit an immediate local vote just because they have seen the block

## Results

| Case | Commits | Pending | Messages / commit | Coverage avg | Eligible avg | Settled spread avg | Block-msg avg | Commit latency avg | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Fixed `4` | 3139 | 1661 | 31,696.5 | 18.7 | 9.0 | 61.7 | 12,371.8 | 75.9 | 19 | 534 |
| Fixed `6` | 3397 | 1403 | 12,735.8 | 26.0 | 12.9 | 61.1 | 3,468.8 | 39.9 | 18 | 192 |
| Adaptive `6 -> 2 @ 8 hops` | 2785 | 2015 | 68,970.3 | 25.6 | 7.2 | 65.0 | 20,849.8 | 118.1 | 23 | 546 |
| Adaptive `6 -> 2 @ 16 hops` | 2907 | 1893 | 46,727.5 | 25.9 | 9.2 | 63.2 | 12,713.7 | 85.7 | 21 | 531 |

## Main Read

The adaptive variants did **not** help in the current simulator.

Relative to fixed width `6`:

### `6 -> 2 @ 8 hops`

- commits `-18.0%`
- pending `+43.6%`
- messages per commit `+441.5%`
- average commit latency `+196.0%`
- p95 latency `+184.4%`

### `6 -> 2 @ 16 hops`

- commits `-14.4%`
- pending `+34.9%`
- messages per commit `+266.9%`
- average commit latency `+114.8%`
- p95 latency `+176.6%`

The softer threshold was better than the aggressive one, but still much worse than fixed `6`.

It also failed to beat fixed `4`:

- commits `-7.4%`
- messages per commit `+47.4%`
- average latency `+12.9%`

So on the current workload, adaptive narrowing is a clear regression.

## Why It Likely Failed

### 1. Far-away narrowing reduces decisive progress too early

The fixed-width sweep already showed that globally smaller neighborhoods are not cheaper here. Narrowing only the far path was supposed to preserve local decisiveness while reducing proxy churn.

But in practice, even the far-away path still seems to matter a lot for getting transactions into a state where decisive local progress can happen.

When that path is narrowed to `2`, the system spends longer circulating unresolved work.

### 2. State-aware fast-reply removes proxy optimism

This was an intentional part of the experiment, and I think it is the right semantics.

But it also means proxy nodes no longer contribute fast positive replies unless they really know the token state.

That sharply reduces early momentum for far-away traffic.

### 3. The current simulator workload is hostile to this rule

This matters a lot.

The integrated runner currently injects random token IDs with `last = 0`, which means many submitted blocks are effectively creating brand-new tokens rather than updating known ones.

Under that workload:

- very few nodes have prior state
- state-aware fast-reply is suppressed often
- adaptive far-path narrowing mostly removes proxy responses without replacing them with strong local knowledge

So this experiment is valid for the current simulator, but it is probably pessimistic for a workload dominated by updates to existing tokens.

## Assessment

For the current implementation and workload:

- fixed width `6` is still the best of the tested options
- adaptive `6 -> 2` does not look production-viable yet
- the state-aware fast-reply rule is still conceptually right, but it exposes that the current transaction generator is too biased toward unknown/new tokens

## Recommendation

Do **not** adopt the adaptive narrowing rule yet.

The next useful experiment is not another threshold tweak. It is a workload fix:

1. add a transaction mix that updates existing known tokens, not just random new ones
2. rerun fixed `6` and adaptive under that more realistic workload
3. only if adaptive improves there, consider more gradual policies like `6 -> 4` or multi-step narrowing instead of `6 -> 2`

## Notes

The state-aware fast-reply change itself should likely stay.

It is a cleaner rule:

- neighborhood nodes vote from knowledge
- proxy nodes route and fetch

The experiment suggests the weakness is not that rule, but that the current synthetic workload does not give the network enough state-bearing transactions to benefit from it.
