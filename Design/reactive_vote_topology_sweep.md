# Reactive Vote Topology Sweep

This note records the current results from
[`reactive_vote_simulation.py`](./reactive_vote_simulation.py) after the
counting fix that made dense and fully connected topologies behave sensibly for
multi-role transactions.

The simulator is intentionally simple:

- symmetric peer relations
- `Vote` and `Commit` only
- no network loss or delay beyond one round per hop
- one-shot vote seeding on first learn
- receiver-side counting:
  - a sender counts for a role if the sender is inside the receiver's local
    range around that role center
- all vote senders are stored as the path back
- once all role counters reach threshold, the node commits and reflects
  `Commit` to stored voters

The goal of this sweep is practical:

> find topologies that keep origin commit fast while keeping the activated set
> and resulting message load down.

## Configuration

Common settings unless noted otherwise:

- threshold `+2`
- `2` roles per transaction
- `range_width = 8`
- origin fanout `targets_per_side = 2`
- relay fanout varies by topology where noted

Terms:

- `Final aware`: fraction of nodes that learned the transaction
- `Final committed`: fraction of nodes that reached commit
- `Votes / tx`, `Commits / tx`: average messages emitted per transaction
- `Node participation / tx-set`: average and p95 number of the sampled
  transactions each node became aware of

## `1024` Node References

These are the most useful reference points for the current model.

### Topologies

- `full_table`
  - every node knows every other node
- `random_uniform_d64`
  - symmetric random graph with target average degree about `64`
- `random_uniform_d32`
  - symmetric random graph with target average degree about `32`
- `ring_core_tail_t1_hybrid`
  - dense local ring core with `1` tail peer per side
  - origin uses `targets_per_side = 2`
  - relays use `targets_per_side = 1`
- `stepwise_tight_a`
  - dense core `±8`
  - sampled mid band to step `16`
  - `1` sampled peer per side in the mid band
  - no far band
- `stepwise_tight_b`
  - dense core `±8`
  - sampled mid band to step `24`
  - `2` sampled peers per side in the mid band
  - no far band

### Summary

| Topology | Avg degree | Origin success | Origin avg | Origin p95 | Quiesce avg | Final aware | Final committed | Votes / tx | Commits / tx | Node participation / tx-set |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `full_table` | `1023.00` | `100%` | `3.00` | `3` | `6.00` | `1.26%` | `1.26%` | `103.6` | `121.3` | `avg 0.25`, `p95 1` |
| `random_uniform_d64` | `63.88` | `100%` | `3.70` | `4` | `10.90` | `28.66%` | `28.66%` | `2244.9` | `2682.4` | `avg 5.73`, `p95 8` |
| `random_uniform_d32` | `31.68` | `100%` | `3.30` | `4` | `12.00` | `51.34%` | `51.34%` | `3886.1` | `4418.5` | `avg 10.27`, `p95 14` |
| `ring_core_tail_t1_hybrid` | `25.09` | `100%` | `2.10` | `2` | `19.40` | `87.24%` | `87.24%` | `2641.9` | `2792.8` | `avg 17.45`, `p95 19` |
| `stepwise_tight_a` | `19.78` | `100%` | `2.05` | `2` | `40.50` | `99.62%` | `99.62%` | `4235.8` | `4309.6` | `avg 19.92`, `p95 20` |
| `stepwise_tight_b` | `23.52` | `100%` | `2.20` | `4` | `29.90` | `98.46%` | `98.46%` | `4273.1` | `4378.1` | `avg 19.69`, `p95 20` |

## Reading The `1024` Results

### `full_table`

- best locality by far
- almost no nodes are activated
- origin still commits quickly

This is the useful lower-bound control for spread and message count.

### `random_uniform_d64` and `d32`

- both keep origin commit reliable
- both activate much less than the ring-like shapes
- degree `64` is notably more local than degree `32`

This is the current best "shared but not global" reference family in the simple
model.

### `ring_core_tail_t1_hybrid`

- still gives very fast origin commit
- message count is moderate
- but it activates most of the graph

This remains a strong "fast origin" reference, but it is not yet a good organic
sharding shape.

### `stepwise_tight_a` and `stepwise_tight_b`

- both commit the origin fast
- both still activate almost the whole graph at `1024`
- both therefore generate near-global vote and commit traffic

So the first stepwise ring shapes are still too permissive at this size.

## `1024` Checkpoint Trace

