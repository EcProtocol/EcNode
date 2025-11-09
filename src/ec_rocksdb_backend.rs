// RocksDB-based persistent storage for tokens and blocks using Column Families
//
// This module provides a production-ready persistent storage backend using RocksDB.
// It uses Column Families to store multiple collections (tokens, blocks) in a single
// database instance, enabling atomic operations and efficient memory usage.
//
// Architecture:
// - Single RocksDB instance with multiple Column Families
// - "tokens" CF: TokenId -> BlockTime mappings
// - "blocks" CF: BlockId -> Block data
// - Shared block cache and Write-Ahead Log
// - Atomic commits across both collections via WriteBatch

use rocksdb::{ColumnFamilyDescriptor, Direction, IteratorMode, Options, WriteBatch, DB};
use std::path::Path;
use std::sync::Arc;

use crate::ec_interface::{Block, BlockId, BlockTime, EcTime, TokenId, TOKENS_PER_BLOCK};
use crate::ec_proof_of_storage::TokenStorageBackend;

// Column family names
const CF_TOKENS: &str = "tokens";
const CF_BLOCKS: &str = "blocks";

/// Main RocksDB database with column families for tokens and blocks
///
/// This manages the shared database instance and provides access to
/// specialized backend structs for tokens and blocks.
///
/// # Example
/// ```rust
/// let db = EcRocksDb::open("./data")?;
/// let tokens = db.tokens_backend();
/// let blocks = db.blocks_backend();
///
/// // Use with ProofOfStorage
/// let proof_system = ProofOfStorage::new(tokens);
/// ```
pub struct EcRocksDb {
    db: Arc<DB>,
}

