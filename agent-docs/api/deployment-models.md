# Deployment Models

## Protocol Goal

EC should provide an open commitment service: public commitments, ordering/timeline, and key-distribution surfaces that independent applications can share without a single application backend owning the shared state.

## Current Status

This is requester-confirmed deployment intent, not implemented product surface. The current implementation is still a simulator/reference node with no UDP transport or stable client library.

### Backend Writer Model

An app developer runs a normal mobile/web backend and also operates or rents EC node write access. The backend submits EC update transactions as a side effect of ordinary application operations.

- apps do not write directly in this model
- backend-to-node writes use tickets/API tokens
- developers can issue tickets themselves when they operate nodes
- SaaS node operators can sell write access by issuing tickets/API tokens
- apps and counterparties can read from any node
- apps from different vendors can share state or flows through the common EC commitment layer

### Wallet Or Direct Client Writer Model

A wallet-style app obtains ticketed write access to selected entry nodes and can write directly. This may support encrypted document storage, credentials, identity flows, voting, or workflow documents such as schema/process/BPMN-style state.

- client keys have no identity semantics and can be ephemeral
- tickets can be bound to the client public key
- tickets are expected to be mostly time-windowed opaque capabilities, though many 256-bit opaque schemes are possible
- a ticket issuer can provide entry-point node public keys, IP/ports, and acceptance metadata
- accepting nodes should verify tickets locally without issuer callouts, preserving the fast commit target
- reads remain open and independent from any node
- clients can verify visibility with a confidence ladder: one `Answer`, block fetches from independent routes, multiple entry points, elections over Answers, or repeated/commit-chain-backed checks

Node-enforceable ticket scope is mechanical, not semantic. Tokens are opaque by design, so nodes cannot know whether a token represents a name, credential, vote, document, or workflow step. Semantic authorization belongs to services that issue tickets or perform backend-side writes.

### Gateway, Watchtower, And Evidence Adjacent Services

Reading is open and free, so read gateways, caches, HTTPS bridges, resolvers, and evidence services do not need to run nodes. Running nodes can still improve their service and help the network.

This model is similar in institutional shape to Let's Encrypt or Certificate Transparency operators: useful public trust infrastructure may be free at the point of use and funded by adjacent business, sponsors, public institutions, enterprise support, compliance needs, or ecosystem value.

Possible forms:

- public read gateway with cached fast responses
- HTTPS/REST bridge for clients that do not speak EC UDP
- enterprise gateway for internal systems
- DNS-like resolver or naming gateway
- watchtower/evidence service retaining Answers, Blocks, CommitBlocks, and peer evidence
- monitoring service for important tokens or identity records

Gateway answers are convenience, not authority. Clients can still read from EC directly or compare against other gateways. Good gateways should expose evidence or proof paths when possible.

## Known Gaps

- Client-ticket issuance, ticket economics, and key-binding rules need design.
- Load balancing and rate limiting across tickets, peers, clients, and operators need investigation.
- Discovery of node IP/port/public keys is a separate topic.
- The wire transport and compact serialization are not implemented.
- Gateway proof modes, cache freshness labels, and stale-response policy are not designed.
- Fraud checks that ordinary nodes can perform during commit-chain sync need analysis.

## Primary Files

- [src/ec_interface.rs](../../src/ec_interface.rs)
- [src/ec_node.rs](../../src/ec_node.rs)
- [src/ec_mempool.rs](../../src/ec_mempool.rs)
- [src/ec_ticket_manager.rs](../../src/ec_ticket_manager.rs)
- [src/ec_peers.rs](../../src/ec_peers.rs)

## Source Material

- [README.md](../../README.md)
- [api/protocol-surface.md](protocol-surface.md)
- [api/encrypted-udp-transport.md](encrypted-udp-transport.md)
- [api/client-integration.md](client-integration.md)
- [protocol/commit-chain-minefield.md](../protocol/commit-chain-minefield.md)
- Requester-confirmed deployment discussion in the current doc-maintenance session.

## Agent Notes

Do not turn deployment models into protocol guarantees. They are expected ways to use the protocol surface.

Do not invent semantic ticket scopes inside core EC. Opaque tokens mean nodes cannot safely enforce application meaning. Nodes can enforce mechanical constraints such as ticket validity, message type, packet size, time window, and rate policy.

Keep the fast-path goal visible: commits should aim for human-timescale latency around 500 ms, and client read/election flows should remain only a few round trips where practical.

Treat misuse as layered:

- invalid packets are dropped by AEAD, parsing, and rate limiting
- valid-ticket overuse is handled by ticket policy, load balancing, and rate limiting
- protocol-invalid writes are handled by tickets, mempool, signatures, and block rules
- semantically unwanted writes are handled by the application or credential service before ticket issuance
