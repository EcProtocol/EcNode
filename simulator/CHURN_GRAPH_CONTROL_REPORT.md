# Churn Graph Control Report

This report evaluates a first attempt at keeping the churned peer graph closer
to the corrected ring-gradient target.

The idea was:

- keep the current random / security-friendly flavor
- add a connected-degree target band
- prune only when clearly above that band
- optionally reduce self-started elections while above the band

## Baseline

Current simplified response-driven churn baseline, no conflict:

- `1091` committed, `709` pending
- wire messages `7.81M`
- latency `avg 22.1`, `p50 14`, `p95 42`
- active connected peers/node `64.2`
- target fit `0.575`
- far leakage `0.393`

This is the reference from the latest churn report.

## Experiment 1: Strong Target Band

Settings:

- connected target `24 ± 4`
- elections above high band: `0`
- prune protection time: `80`

Command:

```bash
EC_LONG_RUN_ROUNDS=600 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_CONNECTED_TARGET=24 \
EC_LONG_RUN_CONNECTED_HYSTERESIS=4 \
EC_LONG_RUN_ELECTIONS_WHEN_OVER_TARGET=0 \
EC_LONG_RUN_PRUNE_PROTECTION_TIME=80 \
cargo run --release --quiet --example integrated_long_run
```

Result:

- `1171` committed, `629` pending
- wire messages `9.67M`
- latency `avg 30.5`, `p50 20`, `p95 91`
- active connected peers/node `28.6`
- target fit `0.702`
- far leakage `0.188`

Reading:

- graph shape moved strongly toward the target
- but latency got much worse
- message cost got worse
- local core coverage stayed too low (`0.359`)

So this setting narrows the graph, but it narrows it in the wrong way.

## Experiment 2: Softer Target Band

Settings:

- connected target `28 ± 4`
- elections above high band: `1`
- prune protection time: `80`

Command:

```bash
EC_LONG_RUN_ROUNDS=600 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_CONNECTED_TARGET=28 \
EC_LONG_RUN_CONNECTED_HYSTERESIS=4 \
EC_LONG_RUN_ELECTIONS_WHEN_OVER_TARGET=1 \
EC_LONG_RUN_PRUNE_PROTECTION_TIME=80 \
cargo run --release --quiet --example integrated_long_run
```

Result:

- `1160` committed, `640` pending
- wire messages `8.70M`
- latency `avg 29.1`, `p50 19`, `p95 87`
- active connected peers/node `31.3`
- target fit `0.686`
- far leakage `0.214`

Reading:

- softer control still improves target fit a lot
- but it still hurts latency badly
- and it still fails to restore enough local core coverage (`0.386`)

So the softer version is directionally better than the strongest version,
but it has the same underlying problem.

## Main Conclusion

Degree control alone is not enough.

These experiments prove something useful:

- the churn graph can be pulled much closer to the corrected ring target
- but if we do that only by pruning and broad election throttling, we lose too much local support

The graph becomes:

- less broad
- less leaky
- but also too thin in the local core

That is why:

- target fit improves
- far leakage drops
- yet commit latency and message cost get worse

## What This Suggests Next

The next graph-control experiment should be more locality-aware.

Instead of mainly reducing degree, it should try to preserve or grow the local core while
rejecting or pruning surplus far links.

The most promising next direction is:

1. keep the shorter prune protection
2. keep far-biased pruning when over target
3. do **not** broadly suppress elections
4. make invitation acceptance stricter for far peers when above target
5. keep accepting or even favoring near invitations so the local core can refill

That would test the more important rule:

- not just “fewer peers”
- but “the right peers”

## Experiment 3: Locality-Aware Invitations Above Target

This experiment keeps the same target band and shorter prune protection, but stops
blunt election throttling. Instead, it changes invitation handling while above the
high side of the target band:

- near peers in the current active-ring ordering are still accepted with high probability
- fade-band peers are capped to a linear fade probability
- far peers are allowed only through a very small tail probability

So the control strategy becomes:

- prune far-biased when over target
- keep elections running
- make growth choose more local peers

### Settings

- connected target `24 ± 4`
- elections above high band: default (`3`)
- prune protection time: `80`

