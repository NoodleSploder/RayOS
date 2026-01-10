//! Byzantine Fault Tolerant (BFT) Consensus
//! 
//! PBFT-inspired consensus tolerating f Byzantine nodes in N-node cluster.
//! Supports N >= 3f+1 with pre-prepare, prepare, commit phases.



/// View number for consensus round (leader change epoch)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewNumber(pub u32);

/// Sequence number of consensus message
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SequenceNumber(pub u32);

/// Node identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(pub u8);

/// Consensus phase
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsensusPhase {
    PrePrepare,
    Prepare,
    Commit,
}

/// Cryptographic signature (simplified)
#[derive(Clone, Copy, Debug)]
pub struct Signature(pub u32);

/// Signed message with authenticator
#[derive(Clone, Copy, Debug)]
pub struct AuthenticatedMessage {
    pub sender: NodeId,
    pub view: ViewNumber,
    pub seq: SequenceNumber,
    pub payload_hash: u32,
    pub signature: Signature,
}

/// Quorum certificate - proof of consensus
#[derive(Clone, Copy, Debug)]
pub struct QuorumCertificate {
    pub view: ViewNumber,
    pub seq: SequenceNumber,
    pub phase: ConsensusPhase,
    pub signature_count: u32,
    pub aggregate_hash: u32,
}

/// Byzantine fault tolerant node
pub struct BFTNode {
    node_id: NodeId,
    view: ViewNumber,
    sequence_number: SequenceNumber,
    
    // Fault tolerance parameters
    total_nodes: u32,
    faulty_tolerance: u32,
    
    // Current consensus round state
    current_phase: ConsensusPhase,
    messages: [AuthenticatedMessage; 128],
    message_count: u16,
    
    // Quorum certificates
    qc_prepare: [QuorumCertificate; 32],
    qc_commit: [QuorumCertificate; 32],
    qc_count: u16,
    
    // Watermark management (prevents unbounded state)
    low_watermark: SequenceNumber,
    high_watermark: SequenceNumber,
    
    // Checkpoint tracking
    checkpoints: [CheckpointState; 16],
    checkpoint_count: u8,
    last_stable_seq: SequenceNumber,
    
    // View change state
    view_timeout_ms: u16,
    last_primary_time: u32,
    
    // Signature storage (simplified)
    signatures: [Signature; 256],
    sig_count: u16,
}

/// Checkpoint for state machine snapshot
#[derive(Clone, Copy, Debug)]
pub struct CheckpointState {
    pub seq: SequenceNumber,
    pub view: ViewNumber,
    pub state_hash: u32,
    pub signature_count: u32,
}

impl BFTNode {
    /// Create new BFT node
    pub fn new(node_id: NodeId, total_nodes: u32) -> Self {
        let faulty_tolerance = total_nodes / 4;
        
        BFTNode {
            node_id,
            view: ViewNumber(0),
            sequence_number: SequenceNumber(0),
            
            total_nodes,
            faulty_tolerance,
            
            current_phase: ConsensusPhase::PrePrepare,
            messages: [AuthenticatedMessage {
                sender: NodeId(0),
                view: ViewNumber(0),
                seq: SequenceNumber(0),
                payload_hash: 0,
                signature: Signature(0),
            }; 128],
            message_count: 0,
            
            qc_prepare: [QuorumCertificate {
                view: ViewNumber(0),
                seq: SequenceNumber(0),
                phase: ConsensusPhase::PrePrepare,
                signature_count: 0,
                aggregate_hash: 0,
            }; 32],
            qc_commit: [QuorumCertificate {
                view: ViewNumber(0),
                seq: SequenceNumber(0),
                phase: ConsensusPhase::Commit,
                signature_count: 0,
                aggregate_hash: 0,
            }; 32],
            qc_count: 0,
            
            low_watermark: SequenceNumber(0),
            high_watermark: SequenceNumber(256),
            
            checkpoints: [CheckpointState {
                seq: SequenceNumber(0),
                view: ViewNumber(0),
                state_hash: 0,
                signature_count: 0,
            }; 16],
            checkpoint_count: 0,
            last_stable_seq: SequenceNumber(0),
            
            view_timeout_ms: 500,
            last_primary_time: 0,
            
            signatures: [Signature(0); 256],
            sig_count: 0,
        }
    }
    
    /// Get current view
    pub fn get_view(&self) -> ViewNumber {
        self.view
    }
    
    /// Get current sequence number
    pub fn get_sequence(&self) -> SequenceNumber {
        self.sequence_number
    }
    
    /// Get Byzantine fault tolerance: f = N/4
    pub fn get_faulty_tolerance(&self) -> u32 {
        self.faulty_tolerance
    }
    
    /// Get quorum size needed: 2f+1
    pub fn get_quorum_size(&self) -> u32 {
        (2 * self.faulty_tolerance) + 1
    }
    
    /// Pre-prepare phase - primary broadcasts
    pub fn pre_prepare(&mut self, request_hash: u32) -> bool {
        // Only primary broadcasts pre-prepare
        let primary_id = self.view.0 as u8 % (self.total_nodes as u8);
        if self.node_id.0 != primary_id {
            return false;
        }
        
        if self.sequence_number.0 >= self.high_watermark.0 {
            return false; // Out of watermark range
        }
        
        let msg = AuthenticatedMessage {
            sender: self.node_id,
            view: self.view,
            seq: self.sequence_number,
            payload_hash: request_hash,
            signature: self.sign_message(request_hash),
        };
        
        if self.message_count < 128 {
            self.messages[self.message_count as usize] = msg;
            self.message_count += 1;
        }
        
        self.current_phase = ConsensusPhase::Prepare;
        true
    }
    
