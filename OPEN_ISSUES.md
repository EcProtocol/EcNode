# Open Issues

This file collects open design questions, implementation gaps, smaller engineering tasks, and ideas that agents should not lose. Keep entries short and move details into the relevant `agent-docs/` topic when they mature.

## Protocol/API Surface

- Implement the UDP packet API: plaintext version/sender public key/nonce header, header as AEAD AAD, ChaCha20Poly1305 payload, and X25519-derived shared secrets.
- Decide compact wire serialization and exact packet byte layout, keeping packets under MTU where possible.
- Connect `MessageEnvelope` to network-layer packet metadata and short-lived reply context without breaking simulator usage.
- Design client-ticket issuance, validation, and economics for write-like client messages.
- Investigate rate limiting for client-heavy UDP traffic; prior eBPF-layer ideas are WIP.
- Clarify how tickets compose with encrypted transport.
- Consider a future orchestrator module that owns tick/message scheduling, gathers outbound messages, and packages them for network transport.
- Clarify whether submodules should emit neighborhood/intention targets rather than final destinations so an orchestrator can optimize multi-message network packages.
- Preserve identity blocks unless a future design replaces their bootstrap role; peer IDs as public key plus salt currently remain useful as findable tokens.
- Track future 256-bit token/hash transition separately from unrelated API work.

## Identity Mining

- Clarify identity TTL policy and timestamp acceptance window.
- Clarify network identity rules and how network isolation enters salt validation.
- Clarify how network identity should enter X25519 shared-secret derivation.

## Tickets

- Integrate ticket validation into `EcNode`.
- Clarify secret rotation behavior and validation windows.

## Peer Lifecycle And Topology

- Use [agent-docs/peers/peer-shape-target.md](agent-docs/peers/peer-shape-target.md) as the current evidence map for target peer-set shape: dense local coverage, controlled fade/remote coverage, protocol-shaped routing progress, bounded message cost, and enough probabilistic selection to avoid fully predictable active sets.
- Test the candidate simpler peer lifecycle model in [agent-docs/peers/peer-lifecycle-structure.md](agent-docs/peers/peer-lifecycle-structure.md): raw gap scanning over known and connected layers, caller-owned target-gap/ranking policy, known-peer maintenance with stale cleanup/invite retry/commit-chain RTT probes/referral discovery, connected-repair token walks from known peer IDs, separate invite-triggered elections, requester-bound signature answers, bounded repair frontier, liveness pruning of stalled peers, election-gated promotion, and weighted pruning of overfilled windows.
- Add current simulator metrics for scanner steering: core/fade/remote coverage, largest known/connected gap vs target gap, first-stage distance reduction toward role coverers, time to first covering-neighborhood contact, and peer-set overlap/entropy inside eligible shape bands.
- Clarify whether pending and connected peers should both participate in active-ring peer selection.
- Clarify whether pending peers should mark the organic range for client-side libraries where peers may never reach `Connected`.
- Define how future client libraries use node-initiated token walks to build peer knowledge without local token storage or `Connected` promotion.
- Define commit-chain RTT probe tickets, cursor advancement to `previous`, and requested-answer vs invite head update rules so quality probes do not enter normal chain tracking or hammer one head.
- Add or document a `my-range` style helper for local active-ring range.
- Preserve 2-above/2-below style balance when changing peers, if still part of the current design.
- Refresh ALIVE state on received blocks if that remains desired.

## Commit-Chain And Minefield

- Analyze fraud detection and incentives to store blocks.
- Clarify top-down commit-chain sync and block/transaction backtracking.
- Investigate long bootstrap traces when peers have opposite logs.
- Clarify per-peer watermark behavior for organic sharding and changing neighborhoods.
- Decide trace drop behavior when block or commit-block fetches make no progress.
- Decide whether each trace should keep its own watermark until completion.
- Decide whether stalled traces should sometimes switch to a random alternate trace.
- Clarify confirm-counter behavior on blocks whose tokens are not in local range.
- Document or implement the design shift from shadow state to state-based 2-slot records in the database.
- Clarify where `ec_ticket_manager` should be used in commit-chain sync.
- Evaluate whether new peers outside the previous read range should be traced all the way back through the retention horizon, or whether tracing 1 +/- address is enough.

## Genesis And Storage

- Ensure genesis token sampling initializes peer token stores according to the intended range behavior.
- Clarify selective storage behavior for only storing tokens/blocks in approximate range.
- Keep future 256-bit token/hash transition separate from unrelated work.

## Simulator

- Consolidate full-scale integrated simulator direction if still desired.
- Consider one scenario-style full-scale simulator that combines all parts.
- Improve deterministic behavior where hash map iteration or similar ordering can affect results.
- Prefer reproducible scenarios over ad hoc long-running artifacts.

## Documentation

- Distill existing `docs/` and `Design/` material into `agent-docs/`.
- Mark or remove stale docs once distilled; git history is the archive.
- Exercise and refine the repo-local `ec-doc-maintainer` skill after real code or docs changes.
- Keep `OPEN_ISSUES.md` as the shared idea/task/gap list; move detail into topic docs when an issue matures.
