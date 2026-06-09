# AGENTS.md

Guidance for coding agents working in this repository.

## Start Here

This file is a table of contents, not the full knowledge base. Read only the deeper docs relevant to your task.

- [agent-docs/README.md](agent-docs/README.md): Agent knowledge base and topic map.
- [agent-docs/protocol-pillars.md](agent-docs/protocol-pillars.md): Core protocol foundations.
- [agent-docs/source-of-truth.md](agent-docs/source-of-truth.md): How to resolve conflicting docs.
- [ARCHITECTURE.md](ARCHITECTURE.md): Current module and simulator architecture.
- [SECURITY.md](SECURITY.md): Security properties, non-goals, and known gaps.
- [VERIFICATION.md](VERIFICATION.md): Build, test, and simulator verification commands.
- [OPEN_ISSUES.md](OPEN_ISSUES.md): Open design questions, implementation gaps, and smaller tasks.
- [skills/ec-doc-maintainer/SKILL.md](skills/ec-doc-maintainer/SKILL.md): Repo-local skill draft for keeping agent docs current. Use it directly or install it into your agent's skill system.

## Project Overview

echo-consent (`ecRust`) is a Rust reference implementation and simulator suite for the EC Protocol: an experimental distributed coordination protocol for shared state with local neighborhoods, bounded retention, proof-of-aligned-storage ideas, peer elections, and conflict-visible token/block commits.

The crate is a library named `ec_rust` in [src/lib.rs](src/lib.rs). There is no `src/main.rs`; runnable behavior is primarily exposed through simulator examples and the `scenario_runner`/`profiling_runner` binaries declared in [Cargo.toml](Cargo.toml).

## Non-Negotiable Repository Rules

- Ignore `Scratch/`; it is outdated.
- Do not restore `CLAUDE.md` unless the user asks.
- Preserve the distinction between protocol design docs, empirical simulator reports, and implementation comments.
- Design documents should focus on concept, analysis, and mathematical proof. Do not include work schedules or rollout plans.
- Diagrams in design docs should be Mermaid.
- Formulas in design docs should use LaTeX.
- Prefer deterministic seeds and explicit scenario parameters when adding or changing simulations.
- Do not silently start the future `TokenHash = Blake3(TokenId)` migration as part of unrelated work.
- Do not assume `ec_rocksdb_backend.rs` builds by default; the feature/dependency is not wired in `Cargo.toml`.

## Common Entry Points

- Core integration: [src/ec_node.rs](src/ec_node.rs)
- Shared types/messages/storage traits: [src/ec_interface.rs](src/ec_interface.rs)
- Voting/conflicts/batching: [src/ec_mempool.rs](src/ec_mempool.rs)
- Peers/elections/topology: [src/ec_peers.rs](src/ec_peers.rs)
- Proof-of-storage/election helpers: [src/ec_proof_of_storage.rs](src/ec_proof_of_storage.rs)
- Identity mining/shared secrets: [src/ec_identity.rs](src/ec_identity.rs)
- Commit-chain sync: [src/ec_commit_chain.rs](src/ec_commit_chain.rs)
- Simulator evidence: [agent-docs/simulator/evidence-index.md](agent-docs/simulator/evidence-index.md)
- Simulator ways of working: [agent-docs/simulator/operations-development.md](agent-docs/simulator/operations-development.md)

## Verification

For core behavior, run at least:

```bash
cargo test
```

For simulator-only behavior, also run the most relevant example or scenario. See [VERIFICATION.md](VERIFICATION.md) for the full command list and current warning baseline.

## High-Blast-Radius Areas

Read [agent-docs/implementation/dangerous-change-areas.md](agent-docs/implementation/dangerous-change-areas.md) before changing message variants, peer topology/pruning defaults, storage traits, token/block ID aliases, or commit-chain sync behavior.

## Documentation Maintenance

`agent-docs/` is the primary agent knowledge base. When changing an area, update the matching agent topic doc and [OPEN_ISSUES.md](OPEN_ISSUES.md) if the protocol goal, implementation status, or known gaps changed.

The intended maintenance workflow is captured as a repo-local skill draft in [skills/ec-doc-maintainer/SKILL.md](skills/ec-doc-maintainer/SKILL.md). If your agent supports installable skills, install or sync this skill into that system; otherwise, read and follow it manually.
