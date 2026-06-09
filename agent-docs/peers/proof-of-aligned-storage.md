# Proof-Of-Aligned-Storage

## Protocol Goal

Peers should demonstrate that they store the local state they claim to host, aligned with the surrounding neighborhood.

## Current Status

Signature-based proof-of-storage helpers and peer-election logic exist in `ec_proof_of_storage`.

## Known Gaps

- Needs current extraction from implementation and design docs.
- Relationship to peer lifecycle and topology should be clarified.

## Primary Files

- [src/ec_proof_of_storage.rs](../../src/ec_proof_of_storage.rs)
- [src/ec_peers.rs](../../src/ec_peers.rs)

## Source Material

- [docs/aligned_storage_proof_eprint_v3.md](../../docs/aligned_storage_proof_eprint_v3.md)
- [Design/signature_based_proof_of_storage_analysis.md](../../Design/signature_based_proof_of_storage_analysis.md)

## Agent Notes

PoAS is a top-level pillar, not an implementation detail of elections.

