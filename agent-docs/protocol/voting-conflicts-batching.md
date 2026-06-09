# Voting, Conflicts, And Batching

## Protocol Goal

Commit clean winners, avoid lower-owner or multi-owner commits, and keep user-signed conflicts visible to counterparties.

## Current Status

Voting, conflict repair, and request batching are implemented across `EcMemPool`, `EcNode`, and message types in `ec_interface`.

## Known Gaps

- Existing design docs are older and should be distilled.
- Need a compact state-machine view.
- Need current simulator evidence links per claim.

## Primary Files

- [src/ec_mempool.rs](../../src/ec_mempool.rs)
- [src/ec_node.rs](../../src/ec_node.rs)
- [src/ec_interface.rs](../../src/ec_interface.rs)

## Source Material

- [Design/vote_flow_and_batching.md](../../Design/vote_flow_and_batching.md)
- [Design/response_driven_commit_flow.md](../../Design/response_driven_commit_flow.md)
- [simulator/ADVERSARIAL_CONFLICT_REPORT.md](../../simulator/ADVERSARIAL_CONFLICT_REPORT.md)

## Agent Notes

Do not hide conflicts by forcing resolution in docs or code unless the user explicitly changes the protocol goal.

