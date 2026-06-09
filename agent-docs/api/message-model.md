# Message Model

## Protocol Goal

Messages should expose the protocol state transitions needed for voting, repair, peer discovery, proof responses, and commit-chain sync.

## Current Status

Primary message variants are defined in [src/ec_interface.rs](../../src/ec_interface.rs): `InitialVote`, `Vote`, `QueryBlock`, `QueryToken`, `RequestBatch`, `Answer`, `Block`, `Referral`, `QueryCommitBlock`, and `CommitBlock`.

## Known Gaps

- Message model may change when the network packet/API surface is designed.
- `RequestBatch` behavior should be distilled from implementation and simulator evidence.
- Ticket validation and transport packaging need clearer boundaries.

## Primary Files

- [src/ec_interface.rs](../../src/ec_interface.rs)
- [src/ec_node.rs](../../src/ec_node.rs)
- [src/ec_mempool.rs](../../src/ec_mempool.rs)
- [src/ec_peers.rs](../../src/ec_peers.rs)

## Source Material

- [Design/vote_flow_and_batching.md](../../Design/vote_flow_and_batching.md)
- [docs/ec_protocol_v0.12.md](../../docs/ec_protocol_v0.12.md)

## Agent Notes

Changes to message variants have high blast radius across tests, simulators, and future API work.

