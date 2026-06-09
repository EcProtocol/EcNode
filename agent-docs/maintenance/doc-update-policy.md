# Doc Update Policy

## Protocol Goal

Keep repository knowledge current enough that agents can work from the repo without hidden context.

## Current Status

`agent-docs/` is the primary agent knowledge base. Older docs remain source material until distilled, moved, or deleted.

Topic-development sessions should be requester-driven: source-map relevant code/docs/reports first, present evidence from highest abstraction downward, interview the requester about intent and source trust, and keep asking for supporting documents or corrections until the requester explicitly says to proceed.

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

When filling a topic from scratch, avoid writing what merely sounds right. Do not edit topic docs before the requester says to proceed. Label proposed design text as proposed until the requester confirms it.
