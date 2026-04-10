# Vote Target Count Experiment

This compares vote fanout settings using the integrated long-run scenario.

The current threshold requires more than `+2` balance to settle a token or witness. That makes `2` vote targets structurally tight, so this experiment tests whether wider first-wave fanout improves latency enough to justify the extra traffic.

## Setup

- scenario: `integrated_long_run`
- network profile: `cross_dc_normal`
- neighborhood width: `6`
- workload: `50%` existing-token updates
- fixed seed variant: `0`

## Full Run: `2` vs `3` Targets

`1600` rounds, same scenario.

| Vote Targets | Committed / Pending | Delivered Messages | Peak In-Flight | Commit Latency avg / p50 / p95 | Total vs Role-Sum Ideal | Total vs Coalesced Ideal |
| --- | --- | ---: | ---: | --- | --- | --- |
| `2` | `3370 / 1430` | `52,627,037` | `184,130` | `37.9 / 16 / 193` | `19.38x` | `29.14x` |
| `3` | `3285 / 1515` | `84,989,948` | `296,879` | `36.8 / 14 / 199` | `33.09x` | `50.79x` |

### Read

- `3` targets slightly improved median latency.
- It did not improve sustained throughput.
- It increased delivered messages by about `61%`.
- It increased peak in-flight queue by about `61%`.
- It widened the reachable vote graph materially.

Conclusion: `3` targets is not a good sustained setting in the current implementation.

## Shorter Run: `2` vs `3` vs `4` Targets

`800` rounds, same scenario.

| Vote Targets | Committed / Pending | Delivered Messages | Peak In-Flight | Commit Latency avg / p50 / p95 | Total vs Role-Sum Ideal | Total vs Coalesced Ideal |
| --- | --- | ---: | ---: | --- | --- | --- |
| `2` | `1646 / 754` | `20,039,152` | `133,515` | `25.3 / 15 / 71` | `9.95x` | `16.64x` |
| `3` | `1637 / 763` | `20,328,202` | `145,345` | `22.0 / 13 / 54` | `11.69x` | `19.84x` |
| `4` | `1710 / 690` | `25,020,899` | `204,499` | `20.5 / 12 / 55` | `14.58x` | `24.36x` |

### Read

- `3` targets looks like a modest short-run latency improvement, but it still slightly underperformed `2` on committed blocks.
- `4` targets gave the best short-run service result:
  - highest commits
  - lowest average latency
  - much better p95 than `2`
- The cost of `4` targets is clear:
  - about `25%` more delivered messages than `2`
  - much higher queue growth
  - materially worse ideal-gap ratios

## Interpretation

1. The threshold mismatch is real.
   - `2` targets is structurally too tight for a `+2` threshold.

2. Simply widening fanout is not enough.
   - `3` and `4` can improve short-run latency, but they also expand the vote graph and raise the base traffic floor.

3. `4` targets is the more promising pairing with batching.
   - Without batching, `4` buys latency at a high traffic cost.
   - With batching, `4` may become viable because the first-wave information gain is higher while the transport cost could be amortized.

## Recommendation

- Do not keep `3` as the default.
- If we want to test a wider fanout path seriously, test `4` together with message batching rather than by itself.
