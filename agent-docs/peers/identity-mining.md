# Identity Mining

## Protocol Goal

Make peer identities costly to create and structurally useful for address placement, network isolation, and shared-secret derivation.

## Current Status

Argon2-based identity generation and validation exist. X25519 shared-secret derivation exists. Timestamp and network-isolation behavior exists in code and needs concise extraction.

## Known Gaps

- Identity TTL policy needs design clarification.
- Network identity and salt rules need a current agent summary.
- API/transport usage needs design work.

## Primary Files

- [src/ec_identity.rs](../../src/ec_identity.rs)

## Source Material

- [docs/identity-block-design.md](../../docs/identity-block-design.md)
- [Design/peer_identity_and_argon2_optimization.md](../../Design/peer_identity_and_argon2_optimization.md)
- [Design/argon2_peer_authentication_design.md](../../Design/argon2_peer_authentication_design.md)
- [notes.txt](../../notes.txt)

## Agent Notes

Keep identity-mining and PoAS linked but distinct.

