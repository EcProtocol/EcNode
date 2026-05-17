# AGENTS.md

Guidance for coding agents working in this repository.

## Project Overview

This repository contains **echo-consent** (`ecRust`), a Rust reference implementation and simulator suite for the EC Protocol: an experimental distributed coordination protocol for shared state with local neighborhoods, bounded retention, proof-of-aligned-storage ideas, peer elections, and conflict-visible token/block commits.

The crate is a library named `ec_rust` in `src/lib.rs`. There is no `src/main.rs` application entry point. Runnable behavior is primarily exposed through simulator examples and the `scenario_runner`/`profiling_runner` binaries declared in `Cargo.toml`.

## Repository-Specific Instructions

- Ignore content under `Scratch/`; it is outdated.
- When asked to make design documents, do not include work schedules or rollout plans.
- Design documents should focus on the concept, analysis, and mathematical proof. Do not frame them as comparisons against the current implementation unless explicitly requested.
- Diagrams should be written in Mermaid.
- Formulas should use LaTeX formatting.
- Prefer deterministic seeds and explicit scenario parameters when adding or changing simulations.
- Preserve the distinction between protocol design docs, empirical simulator reports, and implementation comments.

## Current Architecture

### Core Library: `src/`

- `ec_interface.rs`: Shared protocol types, message envelopes, block/token aliases, storage traits, event types, request batching, and commit-chain message types.
- `ec_node.rs`: Main node facade. Owns the mempool, peer manager, ticket manager, local time, event sink, batching behavior, and commit-chain sync toggles.
- `ec_mempool.rs`: Block state machine for pending/committed/blocked work, vote accounting, conflict-aware repair, vote request scheduling, and diagnostics.
- `ec_peers.rs`: Peer lifecycle and topology management. Handles Identified/Pending/Connected states, invitations, referrals, elections, token sampling, pruning, adaptive neighborhoods, target degree bands, and experimental small-world retention.
- `ec_proof_of_storage.rs`: Signature-based proof-of-storage helpers, token storage backend trait, ring distance, consensus clustering, and peer election logic.
- `ec_commit_chain.rs`: Local append-only commit-chain tracking and sync behavior.
- `ec_memory_backend.rs`: In-memory token/block/commit-chain backend plus batched writes. This is the default backend used by tests and simulators.
- `ec_identity.rs`: Peer identity generation/validation, Argon2-based address mining configs, timestamp validation, network isolation, and X25519 shared-secret derivation.
- `ec_genesis.rs`: Deterministic genesis token generation and selective storage initialization.
- `ec_ticket_manager.rs`: Per-use-case message ticket generation, validation, and rotating secrets.
- `ec_rocksdb_backend.rs`: Optional persistent backend behind `#[cfg(feature = "rocksdb-backend")]`. The Cargo feature/dependency is not currently wired in `Cargo.toml`; do not assume it builds by default.

Important constants currently live in `ec_interface.rs`, including `TOKENS_PER_BLOCK = 6`, `TOKENS_SIGNATURE_SIZE = 10`, and `VOTE_THRESHOLD = 2`.

### Simulators: `simulator/`

The simulator code is part of this repository through Cargo examples and bins, not a separate crate.

- `simulator/consensus/`: Core consensus simulation using `EcNode`, configurable topology, message loss/delay, event sinks, CSV export, and commit/message stats.
- `simulator/peer_lifecycle/`: Peer discovery, election, token allocation, topology shaping, churn, and scenario support.
- `simulator/integrated/`: Combined node, peer lifecycle, churn, commit-chain sync, and transaction-flow simulation.
- `simulator/commit_chain/`: Focused commit-chain simulation.
- Top-level example entry points include `basic_simulation`, `peer_lifecycle_sim`, `commit_chain_sim`, `integrated_simulation`, `integrated_genesis_simulation`, `integrated_long_run`, and `integrated_steady_state`.
- Binaries:
  - `cargo run --bin scenario_runner scenarios/bootstrap.yaml`
  - `cargo run --bin profiling_runner`

Many reports in `simulator/` are empirical outputs from past runs. Treat the newest relevant report plus the executable config as the source of truth for simulator behavior.

### Docs and Design

- `README.md`: Best high-level project introduction and current status.
- `docs/`: Protocol specs and pillar documents, including genesis, identity blocks, ticket system, peer election, proof-of-storage, and protocol versions.
- `Design/`: Design rationale, threat model, viability analysis, topology notes, vote flow, commit-chain notes, simulation analyses, and older exploratory scripts.
- `peer_lifecycle/`: Python analysis/simulation artifacts and peer lifecycle reports.

Do not put generated schedules, task timelines, or rollout plans into design docs. If a design needs a proof, use LaTeX. If it needs a diagram, use Mermaid.

## Development Commands

Use stable Rust and Cargo.

```bash
cargo build
cargo check
cargo fmt --check
cargo fmt
cargo clippy
cargo test
```

Useful simulator commands:

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

Use `RUST_LOG=info` or `RUST_LOG=debug` where the simulator supports logging. Long empirical runs should generally use `--release`.

## Testing Reality

This repo now has a substantial unit and doctest suite. Do not describe it as simulation-only.

Current verification baseline from this state:

- `cargo test` passes.
- Library tests: 123 passed, 2 ignored.
- `scenario_runner` and `profiling_runner` test builds each add 11 passing peer-lifecycle tests.
- Doctests: 18 passed, 1 ignored.
- Expected warnings include duplicate bench/example targets for the Argon2 bench files and many dead-code/unused-field warnings in simulator support code.

When changing core behavior, run at least `cargo test`. For simulator-only changes, also run the most relevant example or `scenario_runner` scenario. For formatting-only or docs-only changes, `cargo fmt --check` or no Rust test may be sufficient; state what was or was not run.

## Message and Data Model Notes

Primary message variants are defined in `ec_interface.rs`:

- `InitialVote`
- `Vote`
- `QueryBlock`
- `QueryToken`
- `RequestBatch`
- `Answer`
- `Block`
- `Referral`
- `QueryCommitBlock`
- `CommitBlock`

`RequestBatch` can coalesce vote and query-like messages depending on peer config flags. `Answer` carries proof-of-storage signature tokens and the sender's commit-chain head.

`TokenId`, `PeerId`, and `BlockId` are currently `u64` aliases for simulation friendliness. `ec_interface.rs` documents a future `TokenHash = Blake3(TokenId)` refactor; do not silently start that migration as part of unrelated work.

## Implementation Guidance

- Follow existing module boundaries. Changes to `ec_interface.rs` usually affect many tests and simulators.
- Keep storage changes aligned across `EcTokens`, `EcTokensV2`, `TokenStorageBackend`, `BatchedBackend`, `MemoryBackend`, and simulator token stores.
- `EcNode` is the integration point; `EcPeers` and `EcMemPool` should remain separately testable.
- Peer topology and pruning behavior is subtle. Check existing tests in `ec_peers.rs` and reports in `simulator/` before changing defaults.
- Use event sinks and diagnostics when adding simulator observability rather than printing directly from core logic.
- Keep random behavior seedable in tests and simulations.
- Avoid broad refactors while protocol experiments are active unless the user asks for them.

## Git and Workspace Notes

- `AGENTS.md` replaces the old `CLAUDE.md` guidance. If `CLAUDE.md` appears deleted in git status, do not restore it unless asked.
- The worktree may contain user edits. Do not revert unrelated changes.
- Generated or long-running artifacts should not be added casually. Prefer small, reproducible scenarios and report exact commands/parameters when recording results.
