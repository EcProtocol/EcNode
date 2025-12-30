//! Peer Identity and Address Generation
//!
//! This module provides cryptographic peer identity generation using:
//! - **X25519 key pairs** for Diffie-Hellman key exchange
//! - **Argon2 proof-of-work** for sybil-resistant address mining
//!
//! # Architecture
//!
//! ## Mining Process
//! 1. Generate X25519 keypair **once** (for peer communication)
//! 2. Try random salts until `Argon2(public_key, salt)` meets difficulty
//! 3. The resulting hash becomes the peer-id/address
//!
//! ## Key Components
//! - **Keypair (fixed)**: Used for all DH key exchanges with other peers
//! - **Salt (mined)**: Proof-of-work nonce that must be transmitted with messages
//! - **Peer-id**: 256-bit hash result used as network address
//!
//! ## Production Message Flow
//!
//! When sending Answer or Referral messages, the salt must be included so recipients can:
//! 1. **Validate**: Verify `peer_id == Argon2(public_key, salt)` with required difficulty
//! 2. **Route**: Compute ring addresses for DHT-based peer discovery
//!
//! ### Future Message Extensions (production)
//! ```rust,ignore
//! Message::Answer {
//!     answer: TokenMapping,
//!     signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
//!     sender_public_key: PublicKey,  // X25519 public key
//!     sender_salt: Salt,              // Proof-of-work salt
//!     head_of_chain: CommitBlockId,
//! }
//!
//! Message::Referral {
//!     token: TokenId,
//!     high: PeerId,
//!     low: PeerId,
//!     high_public_key: PublicKey,     // For high peer
//!     high_salt: Salt,                // For high peer validation
//!     low_public_key: PublicKey,      // For low peer
//!     low_salt: Salt,                 // For low peer validation
//! }
//! ```
//!
//! ## Shared Secrets
//!
//! Shared secrets are computed **on-demand** (not stored in `PeerIdentity`):
//! - Computed via `identity.compute_shared_secret(&their_public_key)`
//! - Cached by `EcPeers` for performance
//! - Each peer pair has a unique symmetric secret

use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2, Params, Version,
};
use rand::rngs::OsRng;
use std::time::Instant;
use x25519_dalek::{PublicKey, StaticSecret};

/// Peer address (256-bit identifier derived from proof-of-work)
pub type PeerId = [u8; 32];

/// Salt for Argon2 hashing (128 bits)
pub type Salt = [u8; 16];

/// Shared secret from Diffie-Hellman key exchange (256 bits)
pub type SharedSecret = [u8; 32];

/// Configuration for address mining difficulty
#[derive(Debug, Clone, Copy)]
pub struct AddressConfig {
    /// Number of trailing zero bits required in the hash
    pub difficulty: u32,
    /// Argon2 memory cost in KiB
    pub memory_cost: u32,
    /// Argon2 time cost (iterations)
    pub time_cost: u32,
    /// Argon2 parallelism
    pub parallelism: u32,
}

impl AddressConfig {
    /// Test configuration: fast mining for development
    /// Expected time: ~1-10 seconds for 4 trailing zero bits
    pub const TEST: Self = AddressConfig {
        difficulty: 4,        // 4 trailing zero bits
        memory_cost: 256,     // 256 KiB
        time_cost: 1,         // 1 iteration
        parallelism: 1,       // Single thread
    };

    /// Production configuration: ~1 day of computation expected
    ///
    /// **Design Philosophy:**
    /// - Validation happens FREQUENTLY (every Answer/Referral message)
    /// - Mining happens ONCE (when joining network)
    /// - Therefore: LOW Argon2 cost + HIGH difficulty for optimal performance
    ///
    /// **Settings:**
    /// - Memory: 4 MiB (still memory-hard, but fast validation ~5ms)
    /// - Time cost: 1 iteration (no redundant work during validation)
    /// - Difficulty: 24 bits (~16 million attempts for 24-hour mining)
    ///
    /// **Performance:**
    /// - Mining time: ~24 hours on modern CPU
    /// - Validation time: ~5ms per peer
    /// - Throughput: ~200 peer validations/sec
    ///
    /// **Comparison to Password Hashing:**
    /// - OWASP Argon2 for passwords: 19-46 MiB, 2 iterations (~20-50ms)
    /// - We use lower memory because validation is frequent, not one-time login
    /// - We compensate with higher difficulty (24 bits vs typical password requirements)
    pub const PRODUCTION: Self = AddressConfig {
        difficulty: 24,       // 24 trailing zero bits (~16.7M attempts)
        memory_cost: 4096,    // 4 MiB (balanced: memory-hard but fast validation)
        time_cost: 1,         // 1 iteration (validation happens frequently)
        parallelism: 1,       // Single thread
    };

