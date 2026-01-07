# Echo Consent Genesis Block Generation

## Purpose

The `ec_genesis` module provides deterministic generation of an initial shared Block/Token set for network bootstrapping. This allows new nodes to agree on a common starting state without coordination.

**Key Properties:**
- **Deterministic**: Same inputs always produce identical genesis state
- **Optional**: Mature networks can bootstrap from existing nodes; controlled by startup parameter
- **Verifiable**: Any node can independently verify the genesis state
- **Non-transferable**: Genesis tokens are "destroyed" (key=0) and cannot be transferred

## Requirements

### Functional Requirements

1. Generate configurable number of blocks (default: 100,000)
2. Each block contains exactly **one** new genesis TokenId in the first slot
3. Remaining 5 slots in each block are empty/default
4. Tokens are deterministically generated using Blake3 hashing
5. All genesis tokens have `last=0` and `key=0` (non-transferable)
6. Each block has `used=1` (one slot used)
7. No signatures on genesis blocks
8. Use BatchedBackend for atomic commitment

### Non-Functional Requirements

1. **Performance**: Batch operations for efficient storage
2. **Memory**: Stream generation to avoid loading all blocks in memory
3. **Verification**: Fast validation of genesis state correctness

## Token Generation Algorithm

### Deterministic Token ID Generation

Tokens are generated using iterative Blake3 hashing:

```
SEED₀ = "This is the Genesis of the Echo Consent Network"
COUNTER = sequence starting at "0000001"

For each token i:
  INPUT = SEED_{i-1} || COUNTER_i
  HASH = Blake3(INPUT)          // 32 bytes
  TOKEN_i = truncate HASH to 64 bits (first 8 bytes, little-endian)
  SEED_i = TOKEN_i              // For next iteration
```

**Note on TokenId size:**
- Current implementation: `TokenId = u64` (64 bits)
- Future implementation: `TokenId = [u8; 32]` (256 bits)
- **Current approach**: Truncate Blake3 hash (32 bytes) to first 8 bytes
- **Future approach**: Use full 32-byte Blake3 hash directly

This design will work for both current and future TokenId representations.

### Counter Format

```
COUNTER_i = zero-padded 7-digit decimal string
  "0000001" → "0000002" → ... → "0100000"
```

**Maximum supported blocks:** 9,999,999 (within 7-digit format)

### Token Generation Examples

```
Token 1:
  Input: "This is the Genesis of the Echo Consent Network" || "0000001"
  Hash:  Blake3("This is the Genesis of the Echo Consent Network0000001")
         = [0x..., 0x..., ...] (32 bytes)
  Token: u64::from_le_bytes(hash[0..8])
         = <64-bit integer>

Token 2:
  Input: <Token_1 as 8 bytes LE> || "0000002"
  Hash:  Blake3(<Token_1 bytes> || "0000002")
  Token: u64::from_le_bytes(hash[0..8])
```

**Seed Chaining:**
- `SEED₀` = UTF-8 bytes of initial string (51 bytes)
- `SEED₁` = `Token_1` as 8 bytes (little-endian u64)
- `SEED₂` = `Token_2` as 8 bytes (little-endian u64)
- ...

## Block Structure

### Genesis Block Layout

Each genesis block contains:

```rust
Block {
    id: BlockId,              // Generated (implementation-dependent)
    time: 0,                  // Fixed at 0 for determinism
    used: 1,                  // ✓ Only first slot is used
    parts: [
        TokenBlock {
            token: TOKEN_i,   // Generated token
            last: 0,          // No parent (genesis)
            key: 0,           // Destroyed key (non-transferable)
        },
        TokenBlock::default(), // Empty slots
        TokenBlock::default(),
        TokenBlock::default(),
        TokenBlock::default(),
        TokenBlock::default(),
    ],
    signatures: [None; 6],    // ✓ No signatures for genesis
}
```

