# Historical Experiment Reports

This directory keeps older one-off experiment reports that were useful during protocol exploration
but are no longer part of the main current documentation set.

The top-level `simulator/` reports now focus on the current working baseline:

- integrated lifecycle behavior
- steady-state benchmark baselines
- corrected ring-gradient target
- churn graph control
- current conflict benchmark

The reports kept here are still useful as experiment history, especially for:

- vote-flow and batching iterations
- neighborhood-width and topology sweeps
- earlier steady-state benchmark variants
- intermediate protocol branches that were later simplified or superseded

## Current Groups

### Vote-flow / batching iterations

- `DELAYED_VOTE_REPLY_REPORT.md`
- `PREFER_UNHEARD_TARGETS_REPORT.md`
- `VOTE_TARGET_COUNT_REPORT.md`
- `BATCHING_PHASE_COMPARISON_REPORT.md`
- `STATE_CHANGE_REPLY_REPORT.md`
- `SIMPLIFIED_RESPONSE_FLOW_REPORT.md`

### Neighborhood and topology sweeps

- `ORGANIC_NEIGHBORHOOD_REPORT.md`
- `NEIGHBORHOOD_WIDTH_SWEEP_REPORT.md`
- `ADAPTIVE_NEIGHBORHOOD_REPORT.md`
- `STEADY_STATE_POLLING_AND_TOPOLOGY_REPORT.md`

### Older baseline variants

- `SPARSE_STEADY_STATE_REPORT.md`
- `STEADY_STATE_TUNING_REPORT.md`
- `LONG_RUN_PROFILE_COMPARISON.md`

These documents may still contain useful details, but they should be treated as historical
experiment notes rather than the current protocol or simulator baseline.
