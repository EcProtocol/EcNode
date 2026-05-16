## The core idea

A **small-world network** is one that has two properties simultaneously, which naively seem to be in tension:

- **High local clustering** — your neighbours tend to know each other. Dense local structure.
- **Short global path length** — despite the local density, any two nodes in the entire network are reachable in surprisingly few hops. Logarithmic in N, not linear.

The canonical observation is Milgram's 1967 social experiment: letters forwarded by acquaintances across the US reached strangers in an average of ~6 hops — "six degrees of separation." The network was simultaneously locally clustered (you know your neighbours well) and globally compact (the world is small).

The structural explanation: a small number of **long-range weak ties** are sufficient to collapse global diameter dramatically. You don't need many. Even replacing 1% of local edges with random long-range connections reduces path length from O(N) to O(log N), while barely touching the clustering coefficient.

## Watts-Strogatz (1998)

The foundational formal paper. They took a regular ring lattice (every node connected to its k nearest neighbours — your ring-gradient structure is this), and *rewired* each edge with probability p to a random distant node.

At p=0: regular lattice, high clustering, long paths.  
At p=1: random graph, low clustering, short paths.  
At p≈0.01–0.1: **both** high clustering *and* short paths. The small-world regime.

The striking result: a tiny fraction of random long-range links is all you need. The local structure is almost entirely preserved; the global structure collapses.

> Watts, D.J. & Strogatz, S.H. (1998). *Collective dynamics of 'small-world' networks.* **Nature**, 393, 440–442.

This is the paper that named the phenomenon formally. Short, readable, important.

## Kleinberg (2000) — the navigability result

Watts-Strogatz shows small-world graphs *exist*. Kleinberg asked a sharper question: **can you route efficiently through one using only local information?** That is, can a node forward a message toward a target without knowing the global graph?

The answer is: only if the long-range links are distributed according to a specific power law. In a 2D grid, if the probability of a long-range connection at distance d scales as **1/d²**, greedy routing (always forward to the neighbour closest to the target) achieves O(log² N) delivery. Any other exponent — too many short links, too many long links — and greedy routing degrades to polynomial.

This is directly relevant to your design. The gradient ring with latency-biased thinning is implicitly implementing a 1D version of Kleinberg's structure. The "steep local, sparse far" connection probability is the right shape. The result says this isn't just intuitive — it's the *unique* distribution that makes greedy local routing efficient.

> Kleinberg, J. (2000). *The small-world phenomenon: An algorithmic perspective.* **Proceedings of the 32nd ACM STOC**, 163–170.

Also available as a Cornell TR. This is the one to read if you want the mathematical substance. The proof is elegant and not long.

## Barabási-Albert (1999) — scale-free networks

Related but distinct. Instead of rewiring, BA asks what happens when networks *grow* with preferential attachment — new nodes connect to existing nodes with probability proportional to their current degree. The result is a power-law degree distribution: a few highly-connected hubs, many low-degree nodes. Many real networks (web, citations, some social) have this property.

Scale-free networks also tend to have small-world path lengths, but for a different structural reason (hubs act as shortcuts). Less directly applicable to EC — you're not building a hub-and-spoke topology — but worth knowing because reviewers sometimes conflate small-world and scale-free.

> Barabási, A.L. & Albert, R. (1999). *Emergence of scaling in random networks.* **Science**, 286, 509–512.

## How this maps to EC Protocol

The connection is close enough to be more than analogy.

Your **gradient ring** is the local lattice structure — the regular, high-clustering component. Each peer is strongly connected to ring-nearby peers. This gives you the dense local commit behaviour your simulator demonstrates.

Your **sparse far-ring connections** are the long-range weak ties — the Watts-Strogatz rewiring. They're what keep global path length short despite local density. The multi-point submission discussion we had is essentially "exploit the short global path length so wavefronts from different origins converge quickly."

Your **latency-biased thinning** is an operationalisation of Kleinberg's result: you're shaping the long-range link distribution to be distance-decaying, which is exactly the distribution that makes greedy local routing efficient. Not by explicit design from the theory, but the intuition arrived at the same place the theory did.

