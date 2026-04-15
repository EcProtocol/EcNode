# Dense Linear Topology Report

This note records the first Rust-side fixed-network comparison against the
dense linear-slope insight from the simplified Python model.

The goal was to test a topology resembling the Python
`shallow_high_core` family in the integrated steady-state runner:

- full-ring linear connection probability
- center probability near `1`
- nonzero far-end probability
- no churn, no joins, no elections

## Topology Under Test

New mode:

- `TopologyMode::RingLinearProbability`

Parameters used here:

- `center_prob = 1.0`
- `far_prob = 0.2`
- `guaranteed_neighbors = 0`

For `192` peers this produced:

- active connected peers/node: `114.3`
- target-fit to corrected ring: `0.508`
- gradient shape:
  - core `0.961`
  - fade `0.900` vs corrected-ring target `0.500`
  - far leakage `0.536`

So this is intentionally much denser and flatter than the corrected ring while
still keeping a linear ring-distance bias.

## Commands

Baseline ring, no conflict:

```bash
EC_STEADY_STATE_ROUNDS=300 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_BLOCKS_PER_ROUND=2 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring \
cargo run --release --quiet --example integrated_steady_state
```

Dense linear, no conflict:

```bash
EC_STEADY_STATE_ROUNDS=300 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_BLOCKS_PER_ROUND=2 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring_linear_probability \
EC_STEADY_STATE_LINEAR_CENTER_PROB=1.0 \
EC_STEADY_STATE_LINEAR_FAR_PROB=0.2 \
EC_STEADY_STATE_LINEAR_GUARANTEED_NEIGHBORS=0 \
cargo run --release --quiet --example integrated_steady_state
```

The same pair was then rerun with:

```bash
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.25
EC_STEADY_STATE_CONFLICT_CONTENDERS=2
```

I also ran one follow-up on the dense linear topology with:

```bash
EC_STEADY_STATE_VOTE_TARGETS=1
```

to see whether the denser graph wanted a narrower vote fanout.

I then ran a second follow-up on the same dense linear topology with:

```bash
EC_STEADY_STATE_VOTE_ACTIVE_ROUNDS=3
```

With the current scheduler this keeps the immediate reactive `+/-2` seed, then
switches the periodic cadence to one active ring followed by one pause.

## Results

### No conflict, current ring vs dense linear

| Topology | Avg connected | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Reachable vote graph | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| corrected ring | `23.0` | `519` | `81` | `7.5 / 10` | `7.69M` | `2.35M` | `73.5` | `39.7` | `3.84x` |
| dense linear | `114.3` | `523` | `77` | `9.5 / 12` | `3.51M` | `3.06M` | `9.8` | `60.9` | `14.20x` |

### Conflict, current ring vs dense linear

| Topology | Avg connected | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Highest-majority | Stalled | Lower-owner commits | Signal coverage |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| corrected ring | `23.0` | `479` | `289` | `7.6 / 10` | `7.65M` | `2.55M` | `36` | `113` | `59` | `0.64` |
| dense linear | `114.3` | `525` | `217` | `9.1 / 12` | `3.09M` | `2.71M` | `57` | `82` | `6` | `0.75` |

### Dense linear with `vote_target_count = 1`

#### No conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Reachable vote graph | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dense linear, targets `2` | `523` | `77` | `9.5 / 12` | `3.51M` | `3.06M` | `9.8` | `60.9` | `14.20x` |
| dense linear, targets `1` | `525` | `75` | `9.0 / 12` | `2.89M` | `2.58M` | `4.2` | `59.4` | `9.25x` |

#### Conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Highest-majority | Stalled | Lower-owner commits |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dense linear, targets `2` | `525` | `217` | `9.1 / 12` | `3.09M` | `2.71M` | `57` | `82` | `6` |
| dense linear, targets `1` | `514` | `228` | `9.8 / 13` | `3.42M` | `2.93M` | `62` | `79` | `4` |

### Dense linear with paused three-ring periodic polling

This keeps the immediate `+/-2` reactive seed, then lets the periodic polling
cycle walk only three ring positions with a pause after every active pair.

#### No conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Reachable vote graph | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dense linear, targets `2`, four-ring periodic | `523` | `77` | `9.5 / 12` | `3.51M` | `3.06M` | `9.8` | `60.9` | `14.20x` |
| dense linear, targets `2`, paused three-ring periodic | `497` | `103` | `11.8 / 14` | `2.47M` | `2.19M` | `9.6` | `50.0` | `10.49x` |

#### Conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Highest-majority | Stalled | Lower-owner commits | Signal coverage |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dense linear, targets `2`, four-ring periodic | `525` | `217` | `9.1 / 12` | `3.09M` | `2.71M` | `57` | `82` | `6` | `0.75` |
| dense linear, targets `2`, paused three-ring periodic | `488` | `257` | `10.7 / 15` | `2.59M` | `2.29M` | `70` | `75` | `2` | `0.75` |

## Reading The Result

This is the useful split.

### 1. The dense linear topology really does localize the **initial vote graph**

Compared with the corrected ring:

- reachable vote graph dropped from about `73.5` peers to about `9.8`
- max connected-graph hops to a role coverer dropped from `4.9` to `1.0`
- logical message volume dropped sharply

That lines up with the simplified model: the dense linear graph makes it much
easier for the first vote wave to land inside the relevant local patch.

### 2. But later settlement still spreads more than we want

Even with the smaller reachable vote graph:

- settled peer spread rose from `39.7` to `60.9`
- block-message factor got much worse
- no-conflict latency got worse

So in the integrated Rust path, the topology helps the early routing stage but
does not by itself keep the later commit/reflection path local.

This is an important difference from the simplified model.

### 3. Conflict handling gets much better on the dense linear graph

This is the strongest result from the first Rust-side test.

Against the corrected ring:

- `highest-majority` improved `36 -> 57`
- `stalled-no-majority` improved `113 -> 82`
- `lower-owner-commit` improved `59 -> 6`
- signal coverage improved `0.64 -> 0.75`

So the dense linear graph is a much better fixed-network base for conflict
containment and convergence than the corrected ring.

### 4. Fanout matters once the graph is already dense

On the dense linear graph, reducing `vote_target_count` from `2` to `1`:

- helped no-conflict message cost and latency
- reduced reachable vote graph again
- improved no-conflict block-message factor materially

But in the conflict run it was mixed:

- safety-shaped outcomes improved slightly again
- throughput and message totals got worse

So this is not a new default yet, but it strongly suggests:

- dense topologies want a narrower vote fanout than sparse topologies do

### 5. The paused three-ring schedule is a real containment / safety trade

Compared with the earlier four-ring periodic cadence on the same dense linear
graph:

- no-conflict settlement became cheaper
- settled peer spread dropped from `60.9` to `50.0`
- block-message factor dropped from `14.20x` to `10.49x`
- wire traffic dropped from `3.06M` to `2.19M`

But it also came with a real no-conflict latency and throughput cost.

Under conflict, though, the same schedule looked stronger:

- `highest-majority` improved `57 -> 70`
- `stalled` improved `82 -> 75`
- `lower-owner-commit` improved `6 -> 2`
- wire traffic also dropped

So this is a promising dense-graph schedule if conflict containment matters
more than pure no-conflict speed.

### Dense linear with repeated inner `+/-2` every other tick

This variant keeps the immediate reactive `+/-2` seed and then repeats that
same inner `+/-2` set every other tick:

- `EC_STEADY_STATE_VOTE_ACTIVE_ROUNDS=1`
- `EC_STEADY_STATE_VOTE_PAIRS_PER_TICK=2`

So the periodic phase becomes:

- send `sequence 0` and `1` together
- pause
- send `sequence 0` and `1` together again

#### `192` peers, no conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dense linear, paused three-ring periodic | `339` | `61` | `10.4 / 14` | `1.20M` | `1.12M` | `48.8` | `8.76x` |
| dense linear, repeated inner `+/-2` every other tick | `304` | `96` | `11.7 / 17` | `2.14M` | `1.74M` | `39.1` | `12.42x` |

#### `192` peers, conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Highest-majority | Stalled | Lower-owner commits | Signal coverage | Settled peer spread |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dense linear, paused three-ring periodic | `316` | `169` | `10.4 / 14` | `1.23M` | `1.12M` | `39` | `46` | `4` | `0.71` | `47.1` |
| dense linear, repeated inner `+/-2` every other tick | `278` | `223` | `11.8 / 17` | `2.04M` | `1.62M` | `34` | `67` | `4` | `0.65` | `36.7` |

This one is useful as a bound:

- it does shrink the settled set further
- but it pays for that with worse latency, fewer commits, and more total
  message cost than the paused three-ring schedule

So repeating the inner `+/-2` set without ever progressing outward looks too
aggressive as a locality control on this dense graph.

### Dense linear with `InitialVote` on the first reactive wave

This experiment keeps the paused three-ring schedule and changes only the first
reactive wave:

