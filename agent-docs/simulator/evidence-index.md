# Simulator Evidence Index

## Protocol Goal

Simulator evidence should make protocol progress inspectable and reproducible. Empirical claims should point to a report, command, scenario/config, parameters, and deterministic seed when available.

## Current Status

Current high-value reports:

- [simulator/STEADY_STATE_REPORT.md](../../simulator/STEADY_STATE_REPORT.md): Snapshot of best-case steady-state performance so far.
- [Design/viability_assessment.md](../../Design/viability_assessment.md): Fairly current progress summary and viability framing.
- [agent-docs/peers/peer-shape-target.md](../peers/peer-shape-target.md): Current synthesis of peer-set target shape evidence and scanner metrics.
- [simulator/RING_TOPOLOGY_CORRECTION_REPORT.md](../../simulator/RING_TOPOLOGY_CORRECTION_REPORT.md): Corrected ring-gradient semantics and baseline after replacing the older hard-neighbor interpretation.
- [simulator/DENSE_LINEAR_TOPOLOGY_REPORT.md](../../simulator/DENSE_LINEAR_TOPOLOGY_REPORT.md): Dense linear fixed-topology comparison, including routing/convergence gains and message-cost regressions.
- [simulator/CORE_TAIL_TOPOLOGY_REPORT.md](../../simulator/CORE_TAIL_TOPOLOGY_REPORT.md): Core plus flat-tail test showing graph route-depth improvements without commit-latency/message-cost improvement.
- [simulator/PEER_LIFECYCLE_GRAPH_SHAPE_REPORT.md](../../simulator/PEER_LIFECYCLE_GRAPH_SHAPE_REPORT.md): Lifecycle shape formation, referral discovery, core/fade/far metrics, and integrated sanity checks.
- [simulator/CHURN_GRAPH_CONTROL_REPORT.md](../../simulator/CHURN_GRAPH_CONTROL_REPORT.md): Churn graph-control experiments showing why degree control alone is too blunt and core-preserving pruning is healthier.
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
- Peer-shape reports use different target metrics over time; prefer [agent-docs/peers/peer-shape-target.md](../peers/peer-shape-target.md) for the current interpretation before comparing old numbers directly.
- The peer-shape rerun policy and target/diagnostic split are centralized in [agent-docs/peers/peer-shape-target.md](../peers/peer-shape-target.md#rerun-matrix).

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
- When a simulator result changes the current interpretation, update the matching `agent-docs/` topic with the finding and decision; do not leave the decision only in a report or chat thread.
- For simulator-only code changes, run the most relevant example or scenario when practical.
- Long empirical runs should generally use `--release`.