**Block Properties:**
- **Small blocks**: Only 1 token per block (5 empty slots)
- **Create-token transaction**: No signatures required
- **Deterministic**: `time=0`, `used=1`, fixed structure

### BlockId Assignment

BlockId generation strategy will evolve with the codebase. Current approach:
- Use sequential IDs: `BlockId = i` (1, 2, 3, ...)
- Future: May use content-based hashing

**Note**: Genesis generation focuses on deterministic Token creation and proper storage via BatchedBackend. BlockId strategy is implementation-dependent and will be adjusted as Block/Token structures evolve.

## Token Mapping Updates

For each genesis token:

```rust
batch.update_token(
    &token_id,           // Generated TokenId
    &block_id,           // Block containing this token
    &0,                  // Parent = 0 (genesis, no parent)
    0                    // time = 0
);
```

This creates the mapping: `TokenId → BlockTime { block, parent: 0, time: 0 }`

## Algorithm Flow

```mermaid
flowchart TD
    A[Start Genesis Generation] --> B[Initialize seed_bytes from string]
    B --> C[Create BatchedBackend]
    C --> D[Begin batch]

    D --> E[For i = 1 to NUM_BLOCKS]
    E --> F[Format counter_i as 7-digit string]
    F --> G[Compute hash = Blake3 seed_bytes || counter_i]

    G --> H[Extract TOKEN_i from hash first 8 bytes]
    H --> I[Create Block with used=1, time=0]
    I --> J[Set parts[0] = TokenBlock with TOKEN_i, last=0, key=0]

    J --> K[Assign BlockId = i sequential]
    K --> L[batch.save_block Block]

    L --> M[batch.update_token TOKEN_i → BlockTime]
    M --> N[Update seed_bytes = TOKEN_i.to_le_bytes]

    N --> O{More blocks?}
    O -->|Yes| E
    O -->|No| P[batch.commit]

    P --> Q{Success?}
    Q -->|Yes| R[Genesis complete]
    Q -->|No| S[Error: rollback]
```

## API Design

### Module Interface

```rust
// src/ec_genesis.rs

use crate::ec_interface::{
    BatchedBackend, Block, BlockId, TokenBlock, TokenId,
    TOKENS_PER_BLOCK
};

/// Configuration for genesis generation
#[derive(Clone, Debug)]
pub struct GenesisConfig {
    /// Number of blocks to generate (default: 100_000)
    pub block_count: usize,

    /// Seed string for first token
    pub seed_string: String,
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self {
            block_count: 100_000,
            seed_string: "This is the Genesis of the Echo Consent Network".to_string(),
        }
    }
}

/// Generate genesis blocks and tokens into the provided backend
///
/// Creates `config.block_count` blocks, each containing one new token.
/// All tokens are deterministically generated from the seed string.
///
/// # Arguments
/// * `backend` - Backend implementing BatchedBackend trait
/// * `config` - Genesis generation configuration
///
/// # Returns
/// * `Ok(())` - Genesis generated successfully and committed
/// * `Err(msg)` - Storage error during batch operations
///
/// # Determinism
/// Given the same config, this function always produces identical state.
/// Token generation is deterministic based on Blake3 hashing.
///
/// # Example
/// ```rust
/// use ec_rust::ec_genesis::{generate_genesis, GenesisConfig};
/// use ec_rust::ec_memory_backend::MemoryBackend;
///
/// let mut backend = MemoryBackend::new();
/// let config = GenesisConfig::default();
/// generate_genesis(&mut backend, config)?;
/// # Ok::<(), String>(())
/// ```
pub fn generate_genesis<B: BatchedBackend>(
    backend: &mut B,
    config: GenesisConfig,
) -> Result<(), String>;
```

### Integration with Node Bootstrap

Called during node initialization when genesis flag is set:

```rust
// In node initialization code (e.g., ec_node.rs or main.rs)
use ec_genesis::{generate_genesis, GenesisConfig};

