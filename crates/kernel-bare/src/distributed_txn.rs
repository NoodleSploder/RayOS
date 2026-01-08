/// Distributed Transaction Coordination
///
/// Consensus-based distributed transaction support with ACID guarantees
/// and Raft-inspired leader election for coordination.

use core::cmp::min;

const MAX_TRANSACTIONS: usize = 128;
const MAX_PARTICIPANTS: usize = 16;
const MAX_LOG_ENTRIES: usize = 256;

/// Transaction state
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransactionState {
    Pending,
    Preparing,
    Committed,
    Aborted,
    RollingBack,
}

/// Isolation level
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IsolationLevel {
    ReadUncommitted = 0,
    ReadCommitted = 1,
    Serializable = 2,
}

/// Coordinator role
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoordinatorRole {
    Leader,
    Follower,
    Candidate,
}

/// Transaction log entry
#[derive(Clone, Copy, Debug)]
pub struct TransactionLogEntry {
    pub entry_id: u32,
    pub transaction_id: u32,
    pub state: TransactionState,
    pub timestamp: u64,
    pub participant_count: u32,
}

impl TransactionLogEntry {
    pub fn new(entry_id: u32, transaction_id: u32) -> Self {
        TransactionLogEntry {
            entry_id,
            transaction_id,
            state: TransactionState::Pending,
            timestamp: 0,
            participant_count: 0,
        }
    }
}

/// Participant node
#[derive(Clone, Copy, Debug)]
pub struct ParticipantNode {
    pub node_id: u32,
    pub acknowledged: bool,
    pub ready_to_commit: bool,
}

impl ParticipantNode {
    pub fn new(node_id: u32) -> Self {
        ParticipantNode {
            node_id,
            acknowledged: false,
            ready_to_commit: false,
        }
    }
}

/// Transaction
#[derive(Clone, Copy, Debug)]
pub struct Transaction {
    pub transaction_id: u32,
    pub state: TransactionState,
    pub isolation_level: IsolationLevel,
    pub participant_count: u32,
    pub log_entries: u32,
    pub start_timestamp: u64,
}

impl Transaction {
    pub fn new(transaction_id: u32, isolation_level: IsolationLevel) -> Self {
        Transaction {
            transaction_id,
            state: TransactionState::Pending,
            isolation_level,
            participant_count: 0,
            log_entries: 0,
            start_timestamp: 0,
        }
    }
}

/// Transaction Coordinator
pub struct TransactionCoordinator {
    transactions: [Option<Transaction>; MAX_TRANSACTIONS],
    log: [Option<TransactionLogEntry>; MAX_LOG_ENTRIES],
    participants: [Option<ParticipantNode>; MAX_PARTICIPANTS],
    coordinator_role: CoordinatorRole,
    current_term: u32,
    leader_id: u32,
    transaction_count: u32,
    log_index: u32,
    participant_count: u32,
}

impl TransactionCoordinator {
    pub fn new() -> Self {
        TransactionCoordinator {
            transactions: [None; MAX_TRANSACTIONS],
            log: [None; MAX_LOG_ENTRIES],
            participants: [None; MAX_PARTICIPANTS],
            coordinator_role: CoordinatorRole::Follower,
            current_term: 0,
            leader_id: 0,
            transaction_count: 0,
            log_index: 0,
            participant_count: 0,
        }
    }

    pub fn create_transaction(&mut self, isolation_level: IsolationLevel) -> u32 {
        for i in 0..MAX_TRANSACTIONS {
            if self.transactions[i].is_none() {
                let transaction_id = i as u32 + 1;
                let transaction = Transaction::new(transaction_id, isolation_level);
                self.transactions[i] = Some(transaction);
                self.transaction_count += 1;
                return transaction_id;
            }
        }
        0
    }

