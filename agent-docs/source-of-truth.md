# Source Of Truth

## Protocol Goal

Agents should be able to decide which repository artifact to trust without relying on external memory. The repository should stay current rather than accumulating competing historical explanations.

## Current Status

Use this order when sources conflict:

1. Current code and tests.
2. Newest relevant simulator report plus the executable config or scenario that produced it.
3. `agent-docs/` topic pages, once a topic has been distilled here.
4. Top-level docs: `ARCHITECTURE.md`, `SECURITY.md`, `VERIFICATION.md`, and `OPEN_ISSUES.md`.
5. Current protocol/design source documents in `docs/` and `Design/`.
6. Raw notes such as `notes.txt`.
7. Historical material.

`Scratch/` is outdated and should be ignored unless the user explicitly asks for archaeology.

## Known Gaps

- Many existing `docs/` and `Design/` files have not yet been distilled into `agent-docs/`.
- Some older design documents are conceptually useful but long, mixed-purpose, or partially stale.
- `docs/ec_protocol_v0.12.md` is known to be somewhat out of date.
- Simulator reports are snapshots, not permanent protocol truth.

## Primary Files

- [AGENTS.md](../AGENTS.md)
- [ARCHITECTURE.md](../ARCHITECTURE.md)
- [SECURITY.md](../SECURITY.md)
- [VERIFICATION.md](../VERIFICATION.md)
- [OPEN_ISSUES.md](../OPEN_ISSUES.md)

## Source Material

- [README.md](../README.md)
- [notes.txt](../notes.txt)
- [Design/viability_assessment.md](../Design/viability_assessment.md)
- [docs/ec_protocol_v0.12.md](../docs/ec_protocol_v0.12.md)

## Agent Notes

When you distill an older document into `agent-docs/`, mark the new page with current status and known gaps. Later cleanup may move or delete obsolete docs; git history is the archive.

If implementation and aspiration diverge, say so directly:

```md
Protocol goal:
Current implementation status:
Known gaps:
```

