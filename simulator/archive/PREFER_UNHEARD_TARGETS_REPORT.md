# Prefer-Unheard Targets Report

This report measures a soft vote-target ordering experiment on top of the delayed vote-reply
baseline:

- build the normal nearby vote target set
- prefer peers we have not yet recorded a vote from for that block
- fall back to already-heard peers when the preferred subset is too small

The goal was to reduce redundant vote traffic without changing the configured fanout.

## Scenario

Same fixed-seed lifecycle scenario as the delayed reply report:

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

## Comparison Against Delayed-Reply Baseline

### `cross_dc_normal`

| Metric | Delayed-reply baseline | Prefer-unheard ordering | Change |
| --- | ---: | ---: | ---: |
| Committed / pending | `3491 / 1309` | `3229 / 1571` | `-262` commits, `+262` pending |
| Delivered messages | `42,094,118` | `54,662,887` | `+29.9%` |
| Peak in-flight queue | `145,512` | `197,586` | `+35.8%` |
| Commit latency avg | `28.7` | `40.2` | `+40.1%` |
| Commit latency p50 | `16` | `19` | `+3` |
| Commit latency p95 | `106` | `189` | `+78.3%` |
| Block messages to settle avg | `2215.0` | `4145.6` | `+87.2%` |
| Total factor vs role-sum ideal | `13.66x` | `25.40x` | `+85.9%` |
| Total factor vs coalesced ideal | `20.55x` | `38.59x` | `+87.8%` |

### `cross_dc_stressed`

| Metric | Delayed-reply baseline | Prefer-unheard ordering | Change |
| --- | ---: | ---: | ---: |
| Committed / pending | `3351 / 1449` | `3303 / 1497` | `-48` commits, `+48` pending |
| Delivered messages | `59,486,050` | `60,948,271` | `+2.5%` |
| Peak in-flight queue | `371,743` | `382,440` | `+2.9%` |
| Commit latency avg | `50.0` | `55.9` | `+11.8%` |
| Commit latency p50 | `25` | `30` | `+5` |
| Commit latency p95 | `217` | `220` | `+1.4%` |
| Block messages to settle avg | `3769.6` | `5097.1` | `+35.2%` |
| Total factor vs role-sum ideal | `24.96x` | `33.37x` | `+33.7%` |
| Total factor vs coalesced ideal | `37.28x` | `50.39x` | `+35.2%` |

## Assessment

This experiment should **not** be kept.

The likely reason is that "already heard from" is not equivalent to "no longer useful":

- those peers may now have the block and be able to fast-reply
- deprioritizing them pushes requests toward peers that are less ready to help immediately
- the result is more spread, more queue growth, and worse settlement cost

That fits the measured changes:

- larger settled peer spread
- materially higher block-message cost
- worse tail latency

## Conclusion

The delayed vote-reply behavior is a good change.

This extra target-ordering heuristic is not.

If we want another pre-batching protocol improvement, it should probably focus on better
coalescing or bounded resend behavior rather than trying to infer too much from the current
heard/not-heard set.
