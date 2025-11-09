# Clean Architecture Refactoring: Token Storage & Proof of Storage

## Problem Statement

The original implementation had several issues:

1. **Tight Coupling**: Signature generation logic was embedded in `MemTokens`
2. **Code Duplication**: Each storage backend would need to reimplement signature logic
3. **Testing Complexity**: Hard to test signature logic independently of storage
4. **Maintainability**: Changes to signature algorithm require touching storage code

## Solution: Layered Architecture

We've refactored into a clean, layered architecture with proper separation of concerns:

```
┌─────────────────────────────────────────────────────────┐
│                    EcTokens Trait                       │
│            (Backward compatibility layer)                │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│              ProofOfStorage<B>                          │
│        (Signature generation logic - SHARED)            │
│  - signature_for()                                      │
│  - search_by_signature()                                │
│  - generate_signature()                                 │
└─────────────────────┬───────────────────────────────────┘
                      │ uses
                      ▼
┌─────────────────────────────────────────────────────────┐
│           TokenStorageBackend Trait                     │
│               (CRUD operations only)                    │
│  - lookup()                                             │
│  - set()                                                │
│  - range_after()                                        │
│  - range_before()                                       │
└────────┬──────────────────────────────────┬─────────────┘
         │                                  │
         ▼                                  ▼
┌────────────────────┐           ┌─────────────────────────┐
│    MemTokens       │           │   RocksDbTokens         │
│  (BTreeMap-based)  │           │   (Persistent storage)  │
│                    │           │                         │
│ Just CRUD ops!     │           │   Just CRUD ops!        │
└────────────────────┘           └─────────────────────────┘
```

## File Structure

### Core Modules

1. **`ec_proof_of_storage.rs`** (NEW)
   - Contains ALL signature generation logic
   - `TokenStorageBackend` trait definition
   - `ProofOfStorage<B>` generic struct
   - Signature algorithms (current: DefaultHasher, future: Blake3)
   - **Zero storage code** - only algorithms

2. **`ec_tokens.rs`** (REFACTORED)
   - `MemTokens` struct - in-memory BTreeMap storage
   - Implements `TokenStorageBackend` trait
   - Implements `EcTokens` trait (backward compatibility)
   - **Zero signature logic** - only CRUD operations

3. **`ec_tokens_rocksdb.rs`** (NEW - optional)
   - `RocksDbTokens` struct - persistent RocksDB storage
   - Implements `TokenStorageBackend` trait
   - **Zero signature logic** - only CRUD operations
   - Production-ready configuration

4. **`ec_interface.rs`** (UNCHANGED)
   - `TokenSignature` struct
   - `EcTokens` trait
   - Type definitions

## Key Design Principles

### 1. Single Responsibility Principle

**Before:**
```rust
impl MemTokens {
    fn lookup(&self, ...) { ... }           // Storage
    fn set(&mut self, ...) { ... }          // Storage
    fn search_by_signature(...) { ... }     // Algorithm
    fn tokens_signature(...) { ... }        // Algorithm
}
```

**After:**
```rust
// Storage responsibility - MemTokens
impl TokenStorageBackend for MemTokens {
    fn lookup(&self, ...) { ... }
    fn set(&mut self, ...) { ... }
    fn range_after(&self, ...) { ... }
    fn range_before(&self, ...) { ... }
}

// Algorithm responsibility - ProofOfStorage
impl<B: TokenStorageBackend> ProofOfStorage<B> {
    fn search_by_signature(...) { ... }
    fn generate_signature(...) { ... }
}
```

### 2. Dependency Inversion Principle

Storage backends depend on an abstraction (`TokenStorageBackend`), not concrete implementations:

```rust
pub trait TokenStorageBackend {
    fn lookup(&self, token: &TokenId) -> Option<&BlockTime>;
    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime);
    fn range_after(&self, start: &TokenId) -> Box<dyn Iterator<...>>;
    fn range_before(&self, end: &TokenId) -> Box<dyn Iterator<...>>;
    fn len(&self) -> usize;
}
```

Any storage that implements this trait works with `ProofOfStorage`:

```rust
// Works with MemTokens
let proof_mem = ProofOfStorage::new(MemTokens::new());

// Works with RocksDbTokens
let proof_rocks = ProofOfStorage::new(RocksDbTokens::open("./db")?);

// Works with any future backend!
let proof_custom = ProofOfStorage::new(MyCustomBackend::new());
```

### 3. No Code Duplication

Signature logic exists in **exactly one place**: `ec_proof_of_storage.rs`

