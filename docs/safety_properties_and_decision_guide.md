# EC Safety Properties and Decision Guide

## Purpose

This document gives an honest security and safety reading of EC Protocol for
application developers, token holders, clients, node operators, insurers, and
monitoring services.

It is not a claim that EC prevents every well-funded attack. It is a decision
document: what the system protects well, where its limits are, how it compares
to other distributed systems, and what users can do when the value at risk is
high.

The central conclusion is:

> EC does not make targeted capture impossible. It makes capture operational,
> local, evidence-risky, and often economically single-use. Applications that
> carry more value can buy or perform more observation.

## Core Safety Claim

EC's safety does not rest on identity mining alone.

A valid mined identity is only a costly, randomly placed ticket into the
candidate pool. It does not by itself confer control over token history, peer
selection, votes, or routing.

Influence is earned through continued participation:

- maintaining a valid mined identity
- remaining online and responsive
- storing aligned neighborhood state
- answering queries usefully
- providing commit-chain data for synchronization
- winning peer elections
- avoiding signed contradictions
- surviving peer churn and local operator policy

This matters because a wealthy adversary can mine many identities in parallel.
The defense is not that they cannot enter. The defense is that entry is not
control, and control requires ongoing useful behavior under observation.

## Threat Model: What a Rich Adversary Can Do

A powerful adversary may:

- rent or buy large compute capacity
- mine many valid peer identities
- filter mined identities for selected address ranges
- operate nodes honestly for some time
- build reputation before attacking
- selectively answer queries
- attempt to identify monitoring clients
- target valuable neighborhoods
- coordinate behavior across many nodes

EC cannot categorically prevent this. This is similar to proof-of-work and
proof-of-stake systems: sufficient external resources can buy influence.

The uncomfortable point is that EC identity mining may be cheaper than global
PoW chain control. A determined adversary may be able to run 100,000 machines
for weeks or months, mine a large identity pool, and operate the resulting nodes
long enough to target high-value state.

That is a real threat for high-value, long-lived targets. The system should be
explained accordingly.

## Why Identity Mining Is Still Useful

Identity mining remains valuable even though it is not sufficient alone.

It provides:

- a cost for each candidate identity
- uniform random address placement
- resistance to free address selection
- a spam barrier for peer discovery and invitations
- a cost to replacing burned identities
- limited protection against long-term hoarding when identities expire

For ordinary users, a one-day mining target makes joining feasible. For
attackers, many identities require many machine-days. The cost is not infinite,
but it is real, recurring, and lost when identities become unusable due to
evidence-backed exclusion.

## The Main Distinction: Entry vs. Authority

Permissionless entry means anyone can create a key pair, mine an identity, send
UDP messages, query the network, and attempt to participate.

Permissionless does not mean every node must accept every other node.

Node operators are free to:

- choose their peers
- exclude nodes based on local policy
- down-rank slow or unhelpful peers
- reject peers with public fraud evidence
- prefer peers with endorsements
- prefer peers with long useful history
- use out-of-band intelligence
- follow application-specific risk rules

The base protocol can remain open while influence remains local, scarce,
earned, and continuously evaluated.

## Silent Honest-Until-Attack Adversaries

The hardest adversary behaves honestly until the attack matters.

While honest, these nodes may actually improve service: they store state, answer
queries, route messages, and help elections. This is an important part of EC's
risk picture. A malicious operator that behaves honestly contributes useful
capacity until it defects.

The danger is the defecting moment:

- presenting inconsistent histories
- hiding later mappings
- trying to rewrite or suppress high-value token state
- misleading a victim or isolated client
- voting in a coordinated way for an attack outcome

Minefield-style evidence does not stop all such behavior before it occurs. It
creates risk after the fact: if the attacker signs contradictory statements, any
client or monitor may have stored enough evidence to identify and exclude them.

This makes many attacks economically single-use. The adversary may spend weeks
or months building position, but the act of exploiting that position can burn
the identities and reputation used to perform the attack.