fn bootstrap_node(enable_genesis: bool) -> Result<EcNode, String> {
    let mut backend = create_backend();

    if enable_genesis {
        let config = GenesisConfig::default();
        generate_genesis(&mut backend, config)?;
    }

    // Continue with node creation
    Ok(EcNode::new(backend))
}
```

**Note**: No CLI tool needed - genesis generation is controlled by startup parameters within the node binary.

## Performance Considerations

### Batch Size

For 100,000 blocks:
- **Block operations**: 100,000 `save_block()` calls
- **Token operations**: 100,000 `update_token()` calls
- **Total operations**: 200,000 writes in single batch

### Memory Usage

**Streaming approach**:
- Generate one block at a time
- Add to batch immediately
- Only batch accumulator held in memory
- Peak memory: O(batch_size), not O(total_blocks)

**MemoryBackend estimation**:
- Batch accumulates operations in vectors
- Memory: ~(100k × 200 bytes blocks) + (100k × 40 bytes tokens) ≈ 24 MB

### Hash Performance

Blake3 performance:
- 100,000 Blake3 hashes for token generation
- Blake3 throughput: ~1-2 GB/s on modern CPUs
- Hash input size: ~15 bytes average (8-byte seed + 7-byte counter)
- Total hash time: <10ms for 100k hashes

**Expected total time**:
- Hashing: <10ms
- Block creation: ~50ms (memory allocation, struct creation)
- Batch commit: <200ms (dominant cost)
- **Total: <500ms for 100k blocks**

## Security Properties

### Determinism Guarantees

**Determinism Theorem:**
```
∀ nodes A, B:
  config_A = config_B ⇒ genesis_state_A = genesis_state_B
