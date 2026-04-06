# Sparse Steady-State Report

This report uses the steady-state simulator in a more realistic fixed connected shape:

- `TopologyMode::Ring`
- `8` neighbors on each side
- `elections_per_tick = 0`
- `connection_timeout` set far beyond the run length
- no joins, crashes, returns, or network changes

That gives a fixed sparse connected graph rather than a fully connected upper-bound graph.

## Baseline Command

```bash
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_BALANCE_THRESHOLD=2 \
EC_STEADY_STATE_BATCHING=true \
EC_STEADY_STATE_BATCH_VOTE_REPLIES=false \
EC_STEADY_STATE_ELECTIONS_PER_TICK=0 \
EC_STEADY_STATE_CONNECTION_TIMEOUT=10500 \
cargo run --release --quiet --example integrated_steady_state
```

## Baseline Result

`192` peers, `500` rounds, `cross_dc_normal`, ring `±8`, `2` targets, threshold `2`:

- `1197` committed, `303` pending
- logical delivered: `132,627,959`
- wire delivered: `9,309,527`
- peak in-flight: `76,858`
- recent throughput: `2.25 commits/round`
- commit latency: `avg 12.7`, `p50 9`, `p95 15` rounds
- total block-message factor:
  - `25.55x` vs role-sum ideal
  - `28.67x` vs coalesced ideal

Important structure:

- avg known / connected peers: `16.0`
- vote-eligible set at entry: `13.0`
- entry distance to token: avg `5.4` hops, p95 `8`

## Variants

### Threshold `1`

Command:

```bash
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_VOTE_BALANCE_THRESHOLD=1 \
cargo run --release --quiet --example integrated_steady_state
```

Result:

- `1254` committed, `246` pending
- logical delivered: `126,631,217`
- wire delivered: `9,250,735`
- peak in-flight: `75,989`
- recent throughput: `3.00 commits/round`
- commit latency: `avg 9.9`, `p50 8`, `p95 13`
- total block-message factor:
  - `10.15x` vs role-sum ideal
  - `11.49x` vs coalesced ideal

Delta vs threshold `2`:

- commits `+57`
- avg latency `-22.0%`
- p95 latency `15 -> 13`
- wire messages `-0.6%`
- total role-sum ideal gap `25.55x -> 10.15x`

This is a large efficiency win on the current workload. It is also the variant with the biggest safety caveat. This benchmark does **not** prove that lowering the threshold preserves false-consensus resistance under adversarial placement or minority-state conditions.

### Vote Targets `3`

Command:

```bash
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_VOTE_TARGETS=3 \
cargo run --release --quiet --example integrated_steady_state
```

Result on the fully connected upper-bound graph was promising, but this sparse ring run has not been repeated yet on the corrected topology. The fully connected result should not be treated as the realistic sparse baseline.

### Vote Targets `4`

Likewise, the earlier `4`-target run was on the fully connected upper-bound graph. It improved latency there, but at a heavy protocol cost. It still needs to be rerun on the sparse ring baseline before we should use it as a design signal.

## Readout

The corrected sparse graph changes the interpretation a lot:

1. Latency is still good.
   - Even on the sparse ring, p95 stayed around `15` rounds at threshold `2`
   - At `25 ms/round`, that is about `0.38s`

2. Base message complexity is much worse than the fully connected upper bound suggested.
   - The role-sum ideal gap jumped from about `6-8x` in the fully connected runs to `25.55x` here
   - The main cause is repeated vote propagation across a sparse connected graph

3. Lowering the vote-balance threshold has a very large effect.
   - This is the strongest performance lever seen so far on the sparse graph
   - It improves both latency and the ideal-gap factors without materially increasing wire traffic

## Assessment

This is the more realistic steady-state baseline to use going forward.

- The system still looks viable on human timescale in steady state.
- The sparse graph reveals much more vote churn than the fully connected benchmark did.
- Threshold policy matters a lot more than batching in this sparse setting.

The next responsible step is not to adopt threshold `1` outright. It is to pair this performance result with a safety-focused experiment around conflicting transactions or adversarial minority-state conditions.
