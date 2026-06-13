# Protocol Surface

## Protocol Goal

Define the external node/client API surface clearly enough that future transport code can route protocol messages without reverse-engineering simulator internals.

## Current Status

The intended external API is a single UDP packet surface. Each accepted packet decrypts to EC message content and is handed to the node message dispatcher. The Rust library is still network-agnostic: `EcNode` consumes and emits `MessageEnvelope` values, while the production UDP/orchestrator layer does not yet exist.

`MessageEnvelope` is an internal dispatch and simulator bridge, not the final wire API. Production ingress should connect it to packet metadata such as source address, sender public key, local socket, and any short-lived reply context.

Current implementation status:

- simulator and tests still use `u64` `PeerId`, `TokenId`, and `BlockId` aliases
- production identities are expected to use 256-bit public-key-bound peer IDs from `ec_identity`
- transport should eventually synthesize or validate envelope sender/receiver context rather than trusting it as wire truth

## Known Gaps

- UDP packet implementation does not exist.
- Compact wire serialization is not selected.
- `MessageEnvelope` still needs a production mapping to network-layer context.
- Client-ticket economics and validation are not designed.
- Rate limiting is WIP and belongs outside core message semantics.
- A future build or feature split may be needed for simulator `u64` IDs versus production 256-bit IDs.

## Primary Files

- [src/lib.rs](../../src/lib.rs)
- [src/ec_interface.rs](../../src/ec_interface.rs)
- [src/ec_node.rs](../../src/ec_node.rs)
- [src/ec_identity.rs](../../src/ec_identity.rs)

## Source Material

- [README.md](../../README.md)
- [docs/identity-block-design.md](../../docs/identity-block-design.md)

## Agent Notes

Do not describe the API as a separate high-level `bind/update/lookup/verify` method surface. The API topic currently means UDP packets carrying EC messages.

Do not treat encryption as peer trust. AEAD proves the packet was encrypted by the holder of the sender private key; `EcNode`, `EcPeers`, tickets, block validation, and rate limiting decide whether the contained message has authority.

Unknown client senders may use ephemeral X25519 keys. Their identity is reply context, not protocol authority. Long-lived peer identity state belongs in peer discovery and connection management.