    /// Alternative: Maximum ASIC resistance (higher memory)
    /// - Validation: ~18ms per peer (~55/sec)
    /// - Mining: ~24 hours with 23 bits difficulty
    /// - Use if ASIC resistance is more important than validation speed
    pub const HIGH_MEMORY: Self = AddressConfig {
        difficulty: 23,       // 23 bits (~8.4M attempts)
        memory_cost: 16384,   // 16 MiB (strong memory-hardness)
        time_cost: 1,
        parallelism: 1,
    };

    /// Alternative: Maximum validation speed (low latency)
    /// - Validation: ~1ms per peer (~850/sec)
    /// - Mining: ~24 hours with 26 bits difficulty
    /// - Use for high-throughput networks where validation is critical
    pub const LOW_LATENCY: Self = AddressConfig {
        difficulty: 26,       // 26 bits (~67M attempts)
        memory_cost: 1024,    // 1 MiB (faster validation)
        time_cost: 1,
        parallelism: 1,
    };
}

/// Peer identity with X25519 keypair
///
/// # Lifecycle
/// 1. **Created** - `PeerIdentity::new()` generates keypair, ready for DH key exchange
/// 2. **Mining** - `identity.mine(config)` finds salt to create valid peer-id
/// 3. **Complete** - Both communication (DH) and addressing (peer-id) available
///
/// The keypair is available immediately after creation, allowing peer communication
/// even before mining completes.
pub struct PeerIdentity {
    /// X25519 static secret (private key for DH)
    pub static_secret: StaticSecret,
    /// X25519 public key (derived from static secret)
    pub public_key: PublicKey,
    /// Random salt used in proof-of-work (None until mined)
    pub salt: Option<Salt>,
    /// Derived peer-id (256-bit address, None until mined)
    pub peer_id: Option<PeerId>,
    /// Number of attempts needed to mine this address
    pub attempts: u64,
    /// Time taken to mine this address
    pub mining_duration_secs: f64,
}

// Manual Debug implementation that excludes the secret key
impl std::fmt::Debug for PeerIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerIdentity")
            .field("public_key", &format_args!("{:?}", self.public_key.as_bytes()))
            .field("salt", &self.salt)
            .field("peer_id", &self.peer_id)
            .field("attempts", &self.attempts)
            .field("mining_duration_secs", &self.mining_duration_secs)
            .finish_non_exhaustive()
    }
}

impl PeerIdentity {
    /// Create a new peer identity with X25519 keypair
    ///
    /// The keypair is generated immediately and can be used for Diffie-Hellman
    /// key exchange with other peers. Mining (to obtain peer-id) is a separate step.
    ///
    /// # Example
    /// ```rust
    /// use ec_rust::{PeerIdentity, AddressConfig};
    ///
    /// // Create identity - keypair ready for use
    /// let mut identity = PeerIdentity::new();
    ///
    /// // Can compute shared secrets immediately
    /// // let secret = identity.compute_shared_secret(&other_peer.public_key);
    ///
    /// // Mine for peer-id (this takes time)
    /// identity.mine(AddressConfig::TEST);
    ///
    /// // Now have complete identity with peer-id
    /// assert!(identity.peer_id().is_some());
    /// ```
    pub fn new() -> Self {
        let static_secret = StaticSecret::random_from_rng(OsRng);
        let public_key = PublicKey::from(&static_secret);

        log::debug!("Generated X25519 keypair for peer identity");

        PeerIdentity {
            static_secret,
            public_key,
            salt: None,
            peer_id: None,
            attempts: 0,
            mining_duration_secs: 0.0,
        }
    }

