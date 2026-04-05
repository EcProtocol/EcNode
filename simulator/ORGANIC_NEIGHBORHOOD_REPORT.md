# Organic Neighborhood And Spread Report

This report measures two different things in the integrated simulator:

1. `External neighborhood size`: how many active peers currently consider a token to be inside their local `I_i(t)` interval.
2. `Transaction spread`: how widely a submitted block can propagate through the vote-pumping graph, and how much block-related message work it actually consumes before commit.

The goal is to separate structural neighborhood size from message amplification.

## Scenario

Both runs used the same long-run integrated scenario in [integrated_long_run.rs](/workspaces/ecRust/simulator/integrated_long_run.rs):

- genesis-backed bootstrap
- `96` initial peers
- growth to `136` total peers
- final active set `122`
- continuous transaction load: `3` blocks/round, block size `1..3`
- churn: joins, crashes, and returns
- source policy: `connected-only`
- fixed seed variant `0`

Profiles:

- `cross_dc_normal`
- `cross_dc_stressed`

## Measurement Definitions

- `Neighborhood coverage`
  - For each submitted token, count active peers whose local `peer_range(self_id)` contains that token.
  - This is the external, outside-the-node notion of "how many peers currently host this token region".
- `Vote-eligible set at entry`
  - For the submitting node, count connected peers inside `peer_range(token)`.
  - This is the local vote window the entry node would count for that token.
- `Entry distance to token`
  - Connected-peer hop distance from the submitter address to the token address, measured on the submitter's active ring view.
- `Reachable vote graph`
  - Starting from the submitter, repeatedly follow `peers_for(token, time)` for each token in the block and take the union of reachable peers and directed edges.
  - This is a static approximation of the vote-pumping graph at submission time.
  - It does not include the witness graph separately.
- `Settled peer spread`
  - Unique peers that sent or received a block-related `Vote`, `QueryBlock`, or `Block` message before the block was observed committed.
- `Block-related messages to settle`
  - Delivered `Vote`, `QueryBlock`, and `Block` messages attributed to a block before commit detection.

## Headline Results

| Metric | `cross_dc_normal` | `cross_dc_stressed` |
| --- | ---: | ---: |
| Committed blocks | 5080 | 5240 |
| Pending blocks | 2120 | 1960 |
| Delivered messages | 117.3M | 94.7M |
| Overall messages / commit | 23,081.6 | 18,066.7 |
| Commit latency p50 | 11 rounds | 19 rounds |
| Commit latency p95 | 268 rounds | 225 rounds |
| Network transit avg | 1.7 rounds | 3.5 rounds |
| Neighborhood coverage avg | 16.9 peers | 15.7 peers |
| Neighborhood coverage p95 | 31 peers | 31 peers |
| Neighborhood coverage max | 96 peers | 96 peers |
| Vote-eligible set avg | 9.0 peers | 9.0 peers |
| Vote-eligible set max | 10 peers | 10 peers |
| Reachable vote graph avg | 16.8 peers | 15.4 peers |
| Reachable vote graph p95 | 25 peers | 22 peers |
| Settled peer spread avg | 54.0 peers | 50.5 peers |
| Settled peer spread p95 | 92 peers | 86 peers |
| Block-related messages to settle avg | 5,373.8 | 4,017.0 |
| Block-related messages to settle p95 | 25,615 | 20,945 |

## Distance Buckets

The run naturally populated three entry-distance buckets:

- `local (<=4 hops)`
- `near (5-16 hops)`
- `mid (17-64 hops)`

No `far (65+ hops)` bucket was populated in this scenario; the sampled max entry distance was `55` hops.

### `cross_dc_normal`

| Bucket | Token samples | Committed blocks | Coverage avg | Commit latency avg | Commit latency p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Local | 2340 | 364 | 23.3 | 24.0 | 69 |
| Near | 5171 | 1350 | 16.1 | 39.6 | 175 |
| Mid | 6893 | 3366 | 15.2 | 58.6 | 339 |

### `cross_dc_stressed`

