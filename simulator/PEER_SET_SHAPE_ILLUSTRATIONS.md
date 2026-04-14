# Peer-Set Shape Illustrations

This note gives a visual and numerical comparison of the peer-set shapes we have tested.

The goal is to make two things easier to see:

1. what the **corrected ring** actually looks like around a node
2. how that differs from the **core + flat tail** and the too-flat **pairwise probabilistic** shape

The horizontal axis is **ring-rank distance** from a node's own position.

- `d = 1` means the closest peer on one side
- `d = 8` means the 8th closest peer on one side
- `d = N/2` is the far side of the ring

The vertical axis is best read as **connection strength / link density**:

- high = very likely or guaranteed to be connected
- low = sparse or rare
- zero = no connection expected

## Shape Overview

### 1. Corrected Ring

Current definition:

- closest `±8` peers: always connected
- next `±8` peers: linearly fading probability
- everything beyond that: no initial edge

For `neighbors = 8`, the one-sided profile is:

| Distance `d` | Connection rule | Probability |
| --- | --- | ---: |
| `1 .. 8` | guaranteed core | `1.0` |
| `9` | fade | `7/8 = 0.875` |
| `10` | fade | `6/8 = 0.750` |
| `11` | fade | `5/8 = 0.625` |
| `12` | fade | `4/8 = 0.500` |
| `13` | fade | `3/8 = 0.375` |
| `14` | fade | `2/8 = 0.250` |
| `15` | fade | `1/8 = 0.125` |
| `16+` | outside support | `0.0` |

Illustration:

```text
connection strength
1.0 | █ █ █ █ █ █ █ █
0.9 |                 ▇
0.8 |
0.7 |                   ▆
0.6 |                     ▅
0.5 |                       ▄
0.4 |                         ▃
0.3 |                           ▂
0.2 |                             ▁
0.1 |
0.0 | . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . .
      1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 ... distance from node center
```

This is a **plateau + cliff** shape.

### 2. Core + Flat Tail

This keeps the same corrected-ring near field, then adds a few evenly spaced long-range links.

Current test shape:

- corrected ring near field
- plus `4` tail peers per side beyond the fade band

For `N = 1024`, one side of the ring has width `512`, and the extra tail offsets come out roughly at:

- `d ≈ 116`
- `d ≈ 215`
- `d ≈ 314`
- `d ≈ 413`

Illustration:

```text
connection strength
1.0 | █ █ █ █ █ █ █ █
0.9 |                 ▇
0.8 |
0.7 |                   ▆
0.6 |                     ▅
0.5 |                       ▄
0.4 |                         ▃
0.3 |                           ▂
0.2 |                             ▁
0.1 |                                                  ·         ·         ·         ·
0.0 | . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . .
      1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 ...                          ... N/2
```

This is a **plateau + cliff + tiny distant spikes** shape.

### 3. Pairwise Probabilistic Ring

This was the too-flat experiment.

Definition:

- every pair is considered
- connection probability falls linearly with 64-bit ring distance
- no guaranteed local core

Illustration:

```text
connection strength
1.0 | █
0.9 | █
0.8 | ▇
0.7 | ▆
0.6 | ▅
0.5 | ▄
0.4 | ▃
0.3 | ▂
0.2 | ▁
0.1 | ·
0.0 | .
      center -----------------------------------------------> far side
```

This is a **broad triangle** shape.

It looks attractive for routing depth, but it over-connects badly and was clearly worse operationally.

## How Sparse Is The Corrected Ring Outside The Core?

This is the important quantitative point.

For `neighbors = 8`:

- dense core per side: `8` peers
- fade support per side: only `7` more possible positions with nonzero probability (`9 .. 15`)
- after `d = 15`, the shape is exactly zero

Expected connected peers per side:

```text
8 guaranteed
+ (7/8 + 6/8 + 5/8 + 4/8 + 3/8 + 2/8 + 1/8)
= 8 + 3.5
= 11.5 peers per side
```

So expected total connected degree is:

```text
2 * 11.5 = 23.0
```

That also means the corrected ring is **extremely sparse** outside the core.

### Example: `N = 1024`

One side of the ring is `512` ranks wide.

- guaranteed dense part: `8 / 512 = 1.56%`
- nonzero fade support extends only to `16 / 512 = 3.13%`
- beyond that, `96.9%` of the one-sided ring has **no initial edge at all**

### Example: `N = 2048`

One side of the ring is `1024` ranks wide.

- guaranteed dense part: `8 / 1024 = 0.78%`
- nonzero support extends only to `16 / 1024 = 1.56%`
- beyond that, `98.4%` of the one-sided ring has **no initial edge at all**

So the corrected ring is not merely "local". It is **sharply local**.

## Why The Core + Tail Still Counts As Sparse

The core + tail shape raises degree from about `23` to about `31`, but it is still sparse in the far field.

For `N = 1024`:

- one side has `512` possible rank positions
- corrected ring uses only the first `15` with any nonzero near-field support
- core + tail adds only `4` explicit long-range peers on that whole side

So the far-side long-range density is approximately:

```text
4 / (512 - 16) ≈ 0.8%
```

That is still sparse.

The issue is not that the tail is dense.
The issue is that even a **small** flat tail changes overlap and vote spread enough to raise message cost sharply.

## Inverted "Distance Goodness" View

If you prefer to read the picture as "high means easy / low means hard", then the same shapes look like this:

### Corrected ring

```text
easy
high  | ████████▇▆▅▄▃▂▁..............................
low   +--------------------------------------------->
        near center                              far
```

### Core + tail

```text
easy
high  | ████████▇▆▅▄▃▂▁..........·.........·....·....·
low   +--------------------------------------------->
        near center                              far
```

### Pairwise probabilistic

```text
easy
high  | █▇▆▅▄▃▂▁·····································
low   +--------------------------------------------->
        near center                              far
```

## What The Tests Say

The shape comparison matches the measurements:

- **corrected ring**:
  - steepest and most local
  - best message complexity of the tested shapes
- **core + flat tail**:
  - dramatically better shortest-path graph depth
  - but worse message complexity, and no latency win on the current protocol
- **pairwise probabilistic**:
  - too broad
  - over-connected
  - clearly worse

So the current working conclusion is:

- the corrected ring is sparse by design, especially outside `±16`
- that steep locality is not a bug; it is part of why message cost stays manageable
- if we want lower latency at large scale, we probably need better **protocol-stage routing**
  more than a flatter peer-set shape
