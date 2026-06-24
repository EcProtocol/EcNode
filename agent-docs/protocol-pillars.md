# Protocol Pillars

These pillars are the foundation agents should preserve when changing protocol, simulator, or documentation behavior.

## Overarching Goal

EC aims to enable transaction commit at human timescale in an open, global network without a fixed global validator set. That is the project-level differentiator from global-gossip blockchains and many distributed-ledger-like systems: useful base-layer responsiveness should come from locality-driven routing, local hosting neighborhoods, peer elections, and bounded repair, not from assuming closed membership or pushing fast user experience to a second layer.

When evaluating protocol, simulator, or peer-topology changes, preserve this comparison frame:

- open participation and churn tolerance
- local token/witness neighborhoods instead of global validation for every transaction
- commit latency and message overhead low enough for interactive use
- conflict visibility and safety despite not having a fixed committee

## 1. Protocol/API Surface

The protocol needs a clear external surface for future clients and nodes. This includes message semantics, identity linkage, shared-secret derivation, and an encrypted UDP packet design based on X25519 plus AEAD, inspired by WireGuard. The exact library and packet format are not decided.

Primary docs:

- [api/protocol-surface.md](api/protocol-surface.md)
- [api/message-model.md](api/message-model.md)
- [api/encrypted-udp-transport.md](api/encrypted-udp-transport.md)

## 2. Proof-of-Aligned-Storage

Peers should earn their role by demonstrating storage aligned with the local neighborhood. Storage is not incidental; it is the alignment credential.

Primary docs:

- [peers/proof-of-aligned-storage.md](peers/proof-of-aligned-storage.md)
- [docs/aligned_storage_proof_eprint_v3.md](../docs/aligned_storage_proof_eprint_v3.md)

## 3. Identity Mining

Peer identity uses Argon2-based mining so identities are costly, peer IDs are effectively random, and network isolation or address-slice behavior can be bound to identity material.

Primary docs:

- [peers/identity-mining.md](peers/identity-mining.md)
- [../docs/identity-block-design.md](../docs/identity-block-design.md)
- [../Design/peer_identity_and_argon2_optimization.md](../Design/peer_identity_and_argon2_optimization.md)

## 4. Peer Election And Topology

The peer graph should provide local coverage, election accountability, and scalable routing. Ring-near peers matter for correctness; sparse far links and latency weighting matter for practical path length and round speed.

Primary docs:

- [peers/elections.md](peers/elections.md)
- [peers/topology-routing.md](peers/topology-routing.md)
- [../docs/peer_election_design.md](../docs/peer_election_design.md)
- [../Design/small-world.md](../Design/small-world.md)

## 5. Vote And Conflict Flow

Transactions are user-signed. The protocol should commit clean winners, avoid committing lower or multiple owners for the same token, and keep user-created conflicts visible.

Primary docs:

- [protocol/voting-conflicts-batching.md](protocol/voting-conflicts-batching.md)
- [../Design/vote_flow_and_batching.md](../Design/vote_flow_and_batching.md)
- [../Design/response_driven_commit_flow.md](../Design/response_driven_commit_flow.md)

## 6. Commit-Chain And Minefield Accountability

Commit-chain state supports local history sync. Minefield accountability is the post-commit idea that contradictory signed attestations should expose rewrite fraud.

Primary docs:

- [protocol/commit-chain-minefield.md](protocol/commit-chain-minefield.md)
- [../Design/commit_chain.md](../Design/commit_chain.md)
- [../Design/minefield_accountability_design.md](../Design/minefield_accountability_design.md)

## 7. Bounded Retention

The protocol assumes finite state retention. Applications must redeem, extend, or tolerate expiry within the public retention window.

Primary docs:

- [protocol/retention.md](protocol/retention.md)
- [../README.md](../README.md)

## 8. Simulator Evidence

Simulator evidence is the current empirical feedback loop. Reports should be tied to deterministic seeds, explicit parameters, scenarios, and executable commands.

Primary docs:

- [simulator/evidence-index.md](simulator/evidence-index.md)
- [simulator/reproducibility.md](simulator/reproducibility.md)
- [../simulator/STEADY_STATE_REPORT.md](../simulator/STEADY_STATE_REPORT.md)
