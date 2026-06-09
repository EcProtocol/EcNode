# Encrypted UDP Transport

## Protocol Goal

Future network packets should use encrypted UDP with identity-linked shared secrets. The current design direction is X25519 plus AEAD, inspired by WireGuard.

## Current Status

X25519 public key and shared-secret helpers exist in identity code. The packet format, AEAD choice, replay handling, and library choices are not yet settled.

## Known Gaps

- AEAD algorithm/library undecided.
- Packet framing undecided.
- Handshake/session lifecycle undecided.
- Replay protection and rotation policy undecided.
- Relationship between tickets and encrypted packets needs design.

## Primary Files

- [src/ec_identity.rs](../../src/ec_identity.rs)
- [src/ec_ticket_manager.rs](../../src/ec_ticket_manager.rs)
- [src/ec_interface.rs](../../src/ec_interface.rs)

## Source Material

- [docs/identity-block-design.md](../../docs/identity-block-design.md)
- [Design/argon2_peer_authentication_design.md](../../Design/argon2_peer_authentication_design.md)
- [notes.txt](../../notes.txt)

## Agent Notes

Keep this topic design-first until the API surface is clearer. Do not smuggle transport assumptions into core protocol modules.

