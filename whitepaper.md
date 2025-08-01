
# A Common Notary for the Internet

Using statefull Internet-services requires trusting the service-provider to keep your data safe and to not manipulate it in ways you did not agree to. Your data lives inside a blackbox owned and operated by that provider.

This hinders interoperability. There is no way to move ownership of your data somewhere else, unless its explictly buids into the product. You can not just take a copy of your data, hand it to someone else and claim that they now own or control it.

This project opens up the "service blackbox". It provides a way to split services into a "data" part and an "ownership" or "tracking" part, by offering a notary for the Internet. Data, documents or state in general, is manipulated in the service-provider system - but the notary keeps a record of owenership and states. So now you can email a document to someone and handover ownership in a way that they can trust, that they can now prove that they control that data.

Given that the core issue here is trust, sharing and making a common service available, this sort of "product" can not be build and offered by one company. It has to be shared.

This paper presents a distributed consensus protocol for token-based transactions using a federated peer-to-peer network - with proof-of-work tests for unmanaged inclusion of network peers. The goal is to build a workable, global, common Notary service for the Internet.

## Project outline

To achieve this, we need to build a fast, globably distributed consensus network. This does this by limiting consensus for specific tokens to a "neighbourhood" set of peers and by offering a public API to read the current state.

The transactions (identified by the SHA of their content) of the system does not contain "user-data" as such. User-data is regarded as toxic. The expected use-case is for users to SHA content and store it using other network-structures. In this way this network can be used for verification and tracking - vastly simplifing the responsiblity of other networks (and allowing users to store/provide data in whatever way or format is suitable for purpose).

Since tokens have no special meaning to this network - they can be created AND "destroyed" (accept no further transactions). This allows use-cases impossible/impractical in networks where tokens are e.g. mined or otherwise limited in form or supply.

The network topology generally does not distinguish between "clients" (end users) and "servers" (federated peers). But it assumes that a large set of peers are stable and available. These federated peers have to continuesly "earn their seat" (address) in the network. In this text we will use nodes to describe both types and peers to mean more nodes that participate in federation.

# Highlevel Design
Users create new transactions and query for existing transactions and mappings. If the network accept a new transaction the token-ids contained in it are added to the front of the transaction-chain for each of those tokens.

A transaction is a collection of upto 6 Tokens (each 256 bit) mapping to SHA (blake2 or 3) of signature public-keys - as well as the transaction-id of the previous transaction to update the token. Plus a few other values (more later).

One token-mapping:
[Token, SHA(public key), Previous-transaction-id] (in all 3 256 bit values/96 bytes)

IF the previous-transaction-id field is 0 - this transaction tries to create this token (first use seen by the network)
IF the SHA(public key) field is 0 - this transaction destroyes this token (no more changes are allowed by the network).
Both may be 0 in which case the token is created - but not usable in further transactions.

IF the previous-transaction-id field contains a value. This must be a valid (findable by the nodes) transaction-id. When validating transactions nodes, must retrieve all previous-linked transactions - and verify that provided signatures have a public key that hashes to the SHA(public key) field - and of cause that the signature is valid.

The SHA(public key) field can also be "-1" (and no signature for that token-update is given). In that case the transaction does not update the token - but verifies that at that time this token is mapped to the previous-transaction-id. More on that use-case later.

New nodes genereate a EC DH key-pair - and use it longterm (find out what the needed mitigation for DH is). A "256 bit node-address" is derived by applying Argon2 to the public key.

The "neighbourhood" of a token is simply defined as the distance from the token-id to node-addresses.

## API
The final system is intended to use UDP encrypted packages - using the shared-key obtained by doing DH from the Sender to the Receiver node. Encryption uses AEAD standard to secure integrety and security of each packet. All packets must be below MTU (~1500 bytes) to achieve the fastes network transfers.

The system tries to avoid network roundtrips as much as possible. So there are no handshake or multiround (single) operations.

The core system offers only these messages which in many cases can be batched in single network packages.

