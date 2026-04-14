# Core + Flat Tail Topology Report

This note evaluates a new steady-state topology shape:

- steep local core around each peer
- the same linear fade band as the corrected ring
- a small evenly spaced long-range tail on each side of the ring

The motivating question was whether a low, flat routing base across the full ring
would lower routing and commit latency on larger steady-state networks without
requiring a broad, flattened peer graph.

## Topology Under Test

New mode:

- `TopologyMode::RingCoreTail { neighbors, tail_peers_per_side }`

Current test shape:

- guaranteed local core: `±8`
- linear fade band: `±9 .. ±16`
- evenly spaced tail: `4` peers per side beyond the fade band

That gives about:

- corrected ring baseline: `23` connected peers per node
- core+tail: `31` connected peers per node

## Additional Metric

The integrated steady-state runner now also reports:

- `Max connected-graph hops to a role coverer`

For each submitted block, this is the maximum over its token roles and witness role of:

- shortest connected-graph hop count from the submitting peer
- to any active peer that covers that role locally

This is useful as a topology/routing proxy, but the current protocol does not
strictly follow shortest-path graph routing, so it should be read as:

- a lower-bound style graph-depth signal
- not a direct predictor of commit latency

## Commands

`1024` peers, perfect network, corrected ring:

```bash
EC_STEADY_STATE_ROUNDS=200 \
EC_STEADY_STATE_INITIAL_PEERS=1024 \
EC_STEADY_STATE_TOTAL_TOKENS=1000000 \
EC_STEADY_STATE_NETWORK_PROFILE=perfect \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_BLOCKS_PER_ROUND=2 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_steady_state
```

`1024` peers, perfect network, core+tail:

```bash
EC_STEADY_STATE_ROUNDS=200 \
EC_STEADY_STATE_INITIAL_PEERS=1024 \
EC_STEADY_STATE_TOTAL_TOKENS=1000000 \
EC_STEADY_STATE_NETWORK_PROFILE=perfect \
EC_STEADY_STATE_TOPOLOGY=ring_core_tail \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_RING_TAIL_PEERS_PER_SIDE=4 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_BLOCKS_PER_ROUND=2 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_steady_state
```

`1024` peers, `cross_dc_normal`, corrected ring:

```bash
EC_STEADY_STATE_ROUNDS=120 \
EC_STEADY_STATE_INITIAL_PEERS=1024 \
EC_STEADY_STATE_TOTAL_TOKENS=1000000 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_BLOCKS_PER_ROUND=1 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_steady_state
```

`1024` peers, `cross_dc_normal`, core+tail:

```bash
EC_STEADY_STATE_ROUNDS=120 \
EC_STEADY_STATE_INITIAL_PEERS=1024 \
EC_STEADY_STATE_TOTAL_TOKENS=1000000 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring_core_tail \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_RING_TAIL_PEERS_PER_SIDE=4 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_BLOCKS_PER_ROUND=1 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_steady_state
```

Exploratory larger run:

```bash
EC_STEADY_STATE_ROUNDS=80 \
EC_STEADY_STATE_INITIAL_PEERS=2048 \
EC_STEADY_STATE_TOTAL_TOKENS=2000000 \
EC_STEADY_STATE_NETWORK_PROFILE=perfect \
EC_STEADY_STATE_TOPOLOGY=ring_core_tail \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_RING_TAIL_PEERS_PER_SIDE=4 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_BLOCKS_PER_ROUND=1 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_steady_state
```

## Results

### `1024` peers, perfect network

| Topology | Active connected | Fit | Far leakage | Max route hops | Commit latency avg / p95 | Wire messages | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| corrected ring | `23.0` | `0.995` | `0.000` | `27.7 / p95 38` | `4.4 / 6` | `4.06M` | `2.68x` |
| core+tail | `31.0` | `0.987` | `0.008` | `3.2 / p95 4` | `4.9 / 8` | `7.42M` | `10.38x` |

### `1024` peers, `cross_dc_normal`

| Topology | Active connected | Fit | Far leakage | Max route hops | Commit latency avg / p95 | Wire messages | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| corrected ring | `23.0` | `0.995` | `0.000` | `28.3 / p95 38` | `6.9 / 8` | `0.64M` | `2.17x` |
| core+tail | `31.0` | `0.987` | `0.008` | `3.3 / p95 4` | `7.0 / 12` | `2.18M` | `6.78x` |

### `2048` peers, perfect network, core+tail

This larger run completed with:

- active connected peers/node: `31.0`
- target fit: `0.994`
- far leakage: `0.004`
- max route hops to a role coverer: avg `5.9`, p95 `8`
- commit latency: avg `4.2`, p95 `5`
- block-message factor vs role-sum ideal: `3.64x`

An attempted `2048`-peer corrected-ring baseline hit the environment limit before completion,
so the larger-size comparison is not yet symmetric.

## Reading The Result

This is the important result:

- adding a small flat tail **dramatically reduces connected-graph route depth**
- but it does **not** improve commit latency on the current protocol
- and it makes message cost much worse

At `1024` peers in perfect network:

- graph route depth fell from about `28` hops to about `3`
- but average commit latency worsened from `4.4` to `4.9` rounds
- p95 worsened from `6` to `8`
- block-message factor worsened from `2.68x` to `10.38x`

The same pattern held under `cross_dc_normal`:

- route depth again fell to about `3`
- average latency stayed roughly flat
- p95 got worse
- message cost rose sharply

## Interpretation

This tells us something important about the current design:

1. A shortest-path graph model is not the dominant latency driver.
   The protocol can already send directly to connected peers chosen around the role key,
   so it effectively skips over many graph edges in one step.

2. Flattening the peer set is still not a free win.
   Even a small flat tail widens overlap and increases settlement spread.
   That increases vote/block work faster than it reduces rounds.

3. The steep local core is still the healthier default shape.
   For the current protocol, a denser flat routing base makes the graph look better
   from a pure path-length perspective, but worse from a message-complexity perspective.

So the current conclusion remains:

- keep the graph steep and local
- be cautious about adding even a small flat tail
- if we want better latency scaling, it is more promising to improve the protocol’s
  stage-to-stage routing behavior than to flatten the peer distribution itself

## What This Suggests Next

The next useful routing metric is probably not plain graph shortest path.
It is something more protocol-shaped, for example:

- how much closer the first reactive seed wave gets to the role neighborhood
- how many reactive stages are needed before a role sees its first positive host vote
- how much each stage reduces ring-rank distance to the covering set

Those should correlate more directly with observed commit latency than a generic BFS depth.
