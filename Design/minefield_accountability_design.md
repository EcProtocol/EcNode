# Minefield: Evidence-Leaking Accountability for Timeline-Corrected Distributed Consensus

## Abstract

This document presents the "Minefield" security mechanism - a cryptographic accountability system that continuously leaks evidence of peer behavior through enhanced general-purpose mapping queries. Rather than a separate accountability service, Minefield integrates evidence collection into routine network operations by requiring all get-mapping responses to include the last 2-4 token mappings with timestamps and peer signatures. This approach creates persistent evidence trails that enable automatic slashing of fraudulent peers while maintaining operational efficiency.

## Problem Statement

Recent analysis corrections reveal that ecRust network acceptance occurs in **2-6 months** rather than the originally projected 6-36 months. This dramatically reduces attack costs and creates new security vulnerabilities:

### Timeline Impact on Security
- **Eclipse attack costs**: Reduced from $234K to $130K (44% decrease)  
- **Attack feasibility**: 10-year timeline vs. 13+ years originally projected
- **Detection windows**: Shortened time to identify coordinated malicious behavior

### Core Security Challenges
1. **Accelerated threat landscape**: Faster peer acceptance enables quicker attack establishment
2. **Reduced economic barriers**: Lower total investment required for coordinated attacks
3. **Collusion vulnerabilities**: Peers can manipulate responses with limited consequences
4. **State manipulation**: Inconsistent token mapping information without accountability
5. **Investment protection gaps**: Current loss mechanisms may be insufficient deterrent

## Core Concept: Evidence-Leaking Mapping Service

Instead of creating a separate accountability service that might alert malicious peers, Minefield modifies the existing get-mapping service to continuously leak evidence of peer state:

### Enhanced Get-Mapping Response
Every mapping query response now includes:
1. **Primary mapping**: The requested token mapping information
2. **Evidence payload**: Last 2-4 token mappings from this peer's view
3. **Timestamp**: When each mapping was committed at this peer
4. **Peer signature**: Cryptographic signature using the peer's latest "lamppost" public key

This design ensures evidence is collected passively during normal network operations, making it impossible for malicious peers to avoid creating accountability trails.

## Technical Design

### Modified Message Structure

```rust
#[derive(Clone, Debug)]
pub struct EnhancedMappingResponse {
    // Original response data
    pub requested_mapping: TokenMapping,
    pub query_id: QueryId,
    
    // Evidence payload (the "leak")
    pub evidence_mappings: Vec<SignedTokenMapping>,
    pub evidence_timestamp: EcTime,
    pub peer_signature: Signature,  // Signed with lamppost key
    pub peer_lamppost_key: PublicKey,
}

#[derive(Clone, Debug)]
pub struct SignedTokenMapping {
    pub token_id: TokenId,
    pub mapping: TokenMapping,
    pub commit_timestamp: EcTime,
    pub block_hash: BlockHash,  // Reference to source block
    pub mapping_signature: Signature,
}

#[derive(Clone, Debug)]
pub struct EvidenceCollection {
    pub peer_id: PeerId,
    pub collected_responses: Vec<EnhancedMappingResponse>,
    pub collection_timespan: (EcTime, EcTime),
    pub inconsistency_count: u32,
    pub confidence_score: f64,
}
```

### Evidence Collection Strategy

**Passive Collection**: Token owners and interested parties naturally collect evidence through routine queries:
- **High-value token monitoring**: Owners query multiple peers for valuable tokens
- **Cross-validation**: Compare responses from different peers for consistency  
- **Temporal tracking**: Monitor how peer views change over time
- **Insurance storage**: Keep signed responses as proof of peer statements

**Evidence Aggregation**:
```rust
impl EvidenceCollector {
    pub fn analyze_peer_consistency(&self, peer_id: PeerId) -> ConsistencyReport {
        let responses = self.get_peer_responses(peer_id);
        let inconsistencies = self.detect_inconsistencies(&responses);
        let temporal_violations = self.check_temporal_consistency(&responses);
        
        ConsistencyReport {
            peer_id,
            total_responses: responses.len(),
            inconsistency_count: inconsistencies.len(),
            temporal_violations: temporal_violations.len(),
            confidence_score: self.calculate_confidence(&inconsistencies, &temporal_violations),
            evidence_quality: self.assess_evidence_quality(&responses),
        }
    }
}
```

## Automatic Slashing Mechanism

### Evidence-Based Slashing Process

**Phase 1: Evidence Accumulation**
- Peers collect inconsistent signed responses during normal operations
- Evidence threshold: 3+ conflicting statements OR 1 provably false statement
- Temporal window: Evidence must span at least 24 hours to prevent timing attacks

**Phase 2: Evidence Validation** 
- Cryptographic verification of all signatures in evidence package
- Cross-referencing with blockchain state to confirm inconsistencies
- Peer density verification to ensure evidence collector legitimacy