## Fraud vs. Poor Service

EC should sharply distinguish fraud from ordinary network failure.

Nodes must not be punished for:

- being offline
- being slow
- being behind
- not knowing a recent commit
- refusing to answer a query
- failing to provide a useful route
- voting according to their local view

Those behaviors may make a node less useful, but they are not fraud.

Fraud requires signed contradiction.

### Fraud Predicate 1: Forked Commit-Chain Answers

A node signs two Answers that point to commit-chain elements which cannot be
part of the same chain.

Commit-chain elements have ordering information such as sequence and timestamp.
Given two signed Answers from the same peer:

- identify the newer claimed chain element
- walk backward from the newer element toward older elements
- the older claimed element must be found before traversal passes below its
  sequence or timestamp, within the relevant retention/proof window

If it is not found, the peer has signed incompatible chain histories.

Evidence bundle:

- signed Answer A from peer P pointing to chain element X
- signed Answer B from peer P pointing to chain element Y
- chain traversal proof from the newer element back past the older position
- proof that X and Y cannot share one ancestry

This is not stale data. It is equivocation.

### Fraud Predicate 2: Mapping Contradicts Claimed Chain Head

A node signs an Answer that claims chain head H while also returning token
mapping M_old, but the ancestry of H already includes a later transaction for
the same token producing M_new.

The peer has simultaneously claimed:

- my commit chain includes this later token update
- the current mapping is still the older value

Those cannot both be true.

Evidence bundle:

- signed Answer from peer P containing chain head H and mapping M_old
- commit-chain proof from H back to transaction Tx
- Tx updates the same token to M_new after M_old

Again, this is not ignorance. The node itself signed the chain head that proves
its mapping answer was outdated.

## Non-Fraud Negative Signals

Many bad behaviors should affect peer selection without becoming fraud proofs.

Examples:

- refusing to provide Answers
- timing out frequently
- refusing commit-chain iteration
- failing to serve blocks needed for sync
- providing low-quality or incomplete sync data
- high latency
- low availability
- poor storage alignment
- inconsistent usefulness across time

These are service-quality signals.

They matter because Answers and commit-chain iteration are also the basis of
peer elections and node synchronization. A node that refuses to answer cannot
prove usefulness and should naturally lose elections. A node that blocks chain
iteration makes itself a poor sync peer and should be down-ranked or excluded by
local policy.

The adversary's choices are therefore constrained:

- answer falsely, and risk fraud evidence
- refuse to answer, and lose election usefulness
- answer honestly, and support the network

## Selective Answering and Monitor Evasion

A malicious node may try to detect who is asking.

It may provide honest-looking answers to ordinary peers, refuse suspicious
monitors, and give different responses to isolated victims.

This is risky because anyone can query. A client identity can be a freshly
generated key pair sending UDP messages. The node cannot reliably know whether a
query came from:

- a token owner
- a merchant
- an insurer
- a watchtower
- another node
- a public gateway
- a rival operator
- a concerned third party

Monitoring strategies should use that uncertainty:

- rotate query identities
- query through different entry points
- vary timing and network location
- compare responses across monitors
- preserve signed Answers even when they look normal
- request commit-chain iteration after selected Answers
- query both popular and high-value rare tokens

Selective answering is not itself fraud. But if selective answering produces
signed incompatible chain claims, it becomes independently verifiable evidence.

## Elections as a General Trust Mechanism

Peer elections are the common mechanism for turning multiple untrusted Answers
into a higher-confidence view.

The election mechanism is specified in
[peer_election_design.md](../docs/peer_election_design.md). Although it is used
by nodes to select peers, it is not conceptually limited to peer management.
Clients, gateways, monitors, insurers, issuers, and other entities may run the
same pattern when they want stronger confidence in a token mapping.

The basic pattern is:

- query multiple nodes
- use diverse first hops and routes
- require valid proof-of-storage signatures
- compare the returned signature token sets
- prefer clusters of responders that agree on the same mapping
- treat split-brain or weak agreement as a risk signal

