# Glossary

This glossary should stay short. Prefer links to topic docs over long definitions.

- `Agent docs`: The primary, terse knowledge base under `agent-docs/`.
- `Block`: A transaction block carrying token updates. See [api/message-model.md](api/message-model.md).
- `Bounded retention`: The protocol goal that state has a finite public retention horizon.
- `Commit-chain`: Local append-only commit history used for sync and later accountability work.
- `Conflict visibility`: The property that user-created conflicts remain visible rather than being hidden by forced resolution.
- `EcNode`: Main Rust node facade in [src/ec_node.rs](../src/ec_node.rs).
- `EcMemPool`: Block state machine and vote/conflict handling in [src/ec_mempool.rs](../src/ec_mempool.rs).
- `EcPeers`: Peer lifecycle, elections, topology, and pruning in [src/ec_peers.rs](../src/ec_peers.rs).
- `Identity mining`: Argon2-based peer identity generation and validation.
- `Minefield accountability`: The post-commit design idea that signed contradictory history attestations expose rewrite fraud.
- `PeerId`: Currently a `u64` alias for simulation friendliness.
- `PoAS`: Proof-of-Aligned-Storage.
- `RequestBatch`: A message variant that can coalesce vote and query-like requests.
- `TokenId`: Currently a `u64` alias. Do not start the future `TokenHash = Blake3(TokenId)` migration as part of unrelated work.
- `VOTE_THRESHOLD`: Current vote threshold constant in [src/ec_interface.rs](../src/ec_interface.rs).

