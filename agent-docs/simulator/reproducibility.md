# Reproducibility

## Protocol Goal

Simulation claims should be replayable and parameterized.

## Current Status

Many simulator entry points support deterministic configuration, but full determinism remains an open issue in some paths.

## Known Gaps

- Hash map iteration and other nondeterministic ordering may affect some simulator runs.
- Reports need a consistent command/seed/parameter footer.

## Primary Files

- [simulator/](../../simulator)
- [scenarios/](../../scenarios)

## Source Material

- [agent-docs/simulator/evidence-index.md](evidence-index.md)
- [notes.txt](../../notes.txt)

## Agent Notes

For simulator changes, record exact commands and parameters in reports.