The important property is that an Answer is not only a bare claim about one
token. The proof-of-storage signature is derived from the requester, the token,
and the returned mapping, and it selects additional stored mappings. When
multiple responders produce matching proof sets, they are demonstrating that
they have the same mapping and are aligned with the same surrounding storage
view.

This gives a reusable confidence ladder:

- a single Answer is a fast low-cost read
- several matching Answers are a stronger read
- an election winner is a peer or answer source with demonstrated aligned
  storage
- a split-brain result is useful evidence that the reader should slow down,
  spawn more channels, or avoid acting

Elections also help with monitor evasion. Since any entity can run an election,
a node cannot reliably know why it is being queried. A query might be part of
peer selection, a client read, a gateway cache refresh, an insurance check, a
watchtower probe, or a fraud investigation. This makes selective answering more
dangerous for malicious nodes.

Because election channels run in parallel through the network, an election does
not need to be much slower than a single-shot query. The extra cost is mostly
additional messages and response aggregation, not sequential waiting through one
route at a time. Applications can therefore choose value-aware verification:
single-shot reads for low-value cases, elections for important reads, and
larger or repeated elections for high-value or suspicious cases.

## Referral-Based Discovery and Route-Around Pressure

Many service-quality risks are softened by the referral mechanism.

A client or node does not need a complete trusted peer list before it can start
learning the network. Starting from a small peer set, even a single honest
bootstrap route can answer or refer queries toward other peers. Repeated random
or targeted token queries reveal peers near many ring positions. With enough
time, and assuming the reachable peer graph is connected, this can teach the
client a broad view of the network rather than trapping it behind the first few
peers it saw.

This matters for poor-service attacks. A few nodes that refuse Answers, serve
stale data, block chain iteration, or provide weak referrals can slow a client
down, but they do not automatically control the client's view. Their refusal
also makes them bad election candidates and bad sync peers.

The useful distinction is:

- if all first-contact paths are adversarial, bootstrap can be misled or stalled
- if at least one path reaches honest peers, referrals and elections give the
  client a way to expand, compare, and route around poor service
- if the client has time to keep querying, poor-service nodes become visible as
  low-quality rather than authoritative

This does not remove bootstrap risk. It changes the shape of the risk from
"trust the first node" to "find at least one honest path, then expand and
compare." High-value clients should still use multiple bootstrap sources, but
the protocol should not be evaluated as if each client is forever limited to its
first few peers.

## Application-Level Countermeasures

Applications are not passive. They can choose data layouts and verification
patterns that make capture harder.

### Use Multiple Tokens

A task can depend on several tokens rather than one.

This spreads risk across multiple neighborhoods. To manipulate the task, an
adversary must influence enough of those neighborhoods at the relevant time.

This is especially useful for high-value operations where the extra query and
storage cost is acceptable.

### Destroy-Create on Update

Many EC applications can update by destroying an old token and creating one or
more new tokens.

This randomly moves value around the ring. A captured neighborhood becomes a
decaying asset: after an update, the valuable state is likely elsewhere.

For payments, vouchers, and other moving-value flows, this is a major defense.
The adversary cannot simply capture one address range and wait forever. Value
moves.

### Query and Store Evidence

High-value token users should store signed Answers over time.

Useful evidence collection includes:

- signed mapping Answers
- chain heads named in Answers
- commit-chain elements needed to verify ancestry
- timestamps and query context
- peer identities and public keys
- contradictory responses from the same peer

The user does not need to be the token owner. Any party can monitor a token.

### Use Query Proxies and Public Gateways

Applications can provide query gateways that:

- answer quickly from cache
- link to stored signed evidence
- refresh popular tokens in the background
- monitor high-value tokens more aggressively
- forget rare low-value tokens unless explicitly configured

Popular or valuable tokens naturally accumulate more observations. Rare tokens
can be monitored on demand.

## Watchtowers and Evidence Services

EC naturally supports independent monitoring businesses.

A watchtower or evidence service can:

