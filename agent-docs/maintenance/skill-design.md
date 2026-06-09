# ec-doc-maintainer Skill Design

## Protocol Goal

Create and refine a Codex skill that keeps `agent-docs/` aligned with code, simulator evidence, and open issues after work is done in an area.

## Current Status

A repo-local draft exists at [skills/ec-doc-maintainer/SKILL.md](../../skills/ec-doc-maintainer/SKILL.md). This page captures the design intent and future refinements.

## Known Gaps

- The draft has not yet been exercised after a substantive code change.
- Need to decide whether the skill should include scripts for link checking or freshness checks.

## Primary Files

- [agent-docs/](../README.md)
- [OPEN_ISSUES.md](../../OPEN_ISSUES.md)
- [skills/ec-doc-maintainer/SKILL.md](../../skills/ec-doc-maintainer/SKILL.md)

## Source Material

- [agent-docs/source-of-truth.md](../source-of-truth.md)
- [agent-docs/maintenance/doc-update-policy.md](doc-update-policy.md)

## Agent Notes

Responsibilities:

- Inspect touched files and identify related agent docs.
- Check whether protocol goal, current status, and known gaps still match the change.
- Update source-material links when new docs or reports are created.
- Add or revise `OPEN_ISSUES.md` entries when implementation and aspiration diverge.
- Avoid generating schedules or rollout plans.
