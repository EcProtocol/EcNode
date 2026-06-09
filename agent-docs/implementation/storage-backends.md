# Storage Backends

## Protocol Goal

Storage abstractions should support token, block, and commit-chain state without hard-coding the reference node to one persistence implementation.

## Current Status

`MemoryBackend` is the default backend used by tests and simulators. RocksDB code exists behind an unwired feature gate.

## Known Gaps

- RocksDB feature/dependency is not wired in `Cargo.toml`.
- Storage changes must be aligned across traits, memory backend, batched writes, and simulator stores.

## Primary Files

- [src/ec_interface.rs](../../src/ec_interface.rs)
- [src/ec_memory_backend.rs](../../src/ec_memory_backend.rs)
- [src/ec_rocksdb_backend.rs](../../src/ec_rocksdb_backend.rs)
- [simulator/consensus/hashmap_tokens.rs](../../simulator/consensus/hashmap_tokens.rs)

## Source Material

- [AGENTS.md](../../AGENTS.md)
- [README.md](../../README.md)

## Agent Notes

Do not assume RocksDB builds by default.