General message layout

    [Version-id: 16 bit] [reserved: 44 bit] [IV 196 bit]
    [Sender public key: 256 bit]
    [MAC: 256 bit]
    [encrypted content... rest of package]

### "Request type" messages:
- Get transaction. Input: Transaction id, as well as a network-address of the requesting peer.
- Get token mapping: Input: SHA(Token id), as well as a network-address of the requesting peer.
- Vote for a transaction to be added to the "front" of each contained token-chain. Input: transaction id and votes (see more later).

These types of messages must include a "ticket value". For transaction and token "get", the ticket is a SHA of a secret value kept by the sender and the token or transaction id being asked for.

For Vote the ticket is "proof" that the sender has obtained rights to submit transactions to this node (see more later) - OR 0 if the sender believes that the receiver has accepted it as a trusted peer (more later). This means that Votes will just be discared if neither of these conditions holds.

IF a peer does not know the mapping of a get-request, it *can* forward the request to a better quilified peer and add the network information to get the response back to the original requester. To support NAT'ed networks the original requester does not put any inforamtion in the network-address part. Upon seeing this a forwarding peer will put the network-address that the messages was received from in the forwarded message.
Since requests for unknown SHAs could be forwarded forever - and given the asyncronous nature of the protocol - peers only forward with some probability.

This also applies to load control. Each peer is free to discard any number of messages it deems needed.

It is the *clear intend* of the system to allow public access to issue get-messages (and responses) (best effort). But to restrict Vote messages either by PoW or by validating various forms of tickets (see concept ideas later). 

### "Reply type" messages:
Responding to Request messages:

- Transaction content. The content of a transaction PLUS signatures (EC signature pair plus public key).
- Token mappings. The mapping of a token-id to the current (latest) transaction-id to use it (as seen by this peer). Head of the token chain. Additinally the token-mapping response contains 8 "surrounding token-mappings".

For a node to accept these messages they must contain a valid ticket. Since the node has the secret to create tickets it simply test if the tickets provided is the SHA of either the transaction-id or token provided making sure it is the answer to something it has asked for. 

The nodes will update the ticket-secret every some milli secounds. Tests will be done with the current and the previous secret.

Hence: If a ticket does not match any - it is either spam or "slow response". So no infinite re-sending will be accepted either.

In one special case the ticket field is allowed to be 0 for token-mapping responses - when peers "introduce" themselfs. See more in "Peer discovery".

#### Token mapping replies
Reply to a token mapping request first requires that the receiver knows the token-id of the requested SHA. Therefor the peers store token-mappings by their SHA in a sorted key/value style database.

Further more, the "surrounding mappings" of the Token mapping message must live up to a set of criteria to counter "man-in-the-middle" peers - here is how it works:

The replying peer calculates an 8-byte SHA (blake2 or 3) of its own public-key + the token-id + the previous-transaction-id of the response. Lets call this the "signature" of the reply.

The peer then scans left and right in its token-mapping-database - sorted by SHA.
The 8 surrounding mappings are selected as the closest set of SHAs to form the "signature" with their last byte.

Example:

    Request token id 1234567. Lets say SHA(1234567) = 0x1500
    The message then contains 0x1500 (plus a ticket, plus address)

*Reply*
The peer calculates the signature 0x1122334455667788. It then looks for the point 0x1500 in its database of head-of-chain token mappings. Scanning to the left and right of this point it checks the last byte of the SHA of the token-ids stored (remeber they are ordred by SHA not token-id) - and collect the narrowest set of SHA to form the signature. Like:

    Lets say:
    SHA(4583838) = 0x19 88
    SHA(7686868) = 0x18 77
    SHA(8989858) = 0x17 66
    SHA(1525445) = 0x16 55
    SHA(0098765) = 0x14 44
    SHA(1563547) = 0x13 33
    SHA(9987647) = 0x12 22
    SHA(3775775) = 0x11 11

    The peer then puts this sequence in the reply:
    4583838 => t8
    7686868 => t7
    8989858 => t6
    1525445 => t5
    0098765 => t4
    1563547 => t3
    9987647 => t2
    3775775 => t1

    As well as the mapping of 
    1234567 => t0

