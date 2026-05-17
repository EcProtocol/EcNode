# EC Protocol — Reference Node

**EC Protocol** is an experimental distributed coordination protocol for shared state — without crypto tokens, without global consensus, with a bounded retention horizon, and with locality-driven scaling.

Think of a limited Distributed Hash Table you can trust. Providing basically 3 globally available operations:
- Bind a public key to a 256 bit opaque `token`. And if the `token` already exist - provide a signature by the current public key to change it.
- Lookup the last `transaction` to change a `token`.
- Lookup a `transaction`.

This repository contains the **reference node implementation** (`EcNode`), written in Rust, together with the protocol design documents and a simulator used to validate behaviour under realistic network conditions.

> *Many independent operators coordinate shared records. Each token lives in a local neighbourhood, not on a global ledger. Safety comes from user signatures. Conflicts stay publicly visible. Rewriting history is cryptographically self-incriminating. All state has a known expiry.*

---

## What this is for

EC Protocol is built for applications where:

- **multiple independent parties** need to share state, and no single one of them should run the database
- **counterparties verify before granting value** — voucher redeemers, vote counters, payment recipients
- **records need to be durable for a known period**, then forgotten
- **transaction cost should scale with neighbourhood size**, not network size

Concrete fits:

- **Vouchers, gift cards, prepaid instruments** — issuer-backed, time-bounded, redemption-driven
- **Decentralized registries** — naming, service discovery, attestation
- **Identity and verifiable credentials** — self-sovereign, expiring, accountable
- **Anonymous voting** — bounded ballots, detectable double-votes
- **Issuer-backed payments** — where a redemption step is part of the flow

Concrete non-fits:

- ❌ A cryptocurrency or smart-contract platform
- ❌ A trustless instant-finality payment system
- ❌ A general-purpose database
- ❌ A replacement for Postgres when one operator is fine

If a single company can run the database, **Postgres is the right choice**. EC is for when *who runs the database* is the problem, and *for how long* is an acceptable constraint.

---

## The third path

Distributed systems usually force a choice between two extremes:

- **Centralized databases** — fast and simple, controlled by one operator
- **Blockchains** — decentralized, but slow, expensive, and globally coupled through infinite history

EC Protocol explores a third path: many independent operators maintain shared state, but without a global ledger, without mining, without tokens, and without unbounded history.

The core design question is not "how do we make consensus cheaper?" It is:

> *How can multiple parties coordinate on shared records — without trusting any single party, without global consensus, with enough durability for redemption and counterparty workflows — but not forever?*

---

## The pillars

The protocol stands on a small number of distinct technical foundations that compose into a coherent whole. Each is documented separately; the README only sketches them.

---

### 1. Proof-of-Aligned-Storage

Nodes earn their place in the network by *continuously demonstrating that they hold the state they claim to host*, aligned with the rest of the local neighbourhood. This is the primary alignment mechanism: storage is not a side-effect of running a node — it is the credential.

Node identities are the product of Proof-of-Work (Argon2, ~1 day target). This makes identity generation expensive, Sybil attacks costly, and node IDs effectively random by hash output properties — with the added structural property that certain bit slices can serve as stable, unforgeable location proxies (see topology below).

This replaces proof-of-work-as-waste and proof-of-stake with a property that is directly useful: verified storage that other peers and clients can query.

See `docs/` for the full PoAS specification.

---

### 2. Small-world peer topology with latency weighting

The network's peer graph is deliberately shaped as a **small-world network**: strongly connected locally, but with a sparse tail of longer-range links that keeps global path length short.

This structure was formally characterised by Watts & Strogatz (1998), who showed that replacing even 1% of local edges with random long-range connections collapses average path length from O(N) to O(log N) while barely reducing local clustering. Kleinberg (2000) sharpened this: for greedy local routing to be *efficient* (O(log² N) hops without global knowledge), long-range connection probability must follow a power-law decay with distance — specifically 1/d² in a 2D grid. Too many or too few long-range links both break efficient navigation.

EC's peer graph is a 1D ring variant of this structure:

- **Ring-address gradient** (non-negotiable): each node maintains strong connections to ring-nearby peers — determined by node address — which is what makes coverer assignment, token locality, and routing correctness work. This is the high-clustering component.
- **Sparse far-ring connections** (required): a small number of far-ring connections are preserved regardless of other thinning. These are the Watts-Strogatz long-range ties — the handful of links that keep global path length logarithmic.
- **Latency weighting** (optimisation layer): within the set of ring-appropriate peers, prefer those with lower observed round-trip time. This is an *additional dimension* on top of the ring gradient, not a replacement. Connection probability becomes a product of ring-distance weight and latency weight, so both ring correctness and physical proximity are satisfied simultaneously.
- **Address-slice fallback**: node IDs produced by PoW have specific bit-slice regions that are random but unforgeable (re-mining to get a specific slice value costs proportional PoW). A designated slice can serve as a latency proxy when RTT has not yet been measured — e.g. for newly discovered peers.

The combined effect: peers that are close on the ring *and* close in network latency are strongly preferred. Peers that are close on the ring but physically distant are still connected, but less so. The ring structure ensures routing correctness; the latency weighting ensures round speed.

