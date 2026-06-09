# Simulator Evidence Index

## Protocol Goal

Simulator evidence should make protocol progress inspectable and reproducible. Empirical claims should point to a report, command, scenario/config, parameters, and deterministic seed when available.

## Current Status

Current high-value reports:

- [simulator/STEADY_STATE_REPORT.md](../../simulator/STEADY_STATE_REPORT.md): Snapshot of best-case steady-state performance so far.
- [Design/viability_assessment.md](../../Design/viability_assessment.md): Fairly current progress summary and viability framing.
- [simulator/INTEGRATED_LONG_RUN_REPORT.md](../../simulator/INTEGRATED_LONG_RUN_REPORT.md): Integrated long-run simulator evidence.
- [simulator/INTEGRATED_CHURN_CONFLICT_REPORT.md](../../simulator/INTEGRATED_CHURN_CONFLICT_REPORT.md): Integrated churn/conflict behavior.
- [simulator/ADVERSARIAL_CONFLICT_REPORT.md](../../simulator/ADVERSARIAL_CONFLICT_REPORT.md): Adversarial conflict-oriented evidence.

Useful executable entry points:

- `cargo run --example basic_simulation`
- `cargo run --example peer_lifecycle_sim`
- `cargo run --example commit_chain_sim`
- `cargo run --example integrated_simulation`
- `cargo run --release --example integrated_long_run`
- `cargo run --release --example integrated_steady_state`
- `cargo run --bin scenario_runner scenarios/bootstrap.yaml`
- `cargo run --bin scenario_runner scenarios/`

## Known Gaps

- Reports are snapshots and may not reflect later code changes.
- Some reports live near simulator code, while older analysis lives in `Design/`.
- Not every report is tied to an easily replayable scenario file yet.
- Some deterministic behavior is still an open issue, including hash map iteration in simulations.

## Primary Files

- [simulator/consensus/](../../simulator/consensus)
- [simulator/peer_lifecycle/](../../simulator/peer_lifecycle)
- [simulator/integrated/](../../simulator/integrated)
- [simulator/commit_chain/](../../simulator/commit_chain)
- [simulator/scenario_runner.rs](../../simulator/scenario_runner.rs)
- [scenarios/bootstrap.yaml](../../scenarios/bootstrap.yaml)

## Source Material

- [README.md](../../README.md)
- [simulator/README.md](../../simulator/README.md)
- [Design/SCENARIO_ANALYSIS.md](../../Design/SCENARIO_ANALYSIS.md)
- [Design/PROFILING.md](../../Design/PROFILING.md)

## Agent Notes

- Prefer deterministic seeds and explicit scenario parameters when adding or changing simulations.
- Treat the newest relevant report plus executable config as the best simulator source of truth.
- For simulator-only code changes, run the most relevant example or scenario when practical.
- Long empirical runs should generally use `--release`.