To make the spread pattern more concrete, here are a few checkpointed round
averages from the same `1024`-node runs.

### `full_table`

| Round | Aware | Committed | Vote msgs | Commit msgs |
| ---: | ---: | ---: | ---: | ---: |
| `0` | `0.10%` | `0.00%` | `8.0` | `0.0` |
| `1` | `0.88%` | `0.00%` | `64.0` | `0.0` |
| `2` | `1.26%` | `0.78%` | `31.6` | `64.0` |
| `4` | `1.26%` | `1.26%` | `0.0` | `19.8` |
| `8` | `1.26%` | `1.26%` | `0.0` | `0.0` |

### `random_uniform_d64`

| Round | Aware | Committed | Vote msgs | Commit msgs |
| ---: | ---: | ---: | ---: | ---: |
| `0` | `0.10%` | `0.00%` | `7.5` | `0.0` |
| `1` | `0.83%` | `0.00%` | `56.6` | `0.0` |
| `2` | `5.36%` | `0.09%` | `357.8` | `2.7` |
| `4` | `25.16%` | `13.32%` | `649.8` | `911.2` |
| `8` | `28.64%` | `28.43%` | `2.4` | `52.2` |
| `16` | `28.66%` | `28.66%` | `0.0` | `0.0` |

### `ring_core_tail_t1_hybrid`

| Round | Aware | Committed | Vote msgs | Commit msgs |
| ---: | ---: | ---: | ---: | ---: |
| `0` | `0.10%` | `0.00%` | `5.3` | `0.0` |
| `1` | `0.62%` | `0.00%` | `16.6` | `0.0` |
| `2` | `1.84%` | `0.12%` | `39.0` | `3.4` |
| `4` | `9.27%` | `2.57%` | `151.5` | `50.3` |
| `8` | `51.22%` | `36.40%` | `336.1` | `384.4` |
| `16` | `86.84%` | `86.05%` | `5.7` | `23.2` |
| `32` | `87.24%` | `87.24%` | `0.0` | `0.0` |

### `stepwise_tight_a`

| Round | Aware | Committed | Vote msgs | Commit msgs |
| ---: | ---: | ---: | ---: | ---: |
| `0` | `0.10%` | `0.00%` | `4.0` | `0.0` |
| `1` | `0.49%` | `0.00%` | `16.2` | `0.0` |
| `2` | `1.62%` | `0.21%` | `47.8` | `5.7` |
| `4` | `8.36%` | `4.61%` | `177.6` | `106.5` |
| `8` | `21.46%` | `19.17%` | `121.6` | `135.7` |
| `16` | `44.29%` | `42.09%` | `118.9` | `118.6` |
| `32` | `89.71%` | `87.36%` | `125.0` | `121.0` |
| `64` | `99.62%` | `99.62%` | `0.0` | `0.0` |

## Larger Population Checks

The scaling behavior is the more interesting part.

### `4096` nodes

| Topology | Avg degree | Origin success | Origin avg | Origin p95 | Quiesce avg | Final aware | Final committed | Votes / tx | Commits / tx | Node participation / tx-set |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `random_uniform_d64` | `64.10` | `100%` | `3.75` | `4` | `12.25` | `24.29%` | `24.29%` | `7238.4` | `8080.9` | `avg 1.94`, `p95 4` |
| `random_uniform_d32` | `32.04` | `100%` | `3.50` | `4` | `14.00` | `40.16%` | `40.16%` | `10885.0` | `11678.5` | `avg 3.21`, `p95 6` |
| `ring_core_tail_t1_hybrid` | `25.03` | `100%` | `2.20` | `4` | `49.25` | `87.69%` | `87.69%` | `10470.6` | `10974.7` | `avg 17.54`, `p95 19` |
| `stepwise_tight_a` | `19.74` | `100%` | `2.20` | `4` | `n/a` | `89.54%` | `88.95%` | `14807.2` | `14916.8` | `avg 17.91`, `p95 19` |
| `stepwise_tight_b` | `23.50` | `100%` | `2.40` | `4` | `96.75` | `98.43%` | `98.43%` | `16377.5` | `16811.6` | `avg 19.69`, `p95 20` |

### `16384` nodes

