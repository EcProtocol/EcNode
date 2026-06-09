# Token Block Lifecycle

## Protocol Goal

Describe how token updates become blocks, how blocks move through pending/committed/blocked states, and how storage retention bounds the lifecycle.

## Current Status

Implemented primarily through `EcMemPool`, shared types in `ec_interface`, and storage backends.

## Known Gaps

- Needs extraction from code and current tests.
- Needs alignment with future client/API wording.

## Primary Files

- [src/ec_mempool.rs](../../src/ec_mempool.rs)
- [src/ec_interface.rs](../../src/ec_interface.rs)
- [src/ec_memory_backend.rs](../../src/ec_memory_backend.rs)

## Source Material

- [docs/ec_protocol_v0.12.md](../../docs/ec_protocol_v0.12.md)
- [README.md](../../README.md)

## Agent Notes

Keep current implementation status distinct from protocol aspiration.

