# Encrypted UDP Transport

## Protocol Goal

Future network packets should use encrypted UDP with identity-linked shared secrets while keeping the packet layer stateless and small enough for efficient message flow.

## Current Status

Design direction confirmed:

- a node exposes one UDP socket for EC packets
- operators running multiple local nodes use multiple ports
- packet plaintext header contains `version`, `sender_public_key`, and a random AEAD nonce
- packet header is authenticated as AEAD associated data
- payload is compact serialized EC message content; today this maps through internal `MessageEnvelope`
- encryption uses static X25519 shared secrets from `ec_identity`
- shared secrets are derived with Blake3 and `network_id` for network isolation
- AEAD target is ChaCha20Poly1305
- no receiver public key or receiver peer ID is included in the packet
- no packet-level session lifecycle or replay cache is intended

Receivers attempt decryption with their local private key and the packet `sender_public_key` unless an outer layer has already dropped or rate-limited the packet. If AEAD validation fails, the packet is discarded before parsing. If decryption succeeds, lower layers still decide whether the message is accepted.

Clients may use ephemeral X25519 keys with no mined identity. Connected nodes have public keys bound to mined identities and are maintained by peer discovery/elections. Referrals, invitations, answers, and block responses may teach peer metadata; identity proof material is not repeated in every packet.

## Known Gaps

- No UDP transport implementation exists yet.
- Compact serialization and exact packet byte layout are undecided.
- `MessageEnvelope` still needs a production transport mapping.
- Client tickets are not designed.
- Rate limiting is WIP.
- `chacha20poly1305` is not currently wired in `Cargo.toml`.

## Primary Files

- [src/ec_identity.rs](../../src/ec_identity.rs)
- [src/ec_ticket_manager.rs](../../src/ec_ticket_manager.rs)
- [src/ec_interface.rs](../../src/ec_interface.rs)

## Source Material

- [docs/identity-block-design.md](../../docs/identity-block-design.md)
- [docs/peer_election_design.md](../../docs/peer_election_design.md)
- [Design/argon2_peer_authentication_design.md](../../Design/argon2_peer_authentication_design.md)

## Agent Notes

Keep this topic design-first until transport code exists. Do not smuggle UDP/session assumptions into core protocol modules.

The packet layer should be dumb: decrypt, authenticate header, parse compact message content, attach temporary reply context, and hand off. It should not maintain long-lived client sessions or decide peer trust.

Replay handling is intentionally not a packet cache. Existing message/ticket/block idempotence and mempool rules should absorb repeats. A separate rate-limiting layer is still needed.

Aim to keep packets below MTU, roughly 1500 bytes. Message and serialization choices should preserve compact, non-fragmented flows.