```

**Proof by construction:**
1. Token generation uses deterministic Blake3 with fixed seed and counter
2. No randomness or system-dependent inputs
3. Block structure is fixed (time=0, used=1, empty slots)
4. Batch commit is atomic (all-or-nothing)

### Collision Resistance

**Token ID uniqueness:**
- Current: 64-bit space (2⁶⁴ possible tokens)
- Birthday bound: √(2⁶⁴) ≈ 4.3 billion tokens before 50% collision
- Genesis size: 100,000 tokens
- Collision probability: ≈ (100,000)² / 2⁶⁵ ≈ 2.7 × 10⁻¹⁰ (negligible)

**Future (256-bit TokenId):**
- Collision probability becomes cryptographically negligible

### Non-Transferability

Genesis tokens are permanently locked:
- **key = 0**: No valid cryptographic key exists for signing transfers
- **last = 0**: No parent chain (genesis origin)
- **Purpose**: Establish initial token set, not for circulation

These tokens serve as "anchors" in the token graph but cannot participate in transactions.

## Testing Strategy

### Unit Tests

1. **Token generation determinism**
   ```rust
   #[test]
   fn test_token_generation_deterministic() {
       let tokens1 = generate_tokens(GenesisConfig::default());
       let tokens2 = generate_tokens(GenesisConfig::default());
       assert_eq!(tokens1, tokens2);
   }
   ```

2. **Counter formatting**
   ```rust
   #[test]
   fn test_counter_format() {
       assert_eq!(format_counter(1), "0000001");
       assert_eq!(format_counter(100000), "0100000");
       assert_eq!(format_counter(9999999), "9999999");
   }
   ```

3. **Block structure validation**
   ```rust
   #[test]
   fn test_genesis_block_structure() {
       let block = create_genesis_block(token_id, 1);
       assert_eq!(block.used, 1);
       assert_eq!(block.time, 0);
       assert_eq!(block.parts[0].last, 0);
       assert_eq!(block.parts[0].key, 0);
       assert!(block.signatures.iter().all(|s| s.is_none()));
   }
   ```

4. **Seed chaining**
   ```rust
   #[test]
   fn test_seed_evolution() {
       let seed0 = "This is the Genesis of the Echo Consent Network";
       let token1 = hash_and_extract(seed0.as_bytes(), "0000001");
       let token2 = hash_and_extract(&token1.to_le_bytes(), "0000002");

       // Verify independence: changing seed0 changes all subsequent tokens
       let alt_seed0 = "Different seed";
       let alt_token1 = hash_and_extract(alt_seed0.as_bytes(), "0000001");
       assert_ne!(token1, alt_token1);
   }
   ```

### Integration Tests

1. **Small genesis (100 blocks)**
   ```rust
   #[test]
   fn test_small_genesis() {
       let mut backend = MemoryBackend::new();
       let config = GenesisConfig { block_count: 100, ..Default::default() };

       generate_genesis(&mut backend, config).unwrap();

       // Verify token count
       assert_eq!(backend.token_count(), 100);
       assert_eq!(backend.block_count(), 100);
   }
   ```

2. **Full genesis (100k blocks)**
   ```rust
   #[test]
   fn test_full_genesis_performance() {
       let mut backend = MemoryBackend::new();
       let config = GenesisConfig::default();

       let start = std::time::Instant::now();
       generate_genesis(&mut backend, config).unwrap();
       let duration = start.elapsed();

       assert!(duration.as_millis() < 1000, "Genesis too slow: {:?}", duration);
       assert_eq!(backend.token_count(), 100_000);
   }
   ```

3. **Token mapping verification**
   ```rust
   #[test]
   fn test_token_mappings() {
       let mut backend = MemoryBackend::new();
       generate_genesis(&mut backend, GenesisConfig::default()).unwrap();

       // Pick random tokens and verify mappings
       let token_id = /* extract from backend */;
       let mapping = backend.get_token(token_id).unwrap();

       assert_eq!(mapping.parent, 0);
       assert_eq!(mapping.time, 0);
   }
   ```

### Reproducibility Test

```rust
#[test]
fn test_genesis_reproducibility() {
    let config = GenesisConfig::default();

    let mut backend1 = MemoryBackend::new();
    generate_genesis(&mut backend1, config.clone()).unwrap();

    let mut backend2 = MemoryBackend::new();
    generate_genesis(&mut backend2, config).unwrap();

    // Extract and compare states
    assert_eq!(
        backend1.get_all_tokens(),
        backend2.get_all_tokens(),
        "Genesis not reproducible!"
    );
}
```

## Implementation Details

### Token Generation Helper

```rust
fn generate_token(seed_bytes: &[u8], counter: usize) -> (TokenId, Vec<u8>) {
    let counter_str = format!("{:07}", counter);

    let mut hasher = blake3::Hasher::new();
    hasher.update(seed_bytes);
    hasher.update(counter_str.as_bytes());

    let hash = hasher.finalize();
    let hash_bytes = hash.as_bytes();

    // Extract first 8 bytes as TokenId (current u64 implementation)
    let token_id = u64::from_le_bytes(hash_bytes[0..8].try_into().unwrap());

    // Next seed is the token bytes
    let next_seed = token_id.to_le_bytes().to_vec();

    (token_id, next_seed)
}
```

### Block Creation Helper

```rust
fn create_genesis_block(token_id: TokenId, block_id: BlockId) -> Block {
    let mut parts = [TokenBlock::default(); TOKENS_PER_BLOCK];
    parts[0] = TokenBlock {
        token: token_id,
        last: 0,    // Genesis parent
        key: 0,     // Non-transferable
    };

    Block {
        id: block_id,
        time: 0,
        used: 1,
        parts,
        signatures: [None; TOKENS_PER_BLOCK],
    }
}
```

### Main Generation Loop

```rust
pub fn generate_genesis<B: BatchedBackend>(
    backend: &mut B,
    config: GenesisConfig,
) -> Result<(), String> {
    let mut seed_bytes = config.seed_string.as_bytes().to_vec();

    let batch = backend.begin();

    for i in 1..=config.block_count {
        // Generate token
        let (token_id, next_seed) = generate_token(&seed_bytes, i);

        // Create block
        let block_id = i as BlockId;
        let block = create_genesis_block(token_id, block_id);

        // Add to batch
        batch.save_block(&block);
        batch.update_token(&token_id, &block_id, &0, 0);

        // Update seed for next iteration
        seed_bytes = next_seed;
    }

    batch.commit()?;
    Ok(())
}
```

## Future Evolution

As Token/Block structures evolve to their final form, this code will adapt:

### TokenId Migration (u64 → 256-bit)

**Current**:
```rust
let token_id = u64::from_le_bytes(hash[0..8].try_into().unwrap());
```

**Future**:
```rust
let token_id: [u8; 32] = *hash.as_bytes();  // Use full 32-byte hash
```

**Impact**: Minimal code change, same deterministic algorithm

### BlockId Evolution

Genesis code is agnostic to BlockId generation strategy:
- Current: Sequential assignment
- Future: Content-based hashing, UUIDs, etc.
- Change point: Single line in `create_genesis_block()`

### Signature System Changes

Genesis blocks have no signatures (`[None; 6]`). If signature representation changes, update initialization.

## Bootstrap Optimizations

### Selective Storage (Ring-Based Filtering)

To enable nodes to bootstrap without storing the entire genesis set, selective storage filters blocks/tokens based on ring distance from the node's peer ID.

#### Ring Distance Calculation

```
ring_distance(a, b) = min(b - a, a - b)  // wrapping arithmetic on u64
```

The ring is the full u64 space, where:
- Distance from 0 to 100 = 100
- Distance from 0 to (MAX - 50) = 51 (wrapping)
- Maximum distance = u64::MAX / 2

#### Storage Decision

```
should_store_token(token_id, peer_id, storage_fraction):
    if storage_fraction >= 1.0:
        return true  // Full archive node

    max_distance = (u64::MAX / 2) * storage_fraction
    return ring_distance(token_id, peer_id) <= max_distance
