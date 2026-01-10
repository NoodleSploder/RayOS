/// Distributed Storage
///
/// Manages distributed storage with replication, consistency, and fault tolerance.
/// Supports sharded data distribution with multi-replica consistency.

use core::cmp::min;

const MAX_STORAGE_NODES: usize = 16;
const MAX_SHARDS: usize = 256;
const MAX_REPLICAS_PER_SHARD: usize = 3;

/// Replica state enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReplicaState {
    Healthy,
    Syncing,
    Degraded,
    Failed,
    Recovering,
    Rebalancing,
    Archived,
}

/// Consistency level enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConsistencyLevel {
    Strong,
    Eventual,
    Causal,
}

/// Replica information
#[derive(Clone, Copy, Debug)]
pub struct ReplicaInfo {
    pub replica_id: u32,
    pub node_id: u32,
    pub state: ReplicaState,
    pub synced_bytes: u64,
    pub total_bytes: u64,
}

impl ReplicaInfo {
    pub fn new(replica_id: u32, node_id: u32, total_bytes: u64) -> Self {
        ReplicaInfo {
            replica_id,
            node_id,
            state: ReplicaState::Syncing,
            synced_bytes: 0,
            total_bytes,
        }
    }

    pub fn sync_progress(&self) -> u32 {
        if self.total_bytes == 0 {
            100
        } else {
            ((self.synced_bytes * 100) / self.total_bytes) as u32
        }
    }
}

/// Shard information
#[derive(Clone, Copy, Debug)]
pub struct ShardInfo {
    pub shard_id: u32,
    pub size_bytes: u64,
    pub replica_count: u8,
    pub consistency_level: ConsistencyLevel,
    pub primary_node: u32,
    pub created_timestamp: u64,
}

impl ShardInfo {
    pub fn new(shard_id: u32, size: u64) -> Self {
        ShardInfo {
            shard_id,
            size_bytes: size,
            replica_count: 1,
            consistency_level: ConsistencyLevel::Strong,
            primary_node: 0,
            created_timestamp: 0,
        }
    }
}

/// Storage node in the cluster
#[derive(Clone, Copy, Debug)]
pub struct StorageNode {
    pub node_id: u32,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub shard_count: u32,
    pub state: u8,
    pub reachable: bool,
}

impl StorageNode {
    pub fn new(node_id: u32, capacity: u64) -> Self {
        StorageNode {
            node_id,
            capacity_bytes: capacity,
            used_bytes: 0,
            shard_count: 0,
            state: 0,
            reachable: true,
        }
    }

    pub fn used_percent(&self) -> u32 {
        if self.capacity_bytes == 0 {
            0
        } else {
            ((self.used_bytes * 100) / self.capacity_bytes) as u32
        }
    }
}

/// Replication policy configuration
#[derive(Clone, Copy, Debug)]
pub struct ReplicationPolicy {
    pub replication_factor: u8,
    pub consistency_level: ConsistencyLevel,
    pub failure_tolerance: u8,
}

impl ReplicationPolicy {
    pub fn new(replication_factor: u8) -> Self {
        ReplicationPolicy {
            replication_factor,
            consistency_level: ConsistencyLevel::Strong,
            failure_tolerance: (replication_factor - 1) / 2,
        }
    }
}

/// Distributed Storage Manager
pub struct DistributedStorageManager {
    nodes: [Option<StorageNode>; MAX_STORAGE_NODES],
    shards: [Option<ShardInfo>; MAX_SHARDS],
    replicas: [Option<ReplicaInfo>; 256],
    policies: [Option<ReplicationPolicy>; 8],
    active_node_count: u32,
    active_shard_count: u32,
    replica_id_counter: u32,
    shard_id_counter: u32,
    total_capacity: u64,
    total_used: u64,
}

impl DistributedStorageManager {
    pub fn new() -> Self {
        DistributedStorageManager {
            nodes: [None; MAX_STORAGE_NODES],
            shards: [None; MAX_SHARDS],
            replicas: [None; 256],
            policies: [None; 8],
            active_node_count: 0,
            active_shard_count: 0,
            replica_id_counter: 9000,
            shard_id_counter: 10000,
            total_capacity: 0,
            total_used: 0,
        }
    }

    pub fn add_node(&mut self, capacity_bytes: u64) -> u32 {
        for i in 0..MAX_STORAGE_NODES {
            if self.nodes[i].is_none() {
                let node_id = i as u32 + 1;
                let node = StorageNode::new(node_id, capacity_bytes);
                self.nodes[i] = Some(node);
                self.active_node_count += 1;
                self.total_capacity += capacity_bytes;
                return node_id;
            }
        }
        0
    }

    pub fn remove_node(&mut self, node_id: u32) -> bool {
        let idx = (node_id as usize) - 1;
        if idx < MAX_STORAGE_NODES {
            if let Some(node) = self.nodes[idx] {
                self.total_capacity -= node.capacity_bytes;
                self.total_used = self.total_used.saturating_sub(node.used_bytes);
                self.nodes[idx] = None;
                self.active_node_count -= 1;
                return true;
            }
        }
        false
    }

    pub fn create_shard(&mut self, size_bytes: u64, replication_factor: u8) -> u32 {
        for i in 0..MAX_SHARDS {
            if self.shards[i].is_none() {
                let shard_id = self.shard_id_counter;
                self.shard_id_counter += 1;
                let mut shard = ShardInfo::new(shard_id, size_bytes);
                shard.replica_count = min(replication_factor, MAX_REPLICAS_PER_SHARD as u8);

                self.shards[i] = Some(shard);
                self.active_shard_count += 1;
                self.total_used += size_bytes;
                return shard_id;
            }
        }
        0
    }

