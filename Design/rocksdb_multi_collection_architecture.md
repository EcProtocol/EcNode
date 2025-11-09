# RocksDB Multi-Collection Architecture Analysis

## Question
Should we use separate RocksDB instances for different collections (tokens, blocks) or use a single database with multiple "tables"?

## TL;DR Recommendation

**Use Column Families (single DB instance)** - This is the recommended approach for RocksDB.

### Quick Comparison

| Approach | Pros | Cons | Recommendation |
|----------|------|------|----------------|
| **Column Families** | Atomic operations across collections, shared cache, lower overhead, better for related data | Slightly more complex API | ✅ **Recommended** |
| **Separate DBs** | Complete isolation, independent tuning | Memory waste, no atomicity, more file handles | Use only if completely independent workloads |

---

## Detailed Analysis

### Approach 1: Column Families (Single DB, Multiple "Tables")

Column Families are RocksDB's native way to organize multiple collections within a single database instance.

#### Architecture

```rust
use rocksdb::{DB, Options, ColumnFamilyDescriptor};

pub struct EcRocksDb {
    db: Arc<DB>,
    // Column family handles are stored in the DB
}

impl EcRocksDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, rocksdb::Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Define column families with different tuning
        let cf_tokens = ColumnFamilyDescriptor::new("tokens", Self::tokens_options());
        let cf_blocks = ColumnFamilyDescriptor::new("blocks", Self::blocks_options());

        let db = DB::open_cf_descriptors(&opts, path, vec![cf_tokens, cf_blocks])?;

        Ok(Self { db: Arc::new(db) })
    }

    fn tokens_options() -> Options {
        let mut opts = Options::default();
        // Optimized for range scans (signature generation)
        opts.set_max_open_files(10000);
        opts.set_compression_per_level(&[
            rocksdb::DBCompressionType::None,
            rocksdb::DBCompressionType::None,
            rocksdb::DBCompressionType::Lz4,
        ]);
        opts
    }

    fn blocks_options() -> Options {
        let mut opts = Options::default();
        // Optimized for random access lookups
        opts.set_bloom_filter(10.0, false);
        opts
    }
}

// Separate backend structs that share the DB
pub struct RocksDbTokens {
    db: Arc<DB>,
}

pub struct RocksDbBlocks {
    db: Arc<DB>,
}

impl TokenStorageBackend for RocksDbTokens {
    fn lookup(&self, token: &TokenId) -> Option<BlockTime> {
        let cf = self.db.cf_handle("tokens").unwrap();
        let key = Self::encode_key(token);
        self.db.get_cf(cf, &key)
            .ok()
            .flatten()
            .and_then(|v| Self::decode_value(&v))
    }

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
        let cf = self.db.cf_handle("tokens").unwrap();
        let key = Self::encode_key(token);
        let value = Self::encode_value(&BlockTime { block: *block, time });
        let _ = self.db.put_cf(cf, &key, &value);
    }

    fn range_after(&self, start: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_> {
        let cf = self.db.cf_handle("tokens").unwrap();
        let start_key = Self::encode_key(start);
        let iter = self.db
            .iterator_cf(cf, IteratorMode::From(&start_key, Direction::Forward))
            .skip(1);
        Box::new(RocksDbIterator::new(iter))
    }

    // ... other methods
}

impl BlockStorageBackend for RocksDbBlocks {
    fn lookup(&self, block_id: &BlockId) -> Option<Block> {
        let cf = self.db.cf_handle("blocks").unwrap();
        let key = Self::encode_key(block_id);
        self.db.get_cf(cf, &key)
            .ok()
            .flatten()
            .and_then(|v| Self::decode_value(&v))
    }

    fn save(&mut self, block: &Block) {
        let cf = self.db.cf_handle("blocks").unwrap();
        let key = Self::encode_key(&block.id);
        let value = Self::encode_value(block);
        let _ = self.db.put_cf(cf, &key, &value);
    }
}
```

#### Benefits

1. **Shared Block Cache**: Both collections share the same memory pool
   - If tokens use 6 GB and blocks use 2 GB, you allocate 8 GB total
   - Cache automatically balances between hot data in both collections