Command, no conflict:

```bash
EC_LONG_RUN_ROUNDS=600 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_CONNECTED_TARGET=24 \
EC_LONG_RUN_CONNECTED_HYSTERESIS=4 \
EC_LONG_RUN_PRUNE_PROTECTION_TIME=80 \
cargo run --release --quiet --example integrated_long_run
```

Command, `25%` conflict families:

```bash
EC_LONG_RUN_ROUNDS=600 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_CONFLICT_FAMILY_FRACTION=0.25 \
EC_LONG_RUN_CONFLICT_CONTENDERS=2 \
EC_LONG_RUN_CONNECTED_TARGET=24 \
EC_LONG_RUN_CONNECTED_HYSTERESIS=4 \
EC_LONG_RUN_PRUNE_PROTECTION_TIME=80 \
cargo run --release --quiet --example integrated_long_run
```

### No-Conflict Result

- `1274` committed, `526` pending
- wire messages `8.57M`
- latency `avg 27.1`, `p50 19`, `p95 74`
- active connected peers/node `33.3`
- target fit `0.683`
- far leakage `0.221`
- core coverage `0.346`
- recovery after crashes: `2` rounds, `1` round

Compared with the earlier graph-control attempts:

- still broader than the ideal (`33.3` vs ideal `23.0`)
- much closer to the target than the baseline broad graph
- far healthier than the pruning-plus-election-throttle variants

This is the important shift:

- the graph stays shaped
- but the local core no longer collapses
- recovery stays fast
- compared with the original broad churn baseline, commit latency is still higher

### Conflict Result

- `1340` committed, `928` pending
- wire messages `8.85M`
- latency `avg 25.4`, `p50 19`, `p95 60`
- active connected peers/node `33.5`
- target fit `0.679`
- far leakage `0.224`
- core coverage `0.333`
- highest-majority `244`
- stalled-no-majority `193`
- lower-owner-commit `73`
- multi-owner-commits `25`
- conflict signal coverage among participants `0.68`
- recovery after crashes: `1` round, `3` rounds

This shows the same basic pattern under contention:

- graph shape stays materially better than the original churn baseline
- liveness and recovery remain good
- conflict handling does not collapse under the narrower graph
- but conflict convergence is not better than the broad baseline

### Readout

This is the first graph-control variant that looks directionally right.

Compared with the no-control churn baseline:

- connected degree drops from about `64` to about `33`
- target fit rises from about `0.58` to about `0.68`
- far leakage drops from about `0.39` to about `0.22`
- committed work rises, but commit latency also rises

Compared with the earlier target-band-only runs:

- local core coverage stays materially healthier
- latency and wire cost recover sharply
- crash recovery remains quick

So the next problem is no longer “how do we control degree at all?” It is:

- how do we continue improving locality from this better starting point
- without paying back too much in recovery or conflict convergence

## Updated Conclusion

Pure degree control was too blunt. Locality-aware invitation control is much better.

It does not fully recover the corrected ring target yet:

- the graph is still broader than ideal
- core coverage is still lower than we want

But it moves the churned network toward the target shape while keeping the protocol healthy.

That makes it a much better basis for the next round of tuning than the earlier
“prune hard and suppress elections” strategy.

The remaining gap is now easier to state:

- shape is better
- liveness is still good
- but we have not yet recovered the broad-baseline latency or conflict convergence

So the next tuning should focus on preserving more local core density and routing quality,
not on stronger degree reduction alone.

## Experiment 4: Core-Aware Invitation Bias

This follow-up keeps the locality-aware invitation policy, but adds one more signal:

- estimate how well the nearest known peers around this node are already filled by
  connected peers
- if that local core is underfilled, raise the acceptance floor for near invitations
  even more aggressively
- keep fade/far acceptance tight

The goal was to preserve more local density without letting the graph flatten back out.

### Settings

Same as Experiment 3:

- connected target `24 ± 4`
- elections above high band: default (`3`)
- prune protection time: `80`

### No-Conflict Result

- `1281` committed, `519` pending
- wire messages `8.25M`
- latency `avg 26.9`, `p50 19`, `p95 80`
- active connected peers/node `30.6`
- target fit `0.700`
- far leakage `0.198`
- core coverage `0.337`
- recovery after crashes: `8` rounds, `2` rounds

