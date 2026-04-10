# Integrated Churn Gradient Report

This report reruns the integrated lifecycle scenario on the current simplified
response-driven protocol and adds a clearer read on peer-set shape under churn.

The goal for this round was twofold:

1. check how the latest protocol behaves under joins, crashes, returns, sync, and transaction flow
2. quantify how close the live peer graph stays to the corrected ring-gradient target over time

## Scenario

Code:
- `simulator/integrated_long_run.rs`

Shared settings:
- `600` rounds
- genesis-backed bootstrap
- `96` initial peers
- `24` joins at round `120`
- `12` crashes at round `300`
- `8` returns at round `350`
- `16` joins at round `420`
- `10` crashes at round `500`
- network profile: `cross_dc_normal`
- neighborhood width: `4`
- vote targets: `2`
- batching on, vote replies standalone
- existing-token workload target: `50%`
- transaction source policy: `connected-only`

Compared runs:
- no-conflict baseline
- conflict workload with `25%` of slots replaced by `2`-contender families

## Commands

No-conflict churn baseline:

```bash
EC_LONG_RUN_ROUNDS=600 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_long_run
```

Conflict churn run:

```bash
EC_LONG_RUN_ROUNDS=600 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_CONFLICT_FAMILY_FRACTION=0.25 \
EC_LONG_RUN_CONFLICT_CONTENDERS=2 \
cargo run --release --quiet --example integrated_long_run
```

## Gradient Metrics

The churn runner now reports two kinds of graph-shape metrics:

- `gradient locality`: the older scalar that asks whether connected peers are numerically near the node on the ring
- `target fit`: a stricter comparison against the corrected ring-gradient target on the same active peer set

The target-fit view also breaks the live graph into three distance bands:

- `core`: the guaranteed local band of the corrected ring target
- `fade`: the linear fade-out band just outside the core
- `far`: everything beyond the target fade band

For this scenario the corrected ring target implies an expected active degree of about `23.0`
peers per node once the active set is large enough.

The ideal shape would look roughly like:

- core coverage close to `1.0`
- fade coverage close to the target `0.50`
- far leakage close to `0.0`

## Final Summary

| Case | Submitted | Committed | Pending | Wire Messages | Peak In-Flight | Avg Latency | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| no-conflict churn | `1800` | `1091` | `709` | `7.81M` | `43,978` | `22.1` | `14` | `42` |
| conflict churn | `2246` | `1255` | `991` | `8.08M` | `41,510` | `21.6` | `15` | `43` |

Important context:

- the conflict run submits more blocks because each conflict family injects multiple contenders
- recovery is now much healthier than in earlier churn runs, so end-of-run pending load matters more than simple crash-recovery time

## Gradient Shape Over Time

### No-conflict churn

| Stage | Active | Avg Connected | Active Connected | Ideal | Target Fit | Core | Fade | Far |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| round `100` early baseline | `96` | `37.2` | `37.2` | `23.0` | `0.598` | `0.49` | `0.43` | `0.358` |
| round `200` post-growth-1 | `120` | `46.1` | `46.1` | `23.0` | `0.599` | `0.47` | `0.45` | `0.362` |
| round `540` late-stage | `122` | `68.1` | `61.4` | `23.0` | `0.524` | `0.59` | `0.57` | `0.484` |
| final snapshot | `122` | `69.3` | `64.2` | `23.0` | `0.517` | `0.627` | `0.617` | `0.501` |

Time-averaged shape values available from the run:

- average core coverage: `0.491`
- average far leakage: `0.393`

### Conflict churn

| Stage | Active | Avg Connected | Active Connected | Ideal | Target Fit | Core | Fade | Far |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| round `100` early baseline | `96` | `37.0` | `37.0` | `23.0` | `0.595` | `0.48` | `0.44` | `0.356` |
| round `200` post-growth-1 | `120` | `46.7` | `46.7` | `23.0` | `0.599` | `0.48` | `0.47` | `0.365` |
| round `540` late-stage | `122` | `69.7` | `63.3` | `23.0` | `0.520` | `0.62` | `0.59` | `0.496` |
| final snapshot | `122` | `70.5` | `66.0` | `23.0` | `0.511` | `0.653` | `0.631` | `0.513` |