2. **Atomic Operations**: Can write to both collections atomically
   ```rust
   let mut batch = WriteBatch::default();
   batch.put_cf(tokens_cf, token_key, token_value);
   batch.put_cf(blocks_cf, block_key, block_value);
   self.db.write(batch)?; // Both succeed or both fail
   ```

3. **Single WAL (Write-Ahead Log)**: Crash recovery is simpler
   - One log file for all collections
   - Consistent recovery point across all data

4. **Fewer File Handles**: One DB = one set of SST files per CF
   - Linux default: 1024 file descriptors per process
   - With separate DBs: Each DB can use 1000+ files

5. **Independent Tuning**: Each CF can have different settings
   - Tokens: Optimized for range scans
   - Blocks: Optimized for random lookups

6. **Lower Memory Overhead**: One memtable/compaction pipeline per CF
   - Separate DBs: Each has its own overhead (~100-500 MB)
   - Column Families: Shared infrastructure

#### Drawbacks

1. **Slightly More Complex API**: Need to specify CF handle in each operation
2. **CF List Must Be Known at Open**: Can't dynamically add CFs without reopening
3. **Compaction Can Block**: Heavy compaction in one CF can slow others

---

### Approach 2: Separate RocksDB Instances

Each collection gets its own completely independent database.

#### Architecture

```rust
pub struct RocksDbTokens {
    db: Arc<DB>,  // Opens "./data/tokens"
}

pub struct RocksDbBlocks {
    db: Arc<DB>,  // Opens "./data/blocks"
}

impl RocksDbTokens {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, rocksdb::Error> {
        let opts = Self::optimized_options();
        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }
}

impl RocksDbBlocks {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, rocksdb::Error> {
        let opts = Self::optimized_options();
        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }
}
```

#### Benefits

1. **Complete Isolation**: One DB crashing doesn't affect the other
2. **Simpler API**: No CF handles needed
3. **Independent Configuration**: Each DB has its own Options
4. **Process-Level Isolation**: Can run in separate processes if needed

#### Drawbacks

1. **No Atomic Operations Across Collections**
   ```rust
   // Cannot make this atomic:
   tokens_db.set(&token, &block_id, time)?;
   blocks_db.save(&block)?;
   // If second operation fails, first already committed!
   ```

2. **Duplicate Memory Usage**: Each DB allocates its own cache
   - Tokens DB: 8 GB cache
   - Blocks DB: 2 GB cache
   - **Total: 10 GB** (vs 8 GB with shared cache)

3. **More File Handles**: Each DB opens thousands of files
   - Risk hitting OS limits (ulimit -n)

4. **Higher Overhead**: Each DB has its own:
   - Write-ahead log
   - Memtable
   - Compaction threads
   - Background jobs

5. **No Cross-Collection Optimization**: Can't prioritize between collections

---

## Performance Comparison

### Memory Usage (100M Tokens + 10M Blocks)

| Resource | Column Families | Separate DBs | Savings |
|----------|----------------|--------------|---------|
| Block Cache | 8 GB (shared) | 10 GB (8+2) | **2 GB** |
| Memtables | 512 MB (2×256) | 768 MB (3×256) | **256 MB** |
| Bloom Filters | ~200 MB | ~220 MB | **20 MB** |
| **Total** | **~8.7 GB** | **~11 GB** | **~2.3 GB (21%)** |

### File Handles (at 100M scale)

| Resource | Column Families | Separate DBs |
|----------|----------------|--------------|
| SST Files | ~2000 (shared) | ~4000 (2×2000) |
| Log Files | 1 WAL | 2 WALs |
| Metadata | 1 set | 2 sets |
| **Total FDs** | **~2100** | **~4200** |

### Transaction Semantics

| Operation | Column Families | Separate DBs |
|-----------|----------------|--------------|
| Write token + block | ✅ Atomic via WriteBatch | ❌ Not atomic |
| Crash recovery | ✅ Consistent state | ⚠️ May diverge |
| Rollback | ✅ Can rollback batch | ❌ Can't rollback across DBs |

---

## Use Case Analysis for ecRust

### Tokens Storage
- **Size**: 10M - 1B entries (~720 MB - 72 GB on disk)
- **Access Pattern**: Read-heavy (95%+ reads), range scans for signatures
- **Updates**: Only when newer timestamp
- **Critical**: Must be consistent with blocks

