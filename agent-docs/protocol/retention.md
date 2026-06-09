# Retention

## Protocol Goal

Bound storage and accountability obligations with a public finite retention horizon. Applications must redeem, extend, or tolerate expiry.

## Current Status

Bounded retention is a protocol pillar. Implementation details need a current extraction pass.

## Known Gaps

- Exact implementation hooks need to be documented.
- Retention interaction with commit-chain sync and minefield accountability needs a compact design summary.

## Primary Files

- [README.md](../../README.md)
- [src/ec_commit_chain.rs](../../src/ec_commit_chain.rs)
- [src/ec_memory_backend.rs](../../src/ec_memory_backend.rs)

## Source Material

- [README.md](../../README.md)
- [Design/viability_assessment.md](../../Design/viability_assessment.md)

## Agent Notes

Do not describe EC as infinite-history infrastructure.