- monitor specified tokens for a fee
- query diverse peers at regular intervals
- store signed Answers and chain proofs
- detect fraud predicates
- alert subscribers
- publish evidence bundles
- provide public lookup by node address

No special authority is required. The service does not need custody of token
keys and does not need permission from token owners.

Public lookup could be simple:

    /node/{peer_id}

returning:

- no evidence found
- fraud evidence found
- under review
- evidence bundles
- first observed time
- last observed time
- violated fraud predicate

The important output is not the monitor's opinion. It is the cryptographic proof
bundle. Other parties can independently verify it.

This creates an ecosystem-level accountability layer:

- token owners can buy monitoring
- insurers can require monitoring
- node operators can consult fraud lists
- applications can prefer clean peers
- public-interest monitors can watch important tokens
- competing services can check each other

## Endorsements and Optional Authority

Identity blocks can support additional tokens or metadata. This makes it
possible to attach endorsement tokens later.

Endorsements may come from:

- application operators
- institutions
- insurers
- infrastructure providers
- community groups
- known node operators

Endorsements should not be mandatory for base-network participation. If they
become a hard validity requirement, EC shifts from permissionless entry toward
incumbent-mediated admission.

A better use is as a peer-selection signal:

- endorsed peers may receive priority
- endorsement may accelerate consideration
- endorsement may reduce newcomer penalty
- high-value applications may require specific endorsements
- lack of endorsement should not make a base identity invalid

This preserves permissionless entry while allowing deployments with higher
trust requirements to opt into stronger policy.

## Why Not Require Rare Network Artifacts?

One proposed entry limiter is to require a valid peer address to be backed by a
rare artifact from network activity, such as a pair of accepted transactions
whose hashes combine to meet a difficulty predicate.

This is not recommended as a base requirement.

Reasons:

- it hurts newcomers most
- it creates a secondary market for entry artifacts
- existing peers gain gatekeeping power
- wealthy adversaries can grind candidate transactions offline
- adversaries can submit only transactions that produce useful artifacts
- the mechanism adds friction without cleanly eliminating large-scale attack

Rare artifacts may be useful for optional reputation or application-specific
policy. They should not be the base permissionless identity gate.

## Why Not Require Existing Identities to Mint New Identities?

Another option is to let existing identity blocks form or authorize new
identity blocks.

This creates an invitation tree or web-of-trust. It can rate-limit growth, but
it has serious edge cases:

- not all peers know all identity ancestors
- partitions produce inconsistent admission views
- incumbents can cartelize entry
- newcomers may need to buy admission
- operator policy becomes confused with protocol validity

As with endorsements, this is better as a soft signal than a hard validity
rule. Existing identities can vouch for new identities, but base validity should
remain tied to proof-of-work, timestamp freshness, and protocol validation.

## Identified Risks and Problems to Track

This section lists risks that are not necessarily solved by the current design.
They should remain visible as the protocol matures, because an open and cheap
service only works if the obvious failure modes are continuously tested.

### Bootstrap and First Contact

New clients and new nodes are most vulnerable before they know diverse peers.

An attacker does not need to control the whole network if they can control the
first peers a newcomer discovers. A client that receives a narrow first view may
be shown stale state, low-quality peers, or a locally captured neighborhood.

The counterpoint is that the newcomer is not permanently bound to that first
view. If any first-contact path reaches honest peers, referrals from random and
targeted queries can widen the client's peer sample over time. Bootstrap risk is
therefore most severe when all initial paths are adversarial, or when the client
must make a high-value decision before it has expanded its view.

Mitigations to develop:

- multiple independent bootstrap sources
- public evidence gateways
- diversity requirements for first contact
- enough discovery time before high-value operations
- conservative high-value reads during bootstrap
- warnings when all first-contact peers are correlated

For high-value operations, a first answer should never be treated as enough.

### Stale but Internally Consistent Answers

A node may provide an old but internally consistent view.

That is not fraud. A node can be behind for ordinary reasons. But stale answers
can still harm users who need freshness before granting value.

