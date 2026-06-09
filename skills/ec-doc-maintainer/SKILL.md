---
name: ec-doc-maintainer
description: Use when maintaining echo-consent agent docs or after changing protocol, simulator, API, security, architecture, or docs. The workflow is requester-driven: source-map first, interview before drafting, then update repo-local agent docs, evidence notes, and open issues.
---

# ec-doc-maintainer

Use this skill for two modes:

- **Maintenance mode**: at the end of work that changes behavior, design intent, simulator evidence, public API shape, security assumptions, or documentation structure.
- **Topic-development mode**: when asked to flesh out an `agent-docs/` topic or resolve an `OPEN_ISSUES.md` item.

Topic-development mode is requester-driven. Do not start by filling pages with plausible protocol text. Start by finding source material, interviewing the requester, and agreeing on scope.

## Workflow

1. Inspect changed files with `git status --short` and `git diff --stat`.
2. If there are existing unstaged doc edits, treat them as draft material until the requester confirms them. Summarize what they assume before building on them.
3. Map changed or requested areas to agent docs:
   - API/transport: `agent-docs/api/`
   - Voting/conflicts/commit flow: `agent-docs/protocol/`
   - Identity/elections/topology/tickets: `agent-docs/peers/`
   - Storage/module boundaries: `agent-docs/implementation/`
   - Simulator/scenarios/reports: `agent-docs/simulator/`
   - Cross-cutting repo guidance: `AGENTS.md`, `ARCHITECTURE.md`, `SECURITY.md`, `VERIFICATION.md`, `OPEN_ISSUES.md`
4. Build a source map before drafting:
   - Primary code files.
   - Existing `agent-docs/` pages.
   - Relevant `docs/`, `Design/`, simulator reports, scenarios, tests, and notes.
   - Known stale or conflicting sources.
5. Interview the requester before substantive writing. Ask concise questions about goals, non-goals, source trust, terminology, and unresolved design choices.
6. Stop after the source map and interview unless the requester has clearly asked you to proceed with edits in the same turn.
7. When editing, mark claims by status where needed:
   - Protocol goal
   - Current implementation
   - Requester-confirmed design
   - Inference from code/reports
   - Open question
8. For each affected topic doc, check whether these sections still match:
   - Protocol Goal
   - Current Status
   - Known Gaps
   - Primary Files
   - Source Material
   - Agent Notes
9. Update `OPEN_ISSUES.md` when a known gap is closed, newly discovered, or clarified.
10. For simulator changes, update `agent-docs/simulator/evidence-index.md` or `operations-development.md` when evidence, commands, scenarios, or reproducibility expectations change.
11. For security-relevant changes, update `SECURITY.md` and the matching topic doc.
12. Preserve distinctions between protocol goals, current implementation, empirical reports, and historical source material.

## Rules

- Keep docs short and current.
- Keep the requester in control of protocol/design meaning.
- Do not invent API names, status categories, threat properties, or protocol guarantees as facts.
- When proposing text, label it as proposed until the requester confirms it.
- Do not add schedules, rollout plans, or date-based roadmaps to design docs.
- Do not treat older `docs/` or `Design/` material as authoritative once a topic has been distilled into `agent-docs/`.
- If implementation and aspiration diverge, state both directly.
- Do not update unrelated docs just to touch them.
- Prefer links to primary files and source material over copying long explanations.
- Prefer a small source map plus questions over a large first draft.

## Verification

For docs-only maintenance, run `git diff --check`. For behavior changes, follow `VERIFICATION.md`.
