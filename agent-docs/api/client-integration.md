# Client Integration

## Protocol Goal

Clients should eventually be able to bind, update, look up, and verify token state through a small API without needing simulator internals.

## Current Status

No stable client library exists. Current runnable behavior is through simulator examples and scenario binaries.

## Known Gaps

- Client request/response API is not designed.
- Counterparty verification flow needs an agent-facing design summary.
- Multi-point submission needs clearer client semantics.

## Primary Files

- [src/ec_node.rs](../../src/ec_node.rs)
- [src/ec_interface.rs](../../src/ec_interface.rs)
- [simulator/](../../simulator)

## Source Material

- [README.md](../../README.md)
- [Design/viability_assessment.md](../../Design/viability_assessment.md)

## Agent Notes

Use this page for design iterations before implementation. Keep non-goals explicit: EC is not a cryptocurrency, smart-contract platform, or general-purpose database.