Compared with Experiment 3:

- graph shape improves again
- wire messages fall again
- average latency is nearly flat
- but p95 latency worsens and recovery becomes less stable

### Conflict Result

- `1287` committed, `960` pending
- wire messages `8.48M`
- latency `avg 27.1`, `p50 19`, `p95 78`
- active connected peers/node `29.6`
- target fit `0.702`
- far leakage `0.197`
- core coverage `0.342`
- highest-majority `213`
- stalled-no-majority `198`
- lower-owner-commit `77`
- multi-owner-commits `20`
- conflict signal coverage among participants `0.65`
- recovery after crashes: `56` rounds, `1` round

Compared with Experiment 3:

- graph shape improves again
- wire traffic drops again
- multi-owner outcomes improve a bit
- but highest-majority falls, stalled families rise slightly, and the first crash recovery gets much worse

### Readout

This variant pushes the graph in the right geometric direction:

- lower degree
- better target fit
- lower far leakage
- lower wire cost

But it also shows the limit of this control style:

- the graph can become cleaner while the protocol gets less forgiving after shocks
- core coverage did not improve enough to justify the recovery regression

So this is a useful boundary result. It suggests:

- we can tighten the graph further
- but not safely by invitation bias alone

The next meaningful work should probably shift from stronger graph shaping to better
local-core preservation and post-shock recovery behavior.

## Experiment 5: Core-Preserving Pruning

This variant backs invitation handling off to the earlier locality-aware policy and
moves the stronger locality control into pruning instead:

- near invitations use the simpler locality-aware acceptance again
- pruning above the target band explicitly prefers far peers over fade peers
- core peers get extremely low prune weight
- if the local core is underfilled, core peers are not pruned at all

The goal was to keep the graph steep without paying for it in admission-side recovery loss.

### Settings

Same as Experiments 3 and 4:

- connected target `24 ± 4`
- elections above high band: default (`3`)
- prune protection time: `80`

### No-Conflict Result

- `1222` committed, `578` pending
- wire messages `7.96M`
- latency `avg 25.8`, `p50 18`, `p95 76`
- active connected peers/node `32.3`
- target fit `0.722`
- far leakage `0.190`
- core coverage `0.445`
- recovery after crashes: `1` round, `1` round

Compared with Experiment 3:

- shape is better across the board
- wire traffic is lower
- average latency is slightly better
- recovery is better
- throughput is lower

Compared with Experiment 4:

- shape remains strong
- recovery becomes healthy again
- p95 latency improves

### Conflict Result

- `1313` committed, `955` pending
- wire messages `8.90M`
- latency `avg 30.0`, `p50 18`, `p95 87`
- active connected peers/node `32.0`
- target fit `0.725`
- far leakage `0.188`
- core coverage `0.457`
- highest-majority `222`
- stalled-no-majority `218`
- lower-owner-commit `75`
- multi-owner-commits `27`
- conflict signal coverage among participants `0.67`
- recovery after crashes: `1` round, `2` rounds

Compared with Experiment 3:

- graph shape is materially better
- recovery is also better
- but conflict convergence is not better, and latency is worse

Compared with Experiment 4:

- graph shape stays excellent
- recovery improves a lot
- conflict metrics are in a similar band, with some small tradeoffs either way

### Readout

This is the best graph-control result so far if we care about:

- keeping the live graph closer to the corrected gradient target
- preserving fast crash recovery
- avoiding the admission-side over-tightening of Experiment 4

It gives us a better operating point:

- degree still well below the original broad churn baseline
- target fit around `0.72`
- far leakage around `0.19`
- core coverage materially better than the earlier locality-aware run
- recovery back to `1/1` or `1/2`

The remaining weakness is now clearer:

- conflict convergence is still not improving with graph shape alone
- and conflict-heavy latency still remains too high

So the graph-control side is finally in a solid place to support the next layer of work.
The next improvement is unlikely to come from shaping the peer graph harder again.
It is more likely to come from conflict- and vote-flow behavior on top of this healthier graph.