And sends it to the network-address in the request-message - or the origin of the message if that field is empty.

(where t0-t8 are transaction ids)

When the receiever sees this sequence it can quickly SHA each of the token-ids and check that they "surround" the SHA of the requested token.
It can also quickly calculated the expected signature from the public-key of the sending peer together with the mapping returned, and check that the SHAs of the surrounding tokens in fact form the signature with their final bytes.

Failure to live up to any of this discards the response as spam.

Further more will the receiver look at the "width" of the response-SHA's - the distance from the fist to the last SHA.

    In the example above that "width" would be: 0x1988 - 0x1111 = 0x0877

Since token SHAs by the properties of such functions are evenly distributed the "width" is expected to reflect the generally densitity of token-SHAs in the network. 
Given some "experience" with the network, a receiver can judge the quality of the sending peer by the "width" - and discard below par responses.

Notice that the response does not have to have exactly 4 mappings on each side of the requested token-id. As long as it surronds it its valid.

A man-in-middel would not be able to get a reply from another peer (different public key) and forward the same or another mapping using those surrounding ids. The signature would not match.

It will also be timeconsuming to calculate 8 made-up token-ids to live up to the required properties (closeness of hashes and final byte values in the required order). 
Remember that the ticket times-out after only some milli seconds and that the "width" of the surrounding SHAs must be below a threshold (see below).

# Peer discovery
Token mapping request/responses not only support "clients" who wants to check the current mapping of a token - it also forms the foundation of peer discovery and network formation.

Since peers can forward messages to better quilified or less loaded peers, this enable nodes to discover previously unknown peers. But requesters need to know SHAs of tokens or transaction-ids beforehand since the protocol does not allow *wildcard* searches.

To facilitate discovery all peers participating in federations *SHOULD* create a transaction with a token matching its public-key. This token-chain may in the future serve further peer-related functionality/verifications, but currently only the first token-mapping is used. 
We will refer to these transactions as "lampposts".

## Bootstraping
All new nodes need to be seeded with a set of known peers (out-of-band). The seed must contain both network-address and public keys to be able to communicate with those nodes. Users would obtain e.g. a 5-10 such from a semi-trusted source.

If the seed peers have correctly registered "lampposts" the bootstraping node can query those from those same peers (using the public-keys as tokens). 

The public-keys of new peers are added to the process as well as token-ids and transaction-ids from valid mapping responses. As well as transactions pointing to previous transaction-ids and other token-ids contained therin etc.

If the seeds does not form a closed loop of peers this process should eventually produce responses from peers across the network, and across the spectrum. Users may also use out-of-band obtained token-ids to verify that the peers do indeed connect with the desired network.

The new node runs this process until enough peers with demonstrated network-connectivity have been obtained.

Different types of nodes may have different requirements for peers. E.g. client-type nodes will need to have set of peers for which they have tickets to submit to. Relay/query-only type of nodes may just want a large set of connected peers etc.

We will proceed to consider nodes that want to get accepted as federation-peers.

## Peer quality and PoW
As described earlier the "width" of surrounding SHAs indicate the quality of the peer sending the response. This is the foundation of the "proof-of-work" (PoW) system used to select peers by all types of nodes in the system.

SHAs by the property of such functions distribute evenly. But locally individual SHAs may have very different distances. Given the "signature" requirements the token-ids needed are very unlikely to be the exact sequience of ids available in the database - but rather to span over a certan "width". Tests show that this width very closely follow the general density of the ids in the network - across all regions of the spectrum.

This enables nodes to assume that peers that keep up and hold their share of the spectrum, will be able to produce signatures of approximatly the same width.

Each peer initiates its own requests using SHAs of token-ids or transaction-ids that it already knows about - and must never process responses for something it didn't request. To be able to reply a peer need to know the token-id of the SHA - and then be able to produce a satisfactory signature within the timelimit and of the expected width.

