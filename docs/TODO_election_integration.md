# Election System Integration TODO

**Status**: Ready for Integration
**Last Updated**: 2025-01-11 (V2.0 API)
**Implementation**: ✅ Complete (37 tests passing)
**Related**: [Peer Election Design Document](./peer_election_design.md)

---

## Overview

This document outlines the work needed to integrate the peer election system (completed in `ec_proof_of_storage.rs`) into the main peer management system (`ec_peers.rs`).

**What's Complete**: ✅
- Core election logic in [src/ec_proof_of_storage.rs](../src/ec_proof_of_storage.rs)
- 37 comprehensive tests validating all scenarios
- **Signature verification** (cryptographic proof of state)
- **Blocked peer tracking** (anti-gaming)
- **Per-election secrets** (isolation and forward secrecy)
- Split-brain detection
- Clean, simplified API

**What's Needed**: 🔨
- Integration into [src/ec_peers.rs](../src/ec_peers.rs) as the "ElectionManager"
- Message routing (Query/Answer/Referral with tickets)
- User-controlled timing (spawn channels, check winner, timeouts)
- Peer lifecycle management (continuous re-election)

---

## Available API (V2.0 from `ec_proof_of_storage`)

### Core Types

```rust
pub struct PeerElection {
    // Main election coordinator
    // Manages channels, responses, signature verification, blocked peers
    // Generates per-election secret automatically
}

pub enum WinnerResult {
    /// Single clear winner found
    Single {
        winner: PeerId,
        cluster: ConsensusCluster,
        cluster_signatures: Vec<(PeerId, TokenSignature)>,
    },

    /// Split-brain: two competing clusters
    SplitBrain {
        cluster1: ConsensusCluster,
        winner1: PeerId,
        signatures1: Vec<(PeerId, TokenSignature)>,
        cluster2: ConsensusCluster,
        winner2: PeerId,
        signatures2: Vec<(PeerId, TokenSignature)>,
    },

    /// No consensus yet (not enough responses or agreement)
    NoConsensus,
}

pub struct ElectionConfig {
    pub consensus_threshold: usize,     // Default: 8/10 mappings
    pub min_cluster_size: usize,        // Default: 2 peers
    pub max_channels: usize,            // Default: 10
    pub majority_threshold: f64,        // Default: 0.6 (60%)
}

pub enum ElectionError {
    UnknownTicket, WrongToken, DuplicateResponse, ChannelAlreadyExists,
    MaxChannelsReached, ChannelBlocked, SignatureVerificationFailed, BlockedPeer,
}
```

### Core Functions (V2.0)

```rust
// Calculate ring distance between two IDs
pub fn ring_distance(a: u64, b: u64) -> u64

// PeerElection methods
impl PeerElection {
    // Create new election (generates random secret automatically)
    pub fn new(
        challenge_token: TokenId,
        my_peer_id: PeerId,          // NEW: needed for signature verification
        config: ElectionConfig
    ) -> Self

    // Create channel (no time parameter)
    pub fn create_channel(&mut self, first_hop: PeerId)
        -> Result<MessageTicket, ElectionError>

    // Handle Answer (NEW - with signature verification)
    pub fn handle_answer(
        &mut self,
        ticket: MessageTicket,
        answer: &TokenMapping,
        signature_mappings: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
        responder_peer: PeerId
    ) -> Result<(), ElectionError>

    // Handle Referral (NEW - for forwarding)
    pub fn handle_referral(
        &mut self,
        ticket: MessageTicket,
        token_challenge: TokenId,
        suggested_peers: [PeerId; 2],
        responder_peer: PeerId
    ) -> Result<PeerId, ElectionError>

    // Check for winner (NEW - replaces try_elect_winner)
    pub fn check_for_winner(&self) -> WinnerResult

    // Query state
    pub fn valid_response_count(&self) -> usize
    pub fn can_create_channel(&self) -> bool
    pub fn challenge_token(&self) -> TokenId
    pub fn channel_count(&self) -> usize
    pub fn blocked_peer_count(&self) -> usize
}
```

