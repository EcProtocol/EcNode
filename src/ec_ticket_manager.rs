use crate::ec_interface::{BlockId, BlockUseCase, EcTime, MessageTicket};
use blake3;
use std::collections::HashMap;

/// Manages cryptographic tickets for block request/response routing
///
/// The TicketManager generates and validates tickets that cryptographically bind
/// block responses to their corresponding requests. This prevents malicious peers
/// from injecting unsolicited blocks.
///
/// # Security Model
///
/// - **Goal:** Prevent unsolicited block injection (only threat that causes corruption)
/// - **Non-goals:** Replay prevention (operations are idempotent)
///
/// # Cryptographic Construction
///
/// Ticket generation: `ticket = Blake3(current_secret || block_id) XOR use_case_secret`
/// Ticket validation: Check against both current and previous secrets
///
/// # Secret Rotation
///
/// Secrets rotate every `rotation_period` ticks to provide forward secrecy.
/// Validation accepts both current and previous secrets, creating a 2× rotation period
/// acceptance window for in-flight messages.
///
/// # Example
///
/// ```
/// use ec_rust::ec_ticket_manager::TicketManager;
/// use ec_rust::ec_interface::BlockUseCase;
///
/// let mut manager = TicketManager::new(100); // 100 tick rotation
/// let block_id = 12345u64;
///
/// // Generate ticket for request
/// let ticket = manager.generate_ticket(block_id, BlockUseCase::MempoolBlock);
///
/// // Later, validate response
/// if let Some(use_case) = manager.validate_ticket(ticket, block_id) {
///     println!("Valid block for use case: {:?}", use_case);
/// }
/// ```
pub struct TicketManager {
    /// Current secret used for ticket generation
    current_secret: [u8; 32],

    /// Previous secret (retained after rotation for grace period)
    previous_secret: Option<[u8; 32]>,

    /// Per-use-case secrets for routing isolation
    use_case_secrets: HashMap<BlockUseCase, u64>,

    /// Last time secrets were rotated
    last_rotation: EcTime,

    /// Number of ticks between secret rotations
    rotation_period: u64,
}

impl TicketManager {
    /// Create a new TicketManager with specified rotation period
    ///
    /// # Arguments
    ///
    /// * `rotation_period` - Ticks between secret rotations (recommend 50-100 for simulation)
    ///
    /// # Example
    ///
    /// ```
    /// use ec_rust::ec_ticket_manager::TicketManager;
    ///
    /// // For simulation with network delays
    /// let manager = TicketManager::new(100);
    /// ```
    pub fn new(rotation_period: u64) -> Self {
        Self {
            current_secret: Self::generate_secret(),
            previous_secret: None,
            use_case_secrets: Self::generate_use_case_secrets(),
            last_rotation: 0,
            rotation_period,
        }
    }

    /// Generate a cryptographically random 256-bit secret
    fn generate_secret() -> [u8; 32] {
        use rand::RngCore;
        let mut secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        secret
    }

    /// Generate independent secrets for each use case
    fn generate_use_case_secrets() -> HashMap<BlockUseCase, u64> {
        use rand::RngCore;
        let mut secrets = HashMap::new();
        let mut rng = rand::thread_rng();

        secrets.insert(BlockUseCase::MempoolBlock, rng.next_u64());
        secrets.insert(BlockUseCase::ParentBlock, rng.next_u64());
        secrets.insert(BlockUseCase::CommitChain, rng.next_u64());
        secrets.insert(BlockUseCase::ValidateWith, rng.next_u64());

        secrets
    }

    /// Generate a ticket for a block request
    ///
    /// # Security
    ///
    /// Ticket = Blake3(current_secret || block_id) XOR use_case_secret
    ///
    /// This construction:
    /// - Prevents ticket forgery (requires knowing current_secret)
    /// - Binds ticket to specific BlockId (cannot substitute different block)
    /// - Isolates use cases (MempoolBlock tickets won't validate as ParentBlock)
    ///
    /// # Arguments
    ///
    /// * `block_id` - The BlockId being requested
    /// * `use_case` - The purpose of this request
    ///
    /// # Example
    ///
    /// ```
    /// use ec_rust::ec_ticket_manager::TicketManager;
    /// use ec_rust::ec_interface::BlockUseCase;
    ///
    /// let manager = TicketManager::new(100);
    /// let ticket = manager.generate_ticket(12345, BlockUseCase::MempoolBlock);
    /// ```
    pub fn generate_ticket(&self, block_id: BlockId, use_case: BlockUseCase) -> MessageTicket {
        let hash = self.hash_with_secret(&self.current_secret, block_id);
        let use_case_secret = self.use_case_secrets[&use_case];
        hash ^ use_case_secret
    }