And the **adaptive peer budget** is a practical mechanism for maintaining the small-world regime as N grows — keeping the ratio of local-to-long-range links in the right range without global coordination.

The one result worth internalising from Kleinberg specifically: the efficient routing property is fragile to the *shape* of the distance distribution. Too many local links (no long-range shortcuts) → long paths. Too many random long-range links (uniform distribution) → short paths but no efficient local routing, you need global knowledge to navigate. The sweet spot is the power-law falloff. Your simulator sweep on the locality bias parameter is, in effect, finding that sweet spot empirically. Worth knowing the theory says there is one, and where it is.

## Further reading if you want to go deeper

**For the theory:**
- Newman, M.E.J. (2003). *The structure and function of complex networks.* **SIAM Review**, 45(2), 167–256. — comprehensive survey, freely available, covers small-world, scale-free, and navigability together.
- Kleinberg, J. (2006). *Complex networks and decentralized search algorithms.* **Proceedings of ICM 2006.** — accessible retrospective by Kleinberg himself.

**For the DHT connection:**
- Rowstron & Druschel (2001). *Pastry: Scalable, distributed object location and routing.* — implements small-world-style routing in a DHT ring. Close structural cousin to what you're building.
- Maymounkov & Mazières (2002). *Kademlia: A peer-to-peer information system based on the XOR metric.* — the XOR metric creates small-world-like properties; understanding why helps you understand where gradient distance is stronger.

**For intuition:**
- Watts, D.J. (2003). *Six Degrees: The Science of a Connected Age.* Norton. — non-technical, good for communicating the ideas to non-specialists. Worth reading for the write-up vocabulary.

---

The short version for your design docs: EC Protocol's gradient ring with sparse far connections and latency-biased thinning is a practical instantiation of a small-world network, where Kleinberg's navigability result provides theoretical grounding for why the distance-decaying connection probability is the right shape, and Watts-Strogatz explains why even a small number of far connections is sufficient for global compactness. The simulator sweep on locality bias is finding the Watts-Strogatz sweet spot empirically.

## Integrated simulator experiment: location-scoped entry

The integrated runner now has an experimental transaction entry filter:

- `EC_LONG_RUN_ENTRY_LOCATIONS=0` keeps the old behavior: all eligible peers can submit.
- `EC_LONG_RUN_ENTRY_LOCATIONS=1` limits transaction entry to one low-bit location cell.
- `EC_LONG_RUN_ENTRY_LOCATIONS=N` creates `N` evenly spaced entry cells on the low-bit location ring.
- `EC_LONG_RUN_ENTRY_LOCATION_BITS` selects how many low bits are treated as location.
- `EC_LONG_RUN_ENTRY_LOCATION_WIDTH` controls each entry cell width as a fraction of the ring.

This gives a direct test for the expected small-world behavior: a transaction should commit quickly for the submitting location while still spreading into other cells.

### Larger run: 300 peers, small-world budget 80

Command shape:

```bash
EC_LONG_RUN_SEED_VARIANT=12 \
EC_LONG_RUN_INITIAL_PEERS=300 \
EC_LONG_RUN_ROUNDS=350 \
EC_LONG_RUN_JOIN_COUNT=0 \
EC_LONG_RUN_CRASH_COUNT=0 \
EC_LONG_RUN_RETURN_COUNT=0 \
EC_LONG_RUN_SECOND_JOIN_COUNT=0 \
EC_LONG_RUN_SECOND_CRASH_COUNT=0 \
EC_LONG_RUN_GENESIS_BLOCKS=20000 \
EC_LONG_RUN_NETWORK_PROFILE=same_dc \
EC_LONG_RUN_TRANSACTION_START_ROUND=140 \
EC_LONG_RUN_BLOCKS_PER_ROUND=2 \
EC_LONG_RUN_BLOCK_SIZE_MIN=1 \
EC_LONG_RUN_BLOCK_SIZE_MAX=2 \
EC_LONG_RUN_ELECTIONS_PER_TICK=8 \
EC_LONG_RUN_ELECTION_TIMEOUT=100 \
EC_LONG_RUN_MIN_COLLECTION_TIME=10 \
EC_LONG_RUN_PRUNE_PROTECTION_TIME=0 \
EC_LONG_RUN_SMALL_WORLD=true \
EC_LONG_RUN_SMALL_WORLD_BUDGET=80 \
EC_LONG_RUN_SMALL_WORLD_HYSTERESIS=10 \
EC_LONG_RUN_SMALL_WORLD_LOCATION_BITS=32 \
EC_LONG_RUN_SMALL_WORLD_FAR_FRACTION=0.10 \
EC_LONG_RUN_SMALL_WORLD_FAR_DISTANCE=0.25 \
EC_LONG_RUN_SMALL_WORLD_DISTANCE_EXPONENT=2.0 \
EC_LONG_RUN_ENTRY_LOCATION_BITS=32 \
EC_LONG_RUN_ENTRY_LOCATION_WIDTH=0.08 \
EC_LONG_RUN_ENTRY_LOCATIONS=<1-or-3> \
cargo run --release --quiet --example integrated_long_run
```

