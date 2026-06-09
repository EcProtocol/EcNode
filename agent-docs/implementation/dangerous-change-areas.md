# Dangerous Change Areas

## Protocol Goal

Make high-blast-radius areas visible before agents edit them.

## Current Status

The repo is pre-production and can accept breaking changes, but module boundaries should become clearer over time.

## Known Gaps

- Need structural checks or a future doc-maintenance skill to keep this list current.

## Primary Files

- [src/ec_interface.rs](../../src/ec_interface.rs)
- [src/ec_node.rs](../../src/ec_node.rs)
- [src/ec_peers.rs](../../src/ec_peers.rs)
- [src/ec_mempool.rs](../../src/ec_mempool.rs)

## Source Material

- [agent-docs/implementation/module-boundaries.md](module-boundaries.md)

## Agent Notes

Be especially careful with:

- Message variants and shared aliases in `ec_interface.rs`.
- Peer topology, pruning, and election defaults.
- Storage trait changes.
- Token/block ID migration ideas.
- Commit-chain sync behavior.
- Random behavior in tests and simulations.

