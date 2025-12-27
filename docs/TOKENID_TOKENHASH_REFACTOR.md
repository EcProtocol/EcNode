# TokenId → TokenHash Refactor Assessment

**Status:** DOCUMENTED - NOT YET IMPLEMENTED
**Date:** 2025-12-27
**Estimated Effort:** 2-3 days focused work + testing
**Recommendation:** Defer until after peer lifecycle implementation

---

## Overview

Currently, `TokenId` is used directly for storage keys, queries, and block contents. This works for simulation but lacks production security properties. The refactor will introduce `TokenHash = Blake3(TokenId)` for storage and queries while keeping real `TokenId` in blocks/answers.

## Current Architecture

```
Query:  TokenId → Storage[TokenId] → BlockTime
Block:  TokenBlock.token = TokenId
Answer: TokenMapping.id = TokenId
```

**Issues:**
- Queries reveal token ownership (no privacy)
- No cryptographic proof that responder actually stores the token
- Storage locations are predictable

## Target Architecture

```
Query:  TokenHash → Storage[TokenHash] → BlockTime
Block:  TokenBlock.token = TokenId (proves preimage)
Answer: TokenMapping.id = TokenId (proves ownership)

where TokenHash = Blake3(TokenId)
```

**Benefits:**
1. **Privacy:** Queries use hash, observers can't determine ownership
2. **Proof-of-Knowledge:** Answering with TokenId proves you know the preimage
3. **Storage Security:** Attackers can't predict storage locations

---

## Refactor Scope

### Code Changes Required

#### 1. Type System (ec_interface.rs)
```rust
// Add new type
pub type TokenHash = [u8; 32];  // or u256 when available

// Keep TokenId for blocks/answers
pub type TokenId = u64;  // or u256 later
```

#### 2. EcTokens Trait (6 implementations)
**Current:**
```rust
fn lookup(&self, token: &TokenId) -> Option<&BlockTime>;
fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime);
```

**New:**
```rust
fn lookup(&self, token_hash: &TokenHash) -> Option<&BlockTime>;
fn set(&mut self, token_hash: &TokenHash, block: &BlockId, time: EcTime);
//                                        ^^^ No TokenId stored - just hash
```

**Important:** Storage only needs the hash as key. When code has a real `TokenId` (from blocks/answers), it hashes on-the-fly before lookup.

**Impacted files:**
- `src/ec_interface.rs` - trait definition
- `src/ec_memory_backend.rs` - 3 implementations
- `src/ec_rocksdb_backend.rs` - 1 implementation
- `src/ec_proof_of_storage.rs` - TestBackend
- `src/ec_mempool.rs` - MockEcTokens

#### 3. Message Types (ec_interface.rs)
**Current:**
```rust
QueryToken {
    token_id: TokenId,
    ...
}
```

**New:**
```rust
QueryToken {
    token_hash: TokenHash,  // What to look up
    ...
}
```

**Note:** `Answer`, `Block`, and `TokenBlock` keep using real `TokenId`

#### 4. Core Logic Updates

**EcNode (src/ec_node.rs):**
- Query construction: `hash(token_id)` before creating QueryToken
- Answer handling: Extract real TokenId from answer, hash it for storage

**EcMemPool (src/ec_mempool.rs):**
```rust
// Block processing - hash on-the-fly
fn process_block(&mut self, block: &Block) {
    for part in block.parts.iter() {
        let token_id = part.token;           // Real TokenId from block
        let token_hash = hash_token(&token_id);  // Hash on-the-fly
        self.tokens.set(&token_hash, &block.id, block.time);
    }
}
```

**ProofOfStorage (src/ec_proof_of_storage.rs):**
```rust
// Signature generation - hash the queried token
fn generate_signature(&self, backend: &impl TokenStorage, token_id: &TokenId, peer: &PeerId)
    -> Option<TokenSignature>
{
    // Hash the queried token to find it in storage
    let token_hash = hash_token(token_id);
    let block_time = backend.lookup(&token_hash)?;

    // Generate signature chunks (same as before)
    let chunks = Self::signature_for(token_id, &block_time.block, peer);

    // Search for signature tokens - hash each candidate before lookup
    for candidate_id in self.search_nearby(token_id) {
        let candidate_hash = hash_token(&candidate_id);
        if let Some(bt) = backend.lookup(&candidate_hash) {
            // Found a token, check if it matches signature chunk
            ...
        }
    }
}
```

**Key insight:** Token proximity search still operates on real TokenIds (not hashes), but each candidate is hashed before storage lookup.

**EcPeers (src/ec_peers.rs):**
- Election challenge tokens need hashing for queries
- Answer processing extracts real TokenId

#### 5. Backend Storage

**MemoryBackend:**
```rust
// Current
tokens: HashMap<TokenId, BlockTime>

// New
tokens: HashMap<TokenHash, BlockTime>
//      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ ONLY hash stored - no reverse lookup needed
```

**RocksDB:**
- Key changes from `TokenId` bytes (8 bytes) to `TokenHash` bytes (32 bytes)
- Value remains `BlockTime` (unchanged)
- No need to store TokenId in value - caller already has it

**Why no reverse lookup?**
All code that needs to lookup tokens already has the real `TokenId`:
- Block processing: `TokenBlock.token` contains real ID → hash before lookup
- Answer handling: `TokenMapping.id` contains real ID → hash before lookup
- Signature generation: Given TokenId from caller → hash before lookup
- Consensus validation: Extracts TokenId from block → hash before lookup