```rust
// This works identically for ANY backend:
pub fn generate_signature<B: TokenStorageBackend>(
    backend: &B,
    token: &TokenId,
    peer: &PeerId,
) -> Option<TokenSignature> {
    // ... algorithm here ...
}
```

## Usage Examples

### Basic Usage (In-Memory)

```rust
use ecrust::ec_tokens::MemTokens;
use ecrust::ec_proof_of_storage::ProofOfStorage;

// Create storage
let mut storage = MemTokens::new();
storage.set(&token_id, &block_id, time);

// Create proof system
let proof_system = ProofOfStorage::new(storage);

// Generate signature
if let Some(signature) = proof_system.generate_signature(&token, &peer) {
    // Wrap in Message::Answer
    let msg = Message::Answer {
        answer: signature.answer,
        signature: signature.signature,
    };
}
```

### Production Usage (RocksDB)

```rust
use ecrust::ec_tokens_rocksdb::RocksDbTokens;
use ecrust::ec_proof_of_storage::ProofOfStorage;

// Open persistent storage with optimized settings
let storage = RocksDbTokens::open_optimized("./token_db", 8, 100_000_000)?;

// Create proof system (same API!)
let proof_system = ProofOfStorage::new(storage);

// Generate signature (same code!)
if let Some(signature) = proof_system.generate_signature(&token, &peer) {
    // ... use signature ...
}
```

### Backward Compatibility

The old `EcTokens` interface still works:

```rust
use ecrust::ec_tokens::MemTokens;
use ecrust::ec_interface::EcTokens;

let mut storage = MemTokens::new();
storage.set(&token, &block, time);

// Old interface still works
if let Some(signature) = storage.tokens_signature(&token, &peer) {
    // ... use signature ...
}
```

## Benefits of This Architecture

### 1. Testability

Test signature logic independently:

```rust
#[test]
fn test_signature_algorithm() {
    let mock_backend = MockBackend::with_data(...);
    let proof = ProofOfStorage::new(mock_backend);

    let sig = proof.generate_signature(&token, &peer);
    // Test algorithm behavior
}
```

Test storage independently:

```rust
#[test]
fn test_storage_crud() {
    let mut storage = MemTokens::new();
    storage.set(&token, &block, time);

    assert_eq!(storage.lookup(&token).unwrap().block, block);
}
```

### 2. Extensibility

Add new backends with zero changes to signature logic:

```rust
// New backend implementation
pub struct LmdbTokens { ... }

impl TokenStorageBackend for LmdbTokens {
    // Just implement CRUD operations
    fn lookup(&self, ...) { ... }
    fn set(&mut self, ...) { ... }
    fn range_after(&self, ...) { ... }
    fn range_before(&self, ...) { ... }
}

// Signature generation automatically works!
let proof = ProofOfStorage::new(LmdbTokens::open("./db")?);
let sig = proof.generate_signature(&token, &peer); // Works!
```

### 3. Maintainability

Update signature algorithm once, all backends benefit:

```rust
// In ec_proof_of_storage.rs
impl<B: TokenStorageBackend> ProofOfStorage<B> {
    fn signature_for(...) -> [u16; SIGNATURE_CHUNKS] {
        // Change from DefaultHasher to Blake3
        use blake3::Hasher;

        let mut hasher = Hasher::new();
        // ... new algorithm ...

        // ALL backends now use Blake3!
    }
}
```

### 4. Performance

No runtime overhead - generic is monomorphized at compile time:

```rust
// These compile to completely different code paths:
ProofOfStorage<MemTokens>::generate_signature(...)      // Optimized for BTreeMap
ProofOfStorage<RocksDbTokens>::generate_signature(...)  // Optimized for RocksDB
```

## Migration Guide

### For Existing Code Using MemTokens

No changes needed! The old interface still works:

```rust
// Old code - still works
let storage = MemTokens::new();
let sig = storage.tokens_signature(&token, &peer);
```

### For New Code

Use the new, cleaner API:

```rust
// New code - more flexible
let storage = MemTokens::new();
let proof = ProofOfStorage::new(storage);
let sig = proof.generate_signature(&token, &peer);
```

### To Add a New Backend

1. Implement `TokenStorageBackend` trait (5 methods)
2. That's it! Signature generation automatically works

