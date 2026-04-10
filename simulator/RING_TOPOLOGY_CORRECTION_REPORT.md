# Ring Topology Correction Report

This note captures the correction to `TopologyMode::Ring` in the steady-state and peer-lifecycle simulators, and the first rerun of the steady-state baseline after that correction.

## What Was Wrong

The previous `TopologyMode::Ring { neighbors }` did **not** model the intended gradient-shaped peer set:

- in the integrated steady-state harness it built a fixed nearest-neighbor ring
- in the peer-lifecycle runner it only added those peers as `Identified`
- the two runners therefore disagreed on startup semantics

That was not the target design.

## Corrected Semantics

`TopologyMode::Ring { neighbors }` now means:

- order peers by ring position (`PeerId`)
- connect the closest `±neighbors` peers with probability `1.0`
- connect the next `±neighbors` peers with a linear fade to `0.0`
- make every selected edge symmetric and initially `Connected`

For `neighbors = 8`, that gives:

- guaranteed local core: `±8`
- fading tail: `±9 .. ±16`
- no initial edges beyond `±16`

This is not the fully organic election-formed graph, but it is much closer to the intended ideal than the old hard `±8` ring.

## Validation

Build validation:

```bash
cargo test -q --bins --examples --no-run
```

Steady-state no-conflict rerun:

```bash
EC_STEADY_STATE_ROUNDS=500 \
cargo run --release --quiet --example integrated_steady_state
```

Steady-state conflict rerun:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.25 \
EC_STEADY_STATE_CONFLICT_CONTENDERS=2 \
cargo run --release --quiet --example integrated_steady_state
```

## Corrected Baseline

`192` peers, `500` rounds, `cross_dc_normal`, corrected ring-gradient topology, threshold `+2`, `2` normal vote targets, first-wave `3`, batching on.

### No conflict

- `1226` committed, `274` pending
- avg known / connected peers: `23.0 / 23.0`
- wire messages: `11.24M`
- commit latency: avg `9.5`, p50 `9`, p95 `11`
- recent throughput: `2.50 commits/round`
- total block-message factor:
  - `5.77x` vs role-sum ideal
  - `6.60x` vs coalesced ideal

Other useful readouts:

- neighborhood coverage: fixed at `13`
- reachable vote graph: avg `70.5` peers, p95 `155`
- settled peer spread: avg `39.1` peers
- local-entry commit latency avg `9.6`
- near-entry commit latency avg `9.4`

### 25% two-way conflict families

- `1242` committed, `621` pending
- avg known / connected peers: `23.0 / 23.0`
- wire messages: `16.35M`
- commit latency: avg `9.5`, p50 `9`, p95 `11`
- recent throughput: `2.83 commits/round`
- total block-message factor:
  - `6.76x` vs role-sum ideal
  - `7.57x` vs coalesced ideal

Conflict outcomes:

- `363` families
- `40` unanimous-highest
- `68` highest-majority
- `116` any-majority
- `247` stalled-no-majority
- `120` lower-owner-commit
- `4` multi-owner-commits
- conflict signal coverage among participants: avg `0.84`

## Interpretation

The correction matters a lot.

The older "sparse ring" reading was harsher than it should have been because it was really a fixed nearest-neighbor graph, not a gradient-shaped connected neighborhood. After correction:

- steady-state latency stays comfortably in the human-timescale range
- the ideal-gap factor drops a lot
- conflict warning reach is good
- conflict convergence is still not strong enough

So the corrected picture is:

- the base transaction path on a formed gradient-like network looks healthier than the old fixed-ring baseline suggested
- the open problems are still conflict convergence and lifecycle/churn recovery, not basic steady-state liveness

## Important Scope Limit

This correction affects the **ring steady-state** harness and the peer-lifecycle runner's ring initialization.

It does **not** change the current integrated churn long-run harness, because that still bootstraps from `RandomIdentified`, not `Ring`.