**Performance win:** Simpler storage, no dual indexes, smaller values

### Test Impact Analysis

**Files with tests:** 5
**Total test count:** 111
**Tests requiring changes:** ~80-90 (all tests that create/query tokens)

**Typical test change:**
```rust
// Before
backend.set(&token_id, &block_id, time);
let result = backend.lookup(&token_id);

// After
let token_hash = hash_token(&token_id);
backend.set(&token_hash, &token_id, &block_id, time);
let result = backend.lookup(&token_hash);
```

**Helper function needed:**
```rust
fn hash_token(token_id: &TokenId) -> TokenHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&token_id.to_le_bytes());
    hasher.finalize().into()
}
```

### Simulator Impact

**Simulator files:** 16
**Impact:** HIGH - all simulators create and query tokens

**Changes needed:**
- Token generation: Compute hash immediately after generating TokenId
- Query construction: Use hash instead of ID
- Block creation: Keep using real TokenIds
- Statistics: May need to track both ID and hash

**Example from peer_lifecycle:**
```rust
// Before
let token = peer_id.wrapping_add(offset);
token_storage.set(&token, &block_id, time);

// After
let token_id = peer_id.wrapping_add(offset);
let token_hash = hash_token(&token_id);
token_storage.set(&token_hash, &token_id, &block_id, time);
```

---

## Migration Plan

### Phase 1: Preparation (Pre-refactor)
✅ Document the refactor (this file)
✅ Add comprehensive tests for current behavior
⏸️ Establish performance baselines
⏸️ Complete peer election implementation

### Phase 2: Type System (Day 1 morning)
1. Add `TokenHash` type to ec_interface.rs
2. Add `hash_token()` helper function
3. Create new `EcTokenStorage` trait alongside old `EcTokens`
4. Ensure code compiles with both traits

### Phase 3: Backend Migration (Day 1 afternoon)
1. Update MemoryBackend to new trait
2. Update RocksDB backend to new trait
3. Add compatibility shim that maps old trait to new
4. Run all tests with shim

### Phase 4: Core Logic (Day 2 morning)
1. Update EcNode message handlers
2. Update EcMemPool token operations
3. Update ProofOfStorage lookups
4. Update EcPeers election logic

### Phase 5: Test Migration (Day 2 afternoon)
1. Create test helper module with `hash_token()`
2. Batch update tests by file:
   - ec_memory_backend.rs tests
   - ec_mempool.rs tests
   - ec_proof_of_storage.rs tests
   - ec_peers.rs tests
3. Run test suite after each file

### Phase 6: Simulator Migration (Day 3)
1. Update consensus simulator
2. Update peer_lifecycle simulator
3. Add hash tracking to statistics
4. Validate all simulation scenarios pass

### Phase 7: Cleanup
1. Remove old `EcTokens` trait
2. Remove compatibility shims
3. Update documentation
4. Performance comparison vs baseline

---

## Risk Assessment

### High Risk Areas
1. **RocksDB migration** - Database format change requires careful handling
   - Mitigation: Create new column family, migrate data, swap
2. **Test breakage** - 80+ tests will break simultaneously
   - Mitigation: Fix tests in batches, use git to track progress
3. **Simulation regression** - Complex scenarios may behave differently
   - Mitigation: Establish baselines before refactor, compare after

### Medium Risk Areas
1. **Performance impact** - Extra Blake3 hashes on every lookup
   - Mitigation: Benchmark before/after, optimize if needed
2. **Debug complexity** - Harder to track tokens when storage uses hashes
   - Mitigation: Add debug helpers that reverse-lookup hashes

### Low Risk Areas
1. **Consensus logic** - Unchanged, only storage layer affected
2. **Message passing** - Minimal changes, well-defined boundaries

---

## Decision: When to Refactor?

### ❌ NOT NOW - Current State
**Blockers:**
- Peer election implementation incomplete (⏸️)
- No performance baselines established (⏸️)
- Major simulation work ongoing (⏸️)

**Impact:**
- Would break all active development
- ~3 days to complete refactor
- High risk of merge conflicts
- Delays other priorities

### ✅ RECOMMENDED TIMING

**After these milestones:**
1. ✅ Core consensus stable and tested
2. ✅ Proof-of-storage signatures working
3. ⏸️ Peer election fully implemented
4. ⏸️ All simulators producing stable results
5. ⏸️ Performance baselines captured

**Ideal timing:** After peer lifecycle simulation is complete and validated

**Why then?**
- Feature-complete system with stable test suite
- Can validate no behavioral regression
- Less active development = fewer conflicts
- Natural breakpoint before production hardening

### 🚫 NEVER DO IT THIS WAY
- ❌ Incremental migration (half the code using hashes, half not)
- ❌ During active feature development
- ❌ Without comprehensive test coverage
- ❌ Without performance baselines

---

## Conclusion

**Recommendation:** Document now, implement later

This refactor is **important for production** but **not urgent for development**. The current TokenId-everywhere approach works fine for testing consensus logic. The security benefits of TokenHash only matter in adversarial environments.

**Best practice:** Finish building and validating the consensus protocol with current architecture, then upgrade to production-grade security in a dedicated refactor sprint.

**Tracking:** This document serves as the spec. When ready to start:
1. Create GitHub issue linking to this doc
2. Create feature branch `refactor/token-hash`
3. Follow migration plan above
4. PR review with full test suite + simulation validation

---

**Document Status:** Living document - update as architecture evolves