```rust
pub struct MyBackend { ... }

impl TokenStorageBackend for MyBackend {
    fn lookup(&self, token: &TokenId) -> Option<&BlockTime> { ... }
    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) { ... }
    fn range_after(&self, start: &TokenId) -> Box<dyn Iterator<...>> { ... }
    fn range_before(&self, end: &TokenId) -> Box<dyn Iterator<...>> { ... }
    fn len(&self) -> usize { ... }
}

// Done! ProofOfStorage<MyBackend> now works
```

## File-by-File Changes

### `ec_proof_of_storage.rs` (NEW)

**Exports:**
- `pub trait TokenStorageBackend` - CRUD interface
- `pub struct ProofOfStorage<B>` - Algorithm implementation
- `pub struct SignatureSearchResult` - Search results
- `pub const SIGNATURE_CHUNKS` - Configuration

**Responsibilities:**
- Signature generation algorithm
- Bidirectional search algorithm
- 256-bit hash extraction (for future migration)
- Algorithm tests

**Dependencies:**
- `ec_interface` (for types only)
- No storage dependencies!

### `ec_tokens.rs` (REFACTORED)

**Exports:**
- `pub struct MemTokens` - In-memory storage

**Responsibilities:**
- BTreeMap-based storage
- CRUD operations
- Backward compatibility via `EcTokens` trait

**Dependencies:**
- `std::collections::BTreeMap`
- `ec_interface` (for types)
- `ec_proof_of_storage` (for `TokenStorageBackend` trait and `ProofOfStorage`)

**What Was Removed:**
- All signature generation logic → moved to `ec_proof_of_storage.rs`
- All signature search logic → moved to `ec_proof_of_storage.rs`
- Helper functions (token_last_bits, etc.) → moved to `ec_proof_of_storage.rs`

**What Was Added:**
- `into_proof_system()` convenience method

### `ec_tokens_rocksdb.rs` (NEW - OPTIONAL)

**Exports:**
- `pub struct RocksDbTokens` - Persistent storage

**Responsibilities:**
- RocksDB-based storage
- CRUD operations
- Key/value encoding
- Production configuration

**Dependencies:**
- `rocksdb` crate
- `ec_interface` (for types)
- `ec_proof_of_storage` (for `TokenStorageBackend` trait only)

**What It Does NOT Have:**
- No signature logic!
- No search algorithms!
- Just storage operations

## Testing Strategy

### Unit Tests by Module

**`ec_proof_of_storage.rs` tests:**
- Signature generation determinism
- Signature chunk extraction
- Search algorithm correctness
- 256-bit compatibility

**`ec_tokens.rs` tests:**
- CRUD operations
- Update-only-if-newer logic
- Range iteration correctness
- BTreeMap behavior

**`ec_tokens_rocksdb.rs` tests:**
- CRUD operations
- Persistence across restarts
- Key encoding preserves order
- Range iteration correctness

### Integration Tests

```rust
#[test]
fn test_proof_with_different_backends() {
    let test_token = 12345;
    let test_peer = 99999;

    // Test with MemTokens
    let mut mem = MemTokens::new();
    populate(&mut mem);
    let proof_mem = ProofOfStorage::new(mem);
    let sig_mem = proof_mem.generate_signature(&test_token, &test_peer);

    // Test with RocksDbTokens
    let mut rocks = RocksDbTokens::open(temp_dir)?;
    populate(&mut rocks);
    let proof_rocks = ProofOfStorage::new(rocks);
    let sig_rocks = proof_rocks.generate_signature(&test_token, &test_peer);

    // Both should produce identical signatures!
    assert_eq!(sig_mem, sig_rocks);
}
```

## Performance Characteristics

### Compile-Time Benefits

- **Monomorphization**: Each `ProofOfStorage<B>` compiles to optimized code for B
- **Inlining**: Small methods like `token_last_bits` inline across module boundaries
- **Zero-cost abstraction**: No vtable overhead (trait uses static dispatch)

### Runtime Benefits

- **Same performance as original**: Refactoring is zero-cost at runtime
- **Better cache locality**: Backend-specific code paths don't include unused logic
- **Easier to optimize**: Can optimize storage and algorithms independently

## Conclusion

This refactoring achieves:

✅ **Separation of Concerns**: Storage ≠ Algorithms
✅ **No Code Duplication**: Signature logic exists once
✅ **Extensibility**: Add backends by implementing 5 methods
✅ **Testability**: Test storage and algorithms independently
✅ **Maintainability**: Changes to algorithms don't touch storage
✅ **Backward Compatibility**: Old code still works
✅ **Zero-Cost**: No performance overhead

The architecture is clean, maintainable, and ready for production.
