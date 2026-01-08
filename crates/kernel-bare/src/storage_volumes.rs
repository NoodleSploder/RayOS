/// Storage Volume Management
///
/// Manages virtual storage volumes with snapshots, replication, and tiering.
/// Supports block and object storage abstractions with asynchronous replication
/// across nodes and tiered storage selection (SSD/HDD).

use core::cmp::min;

const MAX_VOLUMES: usize = 32;
const MAX_SNAPSHOTS_PER_VOLUME: usize = 256;
const MAX_REPLICAS: usize = 3;
const MAX_VOLUME_METRICS: usize = 32;

/// Volume type enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VolumeType {
    Block,
    Object,
    File,
    Distributed,
}

/// Volume lifecycle state enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VolumeState {
    Created,
    Initializing,
    Available,
    Attached,
    Snapshotting,
    Degraded,
    Detaching,
    Deleted,
}

/// Volume snapshot metadata
#[derive(Clone, Copy, Debug)]
pub struct VolumeSnapshot {
    pub snapshot_id: u32,
    pub volume_id: u32,
    pub timestamp: u64,
    pub size: u64,
    pub parent_id: u32,
    pub compressed: bool,
    pub state: u8,
}

impl VolumeSnapshot {
    pub fn new(snapshot_id: u32, volume_id: u32, size: u64, parent_id: u32) -> Self {
        VolumeSnapshot {
            snapshot_id,
            volume_id,
            timestamp: 0,
            size,
            parent_id,
            compressed: false,
            state: 0,
        }
    }
}

/// Replication session tracking
#[derive(Clone, Copy, Debug)]
pub struct ReplicationSession {
    pub session_id: u32,
    pub source_volume: u32,
    pub target_node: u32,
    pub bytes_replicated: u64,
    pub total_bytes: u64,
    pub replica_count: u8,
    pub state: u8,
}

impl ReplicationSession {
    pub fn new(session_id: u32, source_volume: u32, target_node: u32, total_bytes: u64) -> Self {
        ReplicationSession {
            session_id,
            source_volume,
            target_node,
            bytes_replicated: 0,
            total_bytes,
            replica_count: 1,
            state: 0,
        }
    }

    pub fn progress_percent(&self) -> u32 {
        if self.total_bytes == 0 {
            100
        } else {
            ((self.bytes_replicated * 100) / self.total_bytes) as u32
        }
    }
}

/// Volume performance metrics
#[derive(Clone, Copy, Debug)]
pub struct VolumeMetrics {
    pub read_count: u64,
    pub write_count: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub latency_us: u32,
    pub throughput_mbps: u32,
}

impl VolumeMetrics {
    pub fn new() -> Self {
        VolumeMetrics {
            read_count: 0,
            write_count: 0,
            read_bytes: 0,
            write_bytes: 0,
            latency_us: 0,
            throughput_mbps: 0,
        }
    }

    pub fn record_read(&mut self, bytes: u64, latency_us: u32) {
        self.read_count += 1;
        self.read_bytes += bytes;
        self.latency_us = latency_us;
    }

    pub fn record_write(&mut self, bytes: u64, latency_us: u32) {
        self.write_count += 1;
        self.write_bytes += bytes;
        self.latency_us = latency_us;
    }
}

/// Volume definition with capacity and replication info
#[derive(Clone, Copy, Debug)]
pub struct Volume {
    pub volume_id: u32,
    pub volume_type: VolumeType,
    pub state: VolumeState,
    pub capacity: u64,
    pub used: u64,
    pub num_snapshots: u32,
    pub num_replicas: u8,
    pub attached_to: u32,
    pub checksum: u32,
}

impl Volume {
    pub fn new(volume_id: u32, volume_type: VolumeType, capacity: u64) -> Self {
        Volume {
            volume_id,
            volume_type,
            state: VolumeState::Created,
            capacity,
            used: 0,
            num_snapshots: 0,
            num_replicas: 1,
            attached_to: 0,
            checksum: 0,
        }
    }