| Bucket | Token samples | Committed blocks | Coverage avg | Commit latency avg | Commit latency p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Local | 2119 | 384 | 23.4 | 24.5 | 27 |
| Near | 4618 | 1154 | 14.9 | 38.6 | 124 |
| Mid | 7618 | 3702 | 14.1 | 60.2 | 298 |

## Observations

### 1. The "true" neighborhood is materially larger than the nominal local vote window

The external coverage size averaged `16-17` active peers, with `p95 = 31` and worst-case `96`.

That means the effective hosting neighborhood is not close to an idealized `8`-peer vote set. The overlapping local intervals create a materially larger structural group around many tokens.

### 2. The local vote window is also not actually capped at `8` in the current implementation

The measured vote-eligible set at the entry node averaged `9.0` peers and reached `10`.

This happens because:

- the current interval math is inclusive on both ends
- when a node's connected set is small (`<= 10`), `peer_range()` expands to the whole ring

So the implementation is already wider than the design target in some states.

### 3. Static graph size tracks structural neighborhood size fairly closely

Average reachable vote graph size was:

- `16.8` peers in `cross_dc_normal`
- `15.4` peers in `cross_dc_stressed`

That is close to the external coverage average, which suggests the initial vote-pumping graph is mostly reflecting the organic overlap structure rather than exploding immediately on its own.

### 4. Actual settlement spreads about 3.2x wider than the static graph

Average settled peer spread divided by average reachable vote graph:

- `54.0 / 16.8 = 3.21x` in `cross_dc_normal`
- `50.5 / 15.4 = 3.28x` in `cross_dc_stressed`

So even if the initial reachable graph is only mid-teens peers, the block typically touches around fifty peers before commit.

This is the clearest evidence that current message amplification is not just a function of nominal neighborhood size.

### 5. Entry distance matters a lot

Average commit latency by bucket stayed in the same ordering in both profiles:

- local entry: about `24-25` rounds
- near entry: about `39` rounds
- mid entry: about `59-60` rounds

So blocks entering far from their token neighborhoods pay roughly `2.4x` the average latency of local-entry blocks in the current design and implementation.

### 6. Stress changes timing more than geometry

From `cross_dc_normal` to `cross_dc_stressed`:

- coverage avg changed only `-7.1%`
- reachable vote graph avg changed only `-8.3%`
- settled peer spread avg changed only `-6.5%`

But timing and transport changed more:

- network transit avg went from `1.7` to `3.5` rounds
- commit latency p50 increased from `11` to `19` rounds
- block-related messages to settle average fell `-25.2%`

That last point is counterintuitive but important: the stressed profile appears to throttle some propagation churn rather than increasing it. In this run, slower and lossier transport reduced block-related message work per settled block, even while it increased median latency and queue depth.

## Design Implications

### Structural implication

The current organic neighborhood is already larger than the intended local voting intuition. If the goal is to reduce baseline message work, shrinking effective overlap is likely a higher-leverage design move than batching alone.

### Protocol implication

The current implementation still amplifies far beyond the initial reachable vote graph. Even with a mid-teens structural neighborhood, settlement work averaged:

- about `99.5` block-related messages per settled peer in `cross_dc_normal`
- about `79.5` block-related messages per settled peer in `cross_dc_stressed`

So batching and more selective retry behavior are still necessary.

### Implementation implication

The current `peer_range()` behavior should be treated as a concrete design question, not an accident to ignore:

- Is the intended vote window really `8`?
- If yes, should inclusive interval behavior or the `<= 10` whole-ring fallback be tightened?
- If no, the protocol document should stop describing the neighborhood as a strict `±4 => 8 peers` bound.

## Next Experiments

1. Add witness-graph measurement separately from token-graph measurement.
2. Sweep the neighborhood parameter now hard-coded around `4` and compare:
   - external coverage size
   - reachable vote graph size
   - settled peer spread
   - commit latency
3. Run the same report at larger populations to see whether coverage stabilizes, shrinks, or grows with denser active sets.
