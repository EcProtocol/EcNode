# Protocol Surface

## Protocol Goal

Define the external node/client API surface clearly enough that future clients can be built without reverse-engineering simulator internals.

## Current Status

The Rust library is network-agnostic. `EcNode` consumes and emits `MessageEnvelope` values, while transport and client orchestration are not yet a stable production API.

## Known Gaps

- Stable client API is not designed.
- Network packet format is not designed.
- UDP encryption format is not selected beyond the X25519 plus AEAD direction.
- The split between node API, client API, and simulator harness API needs refinement.

## Primary Files

- [src/lib.rs](../../src/lib.rs)
- [src/ec_interface.rs](../../src/ec_interface.rs)
- [src/ec_node.rs](../../src/ec_node.rs)

## Source Material

- [README.md](../../README.md)
- [notes.txt](../../notes.txt)

## Agent Notes

Do not treat simulator entry points as the final client API. Use this page to capture API design sessions before implementation exists.

