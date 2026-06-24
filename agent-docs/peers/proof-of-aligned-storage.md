# Proof-Of-Aligned-Storage

## Protocol Goal

Peers should demonstrate that they store the local state they claim to host, aligned with the surrounding neighborhood.

## Current Status

Signature-based proof-of-storage helpers and peer-election logic exist in `ec_proof_of_storage`.

For peer lifecycle answers, the signature token set is requester-bound:

```text
Blake3(requesting_peer_id || token || block)
```

This means an `Answer` prepared for one requester should not be reusable as a valid answer to another requester. The answer can still reveal candidate walk/election tokens, so lifecycle code must keep local control of density checks and final challenge-token selection.

## Known Gaps

- Needs current extraction from implementation and design docs.
- Relationship to peer lifecycle and topology should be further validated in tests/simulators.

## Primary Files

- [src/ec_proof_of_storage.rs](../../src/ec_proof_of_storage.rs)
- [src/ec_peers.rs](../../src/ec_peers.rs)

## Source Material

- [docs/aligned_storage_proof_eprint_v3.md](../../docs/aligned_storage_proof_eprint_v3.md)
- [Design/signature_based_proof_of_storage_analysis.md](../../Design/signature_based_proof_of_storage_analysis.md)

## Agent Notes

PoAS is a top-level pillar, not an implementation detail of elections.