**Removed from V1.0**:
- ~~`initialize_election_secret()`~~ - No longer needed (per-election secrets)
- ~~`submit_response()`~~ - Replaced by `handle_answer()` with verification
- ~~`try_elect_winner()`~~ - Replaced by `check_for_winner()`
- ~~`should_check_consensus()`~~ - User controls timing
- ~~`is_expired()`~~ - User implements timeout logic
- ~~`ElectionState`~~ - No internal state tracking

---

## Integration Architecture

### High-Level Design

```
┌─────────────────────────────────────────────────────────────┐
│                         EcNode                              │
│  - Calls EcPeers methods                                    │
│  - Routes messages via handle_message()                     │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│                        EcPeers                              │
│  "Election Manager"                                         │
│                                                             │
│  Active Elections:                                          │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ HashMap<ChallengeToken, ElectionState>               │  │
│  │  - Running elections                                  │  │
│  │  - Spawn/manage channels                              │  │
│  │  - Route Query/Answer messages                        │  │
│  │  - Handle split-brain responses                       │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  Elected Peers:                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Vec<ElectedPeer> or similar                          │  │
│  │  - Winners from successful elections                  │  │
│  │  - Maintained connections                             │  │
│  │  - Periodic re-election                               │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│                  PeerElection (library)                     │
│  - Consensus detection                                      │
│  - Split-brain resolution                                   │
│  - Winner selection                                         │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation Phases

### Phase 1: Basic Integration (MVP)

**Goal**: Single election runs to completion, winner selected.

#### Tasks

- [ ] **1.1 Add election state to EcPeers**
  ```rust
  struct EcPeers {
      // ... existing fields ...

      active_elections: HashMap<TokenId, ActiveElection>,
      election_config: ElectionConfig,
      election_secret_initialized: bool,
  }

  struct ActiveElection {
      election: PeerElection,
      phase: ElectionPhase,  // Collection, Election, Complete
  }

  enum ElectionPhase {
      Collection,    // Spawning channels, collecting responses
      Election,      // Checking consensus, handling split-brain
      Complete(ElectionResult),
  }
  ```

- [ ] **1.2 Implement start_election()** (V2.0)
  ```rust
  impl EcPeers {
      /// Start a new peer election for a challenge token
      pub fn start_election(&mut self, challenge_token: TokenId) {
          let config = self.election_config.clone();
          let election = PeerElection::new(
              challenge_token,
              self.my_peer_id,  // NEW: needed for signature verification
              config
          );

          self.active_elections.insert(challenge_token, election);

          // Spawn initial channels (3 by default)
          self.spawn_election_channels(challenge_token, 3);
      }
  }
  ```

**Note**: No `initialize_election_secret()` needed! Each election generates its own secret automatically.

- [ ] **1.3 Implement spawn_election_channels()** (V2.0)
  ```rust
  impl EcPeers {
      /// Spawn N channels for an election (simplified)
      fn spawn_election_channels(&mut self, challenge_token: TokenId, count: usize) {
          let Some(election) = self.active_elections.get_mut(&challenge_token) else {
              return;
          };

          for _ in 0..count {
              if !election.can_create_channel() {
                  break; // Hit max_channels limit
              }

              // Pick random first-hop peer (prefer diverse locations on ring)
              let first_hop = self.pick_random_peer();

              // Create channel and get ticket (may fail if channel exists)
              match election.create_channel(first_hop) {
                  Ok(ticket) => {
                      // Send Query message with ticket
                      self.send_query_message(challenge_token, first_hop, ticket);
                  }
                  Err(ElectionError::ChannelAlreadyExists) => {
                      // Already have channel for this peer, skip
                      continue;
                  }
                  Err(e) => {
                      eprintln!("Failed to create channel: {:?}", e);
                  }
              }
          }
      }

      fn pick_random_peer(&self) -> PeerId {
          // TODO: Implement peer selection
          // - Pick from self.peers randomly
          // - Prefer peers far apart on ring for diversity
          // - Avoid recently used peers in this election
          unimplemented!("Need peer selection strategy")
      }

      fn send_query_message(&self, token: TokenId, first_hop: PeerId, ticket: MessageTicket) {
          // TODO: Route Query message to first_hop
          // Query should include: token_id, ticket
          // This will need integration with EcNode's message routing
          unimplemented!("Need message routing integration")
      }
  }
  ```

- [ ] **1.4 Implement handle_answer() integration** (V2.0)
  ```rust
  impl EcPeers {
      /// Handle incoming Answer message (called from EcNode) - V2.0
      pub fn handle_answer(
          &mut self,
          answer: &TokenMapping,
          signature: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
          ticket: MessageTicket,
          peer_id: PeerId
      ) {
          // Find election by token_id from answer
          let Some(election) = self.active_elections.get_mut(&answer.id) else {
              // No active election for this token - ignore or log
              return;
          };

          // NEW: handle_answer now does signature verification automatically!
          match election.handle_answer(ticket, answer, signature, peer_id) {
              Ok(()) => {
                  // Response accepted and signature verified ✓
                  log::debug!("Valid answer from peer {}", peer_id);
              }
              Err(ElectionError::DuplicateResponse) => {
                  // RED FLAG: Peer caught gaming (channel+peer blocked)
                  log::warn!("Duplicate response detected from peer {}", peer_id);
              }
              Err(ElectionError::SignatureVerificationFailed) => {
                  // RED FLAG: Invalid signature (peer blocked)
                  log::warn!("Invalid signature from peer {}", peer_id);
              }
              Err(ElectionError::WrongToken) => {
                  // Answer for different token
                  log::warn!("Answer for wrong token from peer {}", peer_id);
              }
              Err(ElectionError::BlockedPeer) => {
                  // Response from previously blocked peer
                  log::debug!("Ignoring response from blocked peer {}", peer_id);
              }
              Err(e) => {
                  log::warn!("Error handling answer: {:?}", e);
              }
          }
      }
  }
  ```

- [ ] **1.5 Implement handle_referral() integration** (V2.0 - NEW)
  ```rust
  impl EcPeers {
      /// Handle incoming Referral message - V2.0
      pub fn handle_referral(
          &mut self,
          token: TokenId,
          ticket: MessageTicket,
          suggested_peers: [PeerId; 2],
          peer_id: PeerId
      ) {
          let Some(election) = self.active_elections.get_mut(&token) else {
              return; // No active election
          };

          // NEW: handle_referral destroys channel and returns suggested peer
          match election.handle_referral(ticket, token, suggested_peers, peer_id) {
              Ok(suggested_peer) => {
                  // Old channel destroyed, create new one to suggested peer
                  match election.create_channel(suggested_peer) {
                      Ok(new_ticket) => {
                          self.send_query_message(token, suggested_peer, new_ticket);
                          log::debug!("Referral: forwarding to peer {}", suggested_peer);
                      }
                      Err(e) => {
                          log::warn!("Failed to create channel after referral: {:?}", e);
                      }
                  }
              }
              Err(ElectionError::WrongToken) => {
                  log::warn!("Referral for wrong token");
              }
              Err(e) => {
                  log::warn!("Error handling referral: {:?}", e);
              }
          }
      }
  }
  ```

- [ ] **1.6 Implement election progression** (V2.0 - User Controls)
  ```rust
  impl EcPeers {
      /// Called periodically (from EcNode tick or similar) - V2.0
      pub fn process_elections(&mut self, current_time: EcTime) {
          let mut completed = Vec::new();
          let mut to_resolve: Vec<(TokenId, usize)> = Vec::new();

          for (token, election) in &mut self.active_elections {
              // User controls when to check for winner
              // Example: Check after minimum collection time
              let elapsed = current_time.saturating_sub(election.started_at);

              if elapsed >= MIN_COLLECTION_TIME {
                  // Try to elect winner
                  match election.check_for_winner() {
                      WinnerResult::Single { winner, cluster, .. } => {
                          // Success! Election complete
                          log::info!("Election winner: peer {} (cluster size: {})",
                                    winner, cluster.members.len());
                          self.handle_election_success(*token, winner, &cluster);
                          completed.push(*token);
                      }

                      WinnerResult::SplitBrain { cluster1, cluster2, .. } => {
                          // User decides: spawn more channels or accept
                          log::warn!("Split-brain: cluster1={}, cluster2={}",
                                    cluster1.members.len(), cluster2.members.len());

                          if elapsed < TIMEOUT && election.can_create_channel() {
                              // Strategy: Spawn more channels to resolve
                              let needed = 2; // or calculate based on cluster sizes
                              to_resolve.push((*token, needed));
                          } else {
                              // Accept strongest cluster or abandon
                              // (For MVP, just abandon)
                              log::warn!("Abandoning split-brain election for token {}", token);
                              completed.push(*token);
                          }
                      }

                      WinnerResult::NoConsensus => {
                          // Not enough responses yet
                          if elapsed >= TIMEOUT {
                              // Give up
                              log::warn!("Election timeout for token {}", token);
                              completed.push(*token);
                          }
                      }
                  }
              }
          }

          // Spawn more channels for split-brain elections
          for (token, count) in to_resolve {
              self.spawn_election_channels(token, count);
          }

          // Cleanup completed elections
          for token in completed {
              self.active_elections.remove(&token);
          }
      }

      fn handle_election_success(&mut self, token: TokenId, winner: PeerId,
                                  cluster: &ConsensusCluster) {
          // TODO: Add winner to peer list
          // TODO: Establish connection if not already connected
          // TODO: Record election metrics
          log::info!("Connected to elected peer {} for token {}", winner, token);
      }
  }
  ```

**Note**: User implements timing logic (MIN_COLLECTION_TIME, TIMEOUT).No more `ElectionPhase` enum!

**Testing Phase 1**:
- Single election with agreeing responses → winner selected
- Single election with split-brain → resolution attempted → winner found
- Single election timeout without consensus → failure handled

---

### Phase 2: Message Routing

**Goal**: Properly route Query/Answer messages through the network.

#### Tasks

- [ ] **2.1 Update Message enum (if needed)**
  ```rust
  // In ec_interface.rs - might already have this
  pub enum Message {
      // ... existing variants ...

      Query {
          token: TokenId,
          ticket: MessageTicket,
          // TTL or hop count to prevent infinite loops?
      },

      Answer {
          answer: TokenMapping,
          signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
          ticket: MessageTicket,
      },
  }
  ```

- [ ] **2.2 Implement Query forwarding**
  ```rust
  impl EcNode {
      fn handle_query(&mut self, token: TokenId, ticket: MessageTicket,
                      from_peer: PeerId) {
          // Check if we should respond or forward

          // Option A: Forward to peers closer to token on ring
          let closer_peers = self.peers.find_closer_peers(token, self.peer_id);
          if let Some(next_hop) = closer_peers.first() {
              // Forward Query
              self.send_message(*next_hop, Message::Query { token, ticket });
          }

          // Option B: Respond with our signature
          if let Some(signature) = self.generate_signature(token) {
              // Send Answer back to from_peer
              self.send_message(from_peer, Message::Answer {
                  answer: signature.answer,
                  signature: signature.signature,
                  ticket,
              });
          }
      }
  }
  ```

- [ ] **2.3 Decide on Query forwarding strategy**
  - **Option A**: Forward to closest peer to token (greedy routing)
  - **Option B**: Broadcast to neighbors, first to respond wins
  - **Option C**: Random walk with TTL
  - **Option D**: Hybrid - try local, then forward

  **Decision needed**: Which strategy provides best balance of:
  - Response diversity (avoid same peer answering multiple channels)
  - Response speed (find responders quickly)
  - Network load (minimize message overhead)

- [ ] **2.4 Implement Answer routing back to challenger**
  - Answer should include ticket
  - Route back through first-hop peer (who knows the challenger)
  - Or: include return path in Query message

**Open Questions**:
1. Should Query messages include TTL/hop-count?
2. How to prevent Query loops?
3. Should peers cache recent tickets to detect duplicates?
4. How to handle Answer from non-first-hop peer? (if forwarding changes route)

---

### Phase 3: Continuous Re-Election

**Goal**: Maintain peer quality through periodic elections.

#### Tasks

- [ ] **3.1 Add election scheduling**
  ```rust
  struct EcPeers {
      // ... existing ...

      next_election_time: EcTime,
      election_interval: u64,  // e.g., 30000ms = 30 seconds
      elected_peers: HashMap<PeerId, ElectedPeerInfo>,
  }

  struct ElectedPeerInfo {
      peer_id: PeerId,
      elected_at: EcTime,
      last_election_time: EcTime,
      election_count: usize,
      challenge_token: TokenId,
  }
  ```

- [ ] **3.2 Implement periodic trigger**
  ```rust
  impl EcPeers {
      pub fn process_elections(&mut self, current_time: EcTime) {
          // ... existing election processing ...

          // Check if time for new election
          if current_time >= self.next_election_time {
              self.trigger_next_election(current_time);
              self.next_election_time = current_time + self.election_interval;
          }
      }

      fn trigger_next_election(&mut self, current_time: EcTime) {
          // Pick random token to challenge
          let challenge_token = self.pick_challenge_token();

          // Start election
          self.start_election(challenge_token, current_time);
      }

      fn pick_challenge_token(&self) -> TokenId {
          // Strategy: Random token from address space
          // Or: Token from area we want more peers in
          // Or: Token to verify existing peer
          rand::random()
      }
  }
  ```

- [ ] **3.3 Implement peer replacement strategy**
  ```rust
  impl EcPeers {
      fn handle_election_success(&mut self, token: TokenId, result: ElectionResult,
                                  time: EcTime) {
          let winner = result.winner;

          // Check if winner is already in peer list
          if self.elected_peers.contains_key(&winner) {
              // Update info
              self.elected_peers.get_mut(&winner).unwrap().last_election_time = time;
              self.elected_peers.get_mut(&winner).unwrap().election_count += 1;
          } else {
              // New peer - add to list
              self.elected_peers.insert(winner, ElectedPeerInfo {
                  peer_id: winner,
                  elected_at: time,
                  last_election_time: time,
                  election_count: 1,
                  challenge_token: token,
              });

              // If peer list full, remove oldest/worst peer
              if self.elected_peers.len() > self.max_peers {
                  self.evict_worst_peer();
              }
          }
      }

      fn evict_worst_peer(&mut self) {
          // Strategy: Remove peer with:
          // - Longest time since last successful election
          // - Lowest election_count
          // - Or: Peer furthest from areas we need coverage

          // TODO: Implement eviction strategy
      }
  }
  ```

- [ ] **3.4 Add re-election for existing peers**
  ```rust
  impl EcPeers {
      fn trigger_next_election(&mut self, current_time: EcTime) {
          // 50% of elections: verify existing peer
          // 50% of elections: discover new peer

          if rand::random::<bool>() && !self.elected_peers.is_empty() {
              // Re-elect existing peer
              let peer = self.pick_peer_for_reelection();
              let token = self.pick_token_near_peer(peer);
              self.start_election(token, current_time);
          } else {
              // Discover new peer
              let token = self.pick_challenge_token();
              self.start_election(token, current_time);
          }
      }

      fn pick_peer_for_reelection(&self) -> PeerId {
          // Pick peer that hasn't been verified recently
          self.elected_peers.values()
              .min_by_key(|p| p.last_election_time)
              .map(|p| p.peer_id)
              .unwrap()
      }

      fn pick_token_near_peer(&self, peer: PeerId) -> TokenId {
          // Pick token close to peer on ring
          // So we expect this peer to respond/win
          peer + rand::random::<u64>() % 1000
      }
  }
  ```

**Testing Phase 3**:
- Multiple elections run over time
- Peers get re-elected
- Poor-performing peers get evicted
- Peer list maintains diversity

---

### Phase 4: Advanced Features (Optional)

#### Tasks

- [ ] **4.1 Ring coverage metrics**
  - Track which areas of ring have good peer coverage
  - Bias elections toward under-covered areas

- [ ] **4.2 Peer reputation tracking**
  - Track election participation, agreement rates
  - Prefer high-reputation peers

- [ ] **4.3 Split-brain alerting**
  - If multiple elections detect split-brain, alert operator
  - Could indicate network partition

- [ ] **4.4 Election metrics**
  - Success rate, average time to completion
  - Split-brain frequency
  - Channel utilization

- [ ] **4.5 Adaptive parameters**
  - Adjust consensus_threshold based on network health
  - Adjust election_interval based on peer stability

---

## Open Design Questions

### Q1: Query Forwarding Strategy

**Question**: How should Query messages be routed through the network?

**Options**:
1. **Greedy forwarding** - Forward to peer closest to token
   - Pro: Fast convergence
   - Con: May hit same responders from different channels

2. **Random walk** - Forward randomly with TTL
   - Pro: Good diversity
   - Con: Slower, may not find anyone

3. **Broadcast to neighbors** - Send to all/some neighbors
   - Pro: Fast, diverse responses
   - Con: High message overhead

4. **Try local, forward if fail** - Check own storage first
   - Pro: Efficient if local hit
   - Con: Adds latency if forward needed

**Recommendation**: Start with Option 1 (greedy), measure diversity in practice.

### Q2: Ticket Lookup Efficiency

**Question**: How to efficiently map ticket → election?

**Options**:
1. Linear search through active_elections
   - Simple but O(n)

2. Maintain separate HashMap<MessageTicket, TokenId>
   - Fast O(1) lookup
   - Extra memory and bookkeeping

3. Store tickets in a way that encodes the challenge_token
   - Would require changing ticket generation

**Recommendation**: Option 2 - small memory cost for significant speed gain.

### Q3: Peer Selection for Channels

**Question**: How to pick first-hop peers for channels?

**Options**:
1. **Random from peer list**
   - Simple, good diversity

2. **Prefer distant peers** (on ring)
   - Better geographic/topological diversity

3. **Avoid recently used**
   - Prevent same peer appearing in multiple channels

4. **Weighted by reputation**
   - Prefer reliable peers

**Recommendation**: Hybrid - Random from distant peers, excluding recently used.

### Q4: Re-Election Frequency

**Question**: How often should re-elections run?

**Considerations**:
- Too frequent: High message overhead, wasted resources
- Too infrequent: Peers can drift or go bad without detection
- Network churn rate
- Cost per election (~5 seconds, 3-10 messages)

**Analysis**:
```
With 30s interval:
- 2 elections/minute
- 120 elections/hour
- ~360-1200 messages/hour