- the first reactive wave now sends `InitialVote { block, vote, reply }`
- later protocol stays as before: normal `Vote`, `QueryBlock`, `Block`, and
  terminal replies
- unsolicited plain `Block` handling is unchanged

The goal is to cut the per-hop `Vote -> QueryBlock -> Block` round-trip on the
path toward the role centers.

#### `192` peers, paused three-ring schedule

##### No conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Query-block logical | Vote-triggered block fetches | Logical msgs | Wire msgs | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| paused three-ring, plain first vote | `339` | `61` | `10.4 / 14` | `49,307` | `24,014` | `1.20M` | `1.12M` | `48.8` | `8.76x` |
| paused three-ring, `InitialVote` first wave | `354` | `46` | `5.5 / 8` | `31,110` | `9,403` | `1.27M` | `1.20M` | `46.0` | `4.01x` |

##### Conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Highest-majority | Stalled | Lower-owner commits | Query-block logical | Vote-triggered block fetches | Logical msgs | Wire msgs | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| paused three-ring, plain first vote | `316` | `169` | `10.4 / 14` | `39` | `46` | `4` | `66,542` | `28,813` | `1.23M` | `1.12M` | `47.1` | `8.33x` |
| paused three-ring, `InitialVote` first wave | `314` | `173` | `5.7 / 9` | `33` | `54` | `0` | `46,452` | `13,920` | `1.35M` | `1.22M` | `45.0` | `4.71x` |

#### `1024` peers, paused three-ring schedule

##### No conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Query-block logical | Vote-triggered block fetches | Logical msgs | Wire msgs | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| paused three-ring, plain first vote | `357` | `43` | `11.3 / 15` | `39,790` | `30,225` | `1.44M` | `1.41M` | `60.8` | `13.00x` |
| paused three-ring, `InitialVote` first wave | `372` | `28` | `5.9 / 8` | `26,098` | `11,734` | `1.61M` | `1.58M` | `55.8` | `5.97x` |

##### Conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Highest-majority | Stalled | Lower-owner commits | Query-block logical | Vote-triggered block fetches | Logical msgs | Wire msgs | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| paused three-ring, plain first vote | `336` | `165` | `11.5 / 16` | `46` | `55` | `1` | `57,301` | `35,891` | `1.66M` | `1.58M` | `57.2` | `13.65x` |
| paused three-ring, `InitialVote` first wave | `364` | `131` | `6.1 / 8` | `48` | `47` | `0` | `30,176` | `16,360` | `1.51M` | `1.45M` | `53.6` | `6.26x` |

This is the cleanest fixed-network improvement in the dense-linear branch so
far.

What improved consistently:

- commit latency roughly halved
- `QueryBlock` traffic dropped a lot
- vote-triggered block fetches dropped a lot
- settled peer spread shrank slightly
- block-message factor dropped sharply

So the earlier latency concern now has a much clearer answer:

- graph depth was already short
- the missing cost was the repeated block-fetch round-trip on the first wave
- piggybacking the block on that first wave removes a large part of the extra
  protocol-stage latency

### `far_prob` sweep on top of dense linear + `InitialVote`

With the first-wave transport improved, the next lever was the far end of the
linear probability slope.

I swept:

- `center_prob = 1.0`
- `far_prob in {0.0, 0.05, 0.1, 0.2, 0.4}`
- `192` peers
- paused three-ring schedule
- `InitialVote` first wave

#### `192` peers, no conflict

| `far_prob` | Avg connected | Commits | Pending | Commit latency avg / p95 | Wire msgs | Query-block logical | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `0.0` | `94.7` | `346` | `54` | `5.7 / 9` | `2.55M` | `56,796` | `69.3` | `5.74x` |
| `0.05` | `99.5` | `338` | `62` | `5.5 / 8` | `2.09M` | `44,171` | `58.3` | `4.51x` |
| `0.1` | `104.1` | `353` | `47` | `5.4 / 8` | `1.81M` | `48,912` | `55.5` | `4.53x` |
| `0.2` | `114.3` | `356` | `44` | `5.3 / 8` | `1.32M` | `38,335` | `46.2` | `3.78x` |
| `0.4` | `133.9` | `352` | `48` | `5.3 / 8` | `0.84M` | `51,058` | `34.4` | `3.16x` |

#### `192` peers, conflict

