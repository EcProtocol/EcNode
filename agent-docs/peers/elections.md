# Elections

## Protocol Goal

Select peers that are accountable for local token coverage and can support aligned storage and routing behavior.

## Current Status

Peer election types and ring-distance helpers are exposed from `ec_proof_of_storage`. Peer lifecycle code uses election and token-sampling concepts.

Challenge-token selection is lifecycle policy, not election winner logic. The candidate lifecycle separates two sources:

- invite-triggered elections, where a valid invite in a locally underfilled span may include the inviter as a participant
- node-initiated repair, where local gap scans start token walks from nearby `Identified`, `Pending`, or `Connected` peer IDs

In both cases, the local node should choose the final election token from verified signature material using local randomness. A responder may reveal candidate tokens, but should not decide the election challenge.

## Known Gaps

- Needs current extraction from implementation and tests.
- Need clearer distinction between protocol election design and simulator lifecycle behavior.
- Need tests that preserve the boundary between lifecycle challenge-token selection and winner selection in `ec_proof_of_storage`.

## Primary Files

- [src/ec_proof_of_storage.rs](../../src/ec_proof_of_storage.rs)
- [src/ec_peers.rs](../../src/ec_peers.rs)
- [simulator/peer_lifecycle/](../../simulator/peer_lifecycle)

## Source Material

- [docs/peer_election_design.md](../../docs/peer_election_design.md)
- [Design/peer_management_system.md](../../Design/peer_management_system.md)
- [peer-lifecycle-structure.md](peer-lifecycle-structure.md)

## Agent Notes

Changing election or pruning defaults can invalidate simulator evidence.