    /// Validate a ticket and determine its use case
    ///
    /// Tries validation with both current and previous secrets to handle
    /// messages in flight during rotation.
    ///
    /// # Arguments
    ///
    /// * `ticket` - The ticket to validate
    /// * `block_id` - The BlockId from the message
    ///
    /// # Returns
    ///
    /// * `Some(BlockUseCase)` - If ticket is valid, returns the use case
    /// * `None` - If ticket is invalid (reject message)
    ///
    /// # Example
    ///
    /// ```
    /// use ec_rust::ec_ticket_manager::TicketManager;
    /// use ec_rust::ec_interface::BlockUseCase;
    ///
    /// let manager = TicketManager::new(100);
    /// let block_id = 12345u64;
    /// let ticket = manager.generate_ticket(block_id, BlockUseCase::MempoolBlock);
    ///
    /// match manager.validate_ticket(ticket, block_id) {
    ///     Some(BlockUseCase::MempoolBlock) => println!("Valid mempool block"),
    ///     Some(use_case) => println!("Valid for: {:?}", use_case),
    ///     None => println!("Invalid ticket - reject"),
    /// }
    /// ```
    pub fn validate_ticket(&self, ticket: MessageTicket, block_id: BlockId) -> Option<BlockUseCase> {
        // Try current secret
        if let Some(use_case) = self.try_validate(&self.current_secret, ticket, block_id) {
            return Some(use_case);
        }

        // Try previous secret (grace period for in-flight messages)
        if let Some(prev_secret) = &self.previous_secret {
            return self.try_validate(prev_secret, ticket, block_id);
        }

        None
    }

    /// Attempt validation with a specific secret
    fn try_validate(&self, secret: &[u8; 32], ticket: MessageTicket, block_id: BlockId) -> Option<BlockUseCase> {
        let hash = self.hash_with_secret(secret, block_id);

        // Check against all use case secrets
        for (&use_case, &use_case_secret) in &self.use_case_secrets {
            if ticket == hash ^ use_case_secret {
                return Some(use_case);
            }
        }

        None
    }

    /// Hash a BlockId with a secret using Blake3
    ///
    /// Computes: Blake3(secret || block_id) and returns lower 64 bits as u64
    ///
    /// Note: In production with 256-bit tickets, this would return the full hash
    fn hash_with_secret(&self, secret: &[u8; 32], block_id: BlockId) -> u64 {
        let mut hasher = blake3::Hasher::new();
        hasher.update(secret);
        hasher.update(&block_id.to_le_bytes());
        let hash = hasher.finalize();

        // Extract lower 64 bits as u64
        // In production with MessageTicket = [u8; 32], return hash.as_bytes()
        u64::from_le_bytes(hash.as_bytes()[0..8].try_into().unwrap())
    }

    /// Perform secret rotation if enough time has elapsed
    ///
    /// Should be called every tick. Rotates secrets when:
    /// `current_time >= last_rotation + rotation_period`
    ///
    /// After rotation:
    /// - previous_secret = old current_secret
    /// - current_secret = new random secret
    /// - Acceptance window now covers 2× rotation_period
    ///
    /// # Arguments
    ///
    /// * `current_time` - The current tick time
    ///
    /// # Example
    ///
    /// ```
    /// use ec_rust::ec_ticket_manager::TicketManager;
    ///
    /// let mut manager = TicketManager::new(100);
    ///
    /// for tick in 0..300 {
    ///     manager.tick(tick);
    ///     // Secrets will rotate at tick 100 and 200
    /// }
    /// ```
    pub fn tick(&mut self, current_time: EcTime) {
        if current_time >= self.last_rotation + self.rotation_period {
            log::debug!(
                "Rotating ticket secrets at time {} (period: {})",
                current_time,
                self.rotation_period
            );

            // Keep current as previous, generate new current
            self.previous_secret = Some(self.current_secret);
            self.current_secret = Self::generate_secret();
            self.last_rotation = current_time;
        }
    }