| Topology | Avg degree | Origin success | Origin avg | Origin p95 | Quiesce avg | Final aware | Final committed | Votes / tx | Commits / tx | Node participation / tx-set |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `ring_core_tail_t1_hybrid` | `24.98` | `100%` | `2.25` | `2` | `n/a` | `68.50%` | `67.82%` | `32622.0` | `33772.1` | `avg 5.48`, `p95 7` |
| `stepwise_tight_a` | `19.75` | `100%` | `2.00` | `2` | `n/a` | `22.59%` | `22.45%` | `14826.1` | `14936.1` | `avg 1.81`, `p95 4` |
| `stepwise_tight_b` | `23.50` | `100%` | `2.25` | `2` | `n/a` | `34.48%` | `34.26%` | `22707.1` | `23065.0` | `avg 2.76`, `p95 5` |

## Reading The Scaling Trend

This is the main result from this round.

At `1024`, the tight stepwise ring shapes are still too broad.

By `16384`, that changes a lot:

- `stepwise_tight_a` activates only about `22.6%` of the population
- `stepwise_tight_b` activates about `34.5%`
- both still keep origin commit at about `2.0 - 2.25` rounds

So a fixed-distance dense core plus a limited sampled mid band starts behaving
much more like organic sharding as the population grows.

That is encouraging. It suggests:

- a topology can be "too broad" at small scale
- and still become usefully local at larger scale
- without hurting origin latency much

By contrast:

- `ring_core_tail_t1_hybrid` stays fast
- but still activates a much larger fraction of the graph at `16384`
- and its per-transaction message count grows much faster

## Linear-Probability Families

I also added two more topology families to
[`reactive_vote_simulation.py`](./reactive_vote_simulation.py):

- `linear_probability`
- `linear_probability_with_core`

For both, degree can now be expressed as a percentage of the total population
using `target_degree_percent`. That makes the settings directly comparable
across different node counts.

### What "slope" means here

These topologies use a distance-weighted linear probability:

- closer pairs connect with higher probability
- farther pairs connect with lower probability

The current sweep varies the effective slope by changing total degree as a
percentage of the population:

- higher degree percent gives a flatter, denser graph
- lower degree percent gives a steeper, sparser graph

`linear_probability_with_core` then adds guaranteed local neighbors on top:

- `±4` guaranteed neighbors
- plus the same linear probability graph

### `1024` Nodes

| Topology | Target degree pct | Avg degree | Actual degree pct | Origin success | Origin avg | Origin p95 | Quiesce avg | Final aware | Final committed | Votes / tx | Commits / tx |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `linear_probability` | `20.0%` | `205.55` | `20.09%` | `100%` | `4.05` | `5` | `11.30` | `13.13%` | `13.13%` | `1072.2` | `1453.9` |
| `linear_probability` | `10.0%` | `102.54` | `10.02%` | `100%` | `4.20` | `5` | `13.00` | `26.77%` | `26.77%` | `2177.1` | `2954.2` |
| `linear_probability` | `5.0%` | `50.87` | `4.97%` | `100%` | `3.90` | `5` | `12.25` | `45.44%` | `45.44%` | `3659.5` | `4701.1` |
| `linear_probability` | `2.0%` | `19.82` | `1.94%` | `100%` | `3.05` | `4` | `14.85` | `75.08%` | `75.08%` | `5400.6` | `5601.0` |
| `linear_probability` | `1.0%` | `9.84` | `0.96%` | `100%` | `2.50` | `4` | `12.70` | `95.97%` | `95.97%` | `5523.4` | `5646.3` |
| `linear_probability` | `0.5%` | `5.00` | `0.49%` | `95%` | `2.00` | `2` | `12.45` | `97.62%` | `94.62%` | `4201.6` | `4226.0` |
| `linear_probability_with_core4` | `20.0%` | `210.46` | `20.57%` | `100%` | `4.25` | `5` | `11.45` | `12.99%` | `12.99%` | `1062.5` | `1455.6` |
| `linear_probability_with_core4` | `10.0%` | `108.96` | `10.65%` | `100%` | `4.50` | `5` | `13.30` | `26.52%` | `26.52%` | `2162.9` | `2964.8` |
| `linear_probability_with_core4` | `5.0%` | `58.07` | `5.68%` | `100%` | `4.35` | `5` | `12.70` | `44.88%` | `44.88%` | `3631.4` | `4716.5` |
| `linear_probability_with_core4` | `2.0%` | `27.50` | `2.69%` | `100%` | `3.15` | `4` | `15.05` | `73.61%` | `73.61%` | `5365.4` | `5621.4` |
| `linear_probability_with_core4` | `1.0%` | `17.68` | `1.73%` | `100%` | `2.50` | `4` | `13.65` | `95.62%` | `95.62%` | `5706.6` | `5856.9` |
| `linear_probability_with_core4` | `0.5%` | `12.91` | `1.26%` | `100%` | `2.10` | `2` | `12.10` | `98.86%` | `98.86%` | `5136.3` | `5207.6` |