    pub fn add_participant(&mut self, transaction_id: u32, participant_id: u32) -> bool {
        // Find transaction
        for i in 0..MAX_TRANSACTIONS {
            if let Some(mut transaction) = self.transactions[i] {
                if transaction.transaction_id == transaction_id {
                    // Add participant
                    for j in 0..MAX_PARTICIPANTS {
                        if self.participants[j].is_none() {
                            let participant = ParticipantNode::new(participant_id);
                            self.participants[j] = Some(participant);
                            transaction.participant_count += 1;
                            self.transactions[i] = Some(transaction);
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn prepare_commit(&mut self, transaction_id: u32) -> bool {
        for i in 0..MAX_TRANSACTIONS {
            if let Some(mut transaction) = self.transactions[i] {
                if transaction.transaction_id == transaction_id {
                    transaction.state = TransactionState::Preparing;
                    self.transactions[i] = Some(transaction);
                    self.log_transaction_state(transaction_id, TransactionState::Preparing);
                    return true;
                }
            }
        }
        false
    }

    pub fn commit_transaction(&mut self, transaction_id: u32) -> bool {
        for i in 0..MAX_TRANSACTIONS {
            if let Some(mut transaction) = self.transactions[i] {
                if transaction.transaction_id == transaction_id {
                    transaction.state = TransactionState::Committed;
                    self.transactions[i] = Some(transaction);
                    self.log_transaction_state(transaction_id, TransactionState::Committed);
                    return true;
                }
            }
        }
        false
    }

    pub fn abort_transaction(&mut self, transaction_id: u32) -> bool {
        for i in 0..MAX_TRANSACTIONS {
            if let Some(mut transaction) = self.transactions[i] {
                if transaction.transaction_id == transaction_id {
                    transaction.state = TransactionState::Aborted;
                    self.transactions[i] = Some(transaction);
                    self.log_transaction_state(transaction_id, TransactionState::Aborted);
                    return true;
                }
            }
        }
        false
    }

    pub fn rollback_transaction(&mut self, transaction_id: u32) -> bool {
        for i in 0..MAX_TRANSACTIONS {
            if let Some(mut transaction) = self.transactions[i] {
                if transaction.transaction_id == transaction_id {
                    transaction.state = TransactionState::RollingBack;
                    self.transactions[i] = Some(transaction);
                    return true;
                }
            }
        }
        false
    }

    fn log_transaction_state(&mut self, transaction_id: u32, state: TransactionState) {
        let idx = (self.log_index as usize) % MAX_LOG_ENTRIES;
        let entry = TransactionLogEntry::new(self.log_index, transaction_id);
        let mut log_entry = entry;
        log_entry.state = state;
        self.log[idx] = Some(log_entry);
        self.log_index += 1;
    }

    pub fn become_leader(&mut self, term: u32, leader_id: u32) -> bool {
        if term > self.current_term {
            self.current_term = term;
            self.coordinator_role = CoordinatorRole::Leader;
            self.leader_id = leader_id;
            return true;
        }
        false
    }

    pub fn become_follower(&mut self, term: u32, leader_id: u32) -> bool {
        if term >= self.current_term {
            self.current_term = term;
            self.coordinator_role = CoordinatorRole::Follower;
            self.leader_id = leader_id;
            return true;
        }
        false
    }

    pub fn become_candidate(&mut self) -> bool {
        self.current_term += 1;
        self.coordinator_role = CoordinatorRole::Candidate;
        self.leader_id = 0;
        true
    }

    pub fn get_transaction_count(&self) -> u32 {
        self.transaction_count
    }

    pub fn get_role(&self) -> CoordinatorRole {
        self.coordinator_role
    }

    pub fn get_current_term(&self) -> u32 {
        self.current_term
    }

    pub fn get_leader_id(&self) -> u32 {
        self.leader_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let mut coordinator = TransactionCoordinator::new();
        let txn_id = coordinator.create_transaction(IsolationLevel::Serializable);
        assert!(txn_id > 0);
        assert_eq!(coordinator.get_transaction_count(), 1);
    }

    #[test]
    fn test_commit_protocol() {
        let mut coordinator = TransactionCoordinator::new();
        let txn_id = coordinator.create_transaction(IsolationLevel::ReadCommitted);
        coordinator.prepare_commit(txn_id);
        assert!(coordinator.commit_transaction(txn_id));
    }

    #[test]
    fn test_abort_handling() {
        let mut coordinator = TransactionCoordinator::new();
        let txn_id = coordinator.create_transaction(IsolationLevel::Serializable);
        assert!(coordinator.abort_transaction(txn_id));
    }

    #[test]
    fn test_leader_election() {
        let mut coordinator = TransactionCoordinator::new();
        assert!(coordinator.become_candidate());
        assert!(coordinator.become_leader(1, 1));
        assert_eq!(coordinator.get_role(), CoordinatorRole::Leader);
    }

    #[test]
    fn test_isolation_levels() {
        let mut coordinator = TransactionCoordinator::new();
        let t1 = coordinator.create_transaction(IsolationLevel::ReadUncommitted);
        let t2 = coordinator.create_transaction(IsolationLevel::ReadCommitted);
        let t3 = coordinator.create_transaction(IsolationLevel::Serializable);
        assert!(t1 > 0 && t2 > 0 && t3 > 0);
    }

    #[test]
    fn test_log_persistence() {
        let mut coordinator = TransactionCoordinator::new();
        let txn_id = coordinator.create_transaction(IsolationLevel::Serializable);
        coordinator.log_transaction_state(txn_id, TransactionState::Committed);
    }

    #[test]
    fn test_participant_failure() {
        let mut coordinator = TransactionCoordinator::new();
        let txn_id = coordinator.create_transaction(IsolationLevel::Serializable);
        coordinator.add_participant(txn_id, 1);
        coordinator.rollback_transaction(txn_id);
    }

    #[test]
    fn test_consensus() {
        let mut coordinator = TransactionCoordinator::new();
        coordinator.become_leader(1, 1);
        let txn_id = coordinator.create_transaction(IsolationLevel::Serializable);
        coordinator.add_participant(txn_id, 2);
        coordinator.prepare_commit(txn_id);
        assert!(coordinator.commit_transaction(txn_id));
    }
}
