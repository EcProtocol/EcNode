# Message Model

## Protocol Goal

Messages should expose the protocol state transitions needed for voting, repair, peer discovery, proof responses, and commit-chain sync.

## Current Status

Primary message variants are defined in [src/ec_interface.rs](../../src/ec_interface.rs): `InitialVote`, `Vote`, `QueryBlock`, `QueryToken`, `RequestBatch`, `Answer`, `Block`, `Referral`, `QueryCommitBlock`, and `CommitBlock`.

`MessageEnvelope` is the current internal dispatch shape used by tests and simulators. It is not the final UDP wire API. A future transport/orchestrator layer should connect envelope fields to packet metadata and local socket context.

## Known Gaps

- Message model may change when the network packet/API surface is designed.
- `RequestBatch` behavior should be distilled from implementation and simulator evidence.
- Ticket validation and transport packaging need clearer boundaries.
- Client-ticket rules for write-like client messages are not designed.
- Compact serialization and packet-size limits need a wire-format decision.

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

Admission intent for agents:

- query-style messages can be served for unknown clients when local state permits
- connected peers may use zero-ticket flows only after known-peer verification
- client write/influence messages need valid tickets
- unsolicited client blocks should be discarded unless they answer a request with a valid ticket
- unknown or unconnected senders should receive direct answers or referrals, not forwarding service