*We will assume that the most econnomical and rational way* to comply with this is to capture and store enough *common* token mappings in the region of the peer-address. Even advesary, malisious actors, would recognize this and either try to take over existing peers or comply with the intentions of the system - before attempting to manipulate mappings.

Irrational or faulty peers will either get messages rejected or have to spend large amount of time and energy to produce compliant responses (unlikely for faulty peers).

Thus this leads to a network of peers that wants to play be the rules for a very large majority of the tokens - so it will be generally useful for users of those tokens. We will later look into how to fight issues with manipulation of specific tokens.

### How to obtain initial mappings
New peers can obtain the needed mappings by getting token mapppings around its own address. Upon receiving a response it should by default record all the mapping in the response in its database. It should then issue request for some of the new token-ids and continue in this fashion until it gets to a sufficient density.

When it gets responses for mappings that it has already recorded it should compare the mapping. If there is a conflict it should run a voting-round (described below) for the true mapping and select the "winner". In addition it should get all head-of-chain transactions and store those to enable it to validate new transactions eventually.

The general principle for manageing the token-mapping database is to store all response-mappings. When conflicts are observed, run voting. Continuesly run a "reaper" process that scans over the token-mapping database and prune elements with a probability based on how distant the token-id is from the address of the peer - and how long time since the mapping was created.

If the database gets "thin" the bootstapping process can start again to obtain mappings across the spectrum.

### Stall or false mappings
A peer may know enough common token-ids to respond to requests, but either have out-of-date mapping or try to manipulate mapping of tokens to other transaction-ids than the "true" head-of-chain. This would enable "double-spending" and other unwanted scenarios.

To fight these types of peers, we will collect multiple responses for each request and only accept majority mappings.

Peers must continuously run a peer-collecting and validating process after the initial bootstrapping. It *should* do as follows:

Genereate a random 256 bit number and find the closest token-id in the token-mapping database and request mappings for that. The request should be send to known peers with addresses close to the token-ids as well as randomly selected peers - e.g. selecting from peers recently heard from etc. (piggyback other traffic).

Upon receiving responses a peer must collect and compare the mappings. And when a majority forms (+2 for a specifc) mapping - pick from the set of peers voting for that value, the one with the address *closest* to the requested token-id. Also fetch and store (if not already) the transaction being pointed to. At the same time flip any trusted peers that voted differently into "prospects" (TODO check if this should really be done / consequences).
If no majority has formed within some timelimit, discard the voting-state.

Given that receivers of request could respond from multiple peers to shortcut the process or manipulate the mapping, the requester will use the ticket system to seperate responses into "secret channels". This is how it works:

Instead of just having one secret-value to create tickets for requests - the peer maintains a number of them. Responses will then be tested with those to identify the channels - and voting collect the last message per channel (regardless of the sending peer). Since tickets are opaque to receivers they will have no way of detecting that - and it will not gain any advantage by responding to the same request from multiple peers. Even if the receiver sees 2 different tickets for the same request id, it could still be the same channel just that the timer rolled the secret.

## Introducing new peers to the system
It follows from the above that a completly new peer will have no chance of getting requests, without help - and hence of getting accepted into the network. For that reason peers can send token-mapping responses with a 0-ticket. This signals that its an "introduction" messages.

The main elements of the token-mapping message is the same. But the signature is calculated differently. In this case its the 8 byte SHA of the public-key of the sender, the public-key of the receiver and finally the head-of-chain transaction-id of the "lamppost" as reported in the message (TODO plus the IV of the message?). This ensures that a new peer have to present different mappings to different receivers.

As with messages in general there is no garantee that receiving peers will process such messages.

If the message is processed, all the rules outlined earlier are checked - and if the new peer abide, its added as a "prospect" peer (and its network address is recorded). Prospects do not get Vote-messages or forwarded token-mapping request from the network (but do receive get-transaction), but they have a chance of getting request-messages from the PoW process - and thus of getting accepted as a trusted peer.
Peers for which "introduction" messages have been send are put in a "pending" state, such that if an "introduction" comes back its known that the connection is usable.

So a new peer must keep up with the network until it eventually starts seeing request messages from other peers, it will be in a "replicator" state until it can rely on Vote messages to keep its state up-to-date.

