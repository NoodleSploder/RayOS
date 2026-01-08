//! Raft Consensus Algorithm Implementation
//! 
//! Provides fault-tolerant consensus for distributed cluster state management.
//! Supports 32+ nodes with log replication, leader election, and snapshotting.

#![no_std]

/// Raft node term number (election epoch)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Term(pub u32);

/// Log entry index in replicated log
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogIndex(pub u32);

/// Node identifier in cluster
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(pub u8);

/// State a Raft node can be in
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaftState {
    Follower,
    Candidate,
    Leader,
}

/// Single entry in replicated log
#[derive(Clone, Copy, Debug)]
pub struct LogEntry {
    pub term: Term,
    pub index: LogIndex,
    pub command: u32,
}

/// Snapshot of state machine at index
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub index: LogIndex,
    pub term: Term,
    pub state_hash: u32,
}

/// Raft node - manages consensus participation
pub struct RaftNode {
    node_id: NodeId,
    current_term: Term,
    voted_for: Option<NodeId>,
    state: RaftState,
    
    // Log replication
    log_entries: [LogEntry; 256],
    log_len: u16,
    commit_index: LogIndex,
    last_applied: LogIndex,
    
    // Leader state
    next_index: [LogIndex; 32],
    match_index: [LogIndex; 32],
    
    // Follower/candidate state
    election_timeout_ms: u16,
    heartbeat_interval_ms: u16,
    last_heartbeat_time: u32,
    
    // Snapshots
    snapshots: [Snapshot; 8],
    snapshot_count: u8,
    last_snapshot_index: LogIndex,
    
    // Peer tracking
    peers: [NodeId; 32],
    peer_count: u8,
}

impl RaftNode {
    /// Create new Raft node
    pub fn new(node_id: NodeId, peers: &[NodeId]) -> Self {
        let mut peer_arr = [NodeId(0); 32];
        let mut count = 0;
        for (i, &peer) in peers.iter().enumerate() {
            if i >= 32 { break; }
            peer_arr[i] = peer;
            count += 1;
        }
        
        RaftNode {
            node_id,
            current_term: Term(0),
            voted_for: None,
            state: RaftState::Follower,
            
            log_entries: [LogEntry {
                term: Term(0),
                index: LogIndex(0),
                command: 0,
            }; 256],
            log_len: 1,
            commit_index: LogIndex(0),
            last_applied: LogIndex(0),
            
            next_index: [LogIndex(1); 32],
            match_index: [LogIndex(0); 32],
            
            election_timeout_ms: 150,
            heartbeat_interval_ms: 50,
            last_heartbeat_time: 0,
            
            snapshots: [Snapshot {
                index: LogIndex(0),
                term: Term(0),
                state_hash: 0,
            }; 8],
            snapshot_count: 0,
            last_snapshot_index: LogIndex(0),
            
            peers: peer_arr,
            peer_count: count,
        }
    }
    
    /// Get current state
    pub fn get_state(&self) -> RaftState {
        self.state
    }
    
    /// Get current term
    pub fn get_current_term(&self) -> Term {
        self.current_term
    }
    
    /// Get commit index
    pub fn get_commit_index(&self) -> LogIndex {
        self.commit_index
    }
    
    /// Get log length
    pub fn get_log_len(&self) -> u16 {
        self.log_len
    }
    
    /// Append entry to log
    pub fn append_entry(&mut self, command: u32) -> bool {
        if self.state != RaftState::Leader {
            return false;
        }
        
        if self.log_len >= 255 {
            return false; // Log full
        }
        
        let index = LogIndex(self.log_len as u32);
        self.log_entries[self.log_len as usize] = LogEntry {
            term: self.current_term,
            index,
            command,
        };
        self.log_len += 1;
        true
    }
    
    /// Apply committed entries to state machine
    pub fn apply_committed_entries(&mut self) -> u32 {
        let mut applied_count = 0u32;
        
        while self.last_applied.0 < self.commit_index.0 {
            self.last_applied = LogIndex(self.last_applied.0 + 1);
            applied_count += 1;
        }
        
        applied_count
    }
    
    /// Request vote from node
    pub fn request_vote(&mut self, candidate_term: Term, _candidate_id: NodeId, 
                       last_log_index: LogIndex, last_log_term: Term) -> bool {
        if candidate_term < self.current_term {
            return false;
        }
        
        if candidate_term > self.current_term {
            self.current_term = candidate_term;
            self.voted_for = None;
            self.state = RaftState::Follower;
        }
        
        if self.voted_for.is_some() && self.voted_for != Some(_candidate_id) {
            return false;
        }
        
        if last_log_index.0 < (self.log_len as u32 - 1) {
            return false;
        }
        
        if last_log_index.0 == (self.log_len as u32 - 1) {
            if last_log_term < self.log_entries[(self.log_len - 1) as usize].term {
                return false;
            }
        }
        
        true
    }
    
    /// Start election (become candidate)
    pub fn start_election(&mut self) -> bool {
        self.current_term = Term(self.current_term.0 + 1);
        self.state = RaftState::Candidate;
        self.voted_for = Some(self.node_id);
        self.last_heartbeat_time = 0;
        true
    }
    
