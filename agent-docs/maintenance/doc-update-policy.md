# Doc Update Policy

## Protocol Goal

Keep repository knowledge current enough that agents can work from the repo without hidden context.

## Current Status

`agent-docs/` is the primary agent knowledge base. Older docs remain source material until distilled, moved, or deleted.

## Known Gaps

- A repo-local maintenance skill draft exists in [skills/ec-doc-maintainer/SKILL.md](../../skills/ec-doc-maintainer/SKILL.md), but agents may need to install or manually follow it depending on their environment.
- No mechanical lint checks exist yet for doc freshness or links.

## Primary Files

- [agent-docs/README.md](../README.md)
- [agent-docs/source-of-truth.md](../source-of-truth.md)
- [agent-docs/maintenance/skill-design.md](skill-design.md)

## Source Material

- [OPEN_ISSUES.md](../../OPEN_ISSUES.md)

## Agent Notes

After changing an area, check the matching `agent-docs/` topic and update `OPEN_ISSUES.md` when implementation and goals diverge.
