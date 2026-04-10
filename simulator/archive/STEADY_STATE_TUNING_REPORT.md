# Steady-State Tuning Report

This report compares message-flow variants on the fixed connected steady-state benchmark from [STEADY_STATE_REPORT.md](/workspaces/ecRust/simulator/STEADY_STATE_REPORT.md).

All runs used:

- `192` peers
- `500` rounds
- `cross_dc_normal`
- fixed connected graph
- `elections_per_tick = 0`
- no joins, crashes, returns, or network changes
- neighborhood width `6`
- existing-token workload target `50%`

The baseline steady-state mode is:

- batching on
- vote replies standalone
- vote targets `2`

## Commands

```bash
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_BATCHING=false \
cargo run --release --quiet --example integrated_steady_state

EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_ROUNDS=500 \
cargo run --release --quiet --example integrated_steady_state

EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_BATCHING=true \
EC_STEADY_STATE_BATCH_VOTE_REPLIES=true \
cargo run --release --quiet --example integrated_steady_state

EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_VOTE_TARGETS=3 \
cargo run --release --quiet --example integrated_steady_state

EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_VOTE_TARGETS=4 \
cargo run --release --quiet --example integrated_steady_state
```

## Results

| Mode | Committed / Pending | Logical Delivered | Wire Delivered | Wire Saved | Peak In-Flight | Recent Throughput | Latency avg / p50 / p95 | Total vs Role Ideal | Total vs Coalesced Ideal |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- | --- |
| `2 targets, batching off` | `1322 / 178` | `5,861,423` | `5,861,423` | `0.0%` | `40,455` | `2.58` | `13.6 / 13 / 17` | `6.63x` | `7.58x` |
| `2 targets, phase1` | `1324 / 176` | `5,754,175` | `4,794,585` | `16.7%` | `32,210` | `3.17` | `13.6 / 13 / 16` | `6.79x` | `7.77x` |
| `2 targets, phase2` | `1313 / 187` | `6,042,827` | `4,933,946` | `18.4%` | `34,542` | `2.83` | `13.7 / 13 / 17` | `6.62x` | `7.63x` |
| `3 targets, phase1` | `1358 / 142` | `6,385,930` | `5,329,062` | `16.5%` | `34,688` | `2.67` | `11.6 / 12 / 15` | `8.07x` | `9.21x` |
| `4 targets, phase1` | `1337 / 163` | `9,507,669` | `7,606,065` | `20.0%` | `44,172` | `2.75` | `10.9 / 11 / 14` | `9.50x` | `10.88x` |

## Readout

### Batching in Steady State

In this fixed-graph scenario, batching is still useful, but the gain is smaller than in churn-heavy runs because there is less peer-management overhead to compress.

- `phase1` vs `off`
  - wire messages down `16.7%`
  - peak queue down `20.4%`
  - p95 latency improves slightly: `17 -> 16`
  - committed count is effectively flat

- `phase2` vs `phase1`
  - wire messages rise slightly instead of falling further in this workload
  - latency and committed count are slightly worse

So in steady state, `phase1` remains the better default. `phase2` still does not justify itself.

### Vote Target Count in Steady State

Wider fanout helps latency, but it clearly costs more protocol work.

- `3` targets vs baseline `2`
  - commits `1324 -> 1358`
  - avg latency `13.6 -> 11.6`
  - p95 latency `16 -> 15`
  - wire messages `+11.1%`
  - role-sum ideal gap `6.79x -> 8.07x`

- `4` targets vs baseline `2`
  - commits `1324 -> 1337`
  - avg latency `13.6 -> 10.9`
  - p95 latency `16 -> 14`
  - wire messages `+58.6%`
  - role-sum ideal gap `6.79x -> 9.50x`

This means:

- `3` targets is the interesting compromise
- `4` targets is too expensive for the amount of extra latency improvement it buys

## Assessment

The fixed-graph benchmark gives a cleaner answer than the churn-heavy lifecycle runs:

1. The base transaction protocol is already in a usable range.
2. `phase1` batching is still the right default.
3. `3` targets is the first fanout variant that looks plausibly worthwhile.
4. `4` targets improves latency, but it pushes message complexity too far away from the lower bound.

If the goal is to drive base message complexity down, the current best path is:

- keep `2` targets + `phase1` as the efficiency baseline
- treat `3` targets as the “latency-biased” variant worth revisiting after more scheduling work
- avoid `4` targets unless transport and scheduling become substantially smarter

## Human-Timescale Interpretation

At roughly `25 ms/round`:

- baseline `2 targets, phase1`: p95 about `0.40s`
- `3 targets, phase1`: p95 about `0.38s`
- `4 targets, phase1`: p95 about `0.35s`

So wider fanout does improve user-facing latency, but the steady-state p95 is already comfortably sub-second without it.