| Entry cells | Submitters | Committed | Pending | Commit avg | Commit p50 | Commit p95 | Settled spread avg | Spread p95 | Block msgs avg | Block msgs p95 | Reachable vote graph avg |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 23 | 382 / 420 | 38 | 20.0 | 18 | 48 | 96.8 | 189 | 1508.9 | 3682 | 74.9 |
| 3 | 63 | 387 / 420 | 33 | 19.1 | 14 | 47 | 94.9 | 180 | 1474.8 | 3586 | 78.1 |

Latency by entry-distance bucket:

| Entry cells | Local avg / p95 | Near avg / p95 | Mid avg / p95 |
| --- | ---: | ---: | ---: |
| 1 | 16.2 / 47 | 20.7 / 48 | 20.4 / 47 |
| 3 | 14.7 / 47 | 20.5 / 48 | 18.5 / 47 |

The local bucket is faster than the near and mid buckets in both runs. That matches the desired direction: the node near the submission location gets satisfied sooner, while the transaction still propagates out into the wider graph. Spread remains non-local: the average settled spread is roughly 95-97 peers out of 300, with p95 between 180 and 189.

The graph is not yet filling the configured small-world budget. Final average connected peers were about 49, with max peers reaching 79-81. Earlier runs with the default election timeout produced only about 14-15 average connected peers and much higher settled spread/message counts, so the election timeout is currently a major formation parameter for this experiment.

Initial read: the result supports the thesis, but the peer formation side still needs tuning before we can treat the numbers as a healthy steady-state small-world result. The next useful sweep is likely budget/far-fraction/election-timeout against the same entry-location comparison.

### Follow-up: pushing degree toward fixed-network shape

The first larger-small-world runs showed that p95 latency around `46-47` was mainly a degree/formation problem. Raising the budget alone did not fill the graph:

| Case | Peers | Formation | Avg connected | p95 connected | Commit avg | Commit p50 | Commit p95 | Spread avg | Spread p95 | Block msgs avg |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| budget 160, normal discovery | 300 | organic | 83.3 | 107 | 15.8 | 10 | 41 | 60.3 | 105 | 996.6 |
| budget 160, lower election threshold | 300 | organic | 78.4 | 109 | 16.4 | 9 | 46 | 63.2 | 111 | 1057.6 |
| budget 160, high initial random knowledge | 300 | 160 known peers/node | 171.4 | 179 | 6.5 | 5 | 20 | 27.1 | 36 | 462.8 |
| budget 120, high initial random knowledge | 300 | 160 known peers/node | 125.2 | 130 | 10.4 | 5 | 28 | 37.3 | 56 | 627.4 |
| fixed-shape initial topology | 300 | ring-linear 1.0/0.2/10 | 202.0 | 219 | 6.9 | 5 | 22 | 24.2 | 41 | 395.0 |
| fixed-shape initial topology, 1 block/round | 300 | ring-linear 1.0/0.2/10 | 201.1 | 217 | 6.6 | 5 | 20 | 23.9 | 39 | 391.8 |

This gives a clearer read:

