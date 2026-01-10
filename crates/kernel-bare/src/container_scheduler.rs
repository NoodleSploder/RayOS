//! Advanced Container Scheduling
//!
//! Multi-constraint resource allocation with intelligent bin packing.
//! Supports 512 containers across placement groups with live migration.


/// Container identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerId(pub u16);

/// Compute node identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(pub u8);

/// Resource requirements specification
#[derive(Clone, Copy, Debug)]
pub struct ResourceRequirement {
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub gpu_count: u8,
    pub storage_mb: u32,
    pub bandwidth_mbps: u32,
}

/// Scheduling strategy
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulingStrategy {
    FirstFit,
    BestFit,
    BinPack,
}

/// Placement group constraints
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementGroup {
    Spread,
    Cluster,
    Partition,
}

/// Container with requirements
#[derive(Clone, Copy)]
pub struct Container {
    pub container_id: ContainerId,
    pub requirements: ResourceRequirement,
    pub priority: u8,
    pub placement_group: PlacementGroup,
    pub group_id: u16,
}

/// Node resource capacity
#[derive(Clone, Copy)]
pub struct ComputeNode {
    pub node_id: NodeId,
    pub total_cpu: u32,
    pub available_cpu: u32,
    pub total_memory: u32,
    pub available_memory: u32,
    pub gpu_count: u8,
    pub gpu_available: u8,
    pub storage_total: u32,
    pub storage_available: u32,
}

/// Placement decision
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub container_id: ContainerId,
    pub node_id: NodeId,
    pub placement_valid: bool,
}

/// Container scheduler
pub struct ContainerScheduler {
    // Container registry
    containers: [Container; 512],
    container_count: u16,

    // Node registry
    nodes: [ComputeNode; 32],
    node_count: u8,

    // Placements
    placements: [Placement; 512],
    placement_count: u16,

    // Placement groups
    groups: [PlacementGroup; 64],
    group_count: u8,

    // Scheduling strategy
    strategy: SchedulingStrategy,

    // Migration tracking
    migrations: [Migration; 128],
    migration_count: u8,
}

/// Container migration
#[derive(Clone, Copy, Debug)]
pub struct Migration {
    pub container_id: ContainerId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub progress_percent: u32,
}

impl ContainerScheduler {
    /// Create new scheduler
    pub fn new(strategy: SchedulingStrategy) -> Self {
        ContainerScheduler {
            containers: [Container {
                container_id: ContainerId(0),
                requirements: ResourceRequirement {
                    cpu_cores: 0,
                    memory_mb: 0,
                    gpu_count: 0,
                    storage_mb: 0,
                    bandwidth_mbps: 0,
                },
                priority: 0,
                placement_group: PlacementGroup::Spread,
                group_id: 0,
            }; 512],
            container_count: 0,

            nodes: [ComputeNode {
                node_id: NodeId(0),
                total_cpu: 0,
                available_cpu: 0,
                total_memory: 0,
                available_memory: 0,
                gpu_count: 0,
                gpu_available: 0,
                storage_total: 0,
                storage_available: 0,
            }; 32],
            node_count: 0,

            placements: [Placement {
                container_id: ContainerId(0),
                node_id: NodeId(0),
                placement_valid: false,
            }; 512],
            placement_count: 0,

            groups: [PlacementGroup::Spread; 64],
            group_count: 0,

            strategy,

            migrations: [Migration {
                container_id: ContainerId(0),
                from_node: NodeId(0),
                to_node: NodeId(0),
                progress_percent: 0,
            }; 128],
            migration_count: 0,
        }
    }

    /// Add compute node
    pub fn add_node(&mut self, node_id: NodeId, cpu: u32, memory_mb: u32,
                   gpu: u8, storage_mb: u32) -> bool {
        if self.node_count >= 32 {
            return false;
        }

        self.nodes[self.node_count as usize] = ComputeNode {
            node_id,
            total_cpu: cpu,
            available_cpu: cpu,
            total_memory: memory_mb,
            available_memory: memory_mb,
            gpu_count: gpu,
            gpu_available: gpu,
            storage_total: storage_mb,
            storage_available: storage_mb,
        };
        self.node_count += 1;
        true
    }

    /// Register container
    pub fn register_container(&mut self, container_id: ContainerId, req: ResourceRequirement,
                             priority: u8, placement_group: PlacementGroup, group_id: u16) -> bool {
        if self.container_count >= 512 {
            return false;
        }

        self.containers[self.container_count as usize] = Container {
            container_id,
            requirements: req,
            priority,
            placement_group,
            group_id,
        };
        self.container_count += 1;
        true
    }

    /// Schedule container
    pub fn schedule_container(&mut self, container_id: ContainerId) -> Option<NodeId> {
        // Find container
        let mut container_idx = None;
        for i in 0..self.container_count as usize {
            if self.containers[i].container_id == container_id {
                container_idx = Some(i);
                break;
            }
        }

        let idx = container_idx?;
        let container = self.containers[idx];
        let strategy = self.strategy;

        match strategy {
            SchedulingStrategy::FirstFit => self.schedule_first_fit(&container),
            SchedulingStrategy::BestFit => self.schedule_best_fit(&container),
            SchedulingStrategy::BinPack => self.schedule_bin_pack(&container),
        }
    }

