# Integrated Churn Conflict Report

This report compares two integrated lifecycle runs on the current code path:

- the vote-based peer update shortcut in `ec_node.rs` is disabled
- the current blocked-transition conflict update batch is enabled

The goal is to check two things at once:

1. whether the churn/lifecycle simulator still behaves reasonably without the earlier optimistic shortcut
2. what the current conflict-handling path looks like under real joins, crashes, returns, sync, and transaction flow

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

## Summary

| Case | Submitted | Committed | Pending | Wire Messages | Peak In-Flight | Avg Latency | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| no-conflict churn | `1800` | `1226` | `574` | `8.18M` | `45,273` | `24.4` | `16` | `47` |
| conflict churn | `2286` | `1284` | `1002` | `15.73M` | `99,730` | `22.6` | `14` | `47` |

Important context:
- the conflict run submits more blocks because each conflict family injects multiple contenders
- so the raw committed count is not enough by itself; pending load and queue depth matter more here

## Churn Behavior

### No-conflict baseline

- `136` total peers created, `122` active at end
- late-join time to connected: `26.9` rounds avg, `p95 35`
- rejoin time to connected: `20.5` rounds avg, `p95 22`
- recovery watches:
  - crash at round `300`: recovered in `2` rounds
  - crash at round `500`: recovered in `1` round

### Conflict churn

- `136` total peers created, `122` active at end
- late-join time to connected: `26.5` rounds avg, `p95 33`
- rejoin time to connected: `16.2` rounds avg, `p95 19`
- recovery watches:
  - crash at round `300`: recovered in `14` rounds
  - crash at round `500`: recovered in `64` rounds

### Reading The Churn Path

The important positive result is:

- both runs stayed alive through the full lifecycle scenario
- joiners and returners still integrated
- commit-chain sync stayed active
- transactions kept committing throughout the run

The important negative result is:

- conflict load makes recovery substantially slower
- the network heals, but it does not settle back toward the no-conflict operating region quickly enough

So the current lifecycle path is robust in the liveness sense, but still too “draggy” in the recovery sense.

## Conflict Outcomes

The conflict run created `486` conflict families.

Outcome metrics:

- `51` no-visible
- `389` single-visible
- `46` split
- `3` unanimous-highest
- `254` highest-majority
- `283` any-majority
- `203` stalled-no-majority
- `116` any-lower-visible
- `114` lower-owner-commit
- `52` multi-owner-commits

Signal metrics:

- average participant peers per family: `66.5`
- average signaled participants per family: `44.5`
- average signal coverage among participants: `0.60`

### Reading The Conflict Path Under Churn

This is weaker than the sparse steady-state conflict picture, which is expected.

What still looks good:

- many families do get a majority on the highest contender: `254`
- conflict information is still reaching a substantial share of participants
- the system does not collapse into runaway traffic or complete deadlock

What still looks weak:

- too many stalled families: `203`
- too many lower-owner commits: `114`
- too many multi-owner commits: `52`
- unanimous-highest is still rare: `3`

So under churn, the current conflict handling is functioning more as:

- conflict damping
- warning propagation
- partial majority formation

than as strong convergence on the intended contender.

## Network And Message Cost

No-conflict:

- logical messages delivered: `13.29M`
- wire messages delivered: `8.18M`
- delivered vote messages: `10.62M`
- block-related messages to settle: avg `1958.6`

Conflict:

- logical messages delivered: `25.96M`
- wire messages delivered: `15.73M`
- delivered vote messages: `17.19M`
- block-related messages to settle: avg `1564.7`

The message picture is interesting:

- conflict roughly doubled wire traffic and queue depth
- but it did not catastrophically explode
- p95 commit latency stayed flat at `47` rounds across both runs

That suggests the current system is still load-sensitive, but not brittle.

## Assessment

This comparison supports three conclusions.

1. Removing the earlier vote-based peer shortcut does not break the integrated lifecycle path.
   The network still forms and heals through joins, crashes, and returns.

2. The current conflict-handling path survives churn.
   Even under a very heavy conflict workload, the system continues processing transactions and the network remains functional.

3. Recovery and convergence are still the limiting problems.
   Under conflict, the network heals too slowly back toward steady-state behavior, and conflict families still do not converge strongly enough on the intended contender.

So the current system looks viable in the “does it keep working?” sense, but not yet in the “does it recover and converge cleanly enough?” sense.
