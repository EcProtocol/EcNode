# Peer Lifecycle Graph Shape Report

This note records the first peer-formation-only experiments after wiring
`simulator/peer_lifecycle/runner.rs` to report fixed-network shape metrics.

The question is deliberately narrower than the integrated simulator:

- can peers discover a broad set of candidates from referrals
- can a node then prune toward the intended fixed topology shape
- does churn repair preserve the core without retaining too much far leakage

No transactions are run in the peer-lifecycle simulator. A promising result is
only a reason to test the same policy in the integrated runner.

## Mechanism

The formation model is:

1. peers mine identity blocks with Argon2 proof-of-work, producing evenly spread
   peer IDs or "lampposts" on the token ring
2. genesis gives early peers shared aligned storage, so proof-of-storage
   challenges have a common base
3. random missing-token queries return referrals near many ring positions,
   letting a node learn much more of the graph than it will retain
4. graph control should prune from that learned set toward the fixed-network
   shape: dense local core, fading neighbor probability, and a small far tail

The fixed-network reports that motivate the target are:

- `simulator/FIXED_NETWORK_CONFLICT_LINEAGE_REPORT.md`
- `simulator/FIXED_NETWORK_EXTENSION_STEADY_REPORT.md`

Those runs used a dense linear probability shape, not a small fixed degree:
guaranteed neighbors around the local ring, center probability `1.0`, and far
probability `0.2`.

## Setup

Shared lifecycle settings:

- `96` initial peers
- `240` rounds
- seed variant `7`
- random-identified bootstrap with `6` starting peers per node
- `200,000` random tokens
- `90%` token coverage
- `12` peers crash at round `110`
- `12` peers return at round `140`

Baseline command:

```bash
EC_PEER_LIFECYCLE_SEED_VARIANT=7 \
EC_PEER_LIFECYCLE_INITIAL_PEERS=96 \
EC_PEER_LIFECYCLE_ROUNDS=240 \
cargo run --release --quiet --example peer_lifecycle_sim
```

Dense-shape command:

```bash
EC_PEER_LIFECYCLE_SEED_VARIANT=7 \
EC_PEER_LIFECYCLE_INITIAL_PEERS=96 \
EC_PEER_LIFECYCLE_ROUNDS=240 \
EC_PEER_LIFECYCLE_DENSE_SHAPE_TARGET=true \
EC_PEER_LIFECYCLE_DENSE_SHAPE_NEIGHBORS=10 \
EC_PEER_LIFECYCLE_DENSE_SHAPE_FAR_PROB=0.2 \
EC_PEER_LIFECYCLE_DENSE_SHAPE_HYSTERESIS=4 \
EC_PEER_LIFECYCLE_PRUNE_PROTECTION_TIME=80 \
cargo run --release --quiet --example peer_lifecycle_sim
```

## Lifecycle Results

The earlier `24 ± 4` target was useful as a diagnostic, but it is not the target
shape from the fixed-network reports. On a `96`-peer lifecycle run, the dense
linear target has an expected connected degree of about `58.4`, not `24`.

| Case | Peers | Avg connected | Dense ideal | Dense fit | Core | Fade | Fade target | Far | Messages |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| no target | `96` | `64.1` | `58.4` | `0.587` | `0.763` | `0.778` | `0.750` | `0.610` | `683,702` |
| fixed degree `60 ± 8` | `96` | `60.2` | `58.4` | `0.575` | `0.718` | `0.711` | `0.750` | `0.579` | `681,725` |
| dense-shape pruning | `96` | `54.8` | `58.4` | `0.578` | `0.721` | `0.687` | `0.750` | `0.491` | `694,734` |
| dense-shape pruning + core refill | `96` | `58.0` | `58.4` | `0.606` | `0.868` | `0.685` | `0.750` | `0.497` | `670,710` |

The lifecycle reading is:

- discovery is broad enough; the unrestricted graph knows and keeps many peers
- fixed degree control is too blunt, even when the degree is set near the dense
  target expectation
- shape-aware pruning reduces far retention, but by itself can starve the local
  guaranteed band
- adding a core-refill election bias is the first change that moves the graph in
  the intended direction: the local core improves from `0.763` to `0.868`, total
  messages fall slightly, and the connected degree lands near the dense target

The remaining miss is fade quality. The best lifecycle variant has a strong
core and lower far leakage, but fade coverage is still below the dense-linear
target. That matters because the fixed-network latency results come from the
whole probability shape, not just from keeping a good immediate core.

## Discovery And Hole Diagnostics

`simulator/peer_lifecycle/runner.rs` now prints two extra diagnostics at every
`ReportStats` checkpoint.

The first diagnostic simulates random missing-token probing without mutating the
simulation:

1. pick a random token id
2. ask a random active peer
3. follow referral suggestions up to depth `5`
4. record the active peers reached
5. repeat for `20`, `100`, and `500` probes

