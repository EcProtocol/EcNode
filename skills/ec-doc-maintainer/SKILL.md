---
name: ec-doc-maintainer
description: Use after changing echo-consent protocol, simulator, API, security, architecture, or docs so Codex updates the repo-local agent docs, source-of-truth links, simulator evidence notes, and open issues.
---

# ec-doc-maintainer

Use this skill at the end of work that changes behavior, design intent, simulator evidence, public API shape, security assumptions, or documentation structure.

## Workflow

1. Inspect changed files with `git status --short` and `git diff --stat`.
2. Map changed areas to agent docs:
   - API/transport: `agent-docs/api/`
   - Voting/conflicts/commit flow: `agent-docs/protocol/`
   - Identity/elections/topology/tickets: `agent-docs/peers/`
   - Storage/module boundaries: `agent-docs/implementation/`
   - Simulator/scenarios/reports: `agent-docs/simulator/`
   - Cross-cutting repo guidance: `AGENTS.md`, `ARCHITECTURE.md`, `SECURITY.md`, `VERIFICATION.md`, `OPEN_ISSUES.md`
3. For each affected topic doc, check whether these sections still match:
   - Protocol Goal
   - Current Status
   - Known Gaps
   - Primary Files
   - Source Material
   - Agent Notes
4. Update `OPEN_ISSUES.md` when a known gap is closed, newly discovered, or clarified.
5. For simulator changes, update `agent-docs/simulator/evidence-index.md` or `operations-development.md` when evidence, commands, scenarios, or reproducibility expectations change.
6. For security-relevant changes, update `SECURITY.md` and the matching topic doc.
7. Preserve distinctions between protocol goals, current implementation, empirical reports, and historical source material.

## Rules

- Keep docs short and current.
- Do not add schedules, rollout plans, or date-based roadmaps to design docs.
- Do not treat older `docs/` or `Design/` material as authoritative once a topic has been distilled into `agent-docs/`.
- If implementation and aspiration diverge, state both directly.
- Do not update unrelated docs just to touch them.
- Prefer links to primary files and source material over copying long explanations.

## Verification

For docs-only maintenance, run `git diff --check`. For behavior changes, follow `VERIFICATION.md`.