### Blocks Storage
- **Size**: Fewer entries (~10M blocks max)
- **Access Pattern**: Random lookups, occasional scans
- **Updates**: Write-once (immutable after consensus)
- **Critical**: Must be consistent with token updates

### Consistency Requirements

In the consensus protocol:
```rust
// When a block commits, we need ATOMIC update:
fn commit_block(&mut self, block: &Block) {
    // 1. Save the block
    self.blocks.save(block);

    // 2. Update all token mappings in the block
    for token_block in &block.parts[0..block.used] {
        self.tokens.set(&token_block.token, &block.id, block.time);
    }

    // If either step fails, BOTH should rollback
    // Otherwise: inconsistent state!
}
```

**With Column Families**: ✅ Use WriteBatch for atomicity

**With Separate DBs**: ❌ No atomicity guarantee - risk of inconsistency!

---

## Recommendation: Column Families

### Why Column Families Win

1. **Atomicity is Critical**: Consensus requires consistent token↔block state
2. **Memory Efficiency**: 20%+ savings at scale
3. **File Handle Management**: Won't hit OS limits
4. **Production Standard**: This is how RocksDB is designed to be used

### When to Use Separate DBs

Only use separate databases if:
- ✅ Collections are **completely independent** (no cross-collection transactions)
- ✅ Different **lifecycle** (one is temporary, other is permanent)
- ✅ Different **security contexts** (one is encrypted, other isn't)
- ✅ Need **process isolation** (different processes own different DBs)

**For ecRust**: None of these apply. Tokens and blocks are tightly coupled.

---

## Implementation Guide

### Recommended Architecture

```
src/
├── ec_storage_rocksdb.rs       # Main DB with CF management
├── ec_tokens_rocksdb.rs        # TokenStorageBackend (uses "tokens" CF)
└── ec_blocks_rocksdb.rs        # BlockStorageBackend (uses "blocks" CF)
```

### Key Design Principles

1. **Single DB Instance**: `EcRocksDb` owns the DB, creates CFs
2. **Backend Structs Share DB**: Pass `Arc<DB>` to tokens/blocks backends
3. **CF-Specific Tuning**: Each CF optimized for its workload
4. **Atomic Commits**: Use `WriteBatch` for multi-collection updates
5. **Clean Abstraction**: Backends don't know about each other

### Migration Path

```rust
// Phase 1: Current implementation (in-memory)
let tokens = MemTokens::new();
let blocks = MemBlocks::new();

// Phase 2: RocksDB with column families
let db = EcRocksDb::open("./data")?;
let tokens = db.tokens_backend();
let blocks = db.blocks_backend();

// Same interface, different implementation!
```

---

## Performance Projections

### At 100M Tokens Scale

| Metric | Column Families | Separate DBs |
|--------|----------------|--------------|
| Memory | 8.7 GB | 11 GB |
| Disk Space | 7.2 GB | 7.5 GB |
| File Handles | ~2100 | ~4200 |
| Signature Latency | ~1 ms | ~1 ms (same) |
| Block Lookup | ~50 μs | ~50 μs (same) |
| Atomic Commit | ✅ Yes | ❌ No |

### Breaking Point Analysis

**Separate DBs become viable only at:**
- 10B+ tokens (separate machines required anyway)
- Completely different workload patterns (e.g., one is 100% writes, other 100% reads)
- Regulatory requirement for physical separation

**For typical distributed consensus (10M-1B tokens):**
- ✅ Column Families are superior

---

## References

1. [RocksDB Column Families Wiki](https://github.com/facebook/rocksdb/wiki/Column-Families)
2. [RocksDB Tuning Guide](https://github.com/facebook/rocksdb/wiki/RocksDB-Tuning-Guide)
3. [WriteBatch for Atomicity](https://github.com/facebook/rocksdb/wiki/Basic-Operations#atomic-updates)

---

## Conclusion

**Use Column Families for ecRust**. The consensus protocol requires atomic updates across tokens and blocks, making Column Families the clear choice. The 20% memory savings and file handle reduction are significant bonuses.

The implementation complexity difference is minimal (just adding `_cf` to method calls), while the benefits are substantial.
