# Topology And Routing

## Protocol Goal

Maintain local ring-neighborhood correctness while preserving enough sparse far links and latency awareness for practical routing at scale.

## Current Status

Topology and pruning behavior are implemented in `EcPeers` and simulator peer-lifecycle modules. Small-world design is documented but still experimental.

The candidate lifecycle separates known-peer maintenance, connected topology repair, and invite handling. Known-peer maintenance handles stale cleanup, bounded invite retry/refresh, commit-chain RTT probes, and referral discovery. Connected repair can start token walks from nearby `Identified`, `Pending`, or `Connected` peer IDs instead of maintaining a shadow token set or requiring local token-store coverage in the underfilled area. This may also support future client-library peer discovery, where clients learn peer knowledge but never promote peers to `Connected`.

The current peer-shape target is captured in [peer-shape-target.md](peer-shape-target.md). The short version is: preserve dense local coverage, controlled fade, and sparse useful remote/cell coverage; do not judge topology by connected count or graph shortest path alone.

## Known Gaps

- Needs a current simulator matrix that replays the most important target-shape evidence against the current code.
- Latency-weighted thinning and adaptive peer budget should be tracked against implementation status.
- Test the candidate known-peer maintenance, referral cold-start, connected token-walk, commit-chain RTT probing, and invite-triggered election model before replacing existing topology strategies.
- Add protocol-shaped routing-progress metrics: distance reduction toward role coverers and time to first covering-neighborhood contact.

## Primary Files

- [src/ec_peers.rs](../../src/ec_peers.rs)
- [simulator/peer_lifecycle/](../../simulator/peer_lifecycle)

## Source Material

- [Design/small-world.md](../../Design/small-world.md)
- [Design/routing_depth_scaling.md](../../Design/routing_depth_scaling.md)
- [simulator/PEER_LIFECYCLE_GRAPH_SHAPE_REPORT.md](../../simulator/PEER_LIFECYCLE_GRAPH_SHAPE_REPORT.md)
- [peer-shape-target.md](peer-shape-target.md)
- [peer-lifecycle-structure.md](peer-lifecycle-structure.md)

## Agent Notes

Peer topology is subtle. Prefer targeted changes with deterministic simulator follow-up.
