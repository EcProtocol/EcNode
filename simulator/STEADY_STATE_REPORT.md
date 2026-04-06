# Steady-State Fixed-Graph Report

This report isolates the formed-network path by running a fixed already-connected peer set with:

- `TopologyMode::FullyKnown`
- `elections_per_tick = 0`
- `connection_timeout` set far beyond the run length
- no joins, crashes, returns, or network changes

That makes the integrated simulator behave much closer to the older fixed-topology consensus simulator while still using the current full-node, vote-flow, and batching implementation.

## Example

Use [integrated_steady_state.rs](/workspaces/ecRust/simulator/integrated_steady_state.rs) via:

```bash
cargo run --release --quiet --example integrated_steady_state
```

Useful knobs:

```bash
EC_STEADY_STATE_INITIAL_PEERS=192
EC_STEADY_STATE_ROUNDS=500
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6
EC_STEADY_STATE_BLOCKS_PER_ROUND=3
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5
EC_STEADY_STATE_BATCHING=true
EC_STEADY_STATE_BATCH_VOTE_REPLIES=false
EC_STEADY_STATE_ELECTIONS_PER_TICK=0
EC_STEADY_STATE_CONNECTION_TIMEOUT=10500
```

## Completed Runs

### `160` peers, `600` rounds, `cross_dc_normal`

- `1184` committed, `616` pending
- logical delivered: `25,003,719`
- wire delivered: `16,547,974`
- peak in-flight: `86,107`
- commit latency: `avg 14.1`, `p50 14`, `p95 17` rounds
- total block-message factor:
  - `7.04x` vs role-sum ideal
  - `7.96x` vs coalesced ideal

### `192` peers, `500` rounds, `cross_dc_normal`

- `1324` committed, `176` pending
- logical delivered: `5,754,175`
- wire delivered: `4,794,585`
- peak in-flight: `32,210`
- recent throughput: `3.17 commits/round`
- commit latency: `avg 13.6`, `p50 13`, `p95 16` rounds
- total block-message factor:
  - `6.79x` vs role-sum ideal
  - `7.77x` vs coalesced ideal

## Readout

The important result is that the current implementation behaves much better in formed-network steady state than in churn-heavy lifecycle runs.

At `192` fixed peers:

- latency stayed very flat throughout the run:
  - early p95 `16`
  - mid p95 `17`
  - late p95 `16`
- the active peer graph stayed fixed:
  - avg known `191.0`
  - avg connected `191.0`
- there was no discovery or sync overhead:
  - `query-token = 0`
  - `answer = 0`
  - `commit traces = 0`
  - `elections = 0`

That means the remaining load is much closer to the core transaction protocol itself:

- votes
- block fetches
- referrals

## Human-Timescale Interpretation

For the `192`-peer fixed run:

- at `25 ms/round`:
  - p50 about `0.33s`
  - p95 about `0.40s`
- at `50 ms/round`:
  - p50 about `0.65s`
  - p95 about `0.80s`

So in steady state on a formed network, the current system is already in the range that feels compatible with “human timescale”.

## Assessment

This strengthens the current project assessment:

1. The design does not look fundamentally too slow.
2. The difficult part is the lifecycle overhead: graph maintenance, sync, and churn interaction.
3. When those are removed, the transaction path is much healthier than the churn-inclusive benchmarks suggested.

So the gap to close is now clearer:

- not “make the base protocol possible”
- but “preserve more of this steady-state behavior while the open network keeps changing”