    /// Mine for peer-id using proof-of-work
    ///
    /// Tries different random salts until `Argon2(public_key, salt)` meets the
    /// difficulty requirement. Updates this identity with the mined salt and peer-id.
    ///
    /// # Process
    /// 1. Try random salts (public key is fixed)
    /// 2. Hash with Argon2 until difficulty is met
    /// 3. Store winning salt and resulting peer-id
    ///
    /// # Salt Transmission
    /// The mined salt must be transmitted with Answer/Referral messages so other peers can:
    /// - Validate the peer-id matches hash(public_key, salt)
    /// - Compute ring addresses for DHT routing
    ///
    /// # Panics
    /// Panics if mining has already been completed (peer-id is already set)
    pub fn mine(&mut self, config: AddressConfig) {
        if self.peer_id.is_some() {
            panic!("PeerIdentity has already been mined");
        }

        let start = Instant::now();
        let mut attempts = 0u64;

        log::info!(
            "Starting address mining with difficulty {} (trailing zero bits)",
            config.difficulty
        );

        // Try different salts until we find one that meets the difficulty requirement
        loop {
            attempts += 1;

            // Generate random salt (this is what we're mining)
            let mut salt = [0u8; 16];
            rand::Rng::fill(&mut OsRng, &mut salt);

            // Hash the public key with Argon2 and this salt
            if let Ok(hash) = hash_public_key(&self.public_key, &salt, &config) {
                // Check if hash meets difficulty requirement
                if check_difficulty(&hash, config.difficulty) {
                    let duration = start.elapsed().as_secs_f64();
                    log::info!(
                        "Address mined successfully after {} attempts in {:.2}s",
                        attempts,
                        duration
                    );

                    self.salt = Some(salt);
                    self.peer_id = Some(hash);
                    self.attempts = attempts;
                    self.mining_duration_secs = duration;
                    return;
                }
            }

            // Log progress every 1000 attempts
            if attempts % 1000 == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                log::debug!(
                    "Mining progress: {} attempts, {:.2}s elapsed (public key fixed, trying salts)",
                    attempts,
                    elapsed
                );
            }
        }
    }

    /// Check if mining has been completed
    pub fn is_mined(&self) -> bool {
        self.peer_id.is_some()
    }

    /// Get the peer-id if mining is complete
    pub fn peer_id(&self) -> Option<&PeerId> {
        self.peer_id.as_ref()
    }

    /// Get the salt if mining is complete
    pub fn salt(&self) -> Option<&Salt> {
        self.salt.as_ref()
    }

    /// Validate that a peer-id is correctly derived from the public key and salt
    pub fn validate(
        public_key: &PublicKey,
        salt: &Salt,
        peer_id: &PeerId,
        config: &AddressConfig,
    ) -> bool {
        // Re-compute the hash
        match hash_public_key(public_key, salt, config) {
            Ok(computed_hash) => {
                // Check that the hash matches the claimed peer-id
                if &computed_hash != peer_id {
                    log::warn!("Peer-id validation failed: hash mismatch");
                    return false;
                }

                // Check that the hash meets the difficulty requirement
                if !check_difficulty(&computed_hash, config.difficulty) {
                    log::warn!(
                        "Peer-id validation failed: insufficient difficulty (required {} trailing zeros)",
                        config.difficulty
                    );
                    return false;
                }

                true
            }
            Err(e) => {
                log::warn!("Peer-id validation failed: {}", e);
                false
            }
        }
    }

    /// Compute a shared secret with another peer using X25519 Diffie-Hellman
    ///
    /// This performs ECDH key exchange using our static secret and their public key.
    /// Both parties can independently compute the same shared secret.
    ///
    /// # Design Note
    /// - This is computed **on-demand** when communicating with a peer
    /// - **Not stored** in PeerIdentity (ephemeral per-peer secret)
    /// - `EcPeers` will cache computed secrets for performance
    /// - Each peer pair has a unique shared secret
    ///
    /// # Arguments
    /// * `their_public_key` - The other peer's X25519 public key
    ///
    /// # Returns
    /// A 256-bit shared secret that both parties can independently compute
    ///
    /// # Security Note
    /// The raw shared secret should typically be passed through a KDF (Key Derivation Function)
    /// before use as an encryption key. Consider using HKDF or similar.
    pub fn compute_shared_secret(&self, their_public_key: &PublicKey) -> SharedSecret {
        let shared_secret = self.static_secret.diffie_hellman(their_public_key);
        *shared_secret.as_bytes()
    }
}

/// Hash a public key with Argon2 and the given salt
fn hash_public_key(
    public_key: &PublicKey,
    salt: &Salt,
    config: &AddressConfig,
) -> Result<[u8; 32], String> {
    // Configure Argon2 parameters
    let params = Params::new(
        config.memory_cost,
        config.time_cost,
        config.parallelism,
        Some(32), // Output length: 32 bytes (256 bits)
    )
    .map_err(|e| format!("Invalid Argon2 params: {}", e))?;

    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        Version::V0x13,
        params,
    );

    // Convert salt to SaltString (base64 encoding required by argon2 crate)
    let salt_b64 = SaltString::encode_b64(salt)
        .map_err(|e| format!("Salt encoding error: {}", e))?;

    // Hash the public key bytes
    let public_key_bytes = public_key.as_bytes();
    let hash = argon2
        .hash_password(public_key_bytes, &salt_b64)
        .map_err(|e| format!("Argon2 hashing error: {}", e))?;

    // Extract the hash bytes
    let hash_bytes = hash
        .hash
        .ok_or_else(|| "No hash output".to_string())?;

    let mut result = [0u8; 32];
    result.copy_from_slice(hash_bytes.as_bytes());
    Ok(result)
}