    /// First-fit scheduling
    fn schedule_first_fit(&mut self, container: &Container) -> Option<NodeId> {
        for i in 0..self.node_count as usize {
            if self.can_fit(container, &self.nodes[i]) {
                let node = &mut self.nodes[i];
                node.available_cpu -= container.requirements.cpu_cores;
                node.available_memory -= container.requirements.memory_mb;
                node.gpu_available = node.gpu_available.saturating_sub(container.requirements.gpu_count);
                node.storage_available -= container.requirements.storage_mb;

                if self.placement_count < 512 {
                    self.placements[self.placement_count as usize] = Placement {
                        container_id: container.container_id,
                        node_id: node.node_id,
                        placement_valid: true,
                    };
                    self.placement_count += 1;
                }

                return Some(node.node_id);
            }
        }
        None
    }

    /// Best-fit scheduling (minimize fragmentation)
    fn schedule_best_fit(&mut self, container: &Container) -> Option<NodeId> {
        let mut best_node_idx = None;
        let mut best_waste = u32::MAX;

        for i in 0..self.node_count as usize {
            if self.can_fit(container, &self.nodes[i]) {
                let waste = self.nodes[i].available_memory - container.requirements.memory_mb;
                if waste < best_waste {
                    best_waste = waste;
                    best_node_idx = Some(i);
                }
            }
        }

        best_node_idx.and_then(|i| {
            let node = &mut self.nodes[i];
            node.available_cpu -= container.requirements.cpu_cores;
            node.available_memory -= container.requirements.memory_mb;
            node.gpu_available = node.gpu_available.saturating_sub(container.requirements.gpu_count);
            node.storage_available -= container.requirements.storage_mb;

            if self.placement_count < 512 {
                self.placements[self.placement_count as usize] = Placement {
                    container_id: container.container_id,
                    node_id: node.node_id,
                    placement_valid: true,
                };
                self.placement_count += 1;
            }

            Some(node.node_id)
        })
    }

    /// Bin-pack scheduling (maximize density)
    fn schedule_bin_pack(&mut self, container: &Container) -> Option<NodeId> {
        let mut best_node_idx = None;
        let mut max_utilization = 0u32;

        for i in 0..self.node_count as usize {
            if self.can_fit(container, &self.nodes[i]) {
                let util = (self.nodes[i].total_cpu - self.nodes[i].available_cpu) * 100 / self.nodes[i].total_cpu;
                if util > max_utilization {
                    max_utilization = util;
                    best_node_idx = Some(i);
                }
            }
        }

        best_node_idx.and_then(|i| {
            let node = &mut self.nodes[i];
            node.available_cpu -= container.requirements.cpu_cores;
            node.available_memory -= container.requirements.memory_mb;

            if self.placement_count < 512 {
                self.placements[self.placement_count as usize] = Placement {
                    container_id: container.container_id,
                    node_id: node.node_id,
                    placement_valid: true,
                };
                self.placement_count += 1;
            }

            Some(node.node_id)
        })
    }

    /// Check if container can fit on node
    fn can_fit(&self, container: &Container, node: &ComputeNode) -> bool {
        node.available_cpu >= container.requirements.cpu_cores
            && node.available_memory >= container.requirements.memory_mb
            && node.gpu_available >= container.requirements.gpu_count
            && node.storage_available >= container.requirements.storage_mb
    }

    /// Start live migration
    pub fn start_migration(&mut self, container_id: ContainerId, from_node: NodeId,
                         to_node: NodeId) -> bool {
        if self.migration_count >= 128 {
            return false;
        }

        self.migrations[self.migration_count as usize] = Migration {
            container_id,
            from_node,
            to_node,
            progress_percent: 0,
        };
        self.migration_count += 1;
        true
    }

    /// Update migration progress
    pub fn update_migration_progress(&mut self, container_id: ContainerId, progress: u32) {
        for i in 0..self.migration_count as usize {
            if self.migrations[i].container_id == container_id {
                self.migrations[i].progress_percent = progress.min(100);
                break;
            }
        }
    }

    /// Get scheduled container count
    pub fn get_scheduled_count(&self) -> u16 {
        self.placement_count
    }

    /// Get node utilization
    pub fn get_node_utilization(&self, node_id: NodeId) -> (u32, u32) {
        for i in 0..self.node_count as usize {
            if self.nodes[i].node_id == node_id {
                let cpu_util = (self.nodes[i].total_cpu - self.nodes[i].available_cpu) * 100 / self.nodes[i].total_cpu;
                let mem_util = (self.nodes[i].total_memory - self.nodes[i].available_memory) * 100 / self.nodes[i].total_memory;
                return (cpu_util, mem_util);
            }
        }
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_creation() {
        let scheduler = ContainerScheduler::new(SchedulingStrategy::FirstFit);
        assert_eq!(scheduler.get_scheduled_count(), 0);
    }

    #[test]
    fn test_add_node() {
        let mut scheduler = ContainerScheduler::new(SchedulingStrategy::FirstFit);
        assert!(scheduler.add_node(NodeId(1), 8, 16000, 2, 100000));
    }
}