Mitigations to develop:

- freshness metadata in Answers
- minimum acceptable chain-head recency for high-value reads
- querying multiple peers before acting
- requiring recent monitor or gateway evidence
- application-level delay before irreversible off-protocol settlement

The safety rule should remain: stale is not fraud unless the node also signs a
claim that contradicts its own answer.

### Data Withholding Instead of Lying

An adversary may avoid signed contradiction by refusing to answer, withholding
blocks, blocking commit-chain iteration, or timing out.

This is not fraud, but it can cause local liveness failure and can be used to
pressure isolated users.

This is mainly dangerous when the victim has too few routes. Once the victim can
query through other peers, withholding becomes a service-quality signal: the
node is not useful for elections, sync, or high-confidence reads.

Mitigations to develop:

- strong peer-selection weight for serving sync data
- down-ranking for repeated timeouts or incomplete chain service
- diverse fallback paths for chain iteration
- referral-based route expansion before declaring data unavailable
- public service-quality statistics
- operator policy for excluding unhelpful peers

This risk is especially important because commit-chain iteration is the basis of
node synchronization.

### Gateway Capture and Soft Centralization

Fast public gateways are useful, but they can become soft centralization
points.

A gateway cannot forge signed network evidence, but it can omit evidence,
delay updates, filter which peers it queries, or present stale cached results.

Mitigations to develop:

- proof bundles attached to gateway responses
- visible freshness and peer-diversity metadata
- easy comparison across gateways
- independent gateway monitoring
- client fallback to direct network queries for high-value operations

Gateways should make verification easier, not replace verification.

### Monitoring Privacy

Querying a token reveals interest in that token.

Watchtowers, gateways, and peers may learn which tokens are valuable, which
users are worried, and which state is likely to be acted on soon.

Mitigations to develop:

- rotating client identities
- decoy queries
- batching
- query proxies
- third-party monitoring that hides the ultimate interested party
- application-specific privacy guidance

Monitoring improves safety, but it can also reveal where value is concentrated.

### Evidence Retention Failure

Fraud proofs only matter if someone retains the signed Answers and enough
commit-chain material to verify contradiction.

If evidence is not archived, expires too soon, or is stored in incompatible
formats, accountability weakens.

Mitigations to develop:

- standard fraud-proof bundle format
- recommended evidence retention periods
- compact proof packaging
- public evidence archives
- challenge and correction mechanisms for published evidence

Evidence retention is part of the security model, not just an operational
detail.

### Mass Churn and Identity Expiry Cliffs

Identity expiration limits hoarding, but it may create renewal cliffs.

If many operators renew at similar times, the network may temporarily contain a
large cohort of young identities. Attackers may time campaigns around such
periods.

Mitigations to develop:

- staggered renewal guidance
- grace periods
- peer-selection dampening for very young identities
- limits on young-identity concentration per neighborhood
- monitoring for unusual regional churn

Expiration should reduce hoarding without creating predictable weak periods.

### Peer-Selection Gaming

Any visible selection metric can be optimized by adversaries.

If peer selection rewards latency, uptime, storage score, answer speed, age,
endorsement, or gateway reputation, attackers will tune their infrastructure to
score well.

Mitigations to develop:

- preserve randomness in peer selection
- avoid over-weighting any single metric
- test scoring under adversarial simulation
- include diversity signals
- periodically challenge assumptions about what "good peer" means

Peer selection should prefer useful peers while remaining hard to game
deterministically.

### Correlated Infrastructure

Many identities may appear independent while sharing the same operator,
software, cloud provider, network, legal jurisdiction, or failure mode.

This matters because EC's safety depends on diversity over time, not just raw
identity count.

Mitigations to develop:

- optional infrastructure diversity scoring
- detection of obvious network correlation
- operator-side peer diversity policy
- avoiding mandatory central registries for correlation data
- privacy-preserving ways to reason about independence

This is hard to solve cleanly. It should remain visible as an ecosystem risk.

