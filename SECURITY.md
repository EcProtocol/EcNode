# Security

This document keeps agents focused on the core protocol properties. It is not a production security audit.

## Core Properties

### Commit-Time Safety

The protocol should not commit lower-owner or multi-owner results for the same token under the modeled assumptions. Conflict behavior is part of the protocol, not an exceptional cleanup path.

### Conflict Visibility

Transactions are user-signed. The network should not be able to manufacture conflicts; conflicts come from a key-holder signing contenders. The protocol should keep those conflicts visible to counterparties rather than hiding them through forced resolution.

### Post-Commit Rewrite Accountability

Commit-chain state exists today. Minefield accountability is the design direction for exposing rewrite fraud through contradictory signed attestations to recent history.

### Sybil Resistance

Peer identity mining uses Argon2 to make identities costly and effectively random. This supports Sybil resistance, address placement, and future network isolation rules.

### Storage Alignment

Proof-of-Aligned-Storage is a top-level pillar: peers should demonstrate that they store the state they claim to host in the local neighborhood.

### Identity And Shared Secrets

Identity code includes X25519 shared-secret derivation. Future encrypted transport should bind packet/session behavior to peer identity and network isolation assumptions.

### Encrypted UDP/API Transport

The intended direction is encrypted UDP packets using X25519 plus AEAD, inspired by WireGuard. The packet format, AEAD/library choice, replay protection, and session lifecycle are not yet settled.

### Bounded Retention

Security and storage claims are retention-bounded. EC is not infinite-history infrastructure.

## Non-Goals

- EC is not a cryptocurrency.
- EC is not a smart-contract platform.
- EC is not a trustless instant-finality payment system.
- EC is not a general-purpose database.
- Current code is a reference implementation and simulator suite, not production-ready networking software.

## Known Gaps

- Production transport security is not implemented.
- Minefield accountability is not fully implemented as an enforcement mechanism.
- Identity TTL and network identity rules need clearer current design.
- Ticket validation and encrypted packet design need integration work.

## Agent Notes

When changing security-relevant code, update the relevant `agent-docs/` page and [OPEN_ISSUES.md](OPEN_ISSUES.md) if implementation and protocol goal diverge.