Keeping up works much like the PoW process. Only in this case the random number to lookup is in a smaller range around the address of the peer. And the frequency of requests should be higher.

## Trusted peers
Nodes maintains a set of *trusted peers* across the spectrum - but with increasing density closer to its own address. This facilitate more effecient routing and voting by focusing traffic and keeping neighbours aware of eachother.

Trusted peers can submit transactions *without tickets* and vote for its neighbourhood. This is the main benefit of (or *payment* for) becoming a trusted peer of the network. They pay for the ability to submit new transactions by offering storage for a slice of the data in the network.

When a peer is accepted an "introduction" message is send back. Peers will thus expect to be trusted by the sender upon receiving such a message. Peers periodically sends "introduction" messages to its set of trusted peers. Upon receiving such a message from an already trusted or prospect peer we just updates the last-heard-from state. 
If no such message has arrived for a while from a trusted peer, we should assume that either its gone or has removed us from its set of peers. In those cases a fresh "introduction" message can be send and the peer put into "pending" mode.

Valid responses to get-requests from all trusted and prospect peers add to a quality-score and update last-heard-from.

The aim is motivate operators to continuesly create new peers and try to get them added as prospects - and evetully upgrade to trusted peers. This ensures that an opeartor maintains the ability to submit transactions even as trusted peers from time to time will "loose" to other peers.

Already operating a trusted peer also provides the ability to respond to request using new peers.

Note that an operator can share some parts of the storage between its peers and thus lower the marginal cost.

## Selecting trusted peers
Its not desirable to have peers use or know all other peers in the entire network - even if this is the most effieceint routing configuration. The reason is to keep message routing in the system less predictable and thus safer. By the laws of physics some peers will have longer network latencies between them and networks may fail or become unavailble. To work around this its better for many overlapping, random graphs to form between the available peers.

Secondly the aim of this system is to accumodate global networks of millions of peers - so it might also be impractical in the longer term to keep state for each (even if the required state per peer is rather limited compared to modern memory sizes).

When map-voting rounds are held and won - the peer will check if any of the winning peers are unknown, and if the area of the peer(s) are underpopulated - then send "introduction" messages to those and store them as pending.

When a pending peer returns with an "introduction" message - its turned into a trusted peer.

From time to time a node should prune trusted, pending and prospect peers that it has not heard from in a while or if the area they occupy is too densly populated. At the same time, if it notices thinly populated areas with prospect peers - it should send "introduction" messages and turn them into trusted peers. If it has more prospects to choose from, select the one with the latest heard-from.

# Consensus
We have already touched upon how peers collect mapping-responses, store them and vote if they see conflicting mappings. Tests show that getting +2 vote on equal mappings in a randomized network of peers indicate that this mapping dominates by far the neighbourhood of the token.

The same principle is applied when submitting new transactions to the network. With some additions.

When a node wants to submit a new transaction to the network it picks peers that it *believe* has it registered as either a trusted peer or it will accept the ticket provided. So the initial peer(s) to receive such a transaction-id will not know the content of the transaction (incl signatures) - so they will immediatly request the transaction-content from the submitting node (if Loadbalancing permits it).

Once the actual transaction is received the peer now knows which token-mappings are affected. So for each token it Votes/forward the transaction to peers in the areas around those (reusing if they overlap). It also indicates with the Vote if it believes the mapping represents a valid step on the token-chain. That would be: The previous-transaction-id matches what is recorded in its database - or that the timestamp/counter of the transaction (not descussed before) is less than the one recorded (its an out-of-order update).

If a peer has already commited the submitted transaction - it should imediatly repsond with all-possitive vote to the caller (and indicate that it should not receive further votes for this).

Peers also submits the transaction to the area around the transaction-id itself. This way the transaction will be findable without knowing any token-ids from it AND the process gets a pseudo-random witness to also distribute the transaction to the network.

Now much like with the voting of mapping-responses, the peer keeps a log of votes from trusted peers. Newer votes replace older ones.