**Adaptive peer budget.** Each node maintains a configurable maximum connection count. On discovery:

- If discovered peers fit within budget: keep all (small/early network — full density, matching current simulator results)
- If budget is exceeded: thin by the combined score — ring-bucket coverage first (never sacrifice routing correctness), then latency within each bucket
- Further thinning uses the address-slice proxy for peers without measured RTT

This makes the network **self-scaling without global coordination**. In a small network, every node is fully connected and the topology matches the dense-linear regime that the simulator shows is fast. As the network grows, each node independently transitions to a locally-dense, globally-sparse small-world graph. No operator configuration, no topology management protocol, no global knowledge required.

> Watts & Strogatz (1998). *Collective dynamics of small-world networks.* Nature 393.  
> Kleinberg (2000). *The small-world phenomenon: An algorithmic perspective.* STOC.  
> Newman (2003). *The structure and function of complex networks.* SIAM Review 45(2). (Survey)

---

### 3. User-signed conflicts only

Every transaction is signed by its owner. The network cannot manufacture a conflict; only a key-holder can, by signing two contending transactions on the same token. This sharply narrows the threat model: conflicts are always *provable misbehaviour by a specific key-holder*, never Byzantine injection by third parties.

The protocol either commits the highest contender or leaves the conflict openly visible. It never commits two. Simulator results on a 2000-peer fixed network show **0 lower-owner commits and 0 multi-owner commits** under tested conditions.

Double-signing is therefore self-punishing across all application classes:
- **Payments**: payer burns their own token or stalls both sides — no free money, and redemption surfaces the conflict
- **Vouchers**: holder burns their own entitlement; issuer arbitrates the visible conflict
- **Voting**: double ballot counts zero — neither vote lands

---

### 4. Conflict visibility over forced resolution

Counterparties — redeemers, issuers, vote counters — verify state before granting value. They see either a clean committed winner or an openly contested token. The protocol does not need to *force* resolution of a double-sign; it needs to make the conflict *legible* to the parties who need to act on it.

This is the correct behaviour for issuer-backed applications. A protocol that forcibly picked one contender would hide evidence of holder misbehaviour. Leaving the conflict visible — with both contending transactions known to covering peers — gives counterparties the information they need.

**Multi-point submission** makes this more robust at scale. Counterparties in different network cells can each submit the same signed transaction from their local connection point simultaneously. Multiple simultaneous wavefronts converge rather than a single wavefront expanding — compressing the time to global visibility. For economically significant transactions, both parties are active participants in propagation, not passive observers.

---

### 5. Minefield accountability — durability against rewrite

Once committed, the threat shifts from "decide the wrong thing" to "rewrite history after the fact." EC addresses this through the *minefield* mechanism: every `get-mapping` response includes signed attestations to recent committed history. Any later contradiction between two responses is cryptographic evidence of fraud, triggering automatic slashing on network consensus.

Combined with the PoW identity cost (re-mining takes ~1 day), rewriting is not just hard — it is *self-incriminating and identity-destroying*.

See [`Design/minefield_accountability_design.md`](Design/minefield_accountability_design.md).

---

### 6. Bounded retention — finite history

All state expires after a public, network-wide retention window (targeting ~2 years). Applications must redeem or extend before expiry. This:

- bounds storage per node — no infinite history cost
- makes rewrite resistance tractable — durability is a finite obligation, not eternal
- provides structural support for *right to be forgotten*
- gives application builders a clear semantic contract: redeem within the window or extend

The retention bound is not just an operational detail. It is the structural fact that makes rewrite resistance tractable — the protocol only needs to defend commitments until redemption or extension, not forever. It is also why EC can achieve GDPR-compatible data lifecycle in a distributed setting where blockchains structurally cannot.

---

## The composition

Each pillar is useful on its own. Together they describe a protocol whose safety, durability, and operational cost stay bounded by construction:

| Property | Mechanism |
|---|---|
| Sybil resistance | PoW identity (Argon2, 1-day target) |
| Routing correctness at scale | Ring-address gradient |
| Fast commit in dense networks | Small-world local clustering |
| Fast commit in large networks | Latency-weighted thinning + sparse far links |
| Self-scaling topology | Adaptive peer budget |
| Commit-time safety | User-signed conflicts only |
| Conflict legibility | Visibility-preserving resolution |
| Post-commit durability | Minefield signed attestations |
| Storage boundedness | ~2 year retention window |
| Right to be forgotten | Finite retention by protocol |

---

## Project status

🚧 **Work in progress — research / reference implementation**

The protocol design is actively iterating. Empirical validation via the simulator is the primary evidence base; the node implementation is a reference, not production-ready. APIs, wire formats, and parameters will change.

Recent milestones:

- **Commit-time safety demonstrated**: 0 wrong commits, 0 multi-owner commits in 2000-peer fixed-network conflict runs
- **Churn-graph formation**: recovery and core coverage reach operational targets under realistic churn
- **48% traffic reduction**: parent fetch cooldown, smart voter selection, and reduced non-conflict follow-up cut total messages from 4.31M to 1.99M with +0.2% commits and no latency regression
- **Efficiency hypothesis validated**: repair traffic was policy-driven, not fundamental — targeted policy changes achieved the reduction
- **Four-layer threat model articulated**: commit-time, decision-time, post-commit, and retention-bound properties now formally separated
- **Small-world topology design**: latency-weighted thinning and adaptive peer budget designed; simulator experiments pending