With 60s interval:
- 1 election/minute
- 60 elections/hour
- ~180-600 messages/hour
```

**Recommendation**: Start with 60s, adjust based on metrics.

### Q5: Peer List Size

**Question**: How many elected peers should we maintain?

**Considerations**:
- Need enough for redundancy and ring coverage
- Too many: Maintenance overhead, diminishing returns
- Related to network size and token space distribution

**Analysis** (for 256-bit ring):
- 10 peers: Large gaps, limited redundancy
- 50 peers: Good coverage, manageable
- 100+ peers: Excellent coverage, high maintenance

**Recommendation**: Configurable, default 50 peers.

---

## Testing Strategy

### Unit Tests (in `ec_peers.rs`)

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_start_election() {
        // Verify election created and channels spawned
    }

    #[test]
    fn test_handle_answer_routes_to_correct_election() {
        // Answer with ticket reaches correct election
    }

    #[test]
    fn test_election_completes_on_consensus() {
        // Election moves to Complete when winner found
    }

    #[test]
    fn test_split_brain_spawns_more_channels() {
        // Split-brain detection triggers additional channels
    }

    #[test]
    fn test_election_timeout_cleanup() {
        // Expired elections are cleaned up
    }

    #[test]
    fn test_winner_added_to_peer_list() {
        // Successful election adds peer
    }

    #[test]
    fn test_peer_eviction_on_overflow() {
        // Old peer removed when list full
    }
}
```

