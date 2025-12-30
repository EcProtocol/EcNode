# Peer Identity and Argon2 Proof-of-Work Design

## Overview

The `ec_identity` module provides cryptographic peer identity generation using:
- **X25519 key pairs** for Diffie-Hellman key exchange
- **Argon2id proof-of-work** for sybil-resistant address mining

This design document explains the architecture, parameter choices, and optimization rationale.

---

## Architecture

### Two-Phase Lifecycle

The peer identity creation is split into two independent phases:

#### Phase 1: Keypair Generation (Immediate)
```rust
let mut identity = PeerIdentity::new();
// Keypair ready for DH key exchange immediately
let secret = identity.compute_shared_secret(&peer.public_key);
```

**Purpose:** Enable peer communication immediately, even before mining completes.

#### Phase 2: Mining (Asynchronous)
```rust
// Can run in background/separate thread
identity.mine(AddressConfig::PRODUCTION);
// Now have peer-id and salt
```

**Purpose:** Obtain sybil-resistant peer-id through proof-of-work.

### Key Components

1. **X25519 Keypair (Fixed)**
   - Generated once during `PeerIdentity::new()`
   - Used for all Diffie-Hellman key exchanges
   - Never changes, even during mining

2. **Salt (Mined)**
   - Random 128-bit nonce
   - Mining tries different salts until `Argon2(public_key, salt)` meets difficulty
   - Must be transmitted with Answer/Referral messages
   - Allows peers to validate the peer-id

3. **Peer-ID (Derived)**
   - 256-bit hash result: `peer_id = Argon2(public_key, salt)`
   - Used as network address for DHT routing
   - Verifiable proof-of-work

---

## Mining Process

### Algorithm

```
1. Generate X25519 keypair (once)
2. Loop:
   a. Generate random salt
   b. Compute hash = Argon2(public_key, salt, config)
   c. If hash has required trailing zero bits → Success
   d. Else continue
3. Store winning salt and peer-id
```

### Mining Only Varies the Salt

**Critical Design Decision:** The public key is fixed before mining begins.

**Why?**
- Keypair needed for communication (DH) immediately
- Mining can happen asynchronously/in background
- No need to regenerate keypair for each attempt
- Simpler implementation and clearer semantics

---

## Argon2 Parameter Optimization

### The Validation Problem

**Key Insight:** Validation happens **frequently** (every Answer/Referral message), while mining happens **once**.

**Original Production Settings (WRONG):**
```rust
difficulty: 20 bits
memory_cost: 65536 KiB  // 64 MiB
time_cost: 3 iterations
```

**Benchmark Results:**
- Validation: **237ms per peer**
- Throughput: **~4 validations/sec**
- **Massive bottleneck during peer discovery!**

### Optimization Strategy

**Formula:**
```
Mining time = attempts × single_hash_cost
            = 2^difficulty × Argon2_cost

Validation cost = 1 × Argon2_cost
```

**Trade-off:**
- **Lower Argon2 cost** → Faster validation (critical!)
- **Higher difficulty** → Compensate for lower cost, maintain mining time
- **Result:** Same sybil resistance, much faster validation

### Benchmark Data

| Configuration | Memory | Time Cost | Validation | Throughput | Required Difficulty (24h) |
|--------------|--------|-----------|------------|------------|---------------------------|
| Test | 256 KiB | 1 | 0.34ms | 2899/sec | 28 bits |
| **New Production** | **4 MiB** | **1** | **4.64ms** | **215/sec** | **24 bits** |
| High Memory | 16 MiB | 1 | 18.99ms | 53/sec | 22 bits |
| Old Production | 64 MiB | 3 | **237ms** | **4/sec** | 19 bits |

**Improvement:** 47x faster validation with same ~24 hour mining time!

### Final Production Settings

```rust
AddressConfig::PRODUCTION {
    difficulty: 24,       // 24 bits (~16.7M attempts)
    memory_cost: 4096,    // 4 MiB (balanced: memory-hard but fast)
    time_cost: 1,         // 1 iteration (no redundant work)
    parallelism: 1,       // Single thread
}
```