### Endorsement Capture

Optional endorsements can improve safety for high-value applications, but they
can also become de facto mandatory.

If major applications require the same endorsers, the ecosystem may drift
toward gatekeeping even if the base protocol remains permissionless.

Mitigations to develop:

- multiple competing endorsers
- transparent endorsement policy
- application-specific rather than network-wide endorsement requirements
- continued support for unendorsed base participation
- monitoring for endorsement concentration

Endorsements should add choice, not silently become global admission control.

### Rare but High-Value Tokens

Popularity-based monitoring works well for common tokens, but some valuable
tokens may be rarely queried.

Examples include dormant identities, cold credentials, long-lived registry
entries, and private assets. These may not accumulate evidence naturally.

Mitigations to develop:

- explicit paid or self-hosted monitoring for rare high-value tokens
- application guidance that rarity is not the same as low value
- scheduled background checks for long-lived important state
- use of multiple supporting tokens where practical

Rare high-value state should not rely on organic popularity for protection.

### Application Atomicity Mistakes

Destroy-create and multi-token designs are powerful, but application developers
can misuse them.

Possible failures include partial workflows, unclear lineage, inconsistent
redemption policy, lost auditability, or state machines that assume stronger
finality than EC provides.

Mitigations to develop:

- application design patterns
- reference flows for vouchers, identities, registries, and voting
- audit checklists
- explicit guidance for irreversible off-protocol actions
- simulation of multi-token application behavior

The base protocol can be sound while an application-level protocol is unsafe.

### Owner-Key Compromise

EC prevents third parties from manufacturing conflicts, but it cannot prevent
valid signatures from a compromised owner key.

If an attacker controls the token owner's key, the network will see authorized
updates.

Mitigations to develop:

- key rotation patterns
- recovery tokens
- multi-signature or threshold application schemes
- issuer or guardian policies for high-value credentials
- clear distinction between network fraud and owner-key compromise

This risk belongs mostly to application design, but it is central for users.

### Denial of Service and Cost Shifting

Open querying and cheap participation can be abused.

Attackers may flood popular tokens, force expensive validation paths, request
large chain ranges, or make honest nodes spend bandwidth and storage serving
low-value requests.

Mitigations to develop:

- cheap prechecks before expensive validation
- per-peer and per-token rate limits
- fairness across connected peers
- bounded chain-serving policies
- refusal rights without fraud stigma
- monitoring for load amplification

DoS handling is part of safety because degraded liveness can mislead users even
when cryptographic safety holds.

### Liveness and Perception Attacks

The most dangerous unsolved category may be attacks against what users see, not
against what the protocol can cryptographically prove.

Examples include stale views, isolated clients, captured gateways, withheld sync
data, incomplete monitoring, and strategically timed silence.

The protocol may remain cryptographically safe while users make bad decisions
from incomplete information.

Referral-based discovery and elections are the main protocol-native response:
expand the peer sample, query through independent routes, and compare aligned
storage clusters before acting. This works best when clients are allowed enough
time to discover alternatives. It works least well for urgent high-value
decisions made immediately after bootstrap.

Mitigations to develop:

- value-aware client verification profiles
- gateway freshness labels
- referral expansion before high-value decisions
- diversity requirements for high-value reads
- public monitoring services
- clear application guidance before granting value

This class of risk should be treated as a first-class design problem.

## Risk Picture by Use Case

### Low-Value Tokens

Low-value tokens generally do not need special fear.

Even fraudulent operators help as long as they behave honestly. Attacking small
values is usually uneconomical, and destroy-create flows make it hard to know
which neighborhood to target ahead of time.

Recommended posture:

- normal queries
- ordinary peer selection
- no paid monitoring unless needed for convenience
- rely on application-level redemption checks

### Payments, Vouchers, and Moving Value

Payments and vouchers benefit strongly from destroy-create update patterns.

Value moves around the ring. To reliably attack these flows, an adversary needs
large coverage across many neighborhoods or must get lucky at the moment of
settlement.

