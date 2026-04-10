# Routing Depth Scaling

This note asks a simple question:

> If nodes maintain the intended steep local peer shape, how should route depth scale as the network grows from `10^3` to `10^6` nodes?

The answer depends strongly on what "steep local shape" really means.

Two different shapes can both look local at small scale:

- a **finite local gradient**
- a **dense local core plus sparse long-range routing tail**

Only the second one has a plausible path to near-stable routing depth at very large network sizes.

## Scope

This note builds on:

- [response_driven_commit_flow.md](./response_driven_commit_flow.md)
- [RING_TOPOLOGY_CORRECTION_REPORT.md](../simulator/RING_TOPOLOGY_CORRECTION_REPORT.md)
- [GRADIENT_PROFILE_COMPARISON.md](../simulator/GRADIENT_PROFILE_COMPARISON.md)
- [`simulator/peer_lifecycle/topology.rs`](../simulator/peer_lifecycle/topology.rs)

It is an asymptotic design note, not a proof from the current implementation.

## Current Corrected Ring Benchmark

The corrected ring benchmark currently used in steady-state simulation is:

- guaranteed local peers: `±c`
- fading tail: `±(c+1) .. ±2c`
- no edges beyond `±2c`

For the current simulator benchmark, this is effectively:

- `c = 8`
- guaranteed `±8`
- linear fade to zero by `±16`

This is a good benchmark for:

- locality
- neighborhood overlap
- steady-state transaction cost on a formed network

It is **not** a good asymptotic model for a million-node network.

Why:

- route progress per hop is bounded by a constant multiple of `c`
- if no peer knows anything meaningfully beyond `2c`, route depth grows roughly linearly with ring distance

So the current corrected ring is a good *locality target*, but not yet a *scalable routing target*.

## Two Candidate Shapes

### Model A: Finite Local Gradient

This is the current corrected-ring style:

```text
p(rank distance d) =
  1                      for d <= c
  linearly fades to 0    for c < d <= 2c
  0                      for d > 2c
```

This is strongly local, but route depth grows roughly like:

```text
depth ~ O(N / c)
```

because each hop can only move a bounded distance toward the target.

### Model B: Dense Core + Logarithmic Tail

This is the scalable target shape:

- keep all nearby peers within a small core
- keep only a few routing points in each larger distance band

The clean discrete version is:

```text
core:
  keep all peers with d <= c

tail:
  for each bucket [2^j c, 2^(j+1) c), keep s peers per side
```

This is "spiky" rather than smooth.

It can also be thought of as the discrete form of a heavier-tailed routing law, roughly closer to:

```text
p(d) ~ 1 / d
```

than to:

```text
p(d) ~ exp(-d / L)
```

That distinction matters.

## Why Exponential Is Not Enough

A symmetric exponential around the center looks attractive because it is very local:

```text
p(d) ~ exp(-d / L)
```

But it decays too fast.

That means:

- nearby peers are plentiful
- genuinely long jumps are very rare
- once `N >> L`, greedy routing depth still grows too quickly

So exponential is good for *locality*, but not for *global routing scalability*.

If the goal is to keep depth nearly stable across decades of network size, the tail must be heavier.

## Degree And Depth Under The Logarithmic-Tail Model

Let:

- `c` = guaranteed core peers per side
- `s` = routing points kept per side in each distance bucket
- `N` = total network size

Then the expected connected degree is approximately:

```text
K(N) ~= 2c + 2s * ceil(log2(N / (2c)))
```

This grows only logarithmically with network size.

Greedy routing depth toward a target neighborhood is approximately:

```text
d(target distance Δ) ~= ceil(log2(Δ / c))
```

For a random entry point and random target, a useful average-case approximation is:

```text
d_avg(N) ~= ceil(log2(N / (4c)))
```

This is not exact, but it gives the right scaling intuition.

## Example Numbers

Take a simple design point:

- `c = 8`
- `s = 1`

That means:

- keep `±8` true local peers
- keep one routing point per side in each distance octave beyond that

### Expected connected degree

| Network size | Expected degree/node |
| --- | ---: |
| `1,000` | `~28` |
| `10,000` | `~36` |
| `1,000,000` | `~48` |

So the degree budget stays moderate even as the network grows by three orders of magnitude.

### Expected route depth to host core

| Network size | Avg depth | Median-ish depth | P95-ish depth |
| --- | ---: | ---: | ---: |
| `1,000` | `~5` | `~5` | `~6` |
| `10,000` | `~8-9` | `~8` | `~10` |
| `1,000,000` | `~15-16` | `~15` | `~16-17` |

This is not flat, but it is slow-growing enough to stay plausible.

That is the main reason the logarithmic-tail model is attractive.

## Contrast With The Finite Local Gradient

If the graph stays purely local with no useful long-range tail, progress per hop stays bounded by a constant.