- connection count matters a lot: moving from roughly `80` connected peers to roughly `170-200` connected peers cuts p95 from the mid-40s to about `20-22`
- message cost also drops substantially: block messages to settle fall from about `1000` to `400-460`
- merely reducing block rate from `2` to `1` per round does not remove the p95 tail
- the remaining p95 tail is concentrated in transactions whose entry peer is far from the token neighborhood

The fixed-network reports reached p95 `4-5`, so there is still a real gap. The likely cause in this specific workload is that `EC_LONG_RUN_ENTRY_LOCATIONS=1` restricts submitters by low-bit location, but transaction token IDs are still drawn globally. That means most transactions entering one location are not local to that location's token neighborhood. In the fixed-shaped run, local and near buckets were fast (`local p95 4`, `near p95 6`), while far-token entries drove the tail (`far p95 27`).

Next test target: add a transaction-token locality mode so a "one location" workload can mean both entry peers and submitted token IDs are in the same location cell. That should test the actual thesis directly: fast local commit first, followed by spread/commit into other cells.

### Fixed-topology sweep: how small can the world be?

To separate topology quality from lifecycle formation, the steady-state runner now also accepts location-scoped entry:

- `EC_STEADY_STATE_ENTRY_LOCATIONS`
- `EC_STEADY_STATE_ENTRY_LOCATION_BITS`
- `EC_STEADY_STATE_ENTRY_LOCATION_WIDTH`

The sweep below uses:

- `600` peers
- fixed connected topology, no churn, no elections
- `same_dc`
- `1` block/round, block size `1..=2`
- fresh-token workload
- one entry cell: `EC_STEADY_STATE_ENTRY_LOCATIONS=1`, width `0.08`
- vote targets `2`, first vote targets `6`

| Topology | Avg connected | Connected % | Commit avg | Commit p50 | Commit p95 | Spread avg | Spread p95 | Block msgs avg | Block msgs p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ring-linear far `0.00` | 299.6 | 50% | 4.3 | 5 | 5 | 99.0 | 258 | 2009.5 | 5931 |
| ring-linear far `0.05` | 314.2 | 52% | 4.4 | 5 | 5 | 83.8 | 197 | 1842.8 | 4585 |
| ring-linear far `0.10` | 331.4 | 55% | 4.2 | 4 | 5 | 65.4 | 147 | 1454.6 | 3698 |
| ring-linear far `0.20` | 360.3 | 60% | 4.1 | 4 | 5 | 51.8 | 98 | 1191.7 | 2621 |
| ring-linear far `0.30` | 389.3 | 65% | 3.9 | 4 | 5 | 42.2 | 77 | 928.3 | 2034 |
| ring-linear far `0.40` | 419.9 | 70% | 3.7 | 4 | 5 | 35.6 | 59 | 769.1 | 1544 |
| ring-core-tail `30 + 20/side` | 128.8 | 21% | 5.1 | 5 | 6 | 156.3 | 237 | 3337.7 | 7040 |

Reading:

- latency is forgiving down to roughly half the graph for this fixed steady-state workload: p95 remains `5` for the ring-linear cases from `50%` to `70%`
- message load is not forgiving: settled spread and block messages improve steadily as far connectivity increases
- the sparse-tail topology is not enough, even though it preserves a dense local core; it pushes spread/message load much higher
- the best point in this sweep is not "smallest"; it is the denser far side, around `65-70%` connected, if judged by message load

This sharpens the peer-formation target. A viable pruning policy probably cannot keep only a small local cell plus a tiny far tail. It needs enough far/remote coverage that commits do not have to flood large remote regions to find their way back. In these fixed runs, the lower edge for latency looks near `50%` connected, while the lower edge for acceptable message load looks closer to `60-70%` connected.

### Synthetic low-bit locations

For small synthetic runs, random peer IDs do not reliably create the location
structure we want to test. The integrated simulators now support rewriting the
low peer-id bits after allocation but before topology construction:

- `EC_STEADY_STATE_PEER_ID_LOCATION_PATTERN=0x0000,0x8000`
- `EC_STEADY_STATE_PEER_ID_LOCATION_BITS=16`
- `EC_LONG_RUN_PEER_ID_LOCATION_PATTERN=0x0000,0x8000`
- `EC_LONG_RUN_PEER_ID_LOCATION_BITS=16`