    /// Prepare phase - replicas acknowledge
    pub fn prepare(&mut self, msg: AuthenticatedMessage) -> bool {
        if msg.phase_number() != ConsensusPhase::PrePrepare {
            return false;
        }
        
        if !self.verify_signature(&msg) {
            return false;
        }
        
        if msg.view != self.view || msg.seq < self.low_watermark || msg.seq > self.high_watermark {
            return false;
        }
        
        // Store prepare message
        if self.message_count < 128 {
            self.messages[self.message_count as usize] = msg;
            self.message_count += 1;
        }
        
        self.current_phase = ConsensusPhase::Prepare;
        self.check_prepare_quorum();
        true
    }
    
    /// Commit phase - replicas certify
    pub fn commit(&mut self, seq: SequenceNumber) -> bool {
        if seq < self.low_watermark || seq > self.high_watermark {
            return false;
        }
        
        // Count prepare messages for this sequence
        let mut prepare_count = 0u32;
        for i in 0..self.message_count as usize {
            if self.messages[i].seq == seq && self.messages[i].view == self.view {
                prepare_count += 1;
            }
        }
        
        // Need quorum of prepares
        if prepare_count < self.get_quorum_size() {
            return false;
        }
        
        self.current_phase = ConsensusPhase::Commit;
        self.sequence_number = seq;
        true
    }
    
    /// Check if prepare quorum reached
    fn check_prepare_quorum(&mut self) -> bool {
        let prepare_count = self.message_count as u32;
        prepare_count >= self.get_quorum_size()
    }
    
    /// Create quorum certificate for sequence
    pub fn create_qc(&mut self, seq: SequenceNumber, phase: ConsensusPhase) -> bool {
        if self.qc_count >= 32 {
            return false;
        }
        
        let mut sig_count = 0u32;
        let mut hash_sum = 0u32;
        
        for i in 0..self.message_count as usize {
            if self.messages[i].seq == seq && self.messages[i].view == self.view {
                sig_count += 1;
                hash_sum = hash_sum.wrapping_add(self.messages[i].payload_hash);
            }
        }
        
        if sig_count < self.get_quorum_size() {
            return false;
        }
        
        let qc = QuorumCertificate {
            view: self.view,
            seq,
            phase,
            signature_count: sig_count,
            aggregate_hash: hash_sum,
        };
        
        self.qc_commit[self.qc_count as usize] = qc;
        self.qc_count += 1;
        true
    }
    
    /// Handle view change
    pub fn change_view(&mut self) -> bool {
        self.view = ViewNumber(self.view.0 + 1);
        self.current_phase = ConsensusPhase::PrePrepare;
        self.last_primary_time = 0;
        true
    }
    
    /// Create checkpoint for garbage collection
    pub fn create_checkpoint(&mut self, seq: SequenceNumber) -> bool {
        if self.checkpoint_count >= 16 {
            return false;
        }
        
        let state = CheckpointState {
            seq,
            view: self.view,
            state_hash: (seq.0 ^ self.view.0) as u32,
            signature_count: (2 * self.faulty_tolerance) + 1,
        };
        
        self.checkpoints[self.checkpoint_count as usize] = state;
        self.checkpoint_count += 1;
        self.last_stable_seq = seq;
        
        // Update watermarks
        self.low_watermark = seq;
        self.high_watermark = SequenceNumber(seq.0 + 256);
        
        true
    }
    
    /// Verify message signature
    fn verify_signature(&self, msg: &AuthenticatedMessage) -> bool {
        // Simplified: just check signature is non-zero
        msg.signature.0 != 0
    }
    
    /// Sign message
    fn sign_message(&mut self, payload_hash: u32) -> Signature {
        let sig = Signature(payload_hash.wrapping_add(self.sig_count as u32));
        if self.sig_count < 256 {
            self.signatures[self.sig_count as usize] = sig;
            self.sig_count += 1;
        }
        sig
    }
    
    /// Check if view change timeout expired
    pub fn view_change_timeout_expired(&self, current_time: u32) -> bool {
        current_time > self.last_primary_time + self.view_timeout_ms as u32
    }
    
    /// Get committed sequences
    pub fn get_committed_sequences(&self) -> u32 {
        self.qc_count as u32
    }
    
    /// Get checkpoint count
    pub fn get_checkpoint_count(&self) -> u8 {
        self.checkpoint_count
    }
    
    /// Verify state integrity
    pub fn verify_state_hash(&self, expected_hash: u32) -> bool {
        if self.checkpoint_count > 0 {
            let last = self.checkpoints[(self.checkpoint_count - 1) as usize];
            last.state_hash == expected_hash
        } else {
            true
        }
    }
}

impl AuthenticatedMessage {
    fn phase_number(&self) -> ConsensusPhase {
        ConsensusPhase::PrePrepare
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bft_node_creation() {
        let node = BFTNode::new(NodeId(0), 32);
        assert_eq!(node.get_view(), ViewNumber(0));
        assert_eq!(node.get_faulty_tolerance(), 8);
    }
    
    #[test]
    fn test_quorum_size() {
        let node = BFTNode::new(NodeId(0), 32);
        assert_eq!(node.get_quorum_size(), 17); // 2*8 + 1
    }
    
    #[test]
    fn test_view_change() {
        let mut node = BFTNode::new(NodeId(0), 32);
        assert!(node.change_view());
        assert_eq!(node.get_view(), ViewNumber(1));
    }
}