    /// Get the current rotation period
    pub fn rotation_period(&self) -> u64 {
        self.rotation_period
    }

    /// Get the time of last rotation
    pub fn last_rotation(&self) -> EcTime {
        self.last_rotation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate() {
        let manager = TicketManager::new(100);
        let block_id = 12345u64;

        // Generate ticket
        let ticket = manager.generate_ticket(block_id, BlockUseCase::MempoolBlock);

        // Validate should succeed
        let result = manager.validate_ticket(ticket, block_id);
        assert_eq!(result, Some(BlockUseCase::MempoolBlock));
    }

    #[test]
    fn test_wrong_block_id_fails() {
        let manager = TicketManager::new(100);
        let block_id = 12345u64;

        // Generate ticket for one block
        let ticket = manager.generate_ticket(block_id, BlockUseCase::MempoolBlock);

        // Validate with different block_id should fail
        let result = manager.validate_ticket(ticket, 99999);
        assert_eq!(result, None);
    }

    #[test]
    fn test_use_case_isolation() {
        let manager = TicketManager::new(100);
        let block_id = 12345u64;

        // Generate ticket for MempoolBlock
        let ticket = manager.generate_ticket(block_id, BlockUseCase::MempoolBlock);

        // Should validate as MempoolBlock, not ParentBlock
        let result = manager.validate_ticket(ticket, block_id);
        assert_eq!(result, Some(BlockUseCase::MempoolBlock));
    }

    #[test]
    fn test_rotation_with_grace_period() {
        let mut manager = TicketManager::new(100);
        let block_id = 12345u64;

        // Generate ticket at time 0
        let ticket = manager.generate_ticket(block_id, BlockUseCase::MempoolBlock);

        // Should validate at time 0
        assert!(manager.validate_ticket(ticket, block_id).is_some());

        // Advance to time 50 (before rotation)
        manager.tick(50);
        assert!(manager.validate_ticket(ticket, block_id).is_some());

        // Rotate at time 100
        manager.tick(100);

        // Old ticket should still validate (using previous_secret)
        assert!(manager.validate_ticket(ticket, block_id).is_some());

        // Generate new ticket with new secret
        let new_ticket = manager.generate_ticket(block_id, BlockUseCase::MempoolBlock);

        // New ticket should validate
        assert!(new_ticket != ticket); // Different secrets produce different tickets
        assert!(manager.validate_ticket(new_ticket, block_id).is_some());

        // Rotate again at time 200
        manager.tick(200);

        // Original ticket should now fail (secret from 2 rotations ago)
        assert_eq!(manager.validate_ticket(ticket, block_id), None);

        // But ticket from first rotation should still work
        assert!(manager.validate_ticket(new_ticket, block_id).is_some());
    }

    #[test]
    fn test_invalid_ticket_fails() {
        let manager = TicketManager::new(100);
        let block_id = 12345u64;

        // Random ticket should fail
        let random_ticket = 0xDEADBEEF;
        let result = manager.validate_ticket(random_ticket, block_id);
        assert_eq!(result, None);
    }

    #[test]
    fn test_different_use_cases() {
        let manager = TicketManager::new(100);
        let block_id = 12345u64;

        let ticket1 = manager.generate_ticket(block_id, BlockUseCase::MempoolBlock);
        let ticket2 = manager.generate_ticket(block_id, BlockUseCase::ParentBlock);
        let ticket3 = manager.generate_ticket(block_id, BlockUseCase::CommitChain);
        let ticket4 = manager.generate_ticket(block_id, BlockUseCase::ValidateWith);

        // All should be different
        assert_ne!(ticket1, ticket2);
        assert_ne!(ticket1, ticket3);
        assert_ne!(ticket1, ticket4);
        assert_ne!(ticket2, ticket3);

        // Each should validate to correct use case
        assert_eq!(manager.validate_ticket(ticket1, block_id), Some(BlockUseCase::MempoolBlock));
        assert_eq!(manager.validate_ticket(ticket2, block_id), Some(BlockUseCase::ParentBlock));
        assert_eq!(manager.validate_ticket(ticket3, block_id), Some(BlockUseCase::CommitChain));
        assert_eq!(manager.validate_ticket(ticket4, block_id), Some(BlockUseCase::ValidateWith));
    }
}