### Integration Tests (in `tests/`)

```rust
#[test]
fn test_full_election_with_message_routing() {
    // Create network of nodes
    // Start election on node A
    // Verify Query messages routed
    // Verify Answer messages returned
    // Verify winner selected
}

#[test]
fn test_continuous_reelection() {
    // Run network for N ticks
    // Verify multiple elections complete
    // Verify peer list updated
}

#[test]
fn test_split_brain_resolution_in_network() {
    // Create partitioned network
    // Start election
    // Verify split-brain detected
    // Verify additional channels spawned
    // Verify resolution or timeout
}
```

---

## Migration Notes

### Compatibility

- Election system uses existing `TokenSignature` structure
- No changes needed to `Message` enum if Query/Answer already exist
- Backward compatible: nodes without elections can still respond to queries

### Incremental Rollout

1. **Phase 0**: Deploy election code (inactive)
2. **Phase 1**: Enable elections on subset of nodes (logging only)
3. **Phase 2**: Use election results for peer discovery (alongside existing)
4. **Phase 3**: Primary peer discovery method

---

## Performance Targets

### Single Election

- **Latency**: 2-5 seconds (p50), <8 seconds (p99)
- **Success Rate**: >80% elections find consensus
- **Message Count**: 6-20 messages per election (3-10 channels)