```

For each genesis token:
- Store if token_id is within range, OR
- Store if block_id is within range

**Example**: With `storage_fraction = 0.25` (1/4 of ring):
- 100,000 genesis tokens → ~25,000 stored per node
- Nodes collectively store all genesis tokens (distributed)
- Each node stores tokens "close" to its peer_id

#### Mathematical Properties

**Coverage theorem:**
```
For any token T and storage_fraction f:
  Expected number of nodes storing T = N × f
  where N = total number of nodes
```

**Proof**: Each node stores tokens within fraction f of the ring. The probability that any given node stores a random token is f. By linearity of expectation, E[stores(T)] = N × f.

**Availability guarantee:**
```
For f = 0.25 and N ≥ 8 nodes:
  P(token not stored by any node) < 0.1%  // Birthday-like analysis
```

### Token Seeding for Peer Discovery

During genesis generation, tokens are probabilistically seeded into `EcPeers::TokenSampleCollection` for early peer discovery.

#### Seeding Strategy

```rust
// Probabilistic sampling during generation
const SEED_SAMPLE_PROBABILITY: f64 = 0.01;  // 1% of tokens

for each genesis token_id:
    // Deterministic sampling based on token_id
    seed_hash = (token_id as f64) / (u64::MAX as f64)
    if seed_hash < SEED_SAMPLE_PROBABILITY:
        peers.seed_genesis_token(token_id)  // Capacity-limited