Time-averaged shape values available from the run:

- average core coverage: `0.495`
- average far leakage: `0.395`

## Reading The Gradient Results

This is the clearest result from the round.

The churn path is not failing to connect. It is staying too broad.

Across both runs:

- average connected degree stays around `3x` the corrected ring target
- far leakage climbs to about `0.50` by the final snapshot
- target fit falls from about `0.60` early to about `0.52` late

So the live graph under churn is flattening instead of settling into the intended steep local shape.

That is good news in one sense:

- we are not struggling because the graph is too sparse or too fragmented
- dropping or spacing elections in over-dense regions is a much easier problem than trying to invent missing connectivity

It also gives us a concrete next lever:

- make the election / retention / pruning rules preserve liveness without keeping so many far-away connections alive

## Churn Behavior

### No-conflict churn

- `136` total peers created, `122` active at end
- late-join time to connected: `27.0` rounds avg, `p95 35`
- rejoin time to connected: `18.5` rounds avg, `p95 21`
- recovery watches:
  - crash at round `300`: recovered in `1` round
  - crash at round `500`: recovered in `1` round

### Conflict churn

- `136` total peers created, `122` active at end
- late-join time to connected: `26.2` rounds avg, `p95 33`
- rejoin time to connected: `20.8` rounds avg, `p95 24`
- recovery watches:
  - crash at round `300`: recovered in `1` round
  - crash at round `500`: recovered in `1` round

### Reading The Churn Path

This is a real improvement over the earlier churn runs.

What looks good now:

- the network still forms and heals through joins, crashes, and returns
- both crash waves recovered quickly
- commit-chain sync stayed active
- the conflict workload did not blow the run up

What is still weak:

- the network heals by staying over-connected
- latency under churn is still about `2x` the corrected steady-state ring
- pending load still builds late in the run as the graph broadens

So the latest protocol looks much healthier operationally, but the graph-maintenance policy is still leaving efficiency on the table.

## Conflict Outcomes

The conflict run created `446` conflict families.

Outcome metrics:

- `49` no-visible
- `354` single-visible
- `43` split
- `1` unanimous-highest
- `241` highest-majority
- `274` any-majority
- `172` stalled-no-majority
- `93` any-lower-visible
- `53` lower-owner-commit
- `12` multi-owner-commits

Signal metrics:

- average participant peers per family: `60.2`
- average signaled participants per family: `38.0`
- average signal coverage among participants: `0.56`

### Reading The Conflict Path Under Churn

This is still not “strong convergence”, but it is a workable liveness profile.

What looks good:

- highest-majority families: `241`
- lower-owner commits dropped to `53`
- multi-owner commits dropped to `12`
- the system stayed stable under a very heavy conflict workload

What still looks incomplete:

- stalled families remain common: `172`
- unanimous-highest is still rare: `1`
- the protocol is damping conflict better than it is steering it to the intended highest contender

So under churn, the current conflict handling is functioning as:

- conflict warning propagation
- damping of bad contenders
- partial highest-majority formation

more than as deterministic convergence on the highest contender.

## Assessment

This round supports three conclusions.

1. The latest simplified response-driven protocol survives the full churn path well.
   Recovery after the crash waves is now quick again in both the no-conflict and conflict runs.

2. The main remaining lifecycle drag is graph shape, not missing connectivity.
   The churned network stays far broader than the corrected ring-gradient target, especially in the far band.

3. Conflict handling is improving in the right direction.
   The system stays live, lower-owner and multi-owner outcomes are materially better than earlier churn results, but strong highest-id convergence is still not there.

So the next useful work is not to add more raw connectivity. It is to keep the graph steeper under churn by reducing over-dense far links without hurting recovery.
