# Scenario Runner

## Protocol Goal

Provide reproducible scenario-based simulation runs from YAML files.

## Current Status

`scenario_runner` is declared as a Cargo binary and can run a single scenario or a directory.

## Known Gaps

- Needs current extraction of YAML schema and runner behavior.

## Primary Files

- [simulator/scenario_runner.rs](../../simulator/scenario_runner.rs)
- [scenarios/bootstrap.yaml](../../scenarios/bootstrap.yaml)

## Source Material

- [Design/SCENARIO_ANALYSIS.md](../../Design/SCENARIO_ANALYSIS.md)

## Agent Notes

Prefer explicit parameters and deterministic seeds in new scenarios.