impl EcRocksDb {
    /// Open database with default settings (suitable for development/testing)
    pub fn open(path: impl AsRef<Path>) -> Result<Self, rocksdb::Error> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);

        let cf_tokens = ColumnFamilyDescriptor::new(CF_TOKENS, Self::tokens_cf_options());
        let cf_blocks = ColumnFamilyDescriptor::new(CF_BLOCKS, Self::blocks_cf_options());

        let db = DB::open_cf_descriptors(&db_opts, path, vec![cf_tokens, cf_blocks])?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Open database with optimized settings for production
    ///
    /// # Parameters
    /// - `path`: Database directory path
    /// - `cache_size_gb`: Shared block cache size in GB (recommend 4-16 GB)
    /// - `expected_tokens`: Expected number of tokens (helps size bloom filters)
    ///
    /// # Example
    /// ```rust
    /// // For 100M tokens with 8 GB shared cache
    /// let db = EcRocksDb::open_optimized("./data", 8, 100_000_000)?;
    /// ```
    pub fn open_optimized(
        path: impl AsRef<Path>,
        cache_size_gb: usize,
        _expected_tokens: usize,
    ) -> Result<Self, rocksdb::Error> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);

        // Shared block cache across all column families
        let cache_size = cache_size_gb * 1024 * 1024 * 1024;
        let cache = rocksdb::Cache::new_lru_cache(cache_size);

        let cf_tokens = ColumnFamilyDescriptor::new(
            CF_TOKENS,
            Self::tokens_cf_options_with_cache(cache.clone()),
        );
        let cf_blocks = ColumnFamilyDescriptor::new(
            CF_BLOCKS,
            Self::blocks_cf_options_with_cache(cache),
        );

        let db = DB::open_cf_descriptors(&db_opts, path, vec![cf_tokens, cf_blocks])?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Column family options for tokens (optimized for range scans)
    fn tokens_cf_options() -> Options {
        let mut opts = Options::default();

        // Keep many files open for faster range scans
        opts.set_max_open_files(10000);

        // Write buffer settings (moderate for read-heavy workload)
        opts.set_write_buffer_size(256 * 1024 * 1024); // 256 MB
        opts.set_max_write_buffer_number(4);

        // Compression (no compression for hot data in L0/L1)
        opts.set_compression_per_level(&[
            rocksdb::DBCompressionType::None, // L0
            rocksdb::DBCompressionType::None, // L1
            rocksdb::DBCompressionType::Lz4,  // L2+
        ]);

        opts
    }

    /// Column family options for tokens with shared cache
    fn tokens_cf_options_with_cache(cache: rocksdb::Cache) -> Options {
        let mut opts = Self::tokens_cf_options();

        let mut block_opts = rocksdb::BlockBasedOptions::default();
        block_opts.set_block_cache(&cache);
        block_opts.set_cache_index_and_filter_blocks(true);
        block_opts.set_bloom_filter(10.0, false);
        opts.set_block_based_table_factory(&block_opts);

        opts
    }

    /// Column family options for blocks (optimized for point lookups)
    fn blocks_cf_options() -> Options {
        let mut opts = Options::default();

        // Smaller write buffers (blocks are write-once)
        opts.set_write_buffer_size(128 * 1024 * 1024); // 128 MB
        opts.set_max_write_buffer_number(2);

        // Aggressive compression (blocks are immutable and large)
        opts.set_compression_per_level(&[
            rocksdb::DBCompressionType::Lz4,  // L0
            rocksdb::DBCompressionType::Lz4,  // L1
            rocksdb::DBCompressionType::Zstd, // L2+
        ]);

        opts
    }

    /// Column family options for blocks with shared cache
    fn blocks_cf_options_with_cache(cache: rocksdb::Cache) -> Options {
        let mut opts = Self::blocks_cf_options();

        let mut block_opts = rocksdb::BlockBasedOptions::default();
        block_opts.set_block_cache(&cache);
        block_opts.set_cache_index_and_filter_blocks(true);
        block_opts.set_bloom_filter(10.0, false);
        opts.set_block_based_table_factory(&block_opts);

        opts
    }

    /// Get tokens storage backend
    pub fn tokens_backend(&self) -> RocksDbTokens {
        RocksDbTokens {
            db: Arc::clone(&self.db),
        }
    }

    /// Get blocks storage backend
    pub fn blocks_backend(&self) -> RocksDbBlocks {
        RocksDbBlocks {
            db: Arc::clone(&self.db),
        }
    }

    /// Get database statistics for monitoring
    pub fn stats(&self) -> Option<String> {
        self.db.property_value("rocksdb.stats").ok().flatten()
    }

    /// Compact all column families (useful after bulk loading)
    pub fn compact_all(&self) {
        if let Some(cf) = self.db.cf_handle(CF_TOKENS) {
            self.db.compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
        }
        if let Some(cf) = self.db.cf_handle(CF_BLOCKS) {
            self.db.compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
        }
    }

    /// Atomically commit a block and update all associated token mappings
    ///
    /// This is a critical operation for consensus - both the block save and
    /// token updates must succeed or both must fail.
    ///
    /// # Example
    /// ```rust
    /// db.commit_block_atomic(&block)?;
    /// // Block is saved AND all token mappings are updated atomically
    /// ```
    pub fn commit_block_atomic(&self, block: &Block) -> Result<(), rocksdb::Error> {
        let mut batch = WriteBatch::default();

        let tokens_cf = self.db.cf_handle(CF_TOKENS).expect("tokens CF exists");
        let blocks_cf = self.db.cf_handle(CF_BLOCKS).expect("blocks CF exists");

        // 1. Save the block
        let block_key = RocksDbBlocks::encode_key(&block.id);
        let block_value = RocksDbBlocks::encode_value(block);
        batch.put_cf(blocks_cf, &block_key, &block_value);

        // 2. Update all token mappings in the block
        for i in 0..block.used as usize {
            let token_block = &block.parts[i];
            let token_key = RocksDbTokens::encode_key(&token_block.token);
            let token_value = RocksDbTokens::encode_value(&BlockTime {
                block: block.id,
                time: block.time,
            });
            batch.put_cf(tokens_cf, &token_key, &token_value);
        }

        // 3. Atomic write - all or nothing
        self.db.write(batch)
    }
}

// ============================================================================
// Tokens Storage Backend (Column Family: "tokens")
// ============================================================================

/// RocksDB-backed token storage using column families
///
/// Implements only CRUD operations - all signature logic is in ec_proof_of_storage.rs
pub struct RocksDbTokens {
    db: Arc<DB>,
}