    pub fn create_replica(&mut self, _shard_id: u32, node_id: u32, shard_size: u64) -> u32 {
        for i in 0..256 {
            if self.replicas[i].is_none() {
                let replica_id = self.replica_id_counter;
                self.replica_id_counter += 1;
                let replica = ReplicaInfo::new(replica_id, node_id, shard_size);
                self.replicas[i] = Some(replica);

                let node_idx = (node_id as usize) - 1;
                if node_idx < MAX_STORAGE_NODES {
                    if let Some(mut node) = self.nodes[node_idx] {
                        node.used_bytes += shard_size;
                        node.shard_count += 1;
                        self.nodes[node_idx] = Some(node);
                    }
                }

                return replica_id;
            }
        }
        0
    }

    pub fn sync_replica(&mut self, replica_id: u32, bytes: u64) -> bool {
        for i in 0..256 {
            if let Some(mut replica) = self.replicas[i] {
                if replica.replica_id == replica_id {
                    replica.synced_bytes = min(replica.synced_bytes + bytes, replica.total_bytes);
                    if replica.synced_bytes == replica.total_bytes {
                        replica.state = ReplicaState::Healthy;
                    }
                    self.replicas[i] = Some(replica);
                    return true;
                }
            }
        }
        false
    }

    pub fn mark_node_failed(&mut self, node_id: u32) -> bool {
        let idx = (node_id as usize) - 1;
        if idx < MAX_STORAGE_NODES {
            if let Some(mut node) = self.nodes[idx] {
                node.reachable = false;
                node.state = 3;
                self.nodes[idx] = Some(node);
                return true;
            }
        }
        false
    }

    pub fn mark_replica_failed(&mut self, replica_id: u32) -> bool {
        for i in 0..256 {
            if let Some(mut replica) = self.replicas[i] {
                if replica.replica_id == replica_id {
                    replica.state = ReplicaState::Failed;
                    self.replicas[i] = Some(replica);
                    return true;
                }
            }
        }
        false
    }

    pub fn get_active_node_count(&self) -> u32 {
        self.active_node_count
    }

    pub fn get_active_shard_count(&self) -> u32 {
        self.active_shard_count
    }

    pub fn get_healthy_node_count(&self) -> u32 {
        let mut count = 0;
        for i in 0..MAX_STORAGE_NODES {
            if let Some(node) = self.nodes[i] {
                if node.reachable {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn get_total_capacity(&self) -> u64 {
        self.total_capacity
    }

    pub fn get_total_used(&self) -> u64 {
        self.total_used
    }

    pub fn capacity_percent(&self) -> u32 {
        if self.total_capacity == 0 {
            0
        } else {
            ((self.total_used * 100) / self.total_capacity) as u32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_node() {
        let mut manager = DistributedStorageManager::new();
        let node_id = manager.add_node(1024 * 1024 * 1024);
        assert!(node_id > 0);
        assert_eq!(manager.get_active_node_count(), 1);
    }

    #[test]
    fn test_create_shard() {
        let mut manager = DistributedStorageManager::new();
        manager.add_node(1024 * 1024 * 1024);
        let shard_id = manager.create_shard(100 * 1024 * 1024, 3);
        assert!(shard_id > 0);
        assert_eq!(manager.get_active_shard_count(), 1);
    }

    #[test]
    fn test_create_replica() {
        let mut manager = DistributedStorageManager::new();
        let node_id = manager.add_node(1024 * 1024 * 1024);
        let shard_id = manager.create_shard(100 * 1024 * 1024, 3);
        let replica_id = manager.create_replica(shard_id, node_id, 100 * 1024 * 1024);
        assert!(replica_id > 0);
    }

    #[test]
    fn test_replica_sync() {
        let mut manager = DistributedStorageManager::new();
        let node_id = manager.add_node(1024 * 1024 * 1024);
        let shard_id = manager.create_shard(100 * 1024 * 1024, 3);
        let replica_id = manager.create_replica(shard_id, node_id, 100 * 1024 * 1024);

        manager.sync_replica(replica_id, 50 * 1024 * 1024);
        manager.sync_replica(replica_id, 50 * 1024 * 1024);
        assert!(manager.sync_replica(replica_id, 0));
    }

    #[test]
    fn test_node_failure() {
        let mut manager = DistributedStorageManager::new();
        let node_id = manager.add_node(1024 * 1024 * 1024);
        assert_eq!(manager.get_healthy_node_count(), 1);

        manager.mark_node_failed(node_id);
        assert_eq!(manager.get_healthy_node_count(), 0);
    }

    #[test]
    fn test_remove_node() {
        let mut manager = DistributedStorageManager::new();
        let node_id = manager.add_node(1024 * 1024 * 1024);
        assert!(manager.remove_node(node_id));
        assert_eq!(manager.get_active_node_count(), 0);
    }

    #[test]
    fn test_capacity_tracking() {
        let mut manager = DistributedStorageManager::new();
        manager.add_node(1024 * 1024 * 1024);
        manager.add_node(2048 * 1024 * 1024);
        assert_eq!(manager.get_total_capacity(), 3072 * 1024 * 1024);
    }

    #[test]
    fn test_replica_failure() {
        let mut manager = DistributedStorageManager::new();
        let node_id = manager.add_node(1024 * 1024 * 1024);
        let shard_id = manager.create_shard(100 * 1024 * 1024, 3);
        let replica_id = manager.create_replica(shard_id, node_id, 100 * 1024 * 1024);

        assert!(manager.mark_replica_failed(replica_id));
    }
}
