---
name: ec-doc-maintainer
description: Use when maintaining echo-consent agent docs or after changing protocol, simulator, API, security, architecture, or docs. For new topics this is a hard-gated requester-driven workflow: gather evidence, present it from highest abstraction downward, ask for requester confirmation and more sources, and do not edit topic docs until the requester explicitly says to proceed.
---

# ec-doc-maintainer

Use this skill for two modes:

- **Maintenance mode**: at the end of work that changes behavior, design intent, simulator evidence, public API shape, security assumptions, or documentation structure.
- **Topic-development mode**: when asked to flesh out an `agent-docs/` topic or resolve an `OPEN_ISSUES.md` item.

Topic-development mode is requester-driven. Do not start by filling pages with plausible protocol text. Start by finding source material, interviewing the requester, and agreeing on scope.

## Hard Gate For New Topics

When working on a new topic, unresolved `OPEN_ISSUES.md` item, or underdeveloped `agent-docs/` page, you MUST stay in interview mode until the requester explicitly says to proceed.

Do not treat any of these as permission to draft topic docs:

- "start"
- "look at"
- "work on"
- "flesh out"
- "use the skill"
- "take the first open issue"

Only proceed to substantive edits when the requester says something clearly equivalent to:

- "proceed"
- "write it"
- "make the edits"
- "apply that structure"
- "yes, capture this"

Before that explicit proceed signal, the allowed output is:

1. A source map.
2. An evidence summary from highest abstraction to lowest.
3. Questions asking whether the requester sees it the same way.
4. Requests for additional supporting documents, comments, or corrections.

Do not edit topic docs, `OPEN_ISSUES.md`, or design summaries while still in interview mode. The only exception is when the user is explicitly asking to change this skill or the documentation process itself.

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
5. Present evidence from highest abstraction downward:
   - Protocol pillar or user goal.
   - Current top-level docs and source-of-truth status.
   - Existing agent-doc topic state.
   - Current implementation files and tests.
   - Simulator evidence and reports.
   - Notes and open questions.
6. Interview the requester before substantive writing. Ask whether they see the abstraction/evidence stack the same way and whether they have other supporting documents, comments, or corrections.
7. Continue asking and refining the source map until the requester explicitly says to proceed. Do not collapse silence, partial answers, or agreement with one point into permission to edit.
8. When editing, mark claims by status where needed:
   - Protocol goal
   - Current implementation
   - Requester-confirmed design
   - Inference from code/reports
   - Open question
9. When the requester has explicitly said to proceed, edit only within the agreed scope.
10. For each affected topic doc, check whether these sections still match:
   - Protocol Goal
   - Current Status
   - Known Gaps
   - Primary Files
   - Source Material
   - Agent Notes
11. Update `OPEN_ISSUES.md` when a known gap is closed, newly discovered, or clarified.
12. For simulator changes, update `agent-docs/simulator/evidence-index.md` or `operations-development.md` when evidence, commands, scenarios, or reproducibility expectations change.
13. For security-relevant changes, update `SECURITY.md` and the matching topic doc.
14. Preserve distinctions between protocol goals, current implementation, empirical reports, and historical source material.

## Rules

- Keep docs short and current.
- Keep the requester in control of protocol/design meaning.
- In topic-development mode, asking questions is not optional. Keep asking until the requester says to proceed.
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
