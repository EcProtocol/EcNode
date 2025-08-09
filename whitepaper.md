
# A Common Notary for the Internet

## Table of Contents

1. [Introduction](#introduction)
2. [Project Outline](#project-outline)
3. [High-Level Design](#high-level-design)
4. [API](#api)
   - [Request Type Messages](#request-type-messages)
   - [Reply Type Messages](#reply-type-messages)
   - [Token Mapping Replies](#token-mapping-replies)
5. [Peer Discovery](#peer-discovery)
   - [Bootstrapping](#bootstrapping)
   - [Peer Quality and Proof of Storage](#peer-quality-and-proof-of-storage)
   - [How to Obtain Initial Mappings](#how-to-obtain-initial-mappings)
   - [Stale or False Mappings](#stale-or-false-mappings)
   - [Introducing New Peers to the System](#introducing-new-peers-to-the-system)
   - [Trusted Peers](#trusted-peers)
   - [Selecting Trusted Peers](#selecting-trusted-peers)
6. [Consensus](#consensus)
   - [How Can This Work?](#how-can-this-work)
7. [Load Balancing](#load-balancing)
8. [Ticket Schemes for Clients](#ticket-schemes-for-clients)
   - [API Key](#api-key)
   - [Blinded Tickets](#blinded-tickets)
   - [Proof of Work](#proof-of-work)
9. [Keeping Mappings and Transactions Available](#keeping-mappings-and-transactions-available)
10. [Security](#security)
11. [Performance](#performance)
12. [Example Use Cases](#example-use-cases)
    - [Tickets](#tickets)
    - [Payment](#payment)
    - [Identity](#identity)
    - [DNS](#dns)
    - [Process/Document Flow](#processdocument-flow)

---

## Introduction

Using stateful Internet services requires trusting the service provider to keep your data safe and not manipulate it in ways you did not agree to. Your data lives inside a blackbox owned and operated by that provider.

This hinders interoperability. There is no way to move ownership of your data elsewhere unless it's explicitly built into the product. You cannot simply take a copy of your data, hand it to someone else, and claim that they now own or control it.

This project opens up the "service blackbox." It provides a way to split services into a "data" part and an "ownership" or "tracking" part by offering a notary for the Internet. Data, documents, or state in general are manipulated in the service-provider system, but the notary keeps a record of ownership and states. Now you can email a document to someone and hand over ownership in a way that they can trust—they can prove that they control that data.

Given that the core issue here is trust, sharing, and making a common service available, this sort of "product" cannot be built and offered by one company. It has to be shared.

This paper presents a distributed consensus protocol for token-based transactions using a federated peer-to-peer network with proof-of-storage tests for unmanaged inclusion of network peers. The goal is to build a workable, global, common notary service for the Internet.

## Project Outline

To achieve this, we need to build a fast, globally distributed consensus network. This is accomplished by limiting consensus for specific tokens to a "neighborhood" set of peers and by offering a public API to read the current state.

The transactions (identified by the SHA of their content) in this system do not contain "user-data" as such. User-data is regarded as toxic. The expected use case is for users to SHA content and store it using other network structures. In this way, this network can be used for verification and tracking, vastly simplifying the responsibility of other networks (and allowing users to store/provide data in whatever way or format is suitable for their purpose).

Since tokens have no special meaning to this network, they can be created AND "destroyed" (accept no further transactions). This allows use cases impossible or impractical in networks where tokens are, for example, mined or otherwise limited in form or supply.

The network topology generally does not distinguish between "clients" (end users) and "servers" (federated peers). However, it assumes that a large set of peers are stable and available. These federated peers have to continuously "earn their seat" (address) in the network. In this text, we will use "nodes" to describe both types and "peers" to mean nodes that participate in federation.

# High-Level Design

Users create new transactions and query for existing transactions and mappings. If the network accepts a new transaction, the token IDs contained in it are added to the front of the transaction chain for each of those tokens.

A transaction is a collection of up to 6 tokens (each 256 bit) mapping to SHA Blake3 of signature public keys, as well as the transaction ID of the previous transaction to update the token, plus a few other values (described later).

One token mapping:
[Token, SHA(public key), Previous-transaction-id] (in total: 3 × 256-bit values = 96 bytes)

**Token Creation and Destruction Rules:**
- IF the previous-transaction-id field is 0: this transaction tries to create this token (first use seen by the network)
- IF the SHA(public key) field is 0: this transaction destroys this token (no more changes are allowed by the network)
- Both may be 0, in which case the token is created but not usable in further transactions

**Transaction Validation:**
IF the previous-transaction-id field contains a value, this must be a valid (findable by the nodes) transaction ID. When validating transactions, nodes must retrieve all previous-linked transactions and verify that:
1. Provided signatures have a public key that hashes to the SHA(public key) field
2. The signature is valid

The SHA(public key) field can also be "-1" (and no signature for that token update is given). In that case, the transaction does not update the token but verifies that at that time this token is mapped to the previous-transaction-id. More on that use case later.

**Node Identity:**
New nodes generate an EC DH key pair and use it long-term (mitigation for DH vulnerabilities needs investigation). A "256-bit node address" is derived by applying Argon2 to the public key.

**Token Neighborhoods:**
The "neighborhood" of a token is simply defined as the distance from the token ID to node addresses.

## API

The final system is intended to use UDP encrypted packets using the shared key obtained by performing Diffie-Hellman key exchange from the sender to the receiver node. Encryption uses the ChaCha20-Poly1305 standard to secure integrity and confidentiality of each packet. All packets must be below MTU (~1500 bytes) to achieve the fastest network transfers.

The system tries to avoid network round trips as much as possible. Therefore, there are no handshake or multi-round (single) operations.

The core system offers only these message types, which in many cases can be batched in single network packets.

**General Message Layout:**
```
[Version-id: 16 bit] [Reserved: 44 bit] [IV: 196 bit]
[Sender public key: 256 bit]
[MAC: 256 bit]
[Encrypted content... rest of packet]
```

### "Request Type" Messages:

1. **Get Transaction**: Input: Transaction ID and network address of the requesting peer
2. **Get Token Mapping**: Input: SHA(Token ID) and network address of the requesting peer  
3. **Vote**: Vote for a transaction to be added to the "front" of each contained token chain. Input: transaction ID and votes (detailed later)

**Ticket System:**
These message types must include a "ticket value":
- For transaction and token "get" requests: the ticket is a SHA of a secret value kept by the sender and the token or transaction ID being requested
- For Vote messages: the ticket is "proof" that the sender has obtained rights to submit transactions to this node (detailed later), OR 0 if the sender believes the receiver has accepted it as a trusted peer. Votes will be discarded if neither condition holds.

**Message Forwarding:**
IF a peer does not know the mapping of a get-request, it *can* forward the request to a better qualified peer and add the network information to get the response back to the original requester. To support NAT'd networks, the original requester does not put any information in the network-address part. Upon seeing this, a forwarding peer will put the network address that the message was received from in the forwarded message.

Since requests for unknown SHAs could be forwarded forever—and given the asynchronous nature of the protocol—peers only forward with some probability.

This also applies to load control. Each peer is free to discard any number of messages it deems necessary.

It is the *clear intent* of the system to allow public access to issue get-messages (and responses) on a best-effort basis, but to restrict Vote messages either by proof-of-storage or by validating various forms of tickets (see concept ideas later). 

### "Reply Type" Messages:

Responding to request messages:

1. **Transaction Content**: The content of a transaction PLUS signatures (EC signature pair plus public key)
2. **Token Mappings**: The mapping of a token ID to the current (latest) transaction ID that uses it (as seen by this peer)—the head of the token chain. Additionally, the token-mapping response contains 10 surronding token-mappings based on a signature-scheme (more below)

**Ticket Validation:**
For a node to accept these messages, they must contain a valid ticket. Since the node has the secret to create tickets, it simply tests if the ticket provided is the SHA of either the transaction ID or token provided, ensuring it is the answer to something it has requested.

Nodes update the ticket secret every few milliseconds. Tests are done with both the current and previous secret.

Therefore: If a ticket does not match either, it is either spam or a "slow response." No infinite re-sending will be accepted.

In one special case, the ticket field is allowed to be 0 for token-mapping responses—when peers "introduce" themselves. See more in "Peer Discovery."

#### Token Mapping Replies

A reply to a token mapping request first requires that the receiver knows the token ID of the requested SHA. Therefore, peers store token mappings by their SHA in a sorted key/value database.

To prove the quality of the replier the response also contains a sample of token-mappings from "around" the request SHA. 

Specially a responder calculate a 10 bit Blake3 SHA of: 
- The receivers public key
- The token ID  
- The previous-transaction-id of the response

Let's call this the "signature" of the reply. Split it into 10-bit chunks and then (starting from the requested SHA) scan the sorted token-SHAs database in decending order, stop at the first SHA that ends with the first 10-bit component of the signature - output the token and mapping as the first component of the signature. Then continue from that point until a SHA ending with the next chunk of the signature. And so on until the first 5 elements have been mapped. 
Then repeat the process acending from requested token SHA.

This forms the 10 signature-mappings.

**Validation Process:**
When the receiver sees this sequence, it can quickly:
1. SHA each of the token IDs and check that they "surround" the SHA of the requested token (5 above, 5 below)
2. Calculate the expected signature as described above.
3. Check that the SHAs ends with the 10-bit chunks as described above.

Failure to meet any of these requirements causes the response to be discarded as spam.

## Peer Quality and Proof of Storage
Token-mapping responses form the foundation of the "Proof-of-storage" system underpinning the security of this system. The aim is to force peers to keep up-to-date storage of all/most token-mappings in its neighborhood.

The signature-scheme enables that.

From a single response a receiver can test the distance from the highest SHA to lowest. Since even distribution of mappings is a core foundation of SHA functions - its expected that taking a sample (as driven by the signature scheme) of approximatly the same number elements from all areas of the spectrum has a density directly propornal to the number of elements.

On the other hand this also means that a peer that stores more elements will be able produce "narrower" signatures. So we will expect good quality peers to have signature-width below a certain limit. And this limit will be "global" - it should be observable across the spectrum.

A cheating responder could however with modern hardware calculate SHA's with the desired properties and still be able to make the reponse-timelimit. So more independant responses most be collected.

Given a set of responses the receiver can first check if they agree on the token-mapping of the requested SHA and gather responses until a +2 majority forms for a mapping.

But the receiver can now also verify the quality of the majority responders more closely - and filter out poor responders. 

We will require that half of the mappings reported in the different responses are the same! For peers that store the same token-mappings this will be the natural outcome since they apply the same search across the same data. Even if they have very close dataset will the signature-search find the same tokens with high properbility.

If the signatures however do not have many overlapping token-mappings it will be because the peers did not store the same data. Which would be the case if they each contained samples of the "true set of token mappings".

Storing a lower density will show up if the receiver has compareable responses. The most reliable way to get same dataset is for all peers to collect and store as close to all token-mappings as possible.

A lazy peer might also try to get the tokens from other peers and form a response based on that. But since the signature is calculated based on the receiving public-key, they would see another set of tokens for the same request. And it would require many requests to form a fradulent response - a lot more than can be achived in the time-window.

So anyone who wants to obtain the mapping from the best peers in the system, would run a series of such requests and filter the responses based on these principles.

**Secret Channel System:**
Since receivers could respond from multiple peers to shortcut the process or manipulate mappings, the requester uses the ticket system to separate responses into "secret channels":

- Instead of having one secret value to create tickets for requests, the peer maintains multiple secret values
- Responses are tested with these to identify channels
- Voting collects the last message per channel (regardless of the sending peer)
- Since tickets are opaque to receivers, they have no way of detecting this
- It provides no advantage to respond to the same request from multiple peers
- Even if the receiver sees 2 different tickets for the same request ID, it could still be the same channel if the timer rolled the secret

# Peer Discovery

Token mapping request/responses not only support "clients" who want to check the current mapping of a token—they also form the foundation of peer discovery and network formation.

Since peers can forward messages to better qualified or less loaded peers, this enables nodes to discover previously unknown peers. However, requesters need to know SHAs of tokens or transaction IDs beforehand since the protocol does not allow *wildcard* searches.

To facilitate discovery, all peers participating in federations *MUST* create a transaction with a token matching their public key. This token chain may in the future serve further peer-related functionality/verifications, but currently only the first token mapping is used.

We will refer to these transactions as "lampposts."

## Bootstrapping

All new nodes need to be seeded with a set of known peers (out-of-band). The seed must contain both network addresses and public keys to be able to communicate with those nodes. Users would obtain, for example, 5-10 such peers from a semi-trusted source.

If the seed peers have correctly registered "lampposts," the bootstrapping node can query those from the same peers (using the public keys as tokens).

The public keys of new peers are added to the process, as well as token IDs and transaction IDs from valid mapping responses, along with transactions pointing to previous transaction IDs and other token IDs contained therein.

If the seeds do not form a closed loop of peers, this process should eventually produce responses from peers across the network and across the spectrum. Users may also use out-of-band obtained token IDs to verify that the peers do indeed connect with the desired network.

The new node runs this process until enough peers with demonstrated network connectivity have been obtained.

Different types of nodes may have different requirements for peers:
- Client-type nodes need a set of peers for which they have tickets to submit to
- Relay/query-only type nodes may just want a large set of connected peers

We will proceed to consider nodes that want to get accepted as federation peers.

### How to Obtain Initial Mappings

New peers can obtain the needed mappings by getting token mappings around their own address. Upon receiving a response, they should by default record all mappings in the response in their database. They should then issue requests for some of the new token IDs and continue in this fashion until they reach sufficient density.

When receiving responses for mappings that have already been recorded, the peer should compare the mappings. If there is a conflict, it should run a voting round (described below) for the true mapping and select the "winner." In addition, it should get all head-of-chain transactions and store those to enable validation of new transactions eventually.

**Database Management Principles:**
1. Store all response mappings
2. When conflicts are observed, run voting
3. Continuously run a "reaper" process that scans the token-mapping database and prunes elements with probability based on:
   - How distant the token ID is from the peer's address
   - How long since the mapping was created

If the database gets "thin," the bootstrapping process can start again to obtain mappings across the spectrum.

### Stale or False Mappings

A peer may know enough common token IDs to respond to requests but either have out-of-date mappings or try to manipulate mappings of tokens to other transaction IDs than the "true" head-of-chain. This would enable "double-spending" and other unwanted scenarios.

To combat these types of peers, we will collect multiple responses for each request and only accept majority mappings.

**Continuous Validation Process:**
Peers must continuously run a peer-collecting and validating process after initial bootstrapping:

1. Generate a random 256-bit number and find the closest token ID in the token-mapping database
2. Request mappings for that token ID from:
   - Known peers with addresses close to the token IDs
   - Randomly selected peers (e.g., peers recently heard from)
   - Piggyback on other traffic

3. Upon receiving responses:
   - Collect and compare the mappings
   - When a majority forms (+2 for a specific mapping):
     - Pick from the set of peers voting for that value, the one with the address *closest* to the requested token ID
     - Fetch and store (if not already) the transaction being pointed to
     - Demote any trusted peers that voted differently to "prospects" (TODO: check consequences)
   - If no majority forms within the time limit, discard the voting state


## Introducing New Peers to the System

It follows from the above that a completely new peer will have no chance of getting requests without help—and hence no chance of getting accepted into the network. For this reason, peers can send token-mapping responses with a 0-ticket, signaling that it's an "introduction" message.

**Introduction Message Format:**
The main elements of the token-mapping message are the same, but the signature is calculated differently:
- 8-byte SHA of:
  - Public key of the sender
  - Public key of the receiver  
  - Head-of-chain transaction ID of the "lamppost" as reported in the message
  - (TODO: plus the IV of the message?)

This ensures that a new peer has to present different mappings to different receivers.

As with messages in general, there is no guarantee that receiving peers will process such messages.

**Processing Introduction Messages:**
If the message is processed:
1. All the rules outlined earlier are checked
2. If the new peer complies, it's added as a "prospect" peer (and its network address is recorded)
3. Prospects do not get:
   - Vote messages
   - Forwarded token-mapping requests from the network
4. Prospects do receive:
   - Get-transaction messages
   - A chance of getting request messages from the proof-of-storage process
   - Thus a path to getting accepted as a trusted peer

Peers for which "introduction" messages have been sent are put in a "pending" state, so if an "introduction" comes back, it's known that the connection is usable.

**Replicator State:**
A new peer must keep up with the network until it eventually starts seeing request messages from other peers. It will be in a "replicator" state until it can rely on Vote messages to keep its state up-to-date.

Keeping up works much like the proof-of-storage process, except:
- The random number to look up is in a smaller range around the peer's address
- The frequency of requests should be higher

## Trusted Peers

Nodes maintain a set of *trusted peers* across the spectrum—but with increasing density closer to their own address. This facilitates more efficient routing and voting by focusing traffic and keeping neighbors aware of each other.

**Benefits and Responsibilities:**
Trusted peers can:
- Submit transactions *without tickets*
- Vote for their neighborhood

This is the main benefit of (or *payment* for) becoming a trusted peer of the network. They pay for the ability to submit new transactions by offering storage for a slice of the data in the network.

**Trust Establishment:**
1. When a peer is accepted, an "introduction" message is sent back
2. Peers expect to be trusted by the sender upon receiving such a message
3. Peers periodically send "introduction" messages to their set of trusted peers
4. Upon receiving such a message from an already trusted or prospect peer, only the last-heard-from state is updated
5. If no such message has arrived for a while from a trusted peer, assume that either:
   - It's gone, or
   - It has removed us from its set of peers
   - In these cases, send a fresh "introduction" message and put the peer into "pending" mode

**Quality Tracking:**
Valid responses to get-requests from all trusted and prospect peers:
- Add to a quality score
- Update last-heard-from timestamp

**Incentive Structure:**
The aim is to motivate operators to continuously:
1. Create new peers and try to get them added as prospects
2. Eventually upgrade them to trusted peers

This ensures that an operator maintains the ability to submit transactions even as trusted peers occasionally "lose" to other peers.

Operating a trusted peer also provides the ability to respond to requests using new peers.

**Note:** An operator can share some parts of the storage between its peers and thus lower the marginal cost.

## Selecting Trusted Peers

It's not desirable to have peers use or know all other peers in the entire network—even if this is the most efficient routing configuration. The reasons are:

1. **Security**: Keep message routing in the system less predictable and thus safer
2. **Network Reality**: By the laws of physics, some peers will have longer network latencies, and networks may fail or become unavailable. It's better for many overlapping, random graphs to form between available peers
3. **Scalability**: The aim is to accommodate global networks of millions of peers—it might be impractical in the longer term to keep state for each peer (even if the required state per peer is limited compared to modern memory sizes)

**Selection Process:**
1. When map-voting rounds are held and won:
   - Check if any of the winning peers are unknown
   - If the area of the peer(s) is underpopulated:
     - Send "introduction" messages to those peers
     - Store them as pending

2. When a pending peer returns with an "introduction" message:
   - Turn it into a trusted peer

**Peer Management:**
From time to time, a node should:

**Prune peers that:**
- Haven't been heard from in a while
- Occupy areas that are too densely populated

**Promote peers when:**
- Thinly populated areas have prospect peers
- Send "introduction" messages and turn them into trusted peers
- If multiple prospects are available, select the one with the latest heard-from timestamp

# Consensus

We have already discussed how peers collect mapping responses, store them, and vote if they see conflicting mappings. Tests show that getting +2 votes on equal mappings in a randomized network of peers indicates that this mapping dominates by far the neighborhood of the token.

The same principle is applied when submitting new transactions to the network, with some additions.

## Transaction Submission Process

**Initial Submission:**
1. When a node wants to submit a new transaction, it picks peers that it *believes* have it registered as either:
   - A trusted peer, or  
   - Will accept the ticket provided
2. The initial peer(s) receiving the transaction ID will not know the transaction content (including signatures)
3. They will immediately request the transaction content from the submitting node (if load balancing permits)

**Vote Distribution:**
Once the actual transaction is received:
1. The peer knows which token mappings are affected
2. For each token, it votes/forwards the transaction to peers in the areas around those tokens (reusing if they overlap)
3. It indicates with the vote if it believes the mapping represents a valid step on the token chain:
   - The previous-transaction-id matches what is recorded in its database, OR
   - The timestamp/counter of the transaction is less than the one recorded (it's an out-of-order update)

**Already Committed Transactions:**
If a peer has already committed the submitted transaction, it should immediately respond with an all-positive vote to the caller (and indicate that it should not receive further votes for this).

**Witness Distribution:**
Peers also submit the transaction to the area around the transaction ID itself. This way:
- The transaction will be findable without knowing any token IDs from it
- The process gets a pseudo-random witness to also distribute the transaction to the network

## Voting and Validation

**Vote Tracking:**
Much like with the voting of mapping responses, the peer keeps a log of votes from trusted peers. Newer votes replace older ones.

**Parallel Validation:**
Parallel to the process of posting votes, the peer:
1. Collects all previous transactions
2. Validates that the timestamp/counter in the new transaction is:
   - Larger than any recorded in each of those previous transactions
   - Less than 2 hours into the future
3. Verifies all signatures
4. If any of these checks fail, the peer will actively vote "no" for the transaction (and block it from committing)

**Commit Criteria:**
IF all tokens get a +2 vote from their respective neighborhoods, the peer commits the transaction. The actual "witness" area votes do not count as such (not expected to have up-to-date mappings), but we track that we get votes back from at least 2 peers in that area.

**Database Updates:**
When committing a transaction, the token-mapping database is updated:
- If the timestamp/counter of the previous recording is greater than the new transaction, this update is regarded as out-of-order (no change to that token mapping is done)
- Mappings created from just recording mapping responses with unknown transactions have 0-timestamp (so will always be overridden)
- Mappings for which conflict voting has run will also have fetched the transaction and hence have a timestamp/counter from that

## How Can This Work?

Peers maintain a set of trusted peers across the spectrum—but with increasing density closer to their own address. This means that:

1. **Local Consensus**: Peers in the same area will tend to trust each other and thus send votes between them, eventually deciding the majority

2. **Propagation**: Nodes not in an area will eventually get "committed" messages in response from the core peers of the area, or no answer if they cannot agree (in which case transactions time out)

3. **State Propagation**: A committable transaction will propagate that state back through the peers leading to the origin

**This has been demonstrated by simulation.**

# Load Balancing

The aim is to limit some nodes from pumping disproportionately many transactions into the system. Therefore, peers:

1. Keep a count of new transactions initiated by their trusted peers
2. Employ other load-balancing techniques for "client type" nodes
3. Discard transactions if a sender exceeds its limit

At regular intervals:
- Counters are reset
- Limits are adjusted to the desired overall load for the system

# Ticket Schemes for Clients

The majority of nodes are expected to be "clients" with no ability or desire to participate in federation. Such nodes can:
- Freely query for transactions and mappings
- Submit transactions only if they have tickets to one or more peers

We envision a user base where service providers offer specialized apps—and in return for using the network, they operate a number of peers for which they control access. They are free to implement the type of ticket system appropriate for their use case.

Since a ticket is just an opaque 256-bit value, different schemes are possible. Here are some examples for inspiration:

## API Key

Trusted clients are issued an API key, potentially used together with the public key of the client node (if that is stable). A variation is to mainly use the public key but leave something in the ticket field to distinguish it from peer traffic.

This, of course, makes it possible for the operator to link all transactions back to the user. In many scenarios, this is anyway possible given a parallel data system.

## Blinded Tickets

In some cases, it's not desirable for the operator to be able to track the transactions of individual users. In such cases, an operator may use an out-of-band system (web/app) to cryptographically "blind" tickets together with users.

**Example Process:**
1. A user generates an ephemeral EC key, B
2. The user takes the transaction ID it wants to process without detection and performs scalar multiplication with B, call it H
3. Going to the operator site with H, the user pays for one transaction
4. The operator applies its own secret EC key O to H, yielding S1
5. The user can now apply the inverse of B to remove it, leaving S2

**Transaction Submission:**
When submitting the transaction to an operator-controlled peer using S2 as a ticket:
- The system applies the inverse of O and checks that this equals the transaction ID
- It cannot link S2 or the transaction ID to H

**Time-Based Variation:**
Instead of a single transaction, this could also be done for:
- A SHA of a user's short-lived public key
- A time-range-indicator value (e.g., today's date)

Upon receiving such a ticket:
- The peer inverses the ticket
- Checks that it matches the client public key hashed with the current time-range-indicator
- This would allow any number of transactions during that time range

Other variations of this scheme can lock to other hash commitments.

## Proof of Work

In the same spirit, an open and permissionless system is also possible. Here a client could be asked to provide a ticket which, when hashed together with the transaction ID, would result in a value with some property (e.g., starts with some number of 0-bits).

**Properties:**
- Easy to test for the peer system
- Time-consuming up to some level for users to generate
- Would not allow the operator to profit from the transaction
- Could rate-limit an otherwise open gateway

# Keeping Mappings and Transactions Available

The system spreads transactions and mappings to a number of peers.

New peers that want to enter the network run in "replicator" mode—standing by to take over failing peers.

**Availability Contract:**
The general contract with users is that:
- **Main priority**: Keep the head-of-chains available
- **Secondary**: Some level of history can be expected

Use cases should try to work with this—for example, providing history of documents and hashing them to new tokens such that the head-of-chain also proves that previous versions existed as stated.

**TODO:** It's an open question to test and qualify how well this will work or if further processes are needed to keep availability at desired levels.

# Security

Since transaction chains are hashed like blockchains and transactions are signed by common crypto schemes, isolated client behavior does not seem to be the biggest threat to the system.

Tokens in this system are also not valuable by themselves (unlike other blockchain-style networks). So it does not make sense to "steal" random tokens without knowing what they are. Since related data is not in the system, it does not reveal such properties by itself.

**Main Threat:**
The main threat to the system is manipulation of individual tokens with some known (to the manipulators) quality. Collusion between malicious clients and operators poses the greatest threat.

## Attack Vectors

To take over control of individual token mappings, an operator needs to control the neighborhood of such a token. During the attack, the peers could be made to report false mappings (maybe even just to a selected user/group).

This can be done in two basic ways:

### 1. Create New Peers for the Neighborhood

**Process:**
- The adversary would have to find a sufficient number of key pairs where the public key has an Argon2 hash in the desired range
- This will be a very time and resource-consuming activity (maybe 10-20 such pairs are needed to form a majority)
- The new peers would then have to out-compete the existing peers in the area
- They must go through the whole process outlined above

**Assessment:** This is expected to be a very expensive endeavor.

### 2. Take Over Existing Peers

**Process:**
The adversary seizes control of (hacks or bribes) a majority set in the desired area.

**Assessment:** This is very difficult to completely safeguard against and even detect.

## Mitigations

Several mitigations exist:

**Technical Mitigations:**
- **Low-value tokens**: If the gain from stealing a specific token is below some limit, the cost of stealing it will outweigh it
- **Short-lived tokens**: Given the nature of this network, it's possible to destroy and create new tokens as part of normal state changes—effectively moving the ownership area around the network
- **Multiple tokens**: Use multiple tokens to represent a valuable asset, thus increasing the number of peers that have to be hacked or bribed. Users would then check the head-of-chain of all tokens and ensure they align

**Detection Indicators:**
- **Legal consequences**: Hacks and bribes are illegal, and law enforcement or other authorities could intervene or warn of the fact
- **Conflicting mappings**: When doing get-mapping, conflicting results should be rare after a short settlement period. Seeing different values from otherwise trusted peers could be a warning sign—wait with critical transactions or perform further investigations


# Performance

**TODO:** Performance analysis and benchmarks need to be conducted and documented.

# Example Use Cases

The general nature of the system allows it to support many different use cases, together with parallel storage networks in some cases.

## Tickets

**Process:**
1. Create a token for each ticket and assign ownership
2. Token can be transferred
3. When used, destroy the token

## Payment

**Setup:**
A trusted issuer creates a "note" with value v. The note itself is a document that lives in a parallel network (as described).

**Usage Process:**
1. **Split the note:**
   - Destroy the original token
   - In the same transaction, create 2 documents each with value on both sides
   - Each document must contain the history leading up to that point (logarithmic complexity)
   - Hash the new documents and create these new tokens in the same transaction that destroys the original

2. **Validation:**
   - Look through the history and validate that it correctly splits the value at each point
   - Check the state of recent transactions with the network

3. **Redemption:**
   - Go to the issuer and destroy a "note" together

## Identity

**Setup Process:**
1. Create an identity token—SHA of some name/ID
2. Submit a founding transaction to set the first public key hash
3. Submit a next-transaction on the token to register the real public key and a signature

**Usage:**
- The token now points to a SHA of the next public key and the previous transaction
- Fetch the previous transaction and use the visible public key to verify signatures from the identity

**Key Rotation:**
- Submit a new transaction
- This forms a new "current-next" pair

## DNS

**Concept:**
Use the fact that a token ID is 32 bytes (256 bits)—large enough to contain either an IPv4 or IPv6 address plus port, plus a random part.

**Process:**
1. Create a token as the SHA of a domain name
2. Users look up the last transaction of this token
3. Get the IP/ports of the host as some of the other tokens in the transaction

**Authentication:**
Like in the Identity case, the public keys of previous transactions can be used for signing.

## Process/Document Flow

**Use Case:**
For processes or documents that must be traceable to parties for which the ID (token) makes sense.

**Process:**
1. Register the first version
2. For each step/change:
   - Destroy the old token
   - Create the new state
3. Possibly keep a stable ID across all transactions to find the last update
