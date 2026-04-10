# Batching Phase Comparison

This report compares three batching modes on the same fixed-seed integrated long-run scenario:

- `off`: batching disabled
- `phase1`: request batching enabled for `Vote(reply=true)`, `QueryBlock`, and `QueryToken`
- `phase2`: `phase1` plus batching of `Vote(reply=false)` fast replies

All runs used:

- `800` rounds
- genesis bootstrap
- `cross_dc_normal` or `cross_dc_stressed`
- neighborhood width `6`
- vote targets `2`
- existing-token workload target `50%`
- seed variant `0`

## Commands

```bash
EC_LONG_RUN_ROUNDS=800 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_NEIGHBORHOOD_WIDTH=6 \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_BATCHING=false \
cargo run --release --quiet --example integrated_long_run

EC_LONG_RUN_ROUNDS=800 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_NEIGHBORHOOD_WIDTH=6 \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_long_run

EC_LONG_RUN_ROUNDS=800 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_NEIGHBORHOOD_WIDTH=6 \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_BATCHING=true \
EC_LONG_RUN_BATCH_VOTE_REPLIES=true \
cargo run --release --quiet --example integrated_long_run

EC_LONG_RUN_ROUNDS=800 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_stressed \
EC_LONG_RUN_NEIGHBORHOOD_WIDTH=6 \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_BATCHING=false \
cargo run --release --quiet --example integrated_long_run

EC_LONG_RUN_ROUNDS=800 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_stressed \
EC_LONG_RUN_NEIGHBORHOOD_WIDTH=6 \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_long_run

EC_LONG_RUN_ROUNDS=800 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_stressed \
EC_LONG_RUN_NEIGHBORHOOD_WIDTH=6 \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_BATCHING=true \
EC_LONG_RUN_BATCH_VOTE_REPLIES=true \
cargo run --release --quiet --example integrated_long_run
```

## Results

### `cross_dc_normal`

| Mode | Committed / Pending | Logical Delivered | Wire Delivered | Wire Saved | Peak In-Flight | Latency avg / p50 / p95 | Total vs Role Ideal | Total vs Coalesced Ideal |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| `off` | `1680 / 720` | `15,228,248` | `15,228,248` | `0.0%` | `95,799` | `24.3 / 15 / 62` | `8.74x` | `14.70x` |
| `phase1` | `1673 / 727` | `15,781,021` | `10,132,025` | `35.8%` | `58,707` | `21.6 / 14 / 51` | `7.71x` | `12.76x` |
| `phase2` | `1677 / 723` | `17,203,510` | `7,534,090` | `56.2%` | `26,800` | `24.2 / 15 / 55` | `9.07x` | `15.09x` |

### `cross_dc_stressed`

| Mode | Committed / Pending | Logical Delivered | Wire Delivered | Wire Saved | Peak In-Flight | Latency avg / p50 / p95 | Total vs Role Ideal | Total vs Coalesced Ideal |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| `off` | `1653 / 747` | `16,749,368` | `16,749,368` | `0.0%` | `203,658` | `35.3 / 24 / 80` | `10.07x` | `16.83x` |
| `phase1` | `1586 / 814` | `20,158,051` | `12,338,661` | `38.8%` | `126,655` | `33.4 / 23 / 62` | `9.89x` | `16.87x` |
| `phase2` | `1594 / 806` | `17,994,153` | `8,138,920` | `54.8%` | `55,088` | `33.6 / 24 / 60` | `9.73x` | `16.36x` |

## Readout

### Phase 1

Phase 1 batching is a clean win on transport cost:

- wire messages drop by about `33.5%` in `cross_dc_normal`
- wire messages drop by about `26.3%` in `cross_dc_stressed`
- queue depth drops by about `38%` in both profiles

It also improves latency:

- `cross_dc_normal`: p95 `62 -> 51`
- `cross_dc_stressed`: p95 `80 -> 62`

The tradeoff is that it does not improve committed throughput in this `800`-round run. Commit count stayed close in `normal` and fell in `stressed`.

### Phase 2

Phase 2 batching extends the wire savings much further:

- wire messages drop by about `50.5%` versus `off` in `cross_dc_normal`
- wire messages drop by about `51.4%` versus `off` in `cross_dc_stressed`
- queue depth drops by about `72%` in both profiles

But the protocol-level picture is mixed:

- `cross_dc_normal`: latency mostly regresses back toward the unbatched case and the ideal-gap factors get worse than both `off` and `phase1`
- `cross_dc_stressed`: p95 improves slightly beyond `phase1`, but average latency is flat and commit count still stays below `off`

The important distinction is:

- `phase2` is much better at reducing wire frames
- `phase1` remains better at improving the overall settlement path in `cross_dc_normal`

## Assessment

The current default should stay at `phase1`:

- `enable_request_batching = true`
- `batch_vote_replies = false`

That gives a good first transport win without masking protocol churn.

`phase2` is still valuable as an experiment because it shows the wire-level headroom available once vote replies are coalesced too. But with the current protocol behavior it does not yet translate into better overall efficiency in the `normal` profile.

The likely next step is to keep `phase1` as the default baseline and explore whether `phase2` becomes more attractive when paired with more structured outbound scheduling, rather than simply widening the current opportunistic batcher.