```

**Parameters**:
- Sample probability: 1% (default)
- 100,000 genesis → ~1,000 tokens seeded
- TokenSampleCollection capacity: 1,000 (default)
- Capacity enforcement: Automatic rejection when full

#### Benefits

1. **Early Discovery**: Nodes have genesis tokens to query before any active peers exist
2. **Distributed Coverage**: Each node samples different tokens (based on token_id hash)
3. **DHT Bootstrapping**: Token queries trigger referrals, populating peer lists
4. **Capacity-Bounded**: TokenSampleCollection enforces max capacity automatically

#### Integration with Peer Election

When `EcPeers.tick()` runs with empty peer list:
1. Picks N tokens from TokenSampleCollection
2. Starts elections for these tokens
3. Sends queries to closest known peers (from genesis seeds)
4. Receives Referrals → discovers new peers
5. Gradually builds up Connected peer set

**Cold-start scenario** (no active peers yet):
- Genesis token seeds enable initial elections
- Elections generate random token queries (if collection is low)
- Random queries explore the ID space
- Referrals guide toward token owners
- Network connectivity emerges

### Updated API

```rust
pub fn generate_genesis<B: BatchedBackend>(
    backend: &mut B,
    config: GenesisConfig,
    peers: &mut crate::ec_peers::EcPeers,
    storage_fraction: f64,
) -> Result<usize, Box<dyn std::error::Error>>
```

**Arguments**:
- `backend`: Storage backend
- `config`: Genesis configuration (block_count, seed_string)
- `peers`: Peer manager (provides peer_id, receives token seeds)
- `storage_fraction`: Fraction of ring to store (0.25 = 1/4, 1.0 = all)

**Returns**:
- `Ok(stored_count)`: Number of blocks/tokens actually stored
- `Err(msg)`: Storage error

**Example**:
```rust
use ec_rust::ec_genesis::{generate_genesis, GenesisConfig};
use ec_rust::ec_memory_backend::MemoryBackend;
use ec_rust::ec_peers::EcPeers;

let mut backend = MemoryBackend::new();
let mut peers = EcPeers::new(12345);
let config = GenesisConfig::default();

// Store 1/4 of ring, seed random tokens for discovery
let stored = generate_genesis(&mut backend, config, &mut peers, 0.25)?;

// stored ≈ 25,000 (25% of 100,000)
// peers.TokenSampleCollection ≈ 1,000 genesis tokens seeded
```

### Network Bootstrap Sequence

1. **Genesis Generation** (each node independently):
   ```rust
   let stored = generate_genesis(&mut backend, config, &mut peers, 0.25)?;
   // Each node stores ~25% of genesis (distributed by peer_id)
   // Each node seeds ~1% of genesis into TokenSampleCollection
   ```

2. **Initial Peer Discovery** (first few ticks):
   ```rust
   peers.tick(&token_storage, time);
   // Starts elections using seeded genesis tokens
   // Sends queries → receives referrals → discovers peers
   ```

3. **Network Formation** (gradual):
   - Elections explore the ID space
   - Referrals guide toward token owners
   - Connected peers accumulate
   - Normal consensus begins

### Testing Selective Storage

```rust
#[test]
fn test_selective_storage() {
    let mut backend = MemoryBackend::new();
    let mut peers = EcPeers::new(u64::MAX / 2);  // Center of ring
    let config = GenesisConfig { block_count: 1000, ..Default::default() };

    let stored = generate_genesis(&mut backend, config, &mut peers, 0.25).unwrap();

    // Should store approximately 25% of blocks
    assert!(stored > 200 && stored < 300);
}
```

## Summary

The `ec_genesis` module provides:

✅ **Deterministic** token generation via Blake3 chaining
✅ **Minimal** block structure (1 token, 5 empty slots)
✅ **Atomic** batch commit via BatchedBackend
✅ **Non-transferable** genesis tokens (key=0, last=0)
✅ **Efficient** streaming generation (<500ms for 100k blocks)
✅ **Verifiable** reproducible state across all nodes
✅ **Selective storage** based on ring distance (1/4 of ring by default)
✅ **Peer discovery seeding** for early network bootstrap
✅ **Future-proof** design adapts to TokenId/Block evolution

The implementation integrates with `EcPeers` to enable distributed genesis storage and cold-start peer discovery.