**Performance:**
- Mining: ~24 hours on modern CPU
- Validation: ~5ms per peer
- Throughput: ~200 peer validations/sec
- Still memory-hard (ASIC-resistant with 4 MiB)

### Alternative Configurations

#### HIGH_MEMORY (Maximum ASIC Resistance)
```rust
difficulty: 23 bits
memory_cost: 16384 KiB  // 16 MiB
time_cost: 1
```
- Validation: ~18ms per peer (~55/sec)
- Use if ASIC resistance > validation speed

#### LOW_LATENCY (Maximum Throughput)
```rust
difficulty: 26 bits
memory_cost: 1024 KiB   // 1 MiB
time_cost: 1
```
- Validation: ~1ms per peer (~850/sec)
- Use for high-throughput networks

---

## Comparison to Password Hashing

### OWASP Argon2 Recommendations (Passwords)
- **Use case:** One-time login validation
- **Settings:** 19-46 MiB memory, 2 iterations
- **Cost:** ~20-50ms per hash
- **Frequency:** Rare (only during authentication)

### Our Settings (Peer-ID Validation)
- **Use case:** Frequent validation during network operation
- **Settings:** 4 MiB memory, 1 iteration
- **Cost:** ~5ms per hash
- **Frequency:** High (every Answer/Referral message)
- **Compensation:** Higher difficulty (24 bits vs password requirements)

**Why Different?**
- Passwords: Defend against offline brute-force of database dump
- Peer-IDs: Defend against sybil attacks while enabling fast network operation
- Passwords: Validation is rare, can tolerate 50ms
- Peer-IDs: Validation is frequent, need <10ms for scalability

---

## Security Considerations

### Sybil Resistance

**Threat Model:** Attacker wants to create many peer identities to gain network influence.

**Defense:**
- Mining cost: ~24 hours per identity on modern CPU
- Expected cost for 1000 identities: ~2.7 years of CPU time
- Economic barrier to large-scale sybil attacks

**Why 24 bits is sufficient:**
```
Single peer: 2^24 attempts × 4.64ms = ~21 hours
1000 peers: 1000 × 21 hours = 21,000 hours ≈ 2.4 years
```

### ASIC Resistance

**Argon2id with 4 MiB memory:**
- Memory-hard algorithm (primary defense against ASICs)
- 4 MiB requires actual RAM, not cache
- Still significant hardware cost for parallelization
- Better than pure compute-bound PoW (e.g., SHA-256)

**Trade-off accepted:**
- 4 MiB is less ASIC-resistant than 64 MiB
- But 64 MiB makes validation impractically slow
- Can compensate with higher difficulty if needed
- Validation speed is critical for network operation

### Validation Security

**When validating peer-id:**
1. Receive: `(peer_id, public_key, salt)` in Answer/Referral message
2. Compute: `hash = Argon2(public_key, salt, config)`
3. Verify: `hash == peer_id`
4. Verify: `hash` has required trailing zero bits

**Security properties:**
- Cannot fake peer-id without doing proof-of-work
- Cannot reuse same peer-id with different public key
- Salt prevents pre-computation attacks
- Public key binds peer-id to DH key exchange capability

---

## Message Integration

### Future Message Extensions

When extending Answer/Referral messages for production, include:

```rust
Message::Answer {
    answer: TokenMapping,
    signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
    sender_public_key: X25519PublicKey,  // For DH key exchange
    sender_salt: Salt,                    // For peer-id validation
    head_of_chain: CommitBlockId,
}

Message::Referral {
    token: TokenId,
    high: PeerId,
    low: PeerId,
    high_public_key: X25519PublicKey,     // For DH with high peer
    high_salt: Salt,                      // For validating high peer
    low_public_key: X25519PublicKey,      // For DH with low peer
    low_salt: Salt,                       // For validating low peer
}
```

**Why include salt?**
- Allows recipient to validate `peer_id == Argon2(public_key, salt)`
- Enables ring address computation for DHT routing
- Proves sender did proof-of-work

---

## Shared Secret Management

### On-Demand Computation

```rust
// Compute when needed (not stored in PeerIdentity)
let secret = identity.compute_shared_secret(&peer.public_key);
```