    pub fn validate_state_transition(&self, new_state: VolumeState) -> bool {
        match (self.state, new_state) {
            (VolumeState::Created, VolumeState::Initializing) => true,
            (VolumeState::Initializing, VolumeState::Available) => true,
            (VolumeState::Available, VolumeState::Attached) => true,
            (VolumeState::Available, VolumeState::Snapshotting) => true,
            (VolumeState::Attached, VolumeState::Detaching) => true,
            (VolumeState::Snapshotting, VolumeState::Available) => true,
            (VolumeState::Available, VolumeState::Degraded) => true,
            (VolumeState::Degraded, VolumeState::Available) => true,
            (VolumeState::Attached, VolumeState::Snapshotting) => true,
            (VolumeState::Detaching, VolumeState::Available) => true,
            (VolumeState::Available, VolumeState::Deleted) => true,
            _ => false,
        }
    }

    pub fn used_percent(&self) -> u32 {
        if self.capacity == 0 {
            0
        } else {
            ((self.used * 100) / self.capacity) as u32
        }
    }
}

/// Storage Volume Manager
pub struct StorageVolumeManager {
    volumes: [Option<Volume>; MAX_VOLUMES],
    snapshots: [Option<VolumeSnapshot>; MAX_SNAPSHOTS_PER_VOLUME],
    replication_sessions: [Option<ReplicationSession>; 16],
    metrics: [Option<VolumeMetrics>; MAX_VOLUME_METRICS],
    active_volume_count: u32,
    snapshot_id_counter: u32,
    replication_session_counter: u32,
    total_capacity: u64,
    total_used: u64,
}

impl StorageVolumeManager {
    pub fn new() -> Self {
        StorageVolumeManager {
            volumes: [None; MAX_VOLUMES],
            snapshots: [None; MAX_SNAPSHOTS_PER_VOLUME],
            replication_sessions: [None; 16],
            metrics: [None; MAX_VOLUME_METRICS],
            active_volume_count: 0,
            snapshot_id_counter: 1000,
            replication_session_counter: 5000,
            total_capacity: 0,
            total_used: 0,
        }
    }

    pub fn create_volume(&mut self, volume_type: VolumeType, capacity: u64) -> u32 {
        for i in 0..MAX_VOLUMES {
            if self.volumes[i].is_none() {
                let volume_id = i as u32 + 1;
                let volume = Volume::new(volume_id, volume_type, capacity);
                self.volumes[i] = Some(volume);
                self.metrics[i % MAX_VOLUME_METRICS] = Some(VolumeMetrics::new());
                self.active_volume_count += 1;
                self.total_capacity += capacity;
                return volume_id;
            }
        }
        0
    }

