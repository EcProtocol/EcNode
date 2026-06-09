# Simulator Overview

## Protocol Goal

Use simulations as the empirical feedback loop for protocol behavior under topology, delay, loss, churn, and conflict conditions.

## Current Status

Simulator code lives under `simulator/` and is wired through Cargo examples and binaries.

## Known Gaps

- Needs extraction from simulator README and current runner modules.
- Some older reports should be moved or deleted after distillation.

## Primary Files

- [simulator/](../../simulator)
- [Cargo.toml](../../Cargo.toml)

## Source Material

- [simulator/README.md](../../simulator/README.md)
- [agent-docs/simulator/evidence-index.md](evidence-index.md)
- [agent-docs/simulator/operations-development.md](operations-development.md)

## Agent Notes

The simulator is part of this repo through examples and binaries, not a separate crate.