The rewrite is applied in sorted full-ring order and repeats the pattern, so
with two values the sorted ring alternates between the two low-bit locations.
The rewritten IDs are also inserted back into the simulator token mapping, so
they remain valid peer-id tokens for discovery/election tests.

Important caveat: this does not by itself make the fixed topology
location-aware. The current fixed topology builders still connect by sorted
full peer-id rank. The low-bit pattern is therefore most directly useful for
testing the `ec_peers.rs` small-world pruning logic, which already uses low-bit
location distance. To make fixed-topology tests model "two dense cells with a
few cross-cell links", the next step is a location-aware fixed topology builder
that scores pair distance by the stamped low-bit coordinate instead of full-ring
rank.

### Two-cell fixed topology experiment

The steady-state runner now has a location-aware fixed topology:

- `EC_STEADY_STATE_TOPOLOGY=location_linear_probability`
- `EC_STEADY_STATE_LOCATION_TOPOLOGY_BITS=16`
- `EC_STEADY_STATE_LOCATION_TOPOLOGY_CENTER_PROB=1.0`
- `EC_STEADY_STATE_LOCATION_TOPOLOGY_FAR_PROB=<p>`

With the stamped pattern `0x0000,0x8000`, this creates two dense cells. Same-cell
peers connect with `center_prob`; opposite-cell peers connect with `far_prob`.

The runner also reports `settled location spread`, counting how many low-bit
locations were touched by block-related messages before the submitting owner
committed.

Settings:

- `600` peers
- two stamped locations: `0x0000,0x8000`
- `300` peers per location
- one entry location: `EC_STEADY_STATE_ENTRY_LOCATIONS=1`
- fresh-token workload
- `1` block/round, block size `1..=2`
- `same_dc`

| Cross-cell probability | Avg connected | Cross links/node approx | Commit avg | Commit p95 | Location spread avg/p95 | Peer spread avg/p95 | Block msgs avg/p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `0.000` | 299.0 | 0.0 | 3.0 | 3 | 1.0 / 1 | 18.1 / 22 | 450.4 / 687 |
| `0.001` | 299.4 | 0.3 | 3.0 | 3 | 1.3 / 2 | 21.3 / 40 | 461.6 / 702 |
| `0.005` | 300.5 | 1.5 | 3.0 | 3 | 1.7 / 2 | 26.6 / 42 | 465.8 / 728 |
| `0.020` | 305.2 | 6.0 | 2.9 | 3 | 2.0 / 2 | 33.1 / 43 | 525.3 / 944 |

Reading:

- no cross links are fast, but only because commits stay inside one isolated
  cell; this is not the target behavior
- around `0.1%` cross links begins to reach the other cell but not reliably
- around `0.5%` cross links reaches both cells for most committed blocks
- around `2%` cross links reaches both cells essentially always in this run
- even at `2%`, p95 latency remains `3`, and message load is well below the
  earlier full-ring linear sweeps

This is the clearest small-world result so far. For a two-cell network of 300
peers per cell, the useful target shape appears to be:

- dense local cell: close to connect-all inside the cell
- weak ties: on the order of a few cross-cell peers per node
- for reliable cross-cell settlement in this workload, `~6` cross-cell links per
  node looked much better than `0-2`

That is a much smaller world than the previous continuous-ring fixed topology.
The next election/pruning target should therefore be cell-aware: first fill the
local cell densely, then keep a small but nonzero quota of peers in each remote
location/cell rather than treating the far region as one undifferentiated tail.

### Carrying this back into `ec_peers.rs`

The current `PeerSmallWorldConfig` is close in spirit but not yet the right
shape. It already does the most important lifecycle thing:

- grow freely while below `peer_budget + hysteresis`
- only prune once the connected set is above that high band

That matches the intended behavior: a node should connect to everything useful
it can find until it hits its configured budget.

The missing part is the retention objective. Today the small-world logic has:

- one local-distance score
- one `far_fraction`
- one `far_distance_fraction`
- one undifferentiated "far" bucket