### `4096` Nodes

| Topology | Target degree pct | Avg degree | Actual degree pct | Origin success | Origin avg | Origin p95 | Quiesce avg | Final aware | Final committed | Votes / tx | Commits / tx |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `linear_probability` | `5.0%` | `204.69` | `5.00%` | `100%` | `4.50` | `5` | `11.00` | `6.64%` | `6.64%` | `2174.2` | `2723.5` |
| `linear_probability` | `2.0%` | `81.56` | `1.99%` | `100%` | `4.75` | `6` | `13.62` | `16.72%` | `16.72%` | `5388.6` | `6700.5` |
| `linear_probability` | `1.0%` | `40.99` | `1.00%` | `100%` | `4.12` | `4` | `14.50` | `30.04%` | `30.04%` | `9248.5` | `10691.8` |
| `linear_probability` | `0.5%` | `20.04` | `0.49%` | `100%` | `3.62` | `4` | `20.25` | `57.61%` | `57.61%` | `14797.5` | `15701.6` |
| `linear_probability_with_core4` | `5.0%` | `211.87` | `5.17%` | `100%` | `4.88` | `5` | `11.50` | `6.29%` | `6.29%` | `2060.6` | `2639.2` |
| `linear_probability_with_core4` | `2.0%` | `89.22` | `2.18%` | `100%` | `5.00` | `6` | `14.00` | `16.30%` | `16.30%` | `5285.0` | `6706.2` |
| `linear_probability_with_core4` | `1.0%` | `48.81` | `1.19%` | `100%` | `4.50` | `6` | `14.25` | `29.21%` | `29.21%` | `9129.2` | `10804.8` |
| `linear_probability_with_core4` | `0.5%` | `27.95` | `0.68%` | `100%` | `3.62` | `4` | `20.75` | `55.43%` | `55.43%` | `14641.5` | `15759.9` |

## Reading The Linear Result

This family behaves cleanly and predictably.

At both `1024` and `4096`:

- high degree percentages keep transactions very local
- lowering degree percentage steadily broadens activation
- origin commit stays reliable throughout almost the whole tested range
- very low degree percentages push the model back toward broad activation

The strongest practical range in this family looks roughly like:

- `5%` for highly local activation
- `2%` for a balanced middle ground
- `1%` if broader shared load is acceptable

The `±4` guaranteed core does not change the picture dramatically. It helps keep
local structure explicit, but the main behavior is still driven by the overall
degree percentage.

So this family is a good candidate for larger-population reference testing,
because:

- it is easy to specify in degree-percent terms
- the scaling trend is smooth
- it avoids the near-global activation we see in several of the ring-like
  shapes

## Current Takeaways

The current simplified model says:

1. Fewer active nodes really do mean fewer messages.
2. Fully connected and very dense random graphs keep transactions extremely
   local, but they also concentrate load on a very small patch.
3. Random degree in the `32 - 64` band is the current best compromise reference
   at `1024 - 4096`.
4. The first useful stepwise-density result appears only once the population is
   large enough.
   - `stepwise_tight_a` is the most promising result from this round.
5. Thin ring-like structures still commit the origin very fast, but they
   activate too much of the network if the goal is strong load isolation.

## What Still Is Missing

This model still does not directly measure "owner cluster strength" or
history-verifying neighborhoods. It tells us:

- how large the activated set is
- how fast the origin commits
- how many messages are emitted
- how widely transaction load is shared across the sampled node population

That is already enough to reject several shapes, but not enough yet to prove
that the relevant owner neighborhoods are saturated strongly enough for safety.

## Next Useful Tests

1. Add an explicit host-coverage metric:
   - for each role center, how many nodes in its closest local neighborhood
     became committed
2. Sweep `range_width` on the promising families:
   - `random_uniform_d64`
   - `random_uniform_d32`
   - `stepwise_tight_a`
3. Try one even tighter stepwise variant:
   - same core `±8`
   - mid band to `16`
   - still `1` sampled peer per side
   - but narrower relay fanout, matching the hybrid policy