Recommended posture:

- use destroy-create updates
- use multiple tokens for larger values
- have counterparties verify before granting value
- store Answers for economically meaningful redemptions
- use monitoring for issuers or large merchants

### Voting and Large Temporary Token Sets

Voting is difficult to manipulate quietly when many tokens are involved.

To shift a majority, an adversary must influence many neighborhoods and risks
creating broad evidence. Temporary token lifetimes also reduce the payoff of
slow targeted capture.

Recommended posture:

- spread ballots across many tokens
- query multiple independent peers
- collect evidence during the voting window
- preserve audit data until the challenge period ends
- consider public monitors for high-stakes votes

### Long-Lived High-Value Tokens

Long-lived identities, registries, names, high-value credentials, and similar
objects deserve active monitoring.

They are attractive targets because they persist. The attacker can prepare for
them.

Recommended posture:

- query and store signed Answers regularly
- use independent watchtowers
- require chain proof for important reads
- use multiple supporting tokens where practical
- use endorsements or application-specific trusted peer lists if appropriate
- publish or subscribe to fraud evidence feeds

## Guidance for Clients and Token Holders

Clients should choose verification effort based on value.

For low-value reads:

- query a small number of peers
- accept ordinary latency and ordinary confidence
- do not retain extensive evidence

For medium-value reads:

- query multiple peers
- store signed Answers briefly
- prefer peers with good service history
- use gateways that retain evidence

For high-value reads:

- query diverse peers through diverse paths
- store signed Answers and chain proofs
- use at least one independent monitor
- require recent evidence from known-good peers
- consider multiple-token application design
- delay irreversible off-protocol value transfer until checks complete

For very high-value or long-lived state:

- maintain continuous monitoring
- use public evidence services
- require stronger application policy
- consider endorsement requirements
- design updates to move value when possible

## Guidance for Node Operators

Node operators are free to choose peers according to their own risk policy.

Strong default inputs for peer selection include:

- ring-distance fit
- storage alignment
- answer quality
- commit-chain serving quality
- uptime
- latency
- age and useful tenure
- diversity across discovery paths
- optional endorsements
- fraud evidence feeds
- local deny lists
- application requirements

Operators should distinguish:

- signed contradiction: strong exclusion
- repeated poor service: down-rank or exclude
- ordinary lag: tolerate or lower weight
- lack of endorsement: policy-dependent

Operators should not treat permissionlessness as an obligation to peer with
everyone. Local choice is part of the safety model.

## Honest Comparison With Other Systems

### Centralized Databases

Centralized databases are simpler and safer when one operator is acceptable.

They offer:

- clear authority
- simple recovery
- low latency
- strong operational control

They do not offer:

- independent multi-operator hosting
- permissionless participation
- public evidence of operator contradiction
- decentralized durability

If one trusted operator is fine, a centralized database is usually the right
choice.

### Classical BFT Committees

Classical BFT systems provide strong safety under a known validator set and a
bounded Byzantine fraction.

They offer:

- formal safety properties
- immediate committee accountability
- clear membership

They require:

- known validators
- membership management
- global or committee-level coordination
- stronger assumptions about who participates

EC trades fixed membership for permissionless local participation. That makes
the system more open but less cleanly bounded than a fixed BFT committee.

### Proof-of-Work Blockchains

PoW blockchains make influence proportional to ongoing hash power.

They offer:

- open participation
- global ordering
- simple security story

They cost:

- continuous energy use
- global serialization
- high latency or probabilistic finality
- no natural data expiry

EC uses proof-of-work for identity, not global block production. This is much
cheaper operationally, but it also means EC does not inherit the same global
hashpower security model.

### Proof-of-Stake Systems

PoS systems make influence proportional to staked capital.

They offer:

- lower energy cost than PoW
- explicit slashing if stake is locked
- economically legible validator sets

They introduce:

- token economics
- governance complexity
- stake concentration risk
- regulatory and market exposure

