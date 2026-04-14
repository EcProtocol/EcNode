# Reactive Vote Simulation Reference

This note describes the current simplified transaction model in
[`reactive_vote_simulation.py`](./reactive_vote_simulation.py).

The original [`vote_simulation.py`](./vote_simulation.py) is preserved because
it explores a different propagation problem and should stay available as a
separate reference.

## Purpose

The Rust simulators mix together several effects:

- peer formation and churn
- block transfer and sync
- routing
- batching
- conflict handling

This Python model is narrower by design. It is meant to answer:

> given a chosen peer topology, what should the simplified reactive vote /
> commit path do if we strip away retries, block-fetch overhead, and churn?

## Current Simplified Rules

Assumptions:

1. perfect one-round message delay
2. no block-fetch path
3. no periodic repair wave
4. one transaction at a time
5. two message types only:
   - `Vote`
   - `Commit`

Per transaction:

1. The origin learns the transaction at round `0`.
2. On first learn, a node sends one reactive `Vote` wave toward peers chosen
   around each role center in its own peer table.
3. Every sender of a `Vote` is stored as the path back.
4. A sender counts for role `r` at receiver `n` only if the sender is inside
   `n`'s own local range around role center `r`.
5. Role counting is therefore entirely receiver-local.
6. When all role counters reach threshold (`+2`), the node commits.
7. On commit, the node sends `Commit` to all stored vote senders.
8. If an already committed node later receives a `Vote`, it immediately answers
   with `Commit`.

So the model is:

- receiver-side counting
- one-shot reactive seeding
- reflected commits on full transaction commit

## Important Simplification

The message structure still carries a role mask, but the current counting rule
does **not** depend on that mask.

What matters in the current model is:

- who sent the message
- whether that sender is inside the receiver's local range for each role

That is the intentional simplification used by the current topology sweeps.

## Topology Families In Use

The active topology comparisons now live in:

- [`reactive_vote_topology_sweep.md`](./reactive_vote_topology_sweep.md)
- [`heterogeneous_linear_slope_report.md`](./heterogeneous_linear_slope_report.md)

Current families under active comparison include:

- `full_table`
- `random_uniform`
- `linear_probability`
- `linear_probability_with_core`
- `ring_core_tail`
- `ring_stepwise_sample`
- `heterogeneous_linear_slope`

## Core Sanity Bound

With one-shot seeding, vote traffic should remain linear in the number of
reached nodes.

If:

- each reached node seeds at most once
- each seed wave sends to at most `2 * targets_per_side` peers per role
- a transaction has `R` roles

then:

```text
total_votes <= 2 * targets_per_side * R * reached_nodes
```

For the common current case:

- `targets_per_side = 2`
- `R = 2`

the upper bound is:

```text
total_votes <= 8 * reached_nodes
```

That is still the first sanity check for this simulator.

## Metrics That Matter

The active sweeps now focus on:

- origin commit success
- origin commit round
- final committed-set size
- vote messages per transaction
- commit messages per transaction
- how many sampled transactions each node participates in

Those are the metrics that matter for the current sharding / locality question:

- do transactions stay local?
- does the origin still commit fast?
- how much of the network gets pulled into each transaction?

## Current Read

At this point the simulator is useful for:

- topology comparison
- fanout comparison
- scaling experiments on activation fraction and message cost
- controlled replay experiments that hold role centers fixed while varying
  origin points

It is intentionally **not** yet a model of:

- retries / repair
- block-request delay
- churn
- batching
- conflict handling

Those belong in the Rust simulators, not in this stripped-down reference.
