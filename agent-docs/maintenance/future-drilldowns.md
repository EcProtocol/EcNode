# Future Drilldowns

This is a menu for future documentation/design sessions. It is not a schedule.

## Structure Cleanup

- Decide when distilled topics allow older `docs/` or `Design/` files to be moved or deleted.
- Add status labels to remaining source documents: current, partially superseded, historical, or empirical snapshot.
- Keep `agent-docs/` terse; move long-form human docs into `docs/` only when they are derived from current agent docs.

## Protocol/API Surface

- Define the future client/node API surface: bind, update, lookup, transaction lookup, and counterparty verification.
- Distinguish node API, client API, simulator harness API, and transport packet format.
- Clarify multi-point submission from a client/counterparty perspective.

## Encrypted UDP Transport

- Design X25519 plus AEAD packet framing, inspired by WireGuard.
- Decide handshake/session lifecycle, replay protection, nonce strategy, and key rotation.
- Clarify how peer identity, network identity, tickets, and shared secrets compose.

## Identity Mining

- Distill `ec_identity` behavior into the identity-mining agent doc.
- Clarify identity TTL, timestamp windows, salt/network identity rules, and address-slice behavior.
- Separate current implementation facts from protocol goals.

## Voting, Conflicts, And Batching

- Extract the current mempool state machine into a compact doc.
- Summarize conflict visibility as an invariant.
- Link each major safety/performance claim to the newest relevant simulator evidence.

## Commit-Chain And Minefield

- Separate implemented commit-chain sync from aspirational minefield accountability.
- Clarify fraud evidence shape, storage incentives, and retention-bound obligations.
- Extract current open questions from `notes.txt` into topic docs.

## Peer Lifecycle And Topology

- Distill election, topology, and pruning behavior from `EcPeers` and simulator reports.
- Clarify pending versus connected peers in active-ring selection.
- Track which small-world/topology ideas are implemented, experimental, or only designed.

## Simulator Evidence

- Create a current evidence matrix by question: safety, conflict behavior, churn, topology, steady state, commit-chain sync.
- For each current claim, link report, command, scenario/config, seed, and known limitations.
- Decide which older reports should be archived or deleted after distillation.

## ec-doc-maintainer Skill

- Try the repo-local skill draft after a real code or docs change.
- Refine the workflow based on what it misses.
- Consider adding small scripts later for link checks or doc freshness checks.