    /// Become leader (won election)
    pub fn become_leader(&mut self) -> bool {
        if self.state != RaftState::Candidate {
            return false;
        }
        
        self.state = RaftState::Leader;
        
        // Initialize leader state
        let log_index = LogIndex(self.log_len as u32);
        for i in 0..self.peer_count as usize {
            self.next_index[i] = log_index;
            self.match_index[i] = LogIndex(0);
        }
        
        true
    }
    
    /// Process append entries (leader replication)
    pub fn append_entries(&mut self, leader_term: Term, prev_log_index: LogIndex,
                         prev_log_term: Term, entries: &[u32], leader_commit: LogIndex) -> bool {
        if leader_term < self.current_term {
            return false;
        }
        
        if leader_term > self.current_term {
            self.current_term = leader_term;
            self.voted_for = None;
        }
        
        self.state = RaftState::Follower;
        self.last_heartbeat_time = 0;
        
        // Check log matching property
        if prev_log_index.0 > 0 {
            if prev_log_index.0 as usize >= self.log_len as usize {
                return false; // Log doesn't have prev_log_index
            }
            
            if self.log_entries[prev_log_index.0 as usize].term != prev_log_term {
                return false; // Term mismatch at prev_log_index
            }
        }
        
        // Append new entries
        let mut entry_index = prev_log_index.0 + 1;
        for &command in entries {
            if entry_index as usize >= self.log_len as usize {
                self.log_entries[entry_index as usize] = LogEntry {
                    term: leader_term,
                    index: LogIndex(entry_index),
                    command,
                };
                self.log_len = (entry_index + 1) as u16;
            }
            entry_index += 1;
        }
        
        // Update commit index
        if leader_commit > self.commit_index {
            let new_commit = if leader_commit.0 < self.log_len as u32 - 1 {
                leader_commit
            } else {
                LogIndex((self.log_len - 1) as u32)
            };
            self.commit_index = new_commit;
        }
        
        true
    }
    
    /// Create snapshot at current index
    pub fn create_snapshot(&mut self) -> bool {
        if self.snapshot_count >= 8 {
            return false;
        }
        
        let snapshot = Snapshot {
            index: self.last_applied,
            term: self.current_term,
            state_hash: (self.last_applied.0 ^ self.current_term.0) as u32,
        };
        
        self.snapshots[self.snapshot_count as usize] = snapshot;
        self.last_snapshot_index = self.last_applied;
        self.snapshot_count += 1;
        
        true
    }
    
    /// Get latest snapshot
    pub fn get_last_snapshot(&self) -> Option<Snapshot> {
        if self.snapshot_count > 0 {
            Some(self.snapshots[(self.snapshot_count - 1) as usize])
        } else {
            None
        }
    }
    
    /// Update election timeout
    pub fn update_election_timeout(&mut self, timeout_ms: u16) {
        self.election_timeout_ms = timeout_ms;
    }
    
    /// Check if election timeout expired
    pub fn election_timeout_expired(&self, current_time: u32) -> bool {
        if self.state == RaftState::Leader {
            return false;
        }
        current_time > self.last_heartbeat_time + self.election_timeout_ms as u32
    }
    
    /// Check if heartbeat needed
    pub fn needs_heartbeat(&self, current_time: u32) -> bool {
        if self.state != RaftState::Leader {
            return false;
        }
        current_time > self.last_heartbeat_time + self.heartbeat_interval_ms as u32
    }
    
    /// Get peer list
    pub fn get_peers(&self) -> &[NodeId] {
        &self.peers[..self.peer_count as usize]
    }
    
    /// Count replicas with matching index
    pub fn count_matching_replicas(&self, index: LogIndex) -> u32 {
        let mut count = 1u32; // Count self
        for i in 0..self.peer_count as usize {
            if self.match_index[i] >= index {
                count += 1;
            }
        }
        count
    }
    
    /// Update match index for peer
    pub fn update_match_index(&mut self, peer_id: NodeId, index: LogIndex) -> bool {
        for i in 0..self.peer_count as usize {
            if self.peers[i] == peer_id {
                self.match_index[i] = index;
                
                // Advance commit index if majority replicated
                for j in ((self.commit_index.0 + 1) as usize)..256 {
                    if self.count_matching_replicas(LogIndex(j as u32)) * 2 > (self.peer_count as u32 + 1) {
                        self.commit_index = LogIndex(j as u32);
                    }
                }
                
                return true;
            }
        }
        false
    }
    
    /// Reset heartbeat time
    pub fn reset_heartbeat_time(&mut self, current_time: u32) {
        self.last_heartbeat_time = current_time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_new_node() {
        let node = RaftNode::new(NodeId(0), &[NodeId(1), NodeId(2)]);
        assert_eq!(node.get_state(), RaftState::Follower);
        assert_eq!(node.get_current_term(), Term(0));
    }
    
    #[test]
    fn test_start_election() {
        let mut node = RaftNode::new(NodeId(0), &[NodeId(1)]);
        assert!(node.start_election());
        assert_eq!(node.get_state(), RaftState::Candidate);
    }
    
    #[test]
    fn test_become_leader() {
        let mut node = RaftNode::new(NodeId(0), &[NodeId(1)]);
        node.start_election();
        assert!(node.become_leader());
        assert_eq!(node.get_state(), RaftState::Leader);
    }
}