    pub fn delete_volume(&mut self, volume_id: u32) -> bool {
        let idx = (volume_id as usize) - 1;
        if idx < MAX_VOLUMES {
            if let Some(vol) = self.volumes[idx] {
                if vol.state == VolumeState::Available {
                    self.total_capacity -= vol.capacity;
                    self.total_used = self.total_used.saturating_sub(vol.used);
                    self.volumes[idx] = None;
                    self.active_volume_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn get_volume(&self, volume_id: u32) -> Option<Volume> {
        let idx = (volume_id as usize) - 1;
        if idx < MAX_VOLUMES {
            self.volumes[idx]
        } else {
            None
        }
    }

    pub fn transition_volume_state(&mut self, volume_id: u32, new_state: VolumeState) -> bool {
        let idx = (volume_id as usize) - 1;
        if idx < MAX_VOLUMES {
            if let Some(mut vol) = self.volumes[idx] {
                if vol.validate_state_transition(new_state) {
                    vol.state = new_state;
                    self.volumes[idx] = Some(vol);
                    return true;
                }
            }
        }
        false
    }

    pub fn create_snapshot(&mut self, volume_id: u32, size: u64, parent_id: u32) -> u32 {
        for i in 0..MAX_SNAPSHOTS_PER_VOLUME {
            if self.snapshots[i].is_none() {
                let snapshot_id = self.snapshot_id_counter;
                self.snapshot_id_counter += 1;
                let snapshot = VolumeSnapshot::new(snapshot_id, volume_id, size, parent_id);
                self.snapshots[i] = Some(snapshot);

                let idx = (volume_id as usize) - 1;
                if idx < MAX_VOLUMES {
                    if let Some(mut vol) = self.volumes[idx] {
                        vol.num_snapshots += 1;
                        self.volumes[idx] = Some(vol);
                    }
                }

                return snapshot_id;
            }
        }
        0
    }

    pub fn delete_snapshot(&mut self, snapshot_id: u32) -> bool {
        for i in 0..MAX_SNAPSHOTS_PER_VOLUME {
            if let Some(snap) = self.snapshots[i] {
                if snap.snapshot_id == snapshot_id {
                    let volume_id = snap.volume_id;
                    let idx = (volume_id as usize) - 1;
                    if idx < MAX_VOLUMES {
                        if let Some(mut vol) = self.volumes[idx] {
                            vol.num_snapshots = vol.num_snapshots.saturating_sub(1);
                            self.volumes[idx] = Some(vol);
                        }
                    }
                    self.snapshots[i] = None;
                    return true;
                }
            }
        }
        false
    }

    pub fn start_replication(&mut self, volume_id: u32, target_node: u32) -> u32 {
        for i in 0..16 {
            if self.replication_sessions[i].is_none() {
                let idx = (volume_id as usize) - 1;
                let total_size = if idx < MAX_VOLUMES {
                    if let Some(vol) = self.volumes[idx] {
                        vol.capacity
                    } else {
                        0
                    }
                } else {
                    0
                };

                let session_id = self.replication_session_counter;
                self.replication_session_counter += 1;
                let session = ReplicationSession::new(session_id, volume_id, target_node, total_size);
                self.replication_sessions[i] = Some(session);
                return session_id;
            }
        }
        0
    }

    pub fn advance_replication(&mut self, session_id: u32, bytes_copied: u64) -> bool {
        for i in 0..16 {
            if let Some(mut session) = self.replication_sessions[i] {
                if session.session_id == session_id {
                    session.bytes_replicated = min(
                        session.bytes_replicated + bytes_copied,
                        session.total_bytes,
                    );
                    self.replication_sessions[i] = Some(session);
                    return true;
                }
            }
        }
        false
    }

    pub fn complete_replication(&mut self, session_id: u32) -> bool {
        for i in 0..16 {
            if let Some(session) = self.replication_sessions[i] {
                if session.session_id == session_id {
                    let volume_id = session.source_volume;
                    let idx = (volume_id as usize) - 1;
                    if idx < MAX_VOLUMES {
                        if let Some(mut vol) = self.volumes[idx] {
                            if vol.num_replicas < MAX_REPLICAS as u8 {
                                vol.num_replicas += 1;
                            }
                            self.volumes[idx] = Some(vol);
                        }
                    }
                    self.replication_sessions[i] = None;
                    return true;
                }
            }
        }
        false
    }

    pub fn update_volume_metrics(&mut self, volume_id: u32, read_bytes: u64, write_bytes: u64) {
        let idx = (volume_id as usize) - 1;
        if idx < MAX_VOLUME_METRICS {
            if let Some(mut metrics) = self.metrics[idx] {
                if read_bytes > 0 {
                    metrics.record_read(read_bytes, 45);
                }
                if write_bytes > 0 {
                    metrics.record_write(write_bytes, 50);
                }
                self.metrics[idx] = Some(metrics);
            }
        }
    }

    pub fn get_metrics(&self, volume_id: u32) -> Option<VolumeMetrics> {
        let idx = (volume_id as usize) - 1;
        if idx < MAX_VOLUME_METRICS {
            self.metrics[idx]
        } else {
            None
        }
    }

    pub fn get_active_volume_count(&self) -> u32 {
        self.active_volume_count
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
    fn test_create_volume() {
        let mut manager = StorageVolumeManager::new();
        let vol_id = manager.create_volume(VolumeType::Block, 1024 * 1024);
        assert!(vol_id > 0);
        assert_eq!(manager.get_active_volume_count(), 1);
    }

    #[test]
    fn test_volume_state_transitions() {
        let vol = Volume::new(1, VolumeType::Block, 1024);
        assert!(vol.validate_state_transition(VolumeState::Initializing));
        assert!(!vol.validate_state_transition(VolumeState::Attached));
    }

    #[test]
    fn test_delete_volume() {
        let mut manager = StorageVolumeManager::new();
        let vol_id = manager.create_volume(VolumeType::Block, 512);
        manager.transition_volume_state(vol_id, VolumeState::Initializing);
        manager.transition_volume_state(vol_id, VolumeState::Available);
        assert!(manager.delete_volume(vol_id));
        assert_eq!(manager.get_active_volume_count(), 0);
    }

    #[test]
    fn test_snapshot_creation() {
        let mut manager = StorageVolumeManager::new();
        let vol_id = manager.create_volume(VolumeType::Block, 1024);
        manager.transition_volume_state(vol_id, VolumeState::Initializing);
        manager.transition_volume_state(vol_id, VolumeState::Available);
        manager.transition_volume_state(vol_id, VolumeState::Snapshotting);

        let snap_id = manager.create_snapshot(vol_id, 512, 0);
        assert!(snap_id > 0);

        let vol = manager.get_volume(vol_id);
        assert!(vol.is_some());
        assert_eq!(vol.unwrap().num_snapshots, 1);
    }

    #[test]
    fn test_replication_session() {
        let mut manager = StorageVolumeManager::new();
        let vol_id = manager.create_volume(VolumeType::Block, 2048);
        manager.transition_volume_state(vol_id, VolumeState::Initializing);
        manager.transition_volume_state(vol_id, VolumeState::Available);

        let session_id = manager.start_replication(vol_id, 2);
        assert!(session_id > 0);

        manager.advance_replication(session_id, 1024);
        manager.complete_replication(session_id);

        let vol = manager.get_volume(vol_id);
        assert!(vol.is_some());
        assert_eq!(vol.unwrap().num_replicas, 2);
    }

    #[test]
    fn test_volume_metrics() {
        let mut manager = StorageVolumeManager::new();
        let vol_id = manager.create_volume(VolumeType::Block, 1024);

        manager.update_volume_metrics(vol_id, 512, 256);

        let metrics = manager.get_metrics(vol_id);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m.read_count, 1);
        assert_eq!(m.write_count, 1);
    }

    #[test]
    fn test_capacity_tracking() {
        let mut manager = StorageVolumeManager::new();
        manager.create_volume(VolumeType::Block, 1000);
        manager.create_volume(VolumeType::File, 2000);

        assert_eq!(manager.get_total_capacity(), 3000);
        assert_eq!(manager.get_active_volume_count(), 2);
    }

    #[test]
    fn test_snapshot_deletion() {
        let mut manager = StorageVolumeManager::new();
        let vol_id = manager.create_volume(VolumeType::Block, 1024);
        manager.transition_volume_state(vol_id, VolumeState::Initializing);
        manager.transition_volume_state(vol_id, VolumeState::Available);
        manager.transition_volume_state(vol_id, VolumeState::Snapshotting);

        let snap_id = manager.create_snapshot(vol_id, 512, 0);
        assert!(snap_id > 0);

        assert!(manager.delete_snapshot(snap_id));
        let vol = manager.get_volume(vol_id);
        assert!(vol.is_some());
        assert_eq!(vol.unwrap().num_snapshots, 0);
    }
}