For a current honest read of viability, gaps, and open questions see [`Design/viability_assessment.md`](Design/viability_assessment.md).

This repo is for protocol engineers, distributed systems practitioners, and early contributors who enjoy deep technical work and unfinished edges.

---

## Repository guide

```
EcNode/
├── src/               Reference node implementation (Rust)
├── simulator/         Network simulator + empirical reports
├── Design/            Protocol design, threat model, viability
├── docs/              Protocol specification and pillar references
├── examples/          Runnable examples
├── benches/           Performance benchmarks
├── scenarios/         Scripted network scenarios
└── peer_lifecycle/    Peer lifecycle tooling
```

### Source — `src/`

Rust implementation of the reference node. Key modules:

- `ec_node.rs` — node lifecycle, startup, top-level message dispatch
- `ec_peers.rs` — peer discovery, connection management, locality and thinning
- `ec_mempool.rs` — pending transactions, conflict handling, voting, repair policy
- `ec_commit_chain.rs` — append-only local commit history

### Documentation — `docs/`

Protocol specification and pillar references. Start here for the technical foundations:

- Proof-of-Aligned-Storage specification
- Ring-address topology and routing
- Conflict signalling and visibility model
- Wire formats and message types

### Design — `Design/`

Threat models, viability analysis, and design rationale.

- [`viability_assessment.md`](Design/viability_assessment.md) — current state, evidence, and honest gaps
- [`minefield_accountability_design.md`](Design/minefield_accountability_design.md) — post-commit durability and slashing
- Additional notes on vote flow, batching, routing depth, and peer lifecycle

### Simulator — `simulator/`

Rust simulator validating protocol behaviour under realistic conditions. Reports here are the primary empirical evidence base.

Key reports:

- `FIXED_NETWORK_CONFLICT_LINEAGE_REPORT.md` — commit-time safety, lineage-correct instrumentation, repair traffic optimisation
- `FIXED_NETWORK_EXTENSION_STEADY_REPORT.md` — chain extension and steady-state behaviour
- `CHURN_GRAPH_CONTROL_REPORT.md` — graph formation and recovery under churn
- `DENSE_LINEAR_TOPOLOGY_REPORT.md` — locality-driven latency and spread
- `INTEGRATED_SIMULATION.md` — combined lifecycle behaviour

Each report documents scenario parameters, exact invocation commands, measurements, and interpretation.

---

## Getting started

### Requirements

- Rust (stable, recent)
- Cargo

### Build

```bash
cargo build --release
```

### Run the simulator

The simulator is the best entry point. Reports in `simulator/` include exact invocation commands. A typical starting point:

```bash
cargo run --release --example integrated_steady_state
```

Simulator behaviour is controlled via environment variables documented in the individual reports.

### Devcontainer

A `.devcontainer.json` is included for contributors who prefer a VS Code devcontainer setup.

---

## How to approach this repo

1. Read the **pillars** above — they define the vocabulary used throughout
2. Read [`Design/viability_assessment.md`](Design/viability_assessment.md) for an honest current self-assessment
3. Skim the simulator reports — they show empirically what the protocol does, not just what it is designed to do
4. Read the source with the design documents in hand — the code is legible, but the *why* lives in the design docs

Contributing:

- Focus on correctness and clarity first, performance second
- Expect breaking changes
- Open discussions early

---

## Why no crypto-token?

Deliberate. EC Protocol assumes:

- Nodes are run because applications *need* write access, not for speculation
- Participation is driven by utility, not rewards
- Structural alignment beats incentive alignment

Proof-of-Aligned-Storage gives nodes a useful, verifiable role without attaching economic value to a token. The PoW identity cost provides Sybil resistance without staking. The absence of a token removes an entire category of governance complexity. The system stays boring — and sustainable.

---

## Getting involved

This project benefits most from careful reviewers, systems thinkers, and people who enjoy edge cases and failure modes.

Ways to engage:

- Read and critique the protocol design documents
- Experiment with the node and simulator
- Discuss use cases where shared neutral state is hard today
- Open issues for concrete observations — simulator anomalies, design questions, threat-model cases

---

## License

- **Source code and tooling** in this repository are licensed under the **Apache License 2.0**. See [`LICENSE-2.0.txt`](LICENSE-2.0.txt).
- **Academic papers and manuscripts** (material in `docs/` and `Design/`) are licensed under **Creative Commons** licenses unless explicitly stated otherwise. These are generally also published with license attribution on Zenodo and arXiv.

---

## Final note

This project is intentionally ambitious **and** conservative.

It does not promise global finality, unstoppable execution, or universal trustlessness.

It promises something narrower and more realistic:

> A way for many parties to coordinate shared state — safely, visibly, durably, and with a known expiry — without needing to agree on who is in charge.

If that problem resonates with you, welcome.