Then route depth grows roughly like:

```text
depth ~ average ring distance / average progress per hop
```

Since average ring distance is about `N / 4`, this becomes:

```text
depth ~ O(N / c)
```

With `c = 8`, even optimistic constant-progress estimates become too large very quickly:

| Network size | Local-only depth scale |
| --- | ---: |
| `1,000` | tens of hops |
| `10,000` | hundreds of hops |
| `1,000,000` | tens of thousands of hops |

So the current corrected ring is not enough by itself for very large networks.

It needs to be understood as:

- a local benchmark
- not the full asymptotic routing target

## What This Implies For The Design

If the intended network must scale to `10^6+` nodes while keeping direct base-layer transactions useful, the target peer shape should be:

1. **Dense local core**
   - enough nearby peers to host tokens robustly and settle votes quickly

2. **Sparse logarithmic routing tail**
   - only a few peers per distance octave
   - enough to keep greedy routing depth logarithmic

3. **No broad probabilistic fog**
   - too many medium/far peers flatten the graph and increase message cost

So the desired graph is:

- steep near the center
- but not strictly truncated

This is the important distinction.

## Timing Implications

From [response_driven_commit_flow.md](./response_driven_commit_flow.md), the current conflict-free timing model for one role is approximately:

```text
T_role,current ~= 5d + 1 rounds
```

where `d` is the inward depth to the host core.

Using the depth estimates above:

| Network size | Approx role rounds | At `25 ms/round` | At `50 ms/round` |
| --- | ---: | ---: | ---: |
| `1,000` | `~26` | `~0.65s` | `~1.3s` |
| `10,000` | `~41-46` | `~1.0-1.15s` | `~2.0-2.3s` |
| `1,000,000` | `~76-81` | `~1.9-2.0s` | `~3.8-4.0s` |

That is still within a range that looks plausible for an open global network.

It is not ideal, but it is far better than what a purely local-only graph would imply.

## What A More Reactive Model Means

The current timing model is deliberately honest to the implementation:

- receive `Vote`
- `QueryBlock`
- receive `Block`
- wait for tick-driven forwarding
- next hop sees the forwarded `Vote`

That is why one inward routing step currently costs several rounds rather than one transport delay.

A **more reactive model** means reducing how much routing progress waits for the next tick.

### Current model

Rough per-step picture:

```text
Vote arrives
-> QueryBlock immediately
-> Block returns
-> next outward vote wave waits on tick / outbox cycle
```

This is roughly why:

```text
T_role,current ~= 5d + 1
```

### More reactive model: what changes

The protocol becomes more reactive if one or more of these happen:

1. **Forward immediately after block arrival**
   - once a proxy has the block and knows the next targets, it does not wait for the next normal sweep tick

2. **Push terminal state immediately**
   - `Pending -> Commit` or `Pending -> Blocked` sends replies immediately rather than waiting for the next round boundary

3. **Separate repair from progress**
   - the periodic sweep stays, but only as a repair path
   - normal healthy progress comes from receive-side and state-change triggers

4. **Reuse active fetches**
   - one block fetch satisfies multiple waiting interests
   - later arrivals for the same block do not restart the same path

5. **Piggyback routing hints**
   - when conflict or better candidates are known, replies can carry enough information to redirect the receiver without a full extra discovery cycle

6. **Batch without delaying**
   - batching should compress transport, not postpone progress to the next coarse timer

### Latency effect of a more reactive model

If the next forwarding step can be triggered on block arrival rather than waiting for the periodic poll cycle, the inward step cost can shrink.

A reasonable target model would be closer to:

```text
T_role,reactive ~= 3d + 1 rounds
```

and a more aggressive ideal could approach:

```text
T_role,more-reactive ~= 2d + 1 rounds
```

Under those models, the same `c = 8`, `s = 1` example becomes:

| Network size | `3d + 1` rounds | `2d + 1` rounds |
| --- | ---: | ---: |
| `1,000` | `~16` | `~11` |
| `10,000` | `~25-28` | `~17-19` |
| `1,000,000` | `~46-49` | `~31-33` |

At `25 ms/round`, the million-node case would then move from about `2s` toward about `0.8s-1.2s`.

That is the real importance of becoming more reactive:

- not only fewer messages
- but lower latency growth as routing depth increases

## Practical Takeaway

The current evidence suggests three things:

1. The corrected ring benchmark is a good *locality benchmark*, but not the full large-scale routing model.
2. A scalable target likely needs:
   - dense local core
   - sparse logarithmic tail
3. If that graph exists, route depth can grow slowly enough to stay plausible even at very large `N`.

So the right asymptotic design target is not:

- "smooth exponential around the center"

but closer to:

- "dense core plus sparse bucketed routing points"

That is the shape most compatible with:

- strong locality
- bounded degree
- logarithmic routing depth
- human-timescale latency in a global open network
