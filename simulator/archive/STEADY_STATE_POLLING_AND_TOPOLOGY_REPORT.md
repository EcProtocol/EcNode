# Steady-State Polling And Topology Report

This report compares two focused steady-state experiments on the current
simplified response-driven flow:

1. narrowing the proactive polling sweep from `4` active outward rounds to `3`
2. replacing the corrected ring-gradient startup graph with a fully probabilistic
   pairwise ring-closeness graph

All runs use:

- `500` rounds
- `192` peers
- `cross_dc_normal`
- `neighborhood_width = 6`
- `vote_target_count = 2`
- batching on, vote replies standalone
- no elections / no churn

## Commands

Baseline corrected ring, `4` active rounds, no conflict:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_ACTIVE_ROUNDS=4 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.0 \
cargo run --release --quiet --example integrated_steady_state
```

Baseline corrected ring, `4` active rounds, `25%` conflict families:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_ACTIVE_ROUNDS=4 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.25 \
EC_STEADY_STATE_CONFLICT_CONTENDERS=2 \
cargo run --release --quiet --example integrated_steady_state
```

Corrected ring, `3` active rounds, no conflict:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_ACTIVE_ROUNDS=3 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.0 \
cargo run --release --quiet --example integrated_steady_state
```

Corrected ring, `3` active rounds, `25%` conflict families:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_ACTIVE_ROUNDS=3 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.25 \
EC_STEADY_STATE_CONFLICT_CONTENDERS=2 \
cargo run --release --quiet --example integrated_steady_state
```

Probabilistic ring, `4` active rounds, no conflict:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring_probabilistic \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_ACTIVE_ROUNDS=4 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.0 \
cargo run --release --quiet --example integrated_steady_state
```

## Polling Sweep: `4` Active Rounds vs `3`

### Honest traffic

| Case | Committed | Pending | Wire Messages | Avg Latency | p50 | p95 | Role-Sum Factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ring, `4` active rounds | `1267` | `233` | `6.47M` | `10.7` | `10` | `13` | `4.22x` |
| ring, `3` active rounds | `1247` | `253` | `6.94M` | `11.2` | `10` | `14` | `4.55x` |

### Conflict traffic

| Case | Submitted | Committed | Pending | Wire Messages | Avg Latency | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ring, `4` active rounds | `1855` | `1241` | `614` | `7.88M` | `11.9` | `11` | `14` |
| ring, `3` active rounds | `1880` | `1198` | `682` | `8.19M` | `12.4` | `11` | `14` |

Conflict outcomes:

| Case | Highest-Majority | Stalled | Lower-Owner | Multi-Owner |
| --- | ---: | ---: | ---: | ---: |
| ring, `4` active rounds | `61` | `246` | `112` | `2` |
| ring, `3` active rounds | `86` | `248` | `106` | `0` |

### Reading the polling change

This looks mixed, but readable:

- On honest traffic, narrowing the sweep to `3` active rounds is a small regression.
  It commits slightly fewer blocks, uses more wire traffic, and is a bit slower.
- On conflict traffic, the narrower sweep improves some safety-shaped outcomes:
  higher-majority improves, lower-owner and multi-owner outcomes improve.
- But it still costs throughput and pending load.

So `3` active rounds is not a clean new default. It looks more like a conflict-damping trade:

- slightly better contention behavior
- slightly worse honest steady-state efficiency

## Topology: Corrected Ring vs Probabilistic Ring

The probabilistic topology is a separate experiment.

Assumption used here:

- pair connection probability is a linear **closeness** function on the 64-bit ring
- not raw distance, because raw distance would invert locality and favor far peers

### No-conflict steady-state

| Case | Committed | Pending | Wire Messages | Avg Latency | p50 | p95 | Role-Sum Factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| corrected ring | `1267` | `233` | `6.47M` | `10.7` | `10` | `13` | `4.22x` |
| probabilistic ring | `1320` | `180` | `9.70M` | `12.8` | `12` | `17` | `11.91x` |

### Graph-shape comparison

| Case | Avg Connected | Degree p50 / p95 | Gradient Target Fit | Fit p50 / p95 |
| --- | ---: | ---: | ---: | ---: |
| corrected ring | `23.0` | `23 / 25` | `0.973` | `0.973 / 0.978` |
| probabilistic ring | `95.6` | `96 / 107` | `0.603` | `0.602 / 0.662` |

Band view against the corrected ring target:

| Case | Core | Fade Actual | Fade Target | Far Leakage |
| --- | ---: | ---: | ---: | ---: |
| corrected ring | `1.000` | `0.497` | `0.500` | `0.000` |
| probabilistic ring | `0.949` | `0.879` | `0.500` | `0.423` |

### Reading the topology change

This is the important result:

- the probabilistic pairwise graph is much denser than the corrected ring target
- it keeps strong local connectivity, but also keeps far too many extra links
- that broadening hurts latency and message efficiency badly

So while the pairwise probabilistic construction may feel more organic, in this form it does
not approximate the intended gradient well enough. It over-connects.

The strongest evidence is:

- average degree rises from `23.0` to `95.6`
- target fit falls from `0.973` to `0.603`
- far leakage rises from `0.000` to `0.423`
- role-sum message factor jumps from `4.22x` to `11.91x`

## Assessment

At this point:

1. The corrected ring-gradient remains the better steady-state benchmark.
   It matches the intended shape and is much more efficient.

2. The `3`-round polling sweep is not a clear general improvement.
   It may be worth keeping as an experimental conflict-oriented mode, but not as the new default.

3. The probabilistic pairwise topology in this simple linear-closeness form is too dense.
   It is useful as a diagnostic, because it shows what over-connection does, but it is not a better target profile.
