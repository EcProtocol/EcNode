# Agent Docs

This directory is the primary knowledge base for coding agents working on echo-consent. Keep these documents short, current, and linked to the deeper source material they summarize.

Start here:

- [protocol-pillars.md](protocol-pillars.md): The protocol foundations agents should preserve.
- [source-of-truth.md](source-of-truth.md): How to resolve conflicts between docs, code, reports, and notes.
- [glossary.md](glossary.md): Shared vocabulary for protocol and implementation terms.
- [implementation/module-boundaries.md](implementation/module-boundaries.md): Main Rust modules and ownership boundaries.
- [simulator/evidence-index.md](simulator/evidence-index.md): Current empirical evidence and report status.
- [simulator/operations-development.md](simulator/operations-development.md): Simulator verification and development habits.
- [maintenance/future-drilldowns.md](maintenance/future-drilldowns.md): Suggested future documentation/design sessions.
- [../skills/ec-doc-maintainer/SKILL.md](../skills/ec-doc-maintainer/SKILL.md): Repo-local draft skill for doc maintenance.

## Topic Layout

Each topic document should use this shape:

```md
# Topic Name

## Protocol Goal
What the system wants to achieve.

## Current Status
What exists now, what is implemented, and what is only designed.

## Known Gaps
Missing pieces, unresolved questions, and stale assumptions.

## Primary Files
Rust modules, simulator modules, scenarios, or top-level docs.

## Source Material
Existing docs or reports this page was distilled from.

## Agent Notes
Rules of thumb, danger zones, and invariants.
```

## Current Sections

- `api/`: External protocol/API surface, message model, encrypted UDP transport, and client integration.
- `protocol/`: Token/block lifecycle, voting, conflicts, commit-chain, minefield accountability, and retention.
- `peers/`: Proof-of-Aligned-Storage, identity mining, peer elections, topology/routing, and tickets.
- `implementation/`: Module boundaries, storage backends, and dangerous change areas.
- `simulator/`: Simulator entry points, reproducibility expectations, and evidence index.
- `maintenance/`: Documentation policy, templates, and the future doc-maintenance skill design.