/// Check if a hash has at least `difficulty` trailing zero bits
fn check_difficulty(hash: &[u8; 32], difficulty: u32) -> bool {
    let mut zero_bits = 0u32;

    // Count trailing zero bits from the end of the hash
    for &byte in hash.iter().rev() {
        if byte == 0 {
            zero_bits += 8;
        } else {
            // Count trailing zeros in this byte
            zero_bits += byte.trailing_zeros();
            break;
        }

        if zero_bits >= difficulty {
            return true;
        }
    }

    zero_bits >= difficulty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mine_test_config() {
        // Create identity with keypair
        let mut identity = PeerIdentity::new();

        // Keypair is available before mining
        assert!(!identity.is_mined());
        assert!(identity.peer_id().is_none());

        // Mine for peer-id (should complete quickly)
        identity.mine(AddressConfig::TEST);

        assert!(identity.is_mined());
        assert!(identity.attempts > 0);
        assert!(identity.mining_duration_secs > 0.0);

        // Validate the mined identity
        assert!(PeerIdentity::validate(
            &identity.public_key,
            identity.salt().unwrap(),
            identity.peer_id().unwrap(),
            &AddressConfig::TEST
        ));
    }

    #[test]
    fn test_difficulty_check() {
        // Hash with 8 trailing zero bits
        let hash1 = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
        ];
        assert!(check_difficulty(&hash1, 8));
        assert!(!check_difficulty(&hash1, 9));

        // Hash with 12 trailing zero bits
        let hash2 = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xF0, 0x00,
        ];
        assert!(check_difficulty(&hash2, 12));
        assert!(!check_difficulty(&hash2, 13));
    }

    #[test]
    fn test_validation_rejects_invalid() {
        let mut identity = PeerIdentity::new();
        identity.mine(AddressConfig::TEST);

        // Wrong salt should fail validation
        let wrong_salt = [0xFF; 16];
        assert!(!PeerIdentity::validate(
            &identity.public_key,
            &wrong_salt,
            identity.peer_id().unwrap(),
            &AddressConfig::TEST
        ));

        // Wrong peer-id should fail validation
        let wrong_peer_id = [0xFF; 32];
        assert!(!PeerIdentity::validate(
            &identity.public_key,
            identity.salt().unwrap(),
            &wrong_peer_id,
            &AddressConfig::TEST
        ));
    }

    #[test]
    fn test_shared_secret_computation() {
        // Create two peer identities (no mining needed for DH)
        let alice = PeerIdentity::new();
        let bob = PeerIdentity::new();

        // Alice computes shared secret with Bob's public key
        let alice_shared = alice.compute_shared_secret(&bob.public_key);

        // Bob computes shared secret with Alice's public key
        let bob_shared = bob.compute_shared_secret(&alice.public_key);

        // Both should compute the same shared secret
        assert_eq!(alice_shared, bob_shared);

        // The shared secret should be non-zero
        assert_ne!(alice_shared, [0u8; 32]);
    }

    #[test]
    fn test_shared_secret_different_peers() {
        // Create three peer identities (no mining needed for DH)
        let alice = PeerIdentity::new();
        let bob = PeerIdentity::new();
        let charlie = PeerIdentity::new();

        // Alice-Bob shared secret
        let alice_bob = alice.compute_shared_secret(&bob.public_key);

        // Alice-Charlie shared secret
        let alice_charlie = alice.compute_shared_secret(&charlie.public_key);

        // These should be different
        assert_ne!(alice_bob, alice_charlie);
    }

    #[test]
    fn test_two_phase_creation() {
        // Phase 1: Create identity with keypair
        let mut identity = PeerIdentity::new();

        // Can use for DH immediately
        let other = PeerIdentity::new();
        let _secret = identity.compute_shared_secret(&other.public_key);

        // Not mined yet
        assert!(!identity.is_mined());
        assert!(identity.peer_id().is_none());
        assert!(identity.salt().is_none());

        // Phase 2: Mine for peer-id
        identity.mine(AddressConfig::TEST);

        // Now mined
        assert!(identity.is_mined());
        assert!(identity.peer_id().is_some());
        assert!(identity.salt().is_some());
    }
}