Parallel to the process of posting votes the peer collects all previous transactions - and validate that the timestamp/counter in the new transaction is larger than anyone recorded in each of those (and less than 2 hours into the future) AND then it verifies all signatures. If any of these checks does not hold - the peer will activly start to vote "no" for the transaction (and block it from commiting).

IF all tokens get a +2 vote from their respective neithbourhoods - the peer commits the transaction. The actual "witness" area votes does not count as such (not expected to have up-to-date mappings) but we track that we get votes back from at least 2 peers in that area.

When commiting a transaction the token-mapping database is updated. If the timestamp/counter of the previous recording is greater than the new transaction this update will be regarded as out-of-order (no change to that token-mapping is done). Mappings created from just recording mapping responses with unknown transactions has 0-timestamp (so will always be overriden). Mappings for which conflict voting has run will also have fetched the transaction and hence have a timestamp/counter from that.

## How can this work?
The peers maintains a set of trusted peers across the spectrum - but with increasing density closer to its own address. This means that peers in the same area will tend to trust eachother and thus be sending votes between them - and eventually decide the majority. Nodes not in an area will eventually get "commited" messages in response from the core peers of the area - or no answer if they can not agree on it (in which case transactions time out). 

So a commitable transaction will propagate that state back through the peers leading to the origin. As demonstrated by simulation.

# Load balancing
Aim is to limit some nodes from pumping unporportinal many transactions into the system. Therefor peers keep a count of new transactions initiated by its trusted peers - and employ other load-balancing techniques for "client type" nodes. If a sender exeeds its limit the transaction is discarded.

With some interval, the counters are reset. And the limits ajusted to the desired overall load for the system.

# Ticket schemes for clients
The majority of the nodes are expected to be "clients" with no ability or desire of participating in federation. Such nodes can freely query for transactions and mappings but to submit transactions they need tickets to one or more peers.

We envision a userbase where service-providers offer specialized apps - and in return for using the network, they operate a number of peers for which they control access. They are free to implement the type of ticket system appropriate for their useccase. 

Since a ticket is just an opaque 256 bit value, different schemes are possible. Here follows some for inspiration:

## API key
Trusted clients are issued an API key. Potentially used together with the public-key of the client-node (if that is stable). A variation of this is to mainly use the public-key - but leave something in the ticket field to distinquish it from peer-traffic.

This of cause makes it possible for the operator to link all transactions back to the user. In many such scenarios this is anyway possible given a parallel data-system.

## Blinded tickets
In some cases its not desireable for the operator to be able to track the transactions of indivial users. In such cases an operator may use an out-of-band (web/app) to cryptograhically "blind" tickets together with users. E.g.

A user generates an ephemeral EC key, B. Then takes the transaction-id it wants to process without detection and do a scalar mult with B, call it H.
Then going to the operator site with H the user pays for one transaction, the operator applies its own secret EC key O to H yielding S1.
The user can now apply the inverse of B to remove it - leaving S2.

When submitting the transaction to an operator controlled peer using S2 as ticket, the system applies the inverse of O and checks that this is equal to the transaction-id, without being able to link S2 or the transaction-id to H.

Instead of a single transaction this could also be done for eg. a SHA of a users short-lived public-key and a time-range-indicator-value e.g. todays date. Upon recieving such a ticket the peer inverse the ticket and checks that it matches the client public-key hashed with the current time-range-indicator. This would of cause allow any number of transactions during that time-range.

Other variations of this scheme can lock to other such hash-commitments.

## PoW
In the same spirit. An open and permission-less system is also possible. Here a client could be be asked to provide a ticket which hashed together with the transaction-id would result in a value with some property, e.g. starts with some number of 0-bits etc.

Its easy to test for the peer-system - but would be timeconsuming up to some level for users to generate. So this sort of system would not allow the operator to profit from the transaction - but could rate limit an otherwice open gateway.

# Keeping mappings and transactions available
The system spreads transactions and mappings to a number of peers. 

New peers that wants to enter the network runs in "replicator" mode - standby to take over failing peers.