impl RocksDbTokens {
    /// Encode a TokenId to bytes for storage (big-endian for lexicographic ordering)
    #[inline]
    fn encode_key(token: &TokenId) -> [u8; 8] {
        token.to_be_bytes()
    }

    /// Encode BlockTime to bytes for storage
    #[inline]
    fn encode_value(block_time: &BlockTime) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16);
        buf.extend_from_slice(&block_time.block.to_be_bytes());
        buf.extend_from_slice(&block_time.time.to_be_bytes());
        buf
    }

    /// Decode bytes to BlockTime
    #[inline]
    fn decode_value(bytes: &[u8]) -> Option<BlockTime> {
        if bytes.len() < 16 {
            return None;
        }

        let block = u64::from_be_bytes(bytes[0..8].try_into().ok()?);
        let time = u64::from_be_bytes(bytes[8..16].try_into().ok()?);

        Some(BlockTime { block, time })
    }

    /// Get column family handle for tokens
    #[inline]
    fn cf_handle(&self) -> &rocksdb::ColumnFamily {
        self.db.cf_handle(CF_TOKENS).expect("tokens CF should exist")
    }
}

impl TokenStorageBackend for RocksDbTokens {
    fn lookup(&self, token: &TokenId) -> Option<BlockTime> {
        let cf = self.cf_handle();
        let key = Self::encode_key(token);
        self.db
            .get_cf(cf, &key)
            .ok()
            .flatten()
            .and_then(|value| Self::decode_value(&value))
    }

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
        let cf = self.cf_handle();
        let key = Self::encode_key(token);

        // Check if we should update (only if newer)
        let should_update = if let Ok(Some(existing)) = self.db.get_cf(cf, &key) {
            if let Some(existing_bt) = Self::decode_value(&existing) {
                time > existing_bt.time
            } else {
                true
            }
        } else {
            true
        };

        if should_update {
            let value = Self::encode_value(&BlockTime {
                block: *block,
                time,
            });
            let _ = self.db.put_cf(cf, &key, &value);
        }
    }

    fn range_after(&self, start: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_> {
        let cf = self.cf_handle();
        let start_key = Self::encode_key(start);
        let iter = self
            .db
            .iterator_cf(cf, IteratorMode::From(&start_key, Direction::Forward))
            .skip(1); // Skip the start key itself

        Box::new(RocksDbTokenIterator::new(iter))
    }

    fn range_before(&self, end: &TokenId) -> Box<dyn Iterator<Item = (TokenId, BlockTime)> + '_> {
        let cf = self.cf_handle();
        let end_key = Self::encode_key(end);
        let iter = self
            .db
            .iterator_cf(cf, IteratorMode::From(&end_key, Direction::Reverse))
            .skip(1); // Skip the end key itself

        Box::new(RocksDbTokenIterator::new(iter))
    }

    fn len(&self) -> usize {
        let cf = self.cf_handle();
        // RocksDB doesn't have fast len() - use approximate count
        self.db
            .property_int_value_cf(cf, "rocksdb.estimate-num-keys")
            .unwrap_or(None)
            .unwrap_or(0) as usize
    }
}

/// Iterator for token range scans
struct RocksDbTokenIterator<'a> {
    inner: std::iter::Skip<rocksdb::DBIterator<'a>>,
}

impl<'a> RocksDbTokenIterator<'a> {
    fn new(inner: std::iter::Skip<rocksdb::DBIterator<'a>>) -> Self {
        Self { inner }
    }
}

impl<'a> Iterator for RocksDbTokenIterator<'a> {
    type Item = (TokenId, BlockTime);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().and_then(|result| {
            result.ok().and_then(|(key, value)| {
                // Decode key
                let token_bytes: [u8; 8] = key.as_ref().try_into().ok()?;
                let token = u64::from_be_bytes(token_bytes);

                // Decode value
                let block_time = RocksDbTokens::decode_value(&value)?;

                Some((token, block_time))
            })
        })
    }
}

// ============================================================================
// Blocks Storage Backend (Column Family: "blocks")
// ============================================================================

/// RocksDB-backed block storage using column families
///
/// Implements EcBlocks trait for persistent block storage
pub struct RocksDbBlocks {
    db: Arc<DB>,
}