**Phase 3: Network Propagation**
```rust
#[derive(Clone, Debug)]
pub struct SlashingProposal {
    pub accused_peer: PeerId,
    pub evidence_package: Vec<EnhancedMappingResponse>,
    pub violation_type: ViolationType,
    pub proposer_signature: Signature,
    pub witness_endorsements: Vec<PeerEndorsement>,
    pub confidence_score: f64,
}

#[derive(Clone, Debug)]
pub enum ViolationType {
    InconsistentMappings,     // Different mappings for same token/time
    TemporalViolation,        // Impossible timestamp ordering
    StateForging,             // Mappings inconsistent with blockchain
    SignatureFraud,           // Invalid or forged signatures
}
```

**Phase 4: Consensus Slashing**
- Network votes on slashing proposal using existing consensus mechanism
- Threshold: 67% of responding peers must agree to slash
- Penalties: Immediate exclusion, reputation destruction, connection bans

### Mathematical Foundation

**Evidence Validity**: For evidence $E = \{r_1, r_2, ..., r_k\}$ against peer $p$:

$$\text{Valid}(E) = \bigwedge_{i=1}^{k} \text{VerifySignature}(r_i.signature, r_i.lamppost\_key) \land \text{Inconsistent}(E) \land |E| \geq \theta_{min}$$

**Inconsistency Detection**: For responses $r_i, r_j$ about token $t$:

$$\text{Inconsistent}(r_i, r_j, t) = \begin{cases}
\text{True} & \text{if } r_i.mapping(t) \neq r_j.mapping(t) \land |r_i.timestamp - r_j.timestamp| < \delta \\
\text{True} & \text{if } r_i.timestamp > r_j.timestamp \land r_i.block\_height < r_j.block\_height \\
\text{False} & \text{otherwise}
\end{cases}$$

**Confidence Scoring**: 

$$\text{Confidence}(E) = \alpha \cdot \frac{\text{Inconsistency Count}}{|E|} + \beta \cdot \text{SignatureQuality}(E) + \gamma \cdot \text{TemporalSpread}(E)$$

Where $\alpha + \beta + \gamma = 1$ and confidence must exceed 0.85 for slashing.

## Security Analysis

### Attack Resistance

**Evidence Forgery**: 
- ❌ **Impossible**: Requires forging signatures with peer's private lamppost key
- ✅ **Protected**: Lamppost keys tied to committed blockchain transactions

**Selective Response**: 
- ❌ **Ineffective**: Evidence collected from routine operations, not targeted requests
- ✅ **Continuous**: Normal network usage creates persistent evidence trail

**Coordinated Slashing**: 
- ❌ **Difficult**: Requires 67% consensus among diverse peer network
- ✅ **Protected**: Multiple independent evidence sources required

**Evidence Pollution**: 
- ❌ **Limited**: Bad evidence filtered by signature verification and consistency checks
- ✅ **Resilient**: High confidence thresholds prevent false positives

### Key Concerns and Mitigations

**🚨 Concern: Storage Overhead**
- **Issue**: Continuous evidence collection increases storage requirements
- **Reality**: Primary use case is end-users/clients storing evidence for their own tokens (10-50 responses ~1KB total). Only one surviving copy needed to create "minefield" effect.
- **Mitigation**: Evidence storage naturally limited by economic incentives - users store evidence for valuable tokens they care about

**🚨 Concern: Network Bandwidth**  
- **Issue**: All mapping responses grow from <1KB to slightly >1KB due to evidence payload
- **Design Intent**: This overhead affects ALL responses by design - creates unavoidable evidence leakage
- **Trade-off**: Small bandwidth cost for continuous accountability across entire network

**🚨 Concern: False Positive Slashing**
- **Issue**: Honest peers falsely accused due to network delays or splits  
- **Mitigation**: High confidence thresholds (85%+), temporal windows, multiple evidence sources required

**🚨 Concern: Privacy Leakage**
- **Non-issue**: Token mappings and transactions are inherently public network data
- **Token anonymity**: Tokens typically SHA hashes of original content (unlinkable)
- **Transaction anonymity**: Transaction IDs also SHA hashes
- **No new exposure**: Evidence payload contains only already-public blockchain information

## Implementation Strategy

### Integration Points

**Modified EcNode Response Handler**:
```rust
impl EcNode {
    fn handle_mapping_query(&mut self, query: MappingQuery) -> EnhancedMappingResponse {
        let requested_mapping = self.mempool.get_token_mapping(query.token_id)?;
        let evidence_mappings = self.get_recent_mappings(2..4); // 2-4 recent mappings
        let evidence_timestamp = self.current_time();
        let signature_payload = self.create_signature_payload(&requested_mapping, &evidence_mappings, evidence_timestamp);
        let peer_signature = self.sign_with_lamppost_key(signature_payload);
        
        EnhancedMappingResponse {
            requested_mapping,
            query_id: query.id,
            evidence_mappings,
            evidence_timestamp,
            peer_signature,
            peer_lamppost_key: self.current_lamppost_public_key(),
        }
    }
}
```