EC avoids a native staking token. Its accountability is evidence-backed
exclusion and reputation destruction, not automatic seizure of locked capital
unless an application layer adds such a mechanism.

### DHTs and Peer-to-Peer Storage

Traditional DHTs provide scalable routing and storage lookup.

They often lack:

- strong Sybil resistance
- signed conflict accountability
- application-level safety semantics
- durable evidence trails

EC borrows locality and routing ideas from DHTs but adds proof-of-work identity,
proof-of-aligned storage, commit-chain evidence, and user-signed update rules.

### Federated or Authority-Based Systems

Federated systems rely on known institutions or chosen operators.

They offer:

- practical trust
- clear legal relationships
- easier accountability

They give up:

- fully open participation
- neutral base-layer admission
- resistance to federation capture

EC can support federation-like behavior as an application policy through
endorsements and peer selection, without making it mandatory for the base
network.

## Decision Matrix

| User Need | EC Fit | Recommended Policy |
|---|---:|---|
| One trusted operator is acceptable | Poor | Use a centralized database |
| Open multi-operator state with bounded retention | Strong | Use EC normally |
| Low-value mutable records | Strong | Basic queries and normal peer selection |
| Vouchers or issuer-backed payments | Strong | Verify before redemption; use destroy-create |
| High-volume temporary voting | Plausible | Collect audit evidence during the voting window |
| Long-lived high-value identity or registry | Conditional | Use monitoring, evidence storage, and stronger policy |
| Instant global finality | Poor | Use another system or application-level guarantees |
| Adversary may buy massive compute for one target | Conditional | Spread tokens, monitor, use endorsements or trusted gateways |
| Legal/commercial accountability is required | Conditional | Use endorsed peers and evidence services |

## Practical Safety Posture

The recommended base posture is:

- keep identity creation permissionless
- treat mined identity as candidate status, not authority
- make peer influence depend on observed useful behavior
- let operators choose and exclude peers locally
- use endorsements as optional selection weight
- use fraud proofs only for signed contradictions
- use service quality signals for down-ranking
- encourage watchtowers and public evidence lookup
- encourage high-value apps to monitor and spread risk

In short:

> Peer identity is permissionless. Peer influence is earned gradually.
> Endorsements may improve selection, but they do not replace observed
> behavior. Fraud is signed contradiction; everything else is local policy.

## Open Questions

Several design points remain policy or implementation questions:

- How much weight should age and tenure receive in peer selection?
- Should neighborhoods cap the fraction of young identities admitted per time
  window?
- How should endorsement tokens be represented in identity blocks?
- What is the default retention period for evidence bundles?
- How many chain elements must an Answer include for useful fraud detection?
- What minimum commit-chain serving behavior should a good peer provide?
- How should public fraud evidence be gossiped, indexed, and challenged?
- Should high-value applications define standard monitoring profiles?
- What bootstrap diversity should new clients require before high-value reads?
- What referral/discovery budget should clients use before treating poor
  service as unavailable state?
- How should Answers represent freshness without turning stale data into fraud?
- How can gateways expose proof and freshness without becoming trusted
  authorities?
- What privacy patterns should clients use when monitoring valuable tokens?
- How should peer selection account for correlated infrastructure without
  creating a central registry?
- What reference application patterns prevent multi-token atomicity mistakes?

These questions do not change the core conclusion. They tune the tradeoff
between openness, usability, and resistance to well-funded targeted attack.

## Summary

EC's safety model is not that attackers cannot enter the network. They can.

The safety model is that useful influence is hard to acquire quickly, expensive
to maintain, locally bounded, observable through service behavior, and risky to
abuse because signed contradictions can be stored by anyone and used by
everyone.

For low-value and moving-value applications, this is likely enough with ordinary
verification. For high-value and long-lived state, users should actively gather
evidence, use monitors, spread risk across tokens, and apply stronger peer
selection policy.

That is the honest claim: not absolute prevention, but scalable cost,
localization, evidence, and user-selectable verification proportional to value.