impl RocksDbBlocks {
    /// Encode a BlockId to bytes for storage (big-endian)
    #[inline]
    fn encode_key(block_id: &BlockId) -> [u8; 8] {
        block_id.to_be_bytes()
    }

    /// Encode Block to bytes for storage
    ///
    /// Format:
    /// - 8 bytes: block.id (u64)
    /// - 8 bytes: block.time (u64)
    /// - 1 byte: block.used (u8)
    /// - For each part in parts[0..TOKENS_PER_BLOCK]:
    ///   - 8 bytes: token (u64)
    ///   - 8 bytes: last (u64)
    ///   - 8 bytes: key (u64)
    /// - For each signature in signatures[0..TOKENS_PER_BLOCK]:
    ///   - 1 byte: is_some flag
    ///   - 8 bytes: signature value (if is_some)
    fn encode_value(block: &Block) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256); // Pre-allocate enough space

        // Block header
        buf.extend_from_slice(&block.id.to_be_bytes());
        buf.extend_from_slice(&block.time.to_be_bytes());
        buf.push(block.used);

        // Token blocks (always encode all TOKENS_PER_BLOCK slots)
        for part in &block.parts {
            buf.extend_from_slice(&part.token.to_be_bytes());
            buf.extend_from_slice(&part.last.to_be_bytes());
            buf.extend_from_slice(&part.key.to_be_bytes());
        }

        // Signatures (encode as Option)
        for sig in &block.signatures {
            match sig {
                Some(s) => {
                    buf.push(1); // is_some flag
                    buf.extend_from_slice(&s.to_be_bytes());
                }
                None => {
                    buf.push(0); // is_none flag
                }
            }
        }

        buf
    }

    /// Decode bytes to Block
    fn decode_value(bytes: &[u8]) -> Option<Block> {
        if bytes.len() < 17 {
            return None;
        }

        let mut offset = 0;

        // Block header
        let id = u64::from_be_bytes(bytes[offset..offset + 8].try_into().ok()?);
        offset += 8;
        let time = u64::from_be_bytes(bytes[offset..offset + 8].try_into().ok()?);
        offset += 8;
        let used = bytes[offset];
        offset += 1;

        // Token blocks
        let mut parts = [crate::ec_interface::TokenBlock::default(); TOKENS_PER_BLOCK];
        for part in &mut parts {
            if offset + 24 > bytes.len() {
                return None;
            }
            part.token = u64::from_be_bytes(bytes[offset..offset + 8].try_into().ok()?);
            offset += 8;
            part.last = u64::from_be_bytes(bytes[offset..offset + 8].try_into().ok()?);
            offset += 8;
            part.key = u64::from_be_bytes(bytes[offset..offset + 8].try_into().ok()?);
            offset += 8;
        }

        // Signatures
        let mut signatures = [None; TOKENS_PER_BLOCK];
        for sig in &mut signatures {
            if offset >= bytes.len() {
                return None;
            }
            let is_some = bytes[offset];
            offset += 1;

            if is_some == 1 {
                if offset + 8 > bytes.len() {
                    return None;
                }
                let value = u64::from_be_bytes(bytes[offset..offset + 8].try_into().ok()?);
                offset += 8;
                *sig = Some(value);
            }
        }

        Some(Block {
            id,
            time,
            used,
            parts,
            signatures,
        })
    }

    /// Get column family handle for blocks
    #[inline]
    fn cf_handle(&self) -> &rocksdb::ColumnFamily {
        self.db.cf_handle(CF_BLOCKS).expect("blocks CF should exist")
    }
}

impl crate::ec_interface::EcBlocks for RocksDbBlocks {
    fn lookup(&self, block_id: &BlockId) -> Option<Block> {
        let cf = self.cf_handle();
        let key = Self::encode_key(block_id);
        self.db
            .get_cf(cf, &key)
            .ok()
            .flatten()
            .and_then(|value| Self::decode_value(&value))
    }

    fn exists(&self, block_id: &BlockId) -> bool {
        let cf = self.cf_handle();
        let key = Self::encode_key(block_id);
        self.db.get_cf(cf, &key).ok().flatten().is_some()
    }

