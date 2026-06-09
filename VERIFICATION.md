# Verification

Use stable Rust and Cargo.

## Baseline Commands

```bash
cargo build
cargo check
cargo fmt --check
cargo fmt
cargo clippy
cargo test
```

## By Change Type

- Core protocol behavior: run at least `cargo test`.
- Simulator-only behavior: run the most relevant example or scenario in addition to focused tests when practical.
- Scenario/report work: record command, parameters, and deterministic seed.
- Docs-only changes: no Rust test is required; state that only docs changed.
- Formatting-only changes: `cargo fmt --check` is usually sufficient.

See [agent-docs/simulator/operations-development.md](agent-docs/simulator/operations-development.md) for simulator-specific development habits.

## Useful Simulator Commands

```bash
cargo run --example basic_simulation
cargo run --example peer_lifecycle_sim
cargo run --example commit_chain_sim
cargo run --example integrated_simulation
cargo run --release --example integrated_long_run
cargo run --release --example integrated_steady_state
cargo run --bin scenario_runner scenarios/bootstrap.yaml
cargo run --bin scenario_runner scenarios/
```

Use `RUST_LOG=info` or `RUST_LOG=debug` where supported. Long empirical runs should generally use `--release`.

## Current Baseline

At the time this doc was created, the known baseline from existing repo guidance was:

- `cargo test` passes.
- Library tests: 123 passed, 2 ignored.
- `scenario_runner` and `profiling_runner` test builds each add 11 passing peer-lifecycle tests.
- Doctests: 18 passed, 1 ignored.
- Expected warnings include duplicate bench/example targets for Argon2 bench files and many dead-code/unused-field warnings in simulator support code.

If this drifts, update this document and [agent-docs/simulator/evidence-index.md](agent-docs/simulator/evidence-index.md) when relevant.
