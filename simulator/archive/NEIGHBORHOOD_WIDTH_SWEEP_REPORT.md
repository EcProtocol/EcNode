# Neighborhood Width Sweep Report

This report explores the fixed-width neighborhood parameter around the current default of `4`.

The goal was to answer a simple question:

If we shrink or widen the local vote neighborhood, does the system become cheaper or more expensive in practice?

## Sweep Setup

All runs used the same integrated long-run lifecycle scenario from [integrated_long_run.rs](/workspaces/ecRust/simulator/integrated_long_run.rs) with:

- genesis bootstrap
- `96` initial peers
- join / crash / return churn
- `3` submitted blocks per round
- `connected-only` transaction sources
- `cross_dc_normal` network profile
- fixed seed variant `0`

To keep the sweep tractable, the run length was reduced to `1600` rounds instead of the full `2400`.

Widths tested:

- `2`
- `3`
- `4`
- `5`
- `6`

## Results

| Width | Commits | Pending | Messages delivered | Messages / commit | Coverage avg | Eligible avg | Reachable graph avg | Settled spread avg | Block-msg avg | Commit latency avg | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 2951 | 1849 | 174.3M | 59,066.1 | 10.8 | 5.0 | 17.4 | 63.3 | 15,445.9 | 91.4 | 15 | 534 |
| 3 | 3129 | 1671 | 100.7M | 32,177.9 | 14.8 | 7.0 | 17.9 | 57.8 | 7,338.9 | 59.0 | 12 | 416 |
| 4 | 3361 | 1439 | 53.7M | 15,977.5 | 18.7 | 9.0 | 17.7 | 54.3 | 2,970.1 | 35.7 | 11 | 194 |
| 5 | 3550 | 1250 | 36.6M | 10,298.7 | 22.6 | 10.9 | 18.0 | 53.6 | 2,098.2 | 31.1 | 11 | 148 |
| 6 | 3530 | 1270 | 29.1M | 8,248.4 | 26.1 | 12.8 | 18.0 | 52.1 | 1,312.4 | 24.5 | 11 | 103 |

## What Changed

### External neighborhood size grew as expected

Average external token coverage increased with width:

- width `2`: `10.8`
- width `4`: `18.7`
- width `6`: `26.1`

The local eligible vote set also scaled up in the expected way:

- width `2`: `5.0`
- width `4`: `9.0`
- width `6`: `12.8`

So the knob is definitely working.

### The static reachable vote graph barely moved

Average reachable vote graph size stayed almost flat:

- width `2`: `17.4`
- width `6`: `18.0`

That is important. It says the initial graph that a block can route through is not the main thing exploding here.

### Settlement work changed a lot

Even though the static graph stayed flat, actual settlement work changed dramatically:

- messages per commit fell from `59,066.1` at width `2` to `8,248.4` at width `6`
- block-related messages to settle fell from `15,445.9` to `1,312.4`
- average commit latency fell from `91.4` rounds to `24.5`
- committed blocks increased from `2951` to `3530`

From width `2` to width `6`:

- external coverage grew `+141.7%`
- messages per commit fell `-86.0%`
- average commit latency fell `-73.2%`
- committed blocks rose `+19.6%`

From width `4` to width `6`:

- messages per commit fell `-48.4%`
- average commit latency fell `-31.4%`
- committed blocks rose `+5.0%`

## Assessment

The sweep does **not** support the idea that simply shrinking the neighborhood width will reduce baseline cost in the current implementation.

It points the other way:

- narrower fixed neighborhoods create *more* work
- wider fixed neighborhoods make settlement *cheaper and faster*

That is counterintuitive if you only reason from nominal voter count. But it makes sense once you look at the whole system:

- smaller vote windows make it harder for blocks to get decisive local information quickly
- unresolved blocks stay live longer
- retries and propagation churn accumulate
- total message work rises sharply

So right now the dominant cost is not “too many eligible local voters”.
It is “not enough decisive local progress, so the block keeps circulating”.

## Why This Does Not Kill The Adaptive Idea

Your adaptive idea is still very much alive:

- wide neighborhoods near the token
- narrower neighborhoods farther away

This sweep only says that **globally shrinking the width everywhere** is a bad trade in the current system.

That is a different question from:

Can we keep decisive local neighborhoods near the token while reducing proxy churn far away?

That second question is still promising.

## Important Interaction: Fast Reply

There is one major caveat before we test adaptive width.

The current pending fast-reply behavior can let a node quickly answer based on its local token view even if it is really only acting as a proxy for a far-away token.

That likely biases results against narrower far-away proxy paths, because such nodes may:

- not truly host the token
- not have prior token state
- still produce an immediate local opinion

For an adaptive experiment, I think the right companion rule is:

- fast-reply only if the node has prior state for that token
- otherwise forward/query, but do not immediately cast a local opinion

That would separate:

- routing/proxy behavior
- actual neighborhood voting behavior

much more cleanly.

## Recommendation

Do **not** reduce the fixed neighborhood width below `4` based on this data.

If anything, the current implementation is healthier at `5` or `6` than at `4`.

The better next experiment is:

1. Keep a healthy local width near the token neighborhood.
2. Narrow only the far-away proxy path.
3. Pair that with state-aware fast-reply so proxies do not vote just because they have seen the block.

That will test the real design idea rather than the cruder “shrink the whole neighborhood everywhere” version.