The two-cell fixed experiment says this is too coarse. A small-world peer set
does not want "some random far peers"; it wants deliberate coverage of remote
cells. Otherwise pruning can accidentally keep six weak ties to the same remote
area and zero ties to another, which is exactly the kind of gap that makes
commit signals fail to spread reliably.

The next `ec_peers.rs` target should therefore be:

1. **Budget-first growth**
   - below the budget band: accept useful peers freely
   - do not prune just because the candidate is remote

2. **Local cell density**
   - define a coarse location cell from the low peer-id bits
   - preserve as many same-cell peers as possible
   - local cell peers should have very low prune weight unless the local cell is
     massively overfilled

3. **Remote-cell quotas**
   - bucket connected peers by coarse remote cell
   - keep at least `N` peers in each discovered remote cell, subject to budget
   - prioritize accepting candidates from remote cells below quota
   - strongly prefer pruning from remote cells above quota before pruning from
     underfilled cells

4. **Distance inside cells**
   - use the existing distance exponent as a secondary score
   - the primary score is cell coverage; distance only decides between peers
     once cell needs are satisfied

5. **Formation pressure**
   - focused discovery should target underfilled cells
   - if the local cell is thin, probe locally
   - if a remote cell quota is empty, probe that cell's coordinate range

The likely config extension is:

```rust
pub struct PeerSmallWorldConfig {
    pub peer_budget: usize,
    pub hysteresis: usize,
    pub location_bits: u8,
    pub cell_bits: u8,
    pub remote_cell_target: usize,
    pub min_local_fraction: f64,
    pub far_fraction: f64,
    pub far_distance_fraction: f64,
    pub distance_exponent: f64,
}
```

The old `far_fraction` / `far_distance_fraction` can be kept as a compatibility
mode, but the experimental path should move toward `cell_bits` and
`remote_cell_target`. For the two-cell fixed run, `cell_bits=1` and
`remote_cell_target≈6` reproduced the behavior we want: fast local commit and
reliable two-cell settlement.

This is now wired as an opt-in path in `ec_peers.rs`. Leaving `cell_bits=0` or
`remote_cell_target=0` keeps the previous aggregate far-tail behavior. Setting
both enables cell-aware invitation acceptance and pruning:

- underfilled remote cells are accepted freely, even when distant
- remote cells at or below their target are protected from pruning
- overfilled remote cells become preferred prune candidates
- local-cell peers are protected until the local minimum is reached

### Latency bias from local cells

There is an additional benefit to the cell-shaped graph: if most decisive commit
participants are same-cell or near-cell peers, their RTT should be lower than the
network average. The simulator currently counts latency in rounds, but in a real
deployment the small-world shape should improve wall-clock commit time in two
ways:

- fewer graph hops inside the dense local cell
- shorter RTT between the peers most likely to form the first commit quorum

This gives `ec_peers.rs` a second useful retention signal once the shape
constraints are satisfied. Peer distance/cell coverage should remain the primary
topology control, but ties inside the same cell can be ranked by observed
responsiveness.

### RTT from opaque tickets

The request/response tickets are a natural place to measure responsiveness
because valid responses return the same opaque ticket that the requester issued.
The requester can therefore associate a valid `Answer` or `Block` with the time
the corresponding `Query` or block request was sent.

Current implementation notes:

- `PeerElection` channels already store `sent_at`, and `ChannelResponse` stores
  `received_at`, so election-answer RTT can be measured without changing the
  wire message.
- `TicketManager` block tickets are currently stateless `u64` values derived
  from the block id and rotating secrets. In the simulator, block RTT can be
  measured by keeping a local `ticket -> sent_at` table until the ticket is
  validated.
- In production, where tickets should be full-width opaque values, part of the
  ticket can carry an encrypted or MAC-protected issue timestamp. That preserves
  the external opacity while letting the issuer recover approximate send time
  when the ticket returns.

The RTT signal should be treated as service quality, not safety. A low-latency
peer should not override storage proof, identity validity, or cell-coverage
requirements. It is most useful for ranking candidates inside an already
acceptable local or remote-cell bucket.
