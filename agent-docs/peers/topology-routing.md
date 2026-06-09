# Topology And Routing

## Protocol Goal

Maintain local ring-neighborhood correctness while preserving enough sparse far links and latency awareness for practical routing at scale.

## Current Status

Topology and pruning behavior are implemented in `EcPeers` and simulator peer-lifecycle modules. Small-world design is documented but still experimental.

## Known Gaps

- Needs current extraction from `EcPeers` tests and simulator reports.
- Latency-weighted thinning and adaptive peer budget should be tracked against implementation status.

## Primary Files

- [src/ec_peers.rs](../../src/ec_peers.rs)
- [simulator/peer_lifecycle/](../../simulator/peer_lifecycle)

## Source Material

- [Design/small-world.md](../../Design/small-world.md)
- [Design/routing_depth_scaling.md](../../Design/routing_depth_scaling.md)
- [simulator/PEER_LIFECYCLE_GRAPH_SHAPE_REPORT.md](../../simulator/PEER_LIFECYCLE_GRAPH_SHAPE_REPORT.md)

## Agent Notes

Peer topology is subtle. Prefer targeted changes with deterministic simulator follow-up.

