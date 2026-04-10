# Gradient Profile Comparison

This note compares the corrected ring-gradient steady-state baseline to the current integrated churn/lifecycle runs.

The goal is not to force the churn path to match the fixed ring exactly. The goal is to see how close the live network gets to the same *shape*:

- strongly local connections near the peer id
- sparse tail beyond the close neighborhood
- stable enough to preserve the steady-state transaction envelope

## Configurations Used

### Corrected ring-gradient steady-state

Two representative baselines were used:

1. `192` peers, `neighborhood_width = 6`
2. `122` peers, `neighborhood_width = 4`

Both use:

- `TopologyMode::Ring`
- guaranteed `±8` local neighbors
- linear fade-out to zero by `±16`
- `2` normal vote targets, first-wave `3`
- `cross_dc_normal`
- no joins, crashes, or returns

### Integrated churn/lifecycle

The lifecycle path is the long-run genesis-backed scenario:

- starts from `RandomIdentified`
- grows/shrinks through joins, crashes, and returns
- `neighborhood_width = 4`
- `2` normal vote targets, first-wave `3`
- `cross_dc_normal`

Two runs were compared:

1. no-conflict transaction flow
2. `25%` two-way conflict families

## Key Metric: Gradient Locality

`gradient locality` is the average locality coefficient of the live connected sets:

- `1.0` means very local peer sets
- lower values mean flatter, more globally spread connectivity

This is not a full graph-shape proof, but it is a useful first-order measure of whether the network is preserving a near-heavy / far-light profile.

## Corrected Steady-State Results

### `192` peers, width `6`

No-conflict baseline:

- avg connected peers: `23.0`
- gradient locality: `0.933`
- committed: `1303`
- pending: `197`
- latency: avg `9.6`, p95 `12`
- total block-message factor vs role-sum ideal: `6.20x`

Conflict baseline (`25%` families):

- avg connected peers: `23.0`
- gradient locality: `0.933`
- committed: `1159`
- pending: `695`
- latency: avg `9.7`, p95 `12`
- highest-majority families: `67`
- stalled-no-majority families: `249`
- lower-owner-commit families: `106`

### `122` peers, width `4` (matched target for churn comparison)

- avg connected peers: `23.1`
- gradient locality: `0.894`
- committed: `1008`
- pending: `192`
- latency: avg `10.3`, p95 `13`
- total block-message factor vs role-sum ideal: `8.84x`

This `122`-peer, width-`4` result is the most useful steady-state target for comparing the current long-run churn scenario.

## Steady-State Variant Checks On Corrected Ring

### Threshold `+1` on corrected ring (`192` peers, width `6`)

- gradient locality: `0.933`
- committed: `1315`
- pending: `185`
- latency: avg `8.5`, p95 `10`
- total block-message factor vs role-sum ideal: `2.90x`

This is still the strongest honest-load efficiency lever, but it remains safety-sensitive under conflict.

### `3` vote targets on corrected ring (`192` peers, width `6`)

- gradient locality: `0.933`
- committed: `1276`
- pending: `224`
- latency: avg `9.1`, p95 `11`
- wire messages: `14.47M`
- total block-message factor vs role-sum ideal: `4.11x`

On the corrected ring this remains mixed:

- latency is a bit better than the `+2, 2-target` baseline
- wire traffic is materially higher
- throughput did not improve

So it is still not a clean default.

## Integrated Churn Results

### No-conflict churn

- active peers at end: `122`
- avg connected peers: `69.4`
- gradient locality: final `0.543`, avg `0.553`, min `0.497`
- committed: `1206`
- pending: `594`
- latency: avg `19.0`, p50 `14`, p95 `35`
- recovery after crashes: `1` round, `1` round

### Conflict churn (`25%` families)

- active peers at end: `122`
- avg connected peers: `72.7`
- gradient locality: final `0.545`, avg `0.553`, min `0.488`
- committed: `1365`
- pending: `879`
- latency: avg `19.3`, p50 `13`, p95 `36`
- highest-majority families: `250`
- stalled-no-majority families: `173`
- lower-owner-commit families: `93`
- recovery after crashes: `1` round, `28` rounds

## Readout

The current lifecycle network does not maintain a ring-gradient profile close to the corrected steady-state target.

Matched comparison:

- target steady-state (`122` peers, width `4`): locality `0.894`, avg connected `23.1`
- live churn no-conflict: locality `0.553`, avg connected `69.4`
- live churn conflict: locality `0.553`, avg connected `72.7`

So the churn path is doing two things at once:

1. it is keeping *far more* connected peers than the idealized gradient target
2. those connected peer sets are much less local than the target

That helps explain why:

- the network remains robust and heals quickly
- but it does not recover toward the steady-state efficiency envelope

In short, the current lifecycle path is biased toward broad connectivity and liveness rather than preserving a steep local gradient.

## Assessment

This is a good result in one important sense: the network is not failing to connect. It is over-connecting.

That means the next improvement is probably not "how do we get enough peers?" but rather:

- how do we prune or shape connected sets more aggressively toward locality
- without hurting churn recovery and onboarding

The corrected ring benchmark now gives a clearer target for that work.