On the dense-shape lifecycle run above, a single probe usually reaches only a
small local slice, but repeated probes cover the graph:

| Checkpoint | Active peers | 20 probes cumulative | 100 probes cumulative | 500 probes cumulative | Per-probe avg |
| --- | ---: | ---: | ---: | ---: | ---: |
| bootstrap | `96` | `56` (`58.3%`) | `95` (`99.0%`) | `96` (`100.0%`) | `4.5-4.9` |
| mid-simulation | `96` | `50` (`52.1%`) | `94` (`97.9%`) | `96` (`100.0%`) | `4.3-4.4` |
| after crash | `84` | `47` (`56.0%`) | `80` (`95.2%`) | `84` (`100.0%`) | `3.8-3.9` |
| after return | `96` | `56` (`58.3%`) | `95` (`99.0%`) | `96` (`100.0%`) | `4.2` |

This supports the discovery thesis: the referral graph is globally discoverable
from random probing, even under churn. The issue is not that the graph is
unreachable from random entry points. It is that each individual random probe is
very narrow, so discovery must be batched or scheduled carefully if used by the
integrated runner.

The second diagnostic samples a few active peers and compares their retained
peer set against the fixed dense-linear target (`center_prob=1.0`,
`far_prob=0.2`, `guaranteed_neighbors=10`). For each sampled peer it prints:

- connected count vs dense ideal
- rank-band coverage for `1-10`, `11-20`, `21-40`, and `41-max`
- missing guaranteed-core ranks
- missing high-probability fade ranks
- far excess ranks

The dry-run output confirms that even when the average connected count is close
to the dense ideal, individual nodes still have core holes and far excess. For
example, after the return wave, sampled peers had only `14/20`, `17/20`, and
`17/20` connected in the guaranteed rank `1-10` band, while still retaining
`5-9` peers in the farthest rank band. That is the concrete shape error to fix.

## Integrated Sanity Check

After the lifecycle-only sweep, short `300`-round integrated runs tested the
same dense-shape policy with transaction traffic, vote collection, batching, and
churn enabled. These runs end with `136` total peers and `122` active peers, so
the dense-linear expected active degree is about `73.6`.

Integrated baseline command:

```bash
EC_LONG_RUN_ROUNDS=300 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_long_run
```

Integrated dense-shape command:

```bash
EC_LONG_RUN_ROUNDS=300 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_DENSE_SHAPE_TARGET=true \
EC_LONG_RUN_DENSE_SHAPE_NEIGHBORS=10 \
EC_LONG_RUN_DENSE_SHAPE_FAR_PROB=0.2 \
EC_LONG_RUN_DENSE_SHAPE_HYSTERESIS=4 \
EC_LONG_RUN_PRUNE_PROTECTION_TIME=80 \
cargo run --release --quiet --example integrated_long_run
```

| Case | Committed | Pending | Wire messages | Peak queue | Avg latency | p95 | Active connected | Dense ideal | Dense fit | Core | Fade | Far |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| no target | `776` | `124` | `2.44M` | `20,363` | `12.0` | `40` | `56.0` | `73.6` | `0.521` | `0.527` | `0.504` | `0.439` |
| dense-shape pruning + core refill | `789` | `111` | `2.39M` | `19,154` | `13.7` | `45` | `45.7` | `73.6` | `0.501` | `0.477` | `0.403` | `0.348` |

Older fixed-target checks are now best read as negative controls. The `24 ± 4`
policy reduced the old far-leakage diagnostic, but it also increased wire
traffic and queue depth. That was a sign it was making the graph cleaner but too
thin for the integrated workload.

The dense-shape policy is less harmful than the fixed-count policy:

- commit count improves from `776` to `789`
- pending work falls from `124` to `111`
- wire messages and peak queue fall slightly
- far retention improves from `0.439` to `0.348`

But it still does not form the fixed-network graph. It undershoots the dense
target badly: `45.7` active connected peers against an expected `73.6`, with
core coverage only `0.477` and fade coverage `0.403`. That matches the concern
that the peer sets are not dense enough for the fixed-network latency story.

## Next Step

The promising direction is not tighter degree control. It is better shape
formation:

- keep the core-refill bias
- improve core and fade-band admission instead of treating all non-core peers
  similarly
- keep dense-linear shape metrics in the integrated runner, because the older
  corrected-ring diagnostic can make the wrong policy look better
- then rerun the integrated churn/conflict scenario and compare latency,
  pending work, wire messages, and dense-linear shape fit together

## Integrated Follow-Up

The next integrated-only experiments tested two formation ideas directly against
latency and wire pressure.

### Random Referral Discovery

Command shape:

