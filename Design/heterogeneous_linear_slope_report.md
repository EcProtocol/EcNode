# Heterogeneous Linear Slope Sweep

This note records one final simplified experiment on top of
[`reactive_vote_simulation.py`](./reactive_vote_simulation.py).

The goal was to test a topology family defined directly by two probabilities per
node:

- `center_prob`: connection probability at ring distance `0`
- `far_prob`: connection probability at the far end of the node's domain

with the rule:

- `center_prob >= far_prob`

Each node samples its own `(center_prob, far_prob)` pair from a configured
range, and pairwise symmetric connections are then built from the average of the
two endpoint probabilities at their rank distance.

This gives a population with heterogeneous slopes rather than one shared global
slope.

## Method

To reduce luck from both the role selection and the entry point:

- the same role sets were replayed across all slope families
- for each role set, the same list of origins was replayed across all families

So each family saw exactly the same transaction content and entry-point samples.

Simulation settings:

- roles per transaction: `2`
- threshold: `+2`
- origin fanout: `targets_per_side = 2`
- relay fanout: `targets_per_side = 2`
- range width: `8`
- no loss or retry path

Replay layout:

- `1024` nodes:
  - `12` role sets
  - `12` origins per role set
  - `144` transactions per family
- `4096` nodes:
  - `8` role sets
  - `8` origins per role set
  - `64` transactions per family

## Slope Families

| Family | Center range | Far range |
| --- | --- | --- |
| `steep_high_core` | `[0.95, 1.00]` | `[0.00, 0.02]` |
| `medium_high_core` | `[0.95, 1.00]` | `[0.02, 0.10]` |
| `shallow_high_core` | `[0.95, 1.00]` | `[0.10, 0.30]` |
| `mixed_core` | `[0.75, 1.00]` | `[0.00, 0.10]` |
| `mixed_shallow` | `[0.75, 1.00]` | `[0.10, 0.30]` |

## Important Interpretation Note

These slopes are defined across the **full ring domain**.

That means a linear profile with `center_prob` near `1.0` is automatically very
dense on average, even when the far end is near `0.0`.

For a full-domain linear profile, the average probability is roughly the mean of
the endpoints:

```text
avg_pair_prob ≈ (center_prob + far_prob) / 2
```

So a node with a slope near:

```text
center = 1.0, far = 0.0
```

still implies an average pair probability near `0.5`.

That matters a lot for reading the results below. These are not sparse graphs.
They are dense graphs with different kinds of high-center bias.

## Results

### `1024` nodes

| Family | Avg degree | Degree pct | Origin success | Origin avg | Origin p95 | Committed set | Votes / tx | Commits / tx |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `steep_high_core` | `503.29` | `49.20%` | `100%` | `4.10` | `5` | `6.95%` | `569.3` | `767.8` |
| `medium_high_core` | `528.96` | `51.71%` | `100%` | `3.95` | `5` | `5.09%` | `416.7` | `553.2` |
| `shallow_high_core` | `600.19` | `58.67%` | `100%` | `3.64` | `5` | `3.28%` | `268.8` | `350.4` |
| `mixed_core` | `471.44` | `46.08%` | `100%` | `4.03` | `5` | `5.83%` | `477.7` | `636.1` |
| `mixed_shallow` | `547.82` | `53.55%` | `100%` | `3.70` | `5` | `3.55%` | `290.5` | `378.6` |

### `4096` nodes

| Family | Avg degree | Degree pct | Origin success | Origin avg | Origin p95 | Committed set | Votes / tx | Commits / tx |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `steep_high_core` | `2015.93` | `49.23%` | `100%` | `4.14` | `5` | `0.76%` | `249.4` | `321.9` |
| `medium_high_core` | `2117.12` | `51.70%` | `100%` | `4.00` | `5` | `0.69%` | `226.6` | `289.3` |
| `shallow_high_core` | `2402.67` | `58.67%` | `100%` | `3.70` | `4` | `0.57%` | `186.6` | `232.0` |
| `mixed_core` | `1891.79` | `46.20%` | `100%` | `4.06` | `5` | `0.82%` | `268.1` | `347.3` |
| `mixed_shallow` | `2198.25` | `53.68%` | `100%` | `3.80` | `4` | `0.66%` | `215.2` | `273.1` |

## Reading The Result

The main pattern is consistent across both population sizes.

### 1. These slope families stay very local

All families committed the origin `100%` of the time, but only a very small
fraction of the population reached final commit:

- about `3% - 7%` at `1024` nodes
- below `1%` at `4096` nodes

So this topology family strongly limits the activated set.

### 2. More shallow slopes make the graph denser and the committed set smaller

As the far-end probability rises:

- average degree rises
- origin commit gets a bit faster
- committed-set size gets smaller
- message counts go down

In this model, the denser graphs are not causing wider spread. They are causing
the transaction to settle inside a smaller local patch more quickly.

### 3. Variation in slope matters less than the overall density band

The mixed-center families behave similarly to the high-center families once
their realized degree percent lands in the same rough band.

So for this family, the strongest driver is not heterogeneity by itself. It is
where the realized degree percent ends up:

- around `46% - 59%` in this sweep

### 4. The origin remains on a human-timescale path

Origin commit stays in a narrow range:

- about `3.6 - 4.1` rounds at `1024`
- about `3.7 - 4.1` rounds at `4096`

So within this dense-slope family, slope variation did not destabilize origin
settlement.

## Takeaway

This final experiment says something useful:

- if we define linear slopes across the full ring and keep the center near `1`,
  the resulting graph is very dense
- in that dense regime, transactions stay highly local
- slope variation changes cost a bit, but does not fundamentally change the
  behavior

So if the goal is to study the impact of slope variation itself, the next better
experiment would be to apply the slope only over a **bounded local domain** or
to combine it with an explicit sparse tail. Otherwise the center-near-`1`
condition dominates the result by forcing the whole graph into a high-density
band.
