# Simulator Operations And Development

## Protocol Goal

Simulator work should make protocol claims testable. Development changes should either preserve existing evidence or produce new, reproducible evidence.

## Current Status

Simulator code is part of this repository through Cargo examples and binaries. It is the main empirical feedback loop for protocol behavior.

## Ways Of Working

- Prefer deterministic seeds and explicit scenario parameters.
- Prefer YAML scenarios or named example configs over one-off manual setup.
- When recording results, include command, parameters, seed, code commit/context, and what question the run answers.
- Use `--release` for long empirical runs.
- Keep generated artifacts small unless the user explicitly wants a long run or report.
- Put simulator observability behind event sinks, diagnostics, stats, or CSV export rather than printing from core protocol logic.
- Treat reports as empirical snapshots. The newest relevant report plus executable config is the simulator source of truth.

## Development Checks

For simulator-only changes, run the most relevant command when practical:

```bash
cargo run --example basic_simulation
cargo run --example peer_lifecycle_sim
cargo run --example commit_chain_sim
cargo run --example integrated_simulation
cargo run --release --example integrated_steady_state
cargo run --bin scenario_runner scenarios/bootstrap.yaml
```

For core protocol changes that affect simulator behavior, run `cargo test` first, then a focused simulator command.

## Known Gaps

- Some simulator paths may still have nondeterministic behavior from unordered iteration.
- Report format is not yet standardized.
- Not all empirical claims have a replayable scenario file.

## Primary Files

- [simulator/](../../simulator)
- [scenarios/](../../scenarios)
- [simulator/scenario_runner.rs](../../simulator/scenario_runner.rs)
- [simulator/consensus/event_sinks.rs](../../simulator/consensus/event_sinks.rs)

## Source Material

- [VERIFICATION.md](../../VERIFICATION.md)
- [agent-docs/simulator/evidence-index.md](evidence-index.md)
- [agent-docs/simulator/reproducibility.md](reproducibility.md)

## Agent Notes

If a simulator run is too expensive for the current task, say which command would be most relevant and why it was not run.