### Continuous Operation

- **Election Frequency**: 1-2 per minute
- **Message Overhead**: <500 messages/hour
- **Peer List Churn**: <10% per hour (stable network)
- **CPU**: <1% steady state, <5% during election

---

## References

- **Design Document**: [peer_election_design.md](./peer_election_design.md)
  - Section 3: Protocol Description - detailed message flow
  - Section 5: Split-Brain Detection & Resolution - resolution algorithm
  - Section 6: Attack Resistance Analysis - security considerations
  - Section 10.2: Integration with ec_peers (Future) - architecture notes

- **Implementation**: [src/ec_proof_of_storage.rs](../src/ec_proof_of_storage.rs)
  - Lines 790-888: `PeerElection::try_elect_winner()` - main election logic
  - Lines 605-620: `ElectionAttempt` enum - return values
  - Tests starting line 1780: Comprehensive test suite

- **Current ec_peers**: [src/ec_peers.rs](../src/ec_peers.rs)
  - Line 53-60: `handle_answer()` stub - needs implementation
  - Structure and existing peer management logic

---

## Getting Started (for Next Session)

### Pre-work Checklist

- [ ] Read the design document introduction (sections 1-3)
- [ ] Review `PeerElection` API in ec_proof_of_storage.rs
- [ ] Look at existing `EcPeers` structure
- [ ] Check if `Message::Query` and `Message::Answer` exist

### First Implementation Steps

1. Add `active_elections: HashMap<TokenId, ActiveElection>` to `EcPeers`
2. Call `initialize_election_secret()` in `EcPeers::new()`
3. Implement `start_election()` and `spawn_election_channels()`
4. Implement `handle_answer()` to route responses
5. Add basic `process_elections()` ticker
6. Write first integration test

### Estimated Time

- **Phase 1 (Basic Integration)**: 2-4 hours
- **Phase 2 (Message Routing)**: 2-3 hours (depends on existing routing)
- **Phase 3 (Continuous Re-Election)**: 2-3 hours
- **Testing & Polish**: 2-3 hours

**Total**: ~8-13 hours of development time

---

**End of TODO Document**
