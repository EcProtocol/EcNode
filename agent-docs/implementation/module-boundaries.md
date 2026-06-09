# Module Boundaries

## Protocol Goal

The implementation should keep protocol concerns legible and separately testable. Breaking changes are allowed while the system is pre-production, but they should make module boundaries clearer rather than blur responsibilities.

## Current Status

The crate is a Rust library named `ec_rust`. There is no `src/main.rs`; runnable behavior is exposed through Cargo examples and simulator binaries.

Core modules:

- [src/ec_interface.rs](../../src/ec_interface.rs): Shared protocol types, message envelopes, block/token aliases, storage traits, event types, request batching, and commit-chain message types.
- [src/ec_node.rs](../../src/ec_node.rs): Main node facade. Owns mempool, peer manager, ticket manager, local time, event sink, batching behavior, and commit-chain sync toggles.
- [src/ec_mempool.rs](../../src/ec_mempool.rs): Block state machine for pending, committed, and blocked work; vote accounting; conflict-aware repair; vote scheduling; diagnostics.
- [src/ec_peers.rs](../../src/ec_peers.rs): Peer lifecycle and topology management. Handles identified, pending, and connected states; invitations; referrals; elections; token sampling; pruning; adaptive neighborhoods.
- [src/ec_proof_of_storage.rs](../../src/ec_proof_of_storage.rs): Signature-based proof-of-storage helpers, token storage backend trait, ring distance, consensus clustering, and peer election logic.
- [src/ec_commit_chain.rs](../../src/ec_commit_chain.rs): Local append-only commit-chain tracking and sync behavior.
- [src/ec_memory_backend.rs](../../src/ec_memory_backend.rs): In-memory token, block, and commit-chain backend plus batched writes.
- [src/ec_identity.rs](../../src/ec_identity.rs): Peer identity generation/validation, Argon2 mining configs, timestamp validation, network isolation, and X25519 shared-secret derivation.
- [src/ec_genesis.rs](../../src/ec_genesis.rs): Deterministic genesis token generation and selective storage initialization.
- [src/ec_ticket_manager.rs](../../src/ec_ticket_manager.rs): Per-use-case message ticket generation, validation, and rotating secrets.
- [src/ec_rocksdb_backend.rs](../../src/ec_rocksdb_backend.rs): Optional persistent backend behind `#[cfg(feature = "rocksdb-backend")]`. The Cargo feature/dependency is not wired in `Cargo.toml`.

## Known Gaps

- External network/API orchestration is not yet a stable production surface.
- A future orchestrator may collect outbound messages and package them for network transport.
- The future `TokenHash = Blake3(TokenId)` migration is documented but not started.
- RocksDB support is present as conditional code but is not part of the default build.

## Primary Files

- [src/lib.rs](../../src/lib.rs)
- [Cargo.toml](../../Cargo.toml)
- [src/ec_interface.rs](../../src/ec_interface.rs)
- [src/ec_node.rs](../../src/ec_node.rs)
- [src/ec_peers.rs](../../src/ec_peers.rs)
- [src/ec_mempool.rs](../../src/ec_mempool.rs)

## Source Material

- [AGENTS.md](../../AGENTS.md)
- [README.md](../../README.md)
- [notes.txt](../../notes.txt)

## Agent Notes

- Treat `ec_interface.rs` as high blast radius.
- Keep storage changes aligned across storage traits, `MemoryBackend`, batched writes, and simulator token stores.
- Keep `EcNode` as the integration point.
- Keep `EcPeers` and `EcMemPool` separately testable.
- Use event sinks and diagnostics for simulator observability rather than printing from core logic.
- Keep random behavior seedable in tests and simulations.

