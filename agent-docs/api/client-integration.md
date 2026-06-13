# Client Integration

## Protocol Goal

Clients should eventually interact through the same UDP packet/message API as nodes without needing simulator internals.

## Current Status

No stable client library exists. Current runnable behavior is through simulator examples and scenario binaries.

Client keys have no identity semantics. Clients may use ephemeral X25519 keys and send encrypted packets to a node whose public key, IP, and port they know. AEAD success does not make the sender trusted; it only authenticates the packet key.

Clients can use query-style message flows directly. Write-like or influence-bearing flows require tickets unless the sender is a connected peer.

## Known Gaps

- Client library ergonomics over the UDP message API are not designed.
- Client-ticket issuance and economics are not designed.
- Counterparty verification flow needs an agent-facing design summary.
- Rate limiting for client-heavy traffic is WIP.

## Primary Files

- [src/ec_node.rs](../../src/ec_node.rs)
- [src/ec_interface.rs](../../src/ec_interface.rs)
- [simulator/](../../simulator)

## Source Material

- [README.md](../../README.md)
- [Design/viability_assessment.md](../../Design/viability_assessment.md)

## Agent Notes

Use this page for design iterations before implementation. Keep non-goals explicit: EC is not a cryptocurrency, smart-contract platform, or general-purpose database.

Do not invent a separate high-level method API unless the requester asks for one. Current API direction is UDP packets carrying EC messages.