    fn save(&mut self, block: &Block) {
        let cf = self.cf_handle();
        let key = Self::encode_key(&block.id);
        let value = Self::encode_value(block);
        let _ = self.db.put_cf(cf, &key, &value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ec_interface::TokenBlock;
    use tempfile::TempDir;

    #[test]
    fn test_rocksdb_cf_open() {
        let dir = TempDir::new().unwrap();
        let db = EcRocksDb::open(dir.path()).unwrap();

        let tokens = db.tokens_backend();
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_tokens_basic_operations() {
        let dir = TempDir::new().unwrap();
        let db = EcRocksDb::open(dir.path()).unwrap();
        let mut tokens = db.tokens_backend();

        tokens.set(&100, &1, 42);
        assert!(tokens.len() > 0);

        let result = tokens.lookup(&100);
        assert!(result.is_some());
        let block_time = result.unwrap();
        assert_eq!(block_time.block, 1);
        assert_eq!(block_time.time, 42);
    }

    #[test]
    fn test_tokens_range_iteration() {
        let dir = TempDir::new().unwrap();
        let db = EcRocksDb::open(dir.path()).unwrap();
        let mut tokens = db.tokens_backend();

        // Insert test data
        for i in 0..10 {
            tokens.set(&(i * 100), &i, i);
        }

        // Test range_after
        let result: Vec<_> = tokens.range_after(&250).take(3).map(|(t, _)| t).collect();
        assert_eq!(result, vec![300, 400, 500]);

        // Test range_before
        let result: Vec<_> = tokens.range_before(&250).take(3).map(|(t, _)| t).collect();
        assert_eq!(result, vec![200, 100, 0]);
    }

    #[test]
    fn test_blocks_basic_operations() {
        let dir = TempDir::new().unwrap();
        let db = EcRocksDb::open(dir.path()).unwrap();
        let mut blocks = db.blocks_backend();

        let block = Block {
            id: 123,
            time: 1000,
            used: 2,
            parts: [
                TokenBlock {
                    token: 1,
                    last: 0,
                    key: 100,
                },
                TokenBlock {
                    token: 2,
                    last: 0,
                    key: 200,
                },
                TokenBlock::default(),
                TokenBlock::default(),
                TokenBlock::default(),
                TokenBlock::default(),
            ],
            signatures: [None, None, None, None, None, None],
        };

        blocks.save(&block);
        assert!(blocks.exists(&123));

        let retrieved = blocks.lookup(&123).unwrap();
        assert_eq!(retrieved.id, 123);
        assert_eq!(retrieved.time, 1000);
        assert_eq!(retrieved.used, 2);
        assert_eq!(retrieved.parts[0].token, 1);
        assert_eq!(retrieved.parts[1].token, 2);
    }

    #[test]
    fn test_atomic_commit() {
        let dir = TempDir::new().unwrap();
        let db = EcRocksDb::open(dir.path()).unwrap();

        let block = Block {
            id: 999,
            time: 5000,
            used: 3,
            parts: [
                TokenBlock {
                    token: 10,
                    last: 0,
                    key: 1000,
                },
                TokenBlock {
                    token: 20,
                    last: 0,
                    key: 2000,
                },
                TokenBlock {
                    token: 30,
                    last: 0,
                    key: 3000,
                },
                TokenBlock::default(),
                TokenBlock::default(),
                TokenBlock::default(),
            ],
            signatures: [None, None, None, None, None, None],
        };

        // Atomic commit
        db.commit_block_atomic(&block).unwrap();

        // Verify block was saved
        let blocks = db.blocks_backend();
        assert!(blocks.exists(&999));

        // Verify all tokens were updated
        let tokens = db.tokens_backend();
        assert_eq!(tokens.lookup(&10).unwrap().block, 999);
        assert_eq!(tokens.lookup(&20).unwrap().block, 999);
        assert_eq!(tokens.lookup(&30).unwrap().block, 999);
    }

    #[test]
    fn test_encoding_preserves_order() {
        let tokens = [1u64, 100, 1000, 10000, 100000];

        let encoded: Vec<_> = tokens.iter().map(|t| RocksDbTokens::encode_key(t)).collect();

        // Encoded bytes should maintain sort order
        for i in 1..encoded.len() {
            assert!(encoded[i - 1] < encoded[i]);
        }
    }
}
