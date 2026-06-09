# Commit-Chain And Minefield

## Protocol Goal

Use local commit-chain sync for recent history and develop minefield accountability so contradictory signed attestations expose post-commit rewrite fraud.

## Current Status

Commit-chain tracking and sync exist. Minefield accountability is a design concept and is not fully implemented as a production enforcement mechanism.

## Known Gaps

- Commit-chain sync needs a current implementation summary.
- Minefield accountability needs a concise refreshed design doc.
- Fraud detection incentives and storage obligations remain open.

## Primary Files

- [src/ec_commit_chain.rs](../../src/ec_commit_chain.rs)
- [src/ec_node.rs](../../src/ec_node.rs)
- [simulator/commit_chain/](../../simulator/commit_chain)

## Source Material

- [Design/commit_chain.md](../../Design/commit_chain.md)
- [Design/minefield_accountability_design.md](../../Design/minefield_accountability_design.md)
- [notes.txt](../../notes.txt)

## Agent Notes

Keep commit-chain implementation facts separate from minefield aspirations.