| `far_prob` | Avg connected | Commits | Pending | Commit latency avg / p95 | Wire msgs | Highest-majority | Stalled | Lower-owner commits | Signal coverage | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `0.0` | `94.7` | `338` | `156` | `5.6 / 9` | `2.53M` | `43` | `51` | `0` | `0.68` | `65.7` | `5.69x` |
| `0.05` | `99.5` | `330` | `151` | `5.6 / 8` | `2.01M` | `38` | `43` | `0` | `0.75` | `54.7` | `4.60x` |
| `0.1` | `104.1` | `329` | `182` | `5.8 / 9` | `1.77M` | `43` | `68` | `0` | `0.72` | `53.5` | `5.31x` |
| `0.2` | `114.3` | `352` | `153` | `5.6 / 8` | `1.17M` | `39` | `66` | `1` | `0.78` | `43.9` | `4.32x` |
| `0.4` | `133.9` | `345` | `159` | `5.5 / 8` | `0.69M` | `43` | `61` | `0` | `0.72` | `35.3` | `3.54x` |

The useful read is:

- latency barely moved across the whole band
- locality and message cost improved steadily as `far_prob` rose
- conflict outcomes stayed in roughly the same safety band, with no clear
  collapse at the denser end

So for this fixed-network branch, a higher `far_prob` looked better than the
earlier `0.2` default.

#### `1024`-peer confirmation for `far_prob = 0.4`

I then compared the best locality candidate, `far_prob = 0.4`, against the
current `0.2` baseline at `1024` peers.

##### No conflict

| Setting | Avg connected | Commits | Pending | Commit latency avg / p95 | Wire msgs | Query-block logical | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `far_prob = 0.2` | `613.4` | `372` | `28` | `5.9 / 8` | `1.58M` | `26,098` | `55.8` | `5.97x` |
| `far_prob = 0.4` | `716.3` | `358` | `42` | `5.8 / 8` | `0.80M` | `21,363` | `38.6` | `4.34x` |

##### Conflict

| Setting | Avg connected | Commits | Pending | Commit latency avg / p95 | Wire msgs | Highest-majority | Stalled | Lower-owner commits | Signal coverage | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `far_prob = 0.2` | `613.4` | `364` | `131` | `6.1 / 8` | `1.45M` | `48` | `47` | `0` | `0.77` | `53.6` | `6.26x` |
| `far_prob = 0.4` | `716.3` | `367` | `142` | `5.9 / 8` | `0.76M` | `53` | `56` | `0` | `0.73` | `38.5` | `4.66x` |

So the larger run kept the main locality story intact:

- latency stayed flat
- query-block traffic fell
- settled peer spread shrank materially
- block-message factor improved materially
- wire traffic roughly halved

The only real caution is that denser far tails do raise connected degree further,
so the next question is no longer “does this help?” but “how much dense far tail
can we afford before the peer graph itself becomes too broad for churn?”

### Delaying plain vote-triggered block fetches by one tick

After `InitialVote` was added, plain unknown `Vote` messages still immediately
sent `QueryBlock` on arrival.

I tested a narrower rule:

- `InitialVote` stays eager and still carries the block on the first reactive wave
- plain unknown `Vote` still records the sender immediately
- but the `QueryBlock` for that vote is delayed until the next local tick
- if an `InitialVote` or some other path delivers the block first, the delayed
  fetch is canceled

The goal is to avoid redundant fetches in cases where the eager first wave is
already likely to deliver the block.

These comparisons use the same `far_prob = 0.4` fixed-network baseline as
above, with `200` rounds and `2` blocks per round.

#### `192` peers

##### No conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Query-block logical | Vote-triggered block fetches | Blocks logical | Wire msgs | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| eager plain-vote fetch | `352` | `48` | `5.3 / 8` | `51,058` | `10,292` | `43,436` | `0.84M` | `34.4` | `3.16x` |
| delayed plain-vote fetch | `361` | `39` | `5.4 / 8` | `24,439` | `6,319` | `15,909` | `0.76M` | `36.7` | `3.38x` |

##### Conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Highest-majority | Stalled | Lower-owner commits | Query-block logical | Vote-triggered block fetches | Blocks logical | Wire msgs | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| eager plain-vote fetch | `345` | `159` | `5.5 / 8` | `43` | `61` | `0` | `55,668` | `14,575` | `35,654` | `0.69M` | `35.3` | `3.54x` |
| delayed plain-vote fetch | `353` | `144` | `5.5 / 8` | `48` | `49` | `0` | `26,285` | `8,555` | `18,197` | `0.67M` | `33.3` | `3.38x` |

#### `1024` peers