The general contract with users is that the main priority is to keep the head-of-chains available. But that some level of history can be expected. Usescases should try to work with this - so providing e.g. history of documents and hashing them to new tokens such that head-of-chain also proves that previous versions existed as stated.

TODO its an open question to test and qualify how well this will work or if further processes are needed to keep availablity to desired levels.

# Security
Since transaction chains are hashed like blockchains and transactions are signed by commen crypto schemes - isolated client behaviour does not seem to be the biggest threat to the system.

Tokens in this system are also not valuable by themselfs (unlike other blockchain-style networks). So it does not make sense to "steal" random tokens without knowing what they are. And since related data is not in the system, it does not reveal such properties by itself.

Hence the main threat to the system is manipulation of individual tokens with some known (to the manipulators) quality. Collution between malisious clients and operators poses the greatest threat.

To take over control of individual token-mappings, an operator needs to contol the neighborhood of such a token. During the scam the peers could be made to report false mappings (maybe even just to a selected user/group).

This can be done in two basic ways:

1. Create new peers for the neighborhood.
2. Take over existing peers and keep them running while the manipulation takes place.

Ad.1 The adversary would have to find a suffient number of key-pairs where the public-key has an Argo2 hash in the desired range. This will be a very time and ressource consuming activity also given that maybe 10-20 such pairs are needed to form a majority.

The new peers would then have to out-compete the existing peers in the area - going through the whole process outlined above.

All-in-all this is expected to be a very expensive endvour.

Ad.2 The adversary seizes control of (hack or bribe) a majority set in the desired area. 

This is very difficult to completly safegard against and even detect.

But some mitigations exisits.

- Use low-value tokens. If the gain from stealing a specific token is below some limit the cost of stealing it will out-weight it.
- Short-lived tokens. Given the nature of this network its possible to destroy and create new tokens as part of normal state-changes - effectivly moving the ownership area around the network.
- Use multiple tokens to represent a valuable asset - thus increase the number of peers that has to be hacked or bribed. Users would then check head-of-chain of all tokens and make sure they align.

There might also be signs like:

- Hacks and bribe are illegal and lawinforcement or other autherteries could intervene or warn of the fact.
- Conflicting mappings when doing get-mapping should be rare after a short settlement period. So seeing different values from otherwice trusted peers could be a warning-sign - so wait with critical transactions or perform further investigations etc.


# Performance
TODO

# Example usecases
The general nature of the system allows it to support many different usecases together with in some cases parallel storage-networks.

## Tickets
Create a token for each ticket and give ownership. Can be transfered. When used destroy the token.

## Payment
A trusted issuer creates a "note" with value v. The note itself is a document that lives in a parallel network (as described).

To use a note split it. Destroy the original token - and in the same transaction create 2 documents each with the value on both sides. Each document must contain the history leading up to that point (so no more than logaritmic). Hash the new documents and in the same transaction that destroyes the original create these new tokens.

To validate a "note" look through the history and validate that it correctly splits the value at each point - and checking the state of recent transactions with the network.

To redeem the value of a split document - go to the issuer and destroy a "note" together.

## Identity
Create an identity token - SHA of some name/id - and submit a founding transaction to set the first public-key hash. Then submit a next-transaction on the token to register the real public key and a signature. 

Now the token points to a SHA of the next public key and the previous transaction. Fetch the previous transaction and use the visible public-key to verify signatures from the identity.

To roll the key - submit a new transaction. This forms a new "current-next" pair.

## DNS
Use the fact that a token-id is 32 bytes (256 bits). This is large enough to contain either an IP4 or IP6 address plus port, plus a random-part.

Then create a token as the e.g. SHA of a domain-name. Users lookup the last transaction of this token and gets the IP/Ports of the host as some of the other tokens in the transaction.

Like in the Identity case the public keys of previous transactions can be used for signing.

## Process/Document flow
For some process or document that must be tracable to parties for which the id (token) makes sense. Register the first version.
Then for each step/change - destroy the old token - and create the new state.

Possibly keeping a stable id across all transactions to find the last update.
