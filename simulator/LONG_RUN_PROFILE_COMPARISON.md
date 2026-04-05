# Long-Run Profile Comparison

This report compares the same fixed-seed integrated lifecycle scenario under three network profiles:

- `perfect`
- `cross_dc_normal`
- `cross_dc_stressed`

The goal is to estimate how much of the current cost comes from network conditions versus protocol/message amplification.

## Scenario

- runner: `integrated_long_run`
- rounds: `1600`
- seed variant: `0`
- bootstrap: genesis-backed
- initial peers: `96`
- churn:
  - `+24` joins at round `320`
  - `-12` crashes at round `800`
  - `+8` returns at round `933`
  - `+16` joins at round `1120`
  - `-10` crashes at round `1333`
- peer neighborhood width: `6`
- transaction source policy: `connected-only`
- transaction workload target: `50%` existing-token updates / `50%` new tokens
- actual achieved workload mix:
  - perfect: `50.0%` existing-token parts
  - normal: `50.2%`
  - stressed: `49.5%`

## Lower Bounds

Two lower bounds are reported:

- role-sum lower bound
  - `2 * sum(token-neighborhood sizes) + 2 * witness-neighborhood size`
  - this matches the requested "2 per token + 2 for witness" structural formula
- coalesced lower bound
  - `2 * size(union(token neighborhoods ∪ witness neighborhood))`
  - this is the best case if one request and one reply can cover the whole block per peer

Actual block-message counts are measured until the owner node commits the block, not until every structural peer has converged. Because of that, some individual blocks can still land below the theoretical lower bound, but the total ratios are still useful for comparing runs.

## Results

| Profile | Committed / Pending | Delivered Messages | Peak In-Flight | Commit Latency avg / p50 / p95 (rounds) | Network Transit avg / p95 (rounds) | Block Messages avg / p95 | Total vs Role-Sum Ideal | Total vs Coalesced Ideal |
| --- | --- | ---: | ---: | --- | --- | --- | --- | --- |
| `perfect` | `3334 / 1466` | `61,645,211` | `115,167` | `33.2 / 12 / 165` | `1.0 / 1` | `3518.2 / 14105` | `21.19x` | `32.39x` |
| `cross_dc_normal` | `3370 / 1430` | `52,627,037` | `184,130` | `37.9 / 16 / 193` | `1.7 / 3` | `3123.8 / 13398` | `19.38x` | `29.14x` |
| `cross_dc_stressed` | `3337 / 1463` | `64,878,831` | `354,666` | `56.9 / 25 / 272` | `3.5 / 6` | `4982.8 / 22708` | `32.66x` | `48.95x` |

## Early vs Late Phase

Checkpoint snapshots show the network starts in a much healthier region and degrades mainly through queue growth and vote churn.

| Profile | Early p50 / p95 | Late p50 / p95 | Late In-Flight | Late Wait-Token-Votes |
| --- | --- | --- | ---: | ---: |
| `perfect` | `9 / 34` | `12 / 126` | `0` | `11,871` |
| `cross_dc_normal` | `12 / 32` | `16 / 145` | `53,466` | `10,461` |
| `cross_dc_stressed` | `20 / 42` | `25 / 220` | `198,672` | `11,820` |

The `perfect` profile has no network queue, but it still accumulates large pending vote demand. That is the clearest sign that the dominant bottleneck is protocol behavior, not WAN latency alone.

## Wall-Clock Translation

If one simulation round maps to wall-clock time:

| Profile | p50 @ 25ms | p95 @ 25ms | p50 @ 50ms | p95 @ 50ms |
| --- | --- | --- | --- | --- |
| `perfect` | `0.30s` | `4.13s` | `0.60s` | `8.25s` |
| `cross_dc_normal` | `0.40s` | `4.83s` | `0.80s` | `9.65s` |
| `cross_dc_stressed` | `0.63s` | `6.80s` | `1.25s` | `13.60s` |

The early healthy phase is closer to the "human timescale" target:

- perfect early p95: `34` rounds
  - `0.85s` at `25ms/round`
- normal early p95: `32` rounds
  - `0.80s` at `25ms/round`
- stressed early p95: `42` rounds
  - `1.05s` at `25ms/round`

So the current implementation can briefly operate near the intended UX target, but it does not yet hold that regime under sustained load and churn.

## Main Takeaways

1. Even the perfect network is far from ideal message cost.
   - Total block-message cost is still `21.19x` above the requested role-sum lower bound and `32.39x` above the coalesced per-peer lower bound.
   - That means message amplification is already dominant before WAN effects are added.

2. Mild WAN delay/loss does not simply make everything worse.
   - `cross_dc_normal` delivered fewer total messages than `perfect` and had a slightly lower total ideal-gap ratio.
   - The most likely explanation is that small transit delay passively damps some of the self-amplifying vote churn.

3. Heavier WAN stress does push the system clearly into a worse regime.
   - Peak in-flight queue nearly doubles from `184k` to `355k`.
   - Average commit latency rises from `37.9` to `56.9` rounds.
   - Total block-message cost rises from `29.14x` to `48.95x` over the coalesced ideal.

4. The best current neighborhood knob still looks like width `6`.
   - These runs use width `6` because earlier sweep results showed narrower or adaptive narrowing performed worse on the current workload.

5. Joiners look reasonable; rejoiners still have a realism gap.
   - Late-join time to connected stayed around `27-34` rounds.
   - Rejoiners still reached `Connected` before sync completion in all runs (`8/8`), so recovery metrics remain somewhat optimistic for stale returns.

## Interpretation

The current combined design appears capable of:

- genesis bootstrap
- steady transaction intake
- late join onboarding
- churn and return under active sync

But the present implementation is still too expensive in message load for the target latency envelope to hold reliably at this offered load.

The strongest conclusion from this comparison is:

- batching and resend discipline are still the highest-value next optimizations
- network quality matters, but it is not the main reason the system is off the ideal curve

## Commands

```bash
EC_LONG_RUN_ROUNDS=1600 \
EC_LONG_RUN_NEIGHBORHOOD_WIDTH=6 \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_NETWORK_PROFILE=perfect \
cargo run --release --quiet --example integrated_long_run

EC_LONG_RUN_ROUNDS=1600 \
EC_LONG_RUN_NEIGHBORHOOD_WIDTH=6 \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
cargo run --release --quiet --example integrated_long_run

EC_LONG_RUN_ROUNDS=1600 \
EC_LONG_RUN_NEIGHBORHOOD_WIDTH=6 \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_stressed \
cargo run --release --quiet --example integrated_long_run
```
