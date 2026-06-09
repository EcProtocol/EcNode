# Elections

## Protocol Goal

Select peers that are accountable for local token coverage and can support aligned storage and routing behavior.

## Current Status

Peer election types and ring-distance helpers are exposed from `ec_proof_of_storage`. Peer lifecycle code uses election and token-sampling concepts.

## Known Gaps

- Needs current extraction from implementation and tests.
- Need clearer distinction between protocol election design and simulator lifecycle behavior.

## Primary Files

- [src/ec_proof_of_storage.rs](../../src/ec_proof_of_storage.rs)
- [src/ec_peers.rs](../../src/ec_peers.rs)
- [simulator/peer_lifecycle/](../../simulator/peer_lifecycle)

## Source Material

- [docs/peer_election_design.md](../../docs/peer_election_design.md)
- [Design/peer_management_system.md](../../Design/peer_management_system.md)

## Agent Notes

Changing election or pruning defaults can invalidate simulator evidence.