##### No conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Query-block logical | Vote-triggered block fetches | Blocks logical | Wire msgs | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| eager plain-vote fetch | `358` | `42` | `5.8 / 8` | `21,363` | `11,734` | `20,211` | `0.80M` | `38.6` | `4.34x` |
| delayed plain-vote fetch | `374` | `26` | `5.6 / 8` | `18,561` | `7,118` | `13,975` | `0.82M` | `39.1` | `4.22x` |

##### Conflict

| Setting | Commits | Pending | Commit latency avg / p95 | Highest-majority | Stalled | Lower-owner commits | Query-block logical | Vote-triggered block fetches | Blocks logical | Wire msgs | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| eager plain-vote fetch | `367` | `142` | `5.9 / 8` | `53` | `56` | `0` | `30,176` | `16,360` | `39,952` | `0.76M` | `38.5` | `4.66x` |
| delayed plain-vote fetch | `358` | `128` | `6.0 / 8` | `48` | `38` | `0` | `37,880` | `10,162` | `22,783` | `0.90M` | `40.6` | `4.86x` |

The signal here is mixed but useful.

What clearly improved:

- at `192` peers the delayed fetch rule reduced block traffic a lot
- conflict runs benefited the most
- vote-triggered fetches dropped in all cases

What did **not** become a universal win:

- the `1024` no-conflict run improved slightly, but only modestly
- the `1024` conflict run traded fewer fetches for more total `QueryBlock` and
  a higher overall block-message factor

So delaying plain vote-triggered fetches looks promising as a selective policy,
especially on smaller or more conflict-heavy dense graphs, but it is not yet a
clear unconditional default for the fixed-network branch.

### Dense linear with paused three-ring polling at larger fixed populations

I also reran the same dense linear schedule at larger fixed network sizes with:

- `center_prob = 1.0`
- `far_prob = 0.2`
- `vote_active_rounds = 3`
- `vote_target_count = 2`
- `cross_dc_normal`

To keep runtime reasonable these larger runs used `200` rounds instead of `300`.

#### No conflict

| Peers | Avg connected | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Reachable vote graph | Settled peer spread | Block-message factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `192` | `114.3` | `339` | `61` | `10.4 / 14` | `1.20M` | `1.12M` | `9.9` | `48.8` | `8.76x` |
| `1024` | `613.4` | `357` | `43` | `11.3 / 15` | `1.44M` | `1.41M` | `9.8` | `60.8` | `13.00x` |
| `2048` | `1228.0` | `358` | `42` | `11.7 / 16` | `1.62M` | `1.59M` | `9.8` | `64.7` | `15.03x` |

#### Conflict

| Peers | Avg connected | Commits | Pending | Commit latency avg / p95 | Logical msgs | Wire msgs | Highest-majority | Stalled | Lower-owner commits | Signal coverage |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `192` | `114.3` | `316` | `169` | `10.4 / 14` | `1.23M` | `1.12M` | `39` | `46` | `4` | `0.71` |
| `1024` | `613.4` | `336` | `165` | `11.5 / 16` | `1.66M` | `1.58M` | `46` | `55` | `1` | `0.70` |
| `2048` | `1228.0` | `354` | `129` | `11.9 / 16` | `1.48M` | `1.42M` | `47` | `36` | `1` | `0.76` |

The useful read here is to look at spread both ways:

- in absolute terms, settled peer spread only grows from about `49` peers at
  `192` nodes to about `65` peers at `2048`
- as a fraction of the population, that drops sharply:
  - `192`: about `25%`
  - `1024`: about `6%`
  - `2048`: about `3%`

So the larger fixed dense-linear populations do **not** keep the committed set
flat in absolute terms, but they do improve locality as a share of the total
network.

## Comparison To The Simplified Model

The comparison is now clearer.

What carries over from the Python model:

- dense linear slope localizes the first useful transaction wave
- dense graphs can be a better base for sharding than the old steep sparse ring
- dense linear slope is especially promising for conflict handling

What does **not** carry over cleanly:

- the Rust integrated path still broadens settlement after the first local wave
- so topology alone is not enough to keep the committed set small

That points at the remaining missing lever:

- vote / commit propagation policy on top of the dense graph

## Takeaway

For the fixed-network Rust path:

1. Dense linear slope is promising and worth keeping.
2. It is already a clear win for conflict handling.
3. The paused three-ring schedule improves containment and conflict outcomes on
   top of that dense graph.
4. It is still not a pure no-conflict latency win, so the next best step is to
   keep this topology and tune the vote pattern for dense graphs rather than
   continuing to optimize sparse-ring topologies.