```bash
EC_LONG_RUN_RANDOM_DISCOVERY_ELECTIONS=1 \
EC_LONG_RUN_DENSE_SHAPE_TARGET=true \
EC_LONG_RUN_DENSE_SHAPE_NEIGHBORS=10 \
EC_LONG_RUN_DENSE_SHAPE_FAR_PROB=0.2 \
EC_LONG_RUN_PRUNE_PROTECTION_TIME=80 \
cargo run --release --quiet --example integrated_long_run
```

Random missing-token exploration validated the discovery thesis but hurt the
viability surface:

| Case | Active connected | Dense fit | Known peers | Committed | Pending | Wire | Avg latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dense shape, core `10` | `45.7` | `0.501` | `48.6` | `789` | `111` | `2.39M` | `13.7` | `45` |
| + random discovery always | `60.4` | `0.523` | `65.8` | `731` | `169` | `2.55M` | `19.0` | `62` |
| + random discovery until round `80` | `60.4` | `0.543` | `64.3` | `759` | `141` | `2.38M` | `18.8` | `60` |

So random non-existing-token lookups do teach more of the graph, but doing them
inside the transaction path creates too much election/referral and follow-up
pressure. Even a startup-only window leaves the later transaction flow worse in
this setup.

### Wider Core Admission

The more promising change was to widen the guaranteed core in the shape policy
without adding extra discovery traffic:

| Case | Active connected | Dense fit | Committed | Pending | Wire | Peak queue | Avg latency | p95 | Block msgs avg |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| no target | `56.0` | `0.521` | `776` | `124` | `2.44M` | `20,363` | `12.0` | `40` | `673.4` |
| dense shape, core `10` | `45.7` | `0.501` | `789` | `111` | `2.39M` | `19,154` | `13.7` | `45` | `685.9` |
| dense shape, core `20` | `50.0` | `0.505` | `774` | `126` | `2.41M` | `22,401` | `12.4` | `39` | `619.6` |
| dense shape, core `30` | `54.8` | `0.514` | `768` | `132` | `2.45M` | `23,431` | `12.9` | `40` | `655.4` |

Core `20` is the best result in this small sweep. It does not maximize the
dense-linear fit metric, but it improves the actual block-message factor and
keeps latency near the no-target baseline while still applying some shape
pressure. Core `30` starts to look too broad again.

The current working hypothesis is therefore:

- raw random discovery is too expensive unless it can be scheduled outside hot
  transaction flow or made adaptive to recovery windows
- the integrated runner needs shape sweeps judged by latency/message load first
  and dense-fit second
- the next useful integrated candidate is a core around `20`, plus a more
  selective fade policy that avoids increasing far traffic

### Peer-ID-Only Election Experiment

The follow-up clean-discovery experiment separated referral discovery from
connection elections:

- arbitrary token ids are used only for referral probes
- connection elections are only started for known peer ids
- answer/signature tokens are not used as general election fuel in this mode
- random fallback election tokens are disabled

This better matches the identity-block thesis: peer ids are valid answerable
tokens, while random positions are only a way to learn candidate peer ids.

Command shape:

```bash
EC_LONG_RUN_PEER_ID_ELECTION_ONLY=true \
EC_LONG_RUN_REFERRAL_PROBES_PER_TICK=1 \
EC_LONG_RUN_REFERRAL_PROBE_HOPS=5 \
EC_LONG_RUN_LOCAL_DISCOVERY_TARGET=100 \
EC_LONG_RUN_DENSE_SHAPE_TARGET=true \
EC_LONG_RUN_DENSE_SHAPE_NEIGHBORS=20 \
EC_LONG_RUN_DENSE_SHAPE_FAR_PROB=0.2 \
EC_LONG_RUN_DENSE_SHAPE_HYSTERESIS=6 \
EC_LONG_RUN_PRUNE_PROTECTION_TIME=80 \
cargo run --release --quiet --example integrated_long_run
```

| Case | Active connected | Dense fit | Committed | Pending | Wire | Peak queue | Avg latency | p95 | Referrals |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| core `20`, mixed discovery | `50.0` | `0.505` | `774` | `126` | `2.41M` | `22,401` | `12.4` | `39` | `114k` |
| peer-id only, 1 probe/tick | `47.9` | `0.496` | `746` | `154` | `2.56M` | `23,462` | `23.7` | `76` | `183k` |
| peer-id only, 3 probes/tick | `49.0` | `0.504` | `720` | `180` | `2.75M` | `24,968` | `24.0` | `75` | `263k` |

This is a useful negative result. The cleaner model discovers peer ids and keeps
elections answerable, but the current referral-probe scheduler is not viable in
the integrated workload. It forms too slowly early, then spends too much wire
traffic on referral walks while transactions are active.

The separation is still conceptually useful, but the scheduler needs to become
more selective before it is competitive:

- probe only when a measured shape band has candidate holes
- stop probing once the local candidate set is saturated
- avoid fixed per-tick probing during transaction pressure
- prefer probe batches during join/recovery windows rather than continuous
  background exploration
