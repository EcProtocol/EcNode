# EC Protocol — Reference Node (WIP)

**EC Protocol** is an experimental distributed coordination protocol for shared state **without crypto tokens, global blockchains, or centralized operators**.

This repository contains the **reference node implementation** (`EcNode`) and serves as the main entry point for developers exploring the protocol.

> Think: decentralized coordination and shared state — **without crypto**.

---

## What problem is this trying to solve?

Most distributed systems force you to choose between:

- **Centralized databases** (Postgres, etc.)  
  → fast and simple, but controlled by one operator

- **Blockchains**  
  → decentralized, but slow, expensive, and globally bottlenecked

EC Protocol explores a third path:

> How can *many independent operators* maintain and update shared state  
> **without trusting a single party**,  
> **without global consensus**,  
> and **without rewriting history**?

---

## High-level idea

Instead of a single global ledger, EC Protocol works by:

- **Sharding responsibility organically** across an address space  
- Having nodes **continuously prove alignment** with current state  
- Using **local voting and conservative conflict handling**  
- Never rewriting committed history (safety first)
- Allowing failures and partitions to remain **localized**

There is:
- no global blockchain  
- no leader election  
- no token incentives  
- no fork-choice rule  

Decentralization emerges because **no single operator is required or trusted**.

---

## What EC Protocol *is*

- A **coordination layer** for shared state between organizations
- A way to build **neutral infrastructure** without a central owner
- A protocol that prioritizes **safety, fault isolation, and survivability**
- Designed for **issuer-backed and registry-style applications**

Examples of good fits:
- gift cards / vouchers / prepaid instruments
- decentralized registries (naming, service discovery)
- identity & attestation systems
- voting and batch coordination

---

## What EC Protocol is *not*

- ❌ A cryptocurrency
- ❌ A smart contract platform
- ❌ A general-purpose database
- ❌ A trustless DeFi system
- ❌ A replacement for Postgres when one operator is fine

If a single company can safely run the database, **Postgres is the right choice**.

EC is for when *who runs the database* is the problem.

---

## Project status

🚧 **Work in Progress — Research / Reference Implementation**

- The protocol design is specified and under active iteration
- This node implementation is a **reference**, not production-ready
- APIs, wire formats, and parameters **will change**
- The network is not yet stable or permissionless

This repo is for:
- protocol engineers
- distributed systems researchers
- early contributors who enjoy deep technical work

---

## Repository contents (high level)

- `EcNode/` — reference node implementation
- protocol logic, storage, networking (evolving)
- experiments and prototypes

Expect rough edges.

---

## How to approach this repo

If you are new here:

1. Start with the **concept**, not the code
2. Skim the protocol documentation (linked below)
3. Treat the implementation as exploratory

If you are interested in contributing:
- focus on **correctness and clarity**, not performance
- expect breaking changes
- open discussions early

---

## Documentation

- 📄 **Protocol specification** (design, assumptions, threat model):  
  → see the `docs/` directory or linked design documents

- 📘 **Investor / high-level overview**:  
  → available separately (ask if interested)

---

## Why no token?

This is a deliberate design choice.

EC Protocol assumes that:
- nodes are run because applications *need* write access
- participation is driven by utility, not speculation
- alignment is enforced structurally, not via rewards

This keeps the system boring — and sustainable.

---

## Getting involved

This project benefits most from:
- careful reviewers
- systems thinkers
- people who enjoy edge cases and failure modes

Ways to engage:
- read and critique the protocol
- experiment with the node
- discuss use cases where shared neutral state is hard today

Issues and discussions are welcome.

---

## License

- Source code and tooling in this repository are licensed under the Apache License 2.0.
- Academic papers and manuscripts (e.g. in `/docs/` and `/Design/`) are licensed under Creative Commons licenses unless explicitly stated otherwise. These are generally also published with license attribution on Zenodo and arXiv.
---

## Final note

This project is intentionally ambitious **and** conservative.

It does not promise:
- global finality
- unstoppable execution
- universal trustlessness

It promises something narrower and more realistic:

> A way for many parties to coordinate shared state  
> without needing to agree on who is in charge.

If that problem resonates with you — welcome.