**Design:**
- Shared secrets are computed **on-demand** (not cached in `PeerIdentity`)
- Each peer pair has a unique secret
- `EcPeers` will cache computed secrets for performance
- Allows incremental construction without circular dependencies

**Security note:** Raw shared secret should be passed through HKDF before use as encryption key.

---

## Implementation Details

### Data Structures

```rust
pub struct PeerIdentity {
    static_secret: StaticSecret,      // X25519 private key
    public_key: PublicKey,            // X25519 public key
    salt: Option<Salt>,               // None until mined
    peer_id: Option<PeerId>,          // None until mined
    attempts: u64,                    // Mining statistics
    mining_duration_secs: f64,
}
```

### API

```rust
// Phase 1: Create with keypair
let mut identity = PeerIdentity::new();

// Can use for DH immediately
let secret = identity.compute_shared_secret(&peer.public_key);

// Phase 2: Mine for peer-id
identity.mine(AddressConfig::PRODUCTION);

// Query state
assert!(identity.is_mined());
let peer_id = identity.peer_id().unwrap();
let salt = identity.salt().unwrap();

// Validate peer
let valid = PeerIdentity::validate(
    &public_key,
    &salt,
    &peer_id,
    &AddressConfig::PRODUCTION
);
```

---

## Performance Characteristics

### Mining (One-Time)

| Configuration | Expected Time | Expected Attempts |
|--------------|---------------|-------------------|
| Test (4 bits) | ~0.01 seconds | 16 |
| Production (24 bits) | ~21 hours | 16.7 million |
| High Memory (23 bits) | ~44 hours | 8.4 million |
| Low Latency (26 bits) | ~18 hours | 67 million |

### Validation (Frequent)

| Configuration | Time per Peer | Throughput |
|--------------|---------------|------------|
| Test | 0.34ms | ~2900/sec |
| Production | 4.64ms | ~215/sec |
| High Memory | 18.99ms | ~55/sec |
| Low Latency | 1.18ms | ~850/sec |

### Network Impact

**Scenario:** Peer discovers 100 new peers and validates their identities

| Configuration | Total Validation Time |
|--------------|----------------------|
| Old Production (64 MiB) | **23.7 seconds** ❌ |
| New Production (4 MiB) | **0.46 seconds** ✅ |
| Low Latency (1 MiB) | **0.12 seconds** ✅ |

**Impact:** 50x faster peer discovery with optimized settings!

---

## Recommendations

### For Typical Networks (Default)
Use `AddressConfig::PRODUCTION`:
- Balanced memory usage (4 MiB)
- Fast validation (~5ms)
- Good ASIC resistance
- 24-hour mining time

### For High-Security Networks
Use `AddressConfig::HIGH_MEMORY`:
- Maximum ASIC resistance (16 MiB)
- Acceptable validation (~19ms)
- Longer mining time acceptable
- Defense in depth

### For High-Throughput Networks
Use `AddressConfig::LOW_LATENCY`:
- Minimum validation latency (~1ms)
- Very high throughput (~850/sec)
- Rely on difficulty for sybil resistance
- Less ASIC-resistant but very practical

---

## Future Considerations

### Dynamic Difficulty Adjustment

Could implement network-wide difficulty adjustment based on:
- Average mining time observed
- Network growth rate
- Computational power trends

### Multi-Tier Identities

Could support different difficulty tiers:
- Low difficulty: Limited permissions (read-only)
- Medium difficulty: Normal peer
- High difficulty: Enhanced permissions (validator, etc.)

### Hardware Acceleration

Could leverage GPU/specialized hardware for:
- Faster mining (users opt-in for faster join)
- Keep validation on CPU (preserve verification cost)

---

## Conclusion

The optimized Argon2 parameter selection achieves:
1. ✅ **Fast validation** (~5ms) - critical for network operation
2. ✅ **Strong sybil resistance** (~24 hour mining) - economic barrier
3. ✅ **Memory-hardness** (4 MiB) - ASIC resistance
4. ✅ **Two-phase creation** - keypair usable before mining completes
5. ✅ **Verifiable proof-of-work** - salt enables validation

**Key Insight:** Mining happens once, validation happens frequently. Optimize accordingly.

**Performance Gain:** 47x faster validation vs. naive password-hashing-inspired settings.
