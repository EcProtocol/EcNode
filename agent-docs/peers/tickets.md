# Tickets

## Protocol Goal

Tickets should constrain or authorize message use cases without leaking unnecessary state and should compose with identity and future encrypted transport.

## Current Status

Ticket generation, validation, and rotating secrets exist in `ec_ticket_manager`. Integration details need extraction.

## Known Gaps

- Ticket validation in `EcNode` is an open issue.
- Interaction with encrypted UDP packet design is unresolved.

## Primary Files

- [src/ec_ticket_manager.rs](../../src/ec_ticket_manager.rs)
- [src/ec_node.rs](../../src/ec_node.rs)
- [src/ec_interface.rs](../../src/ec_interface.rs)

## Source Material

- [docs/ticket_system_design.md](../../docs/ticket_system_design.md)
- [notes.txt](../../notes.txt)

## Agent Notes

Do not treat tickets as transport encryption.