**Evidence Collection Service**:
```rust
pub struct EvidenceCollectionService {
    evidence_store: HashMap<PeerId, EvidenceCollection>,
    consistency_analyzer: ConsistencyAnalyzer,
    slashing_detector: SlashingDetector,
    confidence_threshold: f64,
}

impl EvidenceCollectionService {
    pub fn process_mapping_response(&mut self, response: EnhancedMappingResponse) {
        // Verify signatures
        if !self.verify_response_signatures(&response) {
            return; // Invalid response, ignore
        }
        
        // Store evidence
        self.evidence_store
            .entry(response.peer_id)
            .or_default()
            .collected_responses
            .push(response);
        
        // Check for slashing conditions
        if let Some(violation) = self.slashing_detector.check_violations(&response) {
            self.initiate_slashing_proposal(violation);
        }
    }
}
```

## Economic Incentives

**Evidence Collection Rewards**:
- **Natural incentives**: Token owners protect their investments through monitoring
- **Low cost**: Evidence collection happens during normal queries
- **High value**: Strong evidence enables recovery of losses through slashing

**Slashing Economics**:
- **Deterrent effect**: Certain punishment for detected fraud
- **Investment loss**: Complete loss of identity generation, sync, and trust-building costs
- **Network exclusion**: Permanent reputation damage and connection bans
- **Legal exposure**: Signed evidence can support prosecution

## Automatic Slashing Implementation

### Slashing Trigger Conditions

```rust
#[derive(Clone, Debug)]
pub enum SlashingTrigger {
    // Immediate triggers (high confidence)
    ProvenInconsistency {
        conflicting_responses: Vec<EnhancedMappingResponse>,
        confidence: f64, // Must be > 0.95
    },
    
    // Pattern-based triggers (accumulated evidence)
    RepeatedViolations {
        violation_history: Vec<EvidenceViolation>,
        pattern_strength: f64, // Must be > 0.85
        time_span: Duration,
    },
    
    // Cryptographic proof triggers  
    SignatureFraud {
        forged_response: EnhancedMappingResponse,
        proof_of_forgery: CryptographicProof,
    },
}
```

### Consensus-Based Slashing Process

**Distributed Slashing Vote**:
```rust
impl ConsensusSlashing {
    pub fn propose_slashing(&mut self, proposal: SlashingProposal) -> SlashingOutcome {
        // Phase 1: Evidence validation
        let validation_result = self.validate_evidence_package(&proposal.evidence_package);
        if !validation_result.is_valid {
            return SlashingOutcome::Rejected("Invalid evidence".to_string());
        }
        
        // Phase 2: Network vote
        let vote_result = self.conduct_slashing_vote(&proposal);
        if vote_result.approval_rate < 0.67 {
            return SlashingOutcome::Rejected("Insufficient consensus".to_string());
        }
        
        // Phase 3: Execute slashing
        self.execute_slashing(&proposal.accused_peer);
        SlashingOutcome::Executed
    }
    
    fn execute_slashing(&mut self, peer_id: &PeerId) {
        // Immediate network exclusion
        self.peer_manager.ban_peer(peer_id, BanDuration::Permanent);
        
        // Reputation destruction
        self.reputation_system.set_reputation(peer_id, ReputationScore::Slashed);
        
        // Connection termination
        self.connection_manager.terminate_all_connections(peer_id);
        
        // Evidence archival for future reference
        self.evidence_archive.archive_slashing_case(peer_id, &evidence_package);
    }
}
```

## Conclusion

The evidence-leaking approach transforms routine network operations into continuous accountability monitoring. By integrating evidence collection into essential get-mapping services, the system ensures that malicious peers cannot avoid creating incriminating evidence trails while honest peers naturally collect insurance against fraud.

**Key Advantages**:
- **Passive evidence collection**: No separate accountability protocol needed
- **Continuous monitoring**: Evidence accumulated during normal operations  
- **Automatic enforcement**: High-confidence slashing without human intervention
- **Economic efficiency**: Minimal overhead for maximum security enhancement

**Critical Success Factors**:
- **Signature security**: Lamppost key management and rotation protocols
- **Evidence quality**: High confidence thresholds prevent false positives
- **Network consensus**: Distributed slashing decisions prevent coordination attacks
- **Storage management**: Efficient evidence retention and compression strategies

The mechanism creates a "minefield" effect where any fraudulent behavior risks triggering evidence collection and automatic punishment, providing strong economic deterrence against coordinated attacks in the timeline-compressed threat environment.