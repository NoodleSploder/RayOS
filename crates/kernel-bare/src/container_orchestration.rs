
const MAX_CONTAINERS: usize = 128;
const MAX_PODS: usize = 32;
const MAX_IMAGES: usize = 16;
const MAX_HEALTH_CHECKS: usize = 64;

/// Container state enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContainerState {
    Created,
    Starting,
    Running,
    Paused,
    Stopping,
    Stopped,
    Failed,
    Restarting,
    Terminated,
}

/// Restart policy enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RestartPolicy {
    Never,
    Always,
    OnFailure,
}

/// Container resource limits
#[derive(Clone, Copy, Debug)]
pub struct ContainerResourceLimit {
    pub cpu_cores: u16,
    pub memory_mb: u32,
    pub disk_mb: u32,
    pub network_bandwidth_mbps: u32,
}

impl ContainerResourceLimit {
    pub fn new(cpu: u16, mem: u32) -> Self {
        ContainerResourceLimit {
            cpu_cores: cpu,
            memory_mb: mem,
            disk_mb: 1000,
            network_bandwidth_mbps: 1000,
        }
    }
}

/// Container image definition
#[derive(Clone, Copy, Debug)]
pub struct ContainerImage {
    pub image_id: u32,
    pub name: [u8; 64],
    pub tag: [u8; 32],
    pub size_mb: u32,
    pub layer_count: u8,
    pub checksum: u32,
}

impl ContainerImage {
    pub fn new(image_id: u32, size: u32) -> Self {
        ContainerImage {
            image_id,
            name: [0; 64],
            tag: [0; 32],
            size_mb: size,
            layer_count: 1,
            checksum: 0,
        }
    }
}

/// Health check definition
#[derive(Clone, Copy, Debug)]
pub struct HealthCheck {
    pub check_id: u32,
    pub interval_sec: u32,
    pub timeout_sec: u32,
    pub retries: u8,
    pub consecutive_failures: u8,
    pub state: u8,
}

impl HealthCheck {
    pub fn new(check_id: u32) -> Self {
        HealthCheck {
            check_id,
            interval_sec: 30,
            timeout_sec: 5,
            retries: 3,
            consecutive_failures: 0,
            state: 0,
        }
    }
}

/// Container definition
#[derive(Clone, Copy, Debug)]
pub struct Container {
    pub container_id: u32,
    pub image_id: u32,
    pub state: ContainerState,
    pub pid: u32,
    pub resource_limit: ContainerResourceLimit,
    pub restart_policy: RestartPolicy,
    pub restart_count: u32,
    pub uptime_sec: u64,
}

impl Container {
    pub fn new(container_id: u32, image_id: u32, cpu: u16, memory: u32) -> Self {
        Container {
            container_id,
            image_id,
            state: ContainerState::Created,
            pid: 0,
            resource_limit: ContainerResourceLimit::new(cpu, memory),
            restart_policy: RestartPolicy::OnFailure,
            restart_count: 0,
            uptime_sec: 0,
        }
    }

    pub fn validate_state_transition(&self, new_state: ContainerState) -> bool {
        match (self.state, new_state) {
            (ContainerState::Created, ContainerState::Starting) => true,
            (ContainerState::Starting, ContainerState::Running) => true,
            (ContainerState::Running, ContainerState::Paused) => true,
            (ContainerState::Paused, ContainerState::Running) => true,
            (ContainerState::Running, ContainerState::Stopping) => true,
            (ContainerState::Stopping, ContainerState::Stopped) => true,
            (ContainerState::Stopped, ContainerState::Starting) => true,
            (ContainerState::Running, ContainerState::Failed) => true,
            (ContainerState::Failed, ContainerState::Restarting) => true,
            (ContainerState::Restarting, ContainerState::Starting) => true,
            (ContainerState::Stopped, ContainerState::Terminated) => true,
            _ => false,
        }
    }
}

/// Pod definition (Kubernetes-like grouping)
#[derive(Clone, Copy, Debug)]
pub struct Pod {
    pub pod_id: u32,
    pub container_count: u8,
    pub network_namespace: u32,
    pub state: u8,
    pub created_timestamp: u64,
    pub restart_policy: RestartPolicy,
}

impl Pod {
    pub fn new(pod_id: u32) -> Self {
        Pod {
            pod_id,
            container_count: 0,
            network_namespace: 0,
            state: 0,
            created_timestamp: 0,
            restart_policy: RestartPolicy::OnFailure,
        }
    }

    pub fn can_add_container(&self) -> bool {
        (self.container_count as usize) < 4
    }
}

/// Container Orchestrator
pub struct ContainerOrchestrator {
    containers: [Option<Container>; MAX_CONTAINERS],
    pods: [Option<Pod>; MAX_PODS],
    images: [Option<ContainerImage>; MAX_IMAGES],
    health_checks: [Option<HealthCheck>; MAX_HEALTH_CHECKS],
    active_container_count: u32,
    active_pod_count: u32,
    image_id_counter: u32,
    container_id_counter: u32,
    pod_id_counter: u32,
    health_check_counter: u32,
}

impl ContainerOrchestrator {
    pub fn new() -> Self {
        ContainerOrchestrator {
            containers: [None; MAX_CONTAINERS],
            pods: [None; MAX_PODS],
            images: [None; MAX_IMAGES],
            health_checks: [None; MAX_HEALTH_CHECKS],
            active_container_count: 0,
            active_pod_count: 0,
            image_id_counter: 3000,
            container_id_counter: 4000,
            pod_id_counter: 5000,
            health_check_counter: 6000,
        }
    }

    pub fn register_image(&mut self, size_mb: u32) -> u32 {
        for i in 0..MAX_IMAGES {
            if self.images[i].is_none() {
                let image_id = self.image_id_counter;
                self.image_id_counter += 1;
                let image = ContainerImage::new(image_id, size_mb);
                self.images[i] = Some(image);
                return image_id;
            }
        }
        0
    }

    pub fn create_pod(&mut self) -> u32 {
        for i in 0..MAX_PODS {
            if self.pods[i].is_none() {
                let pod_id = self.pod_id_counter;
                self.pod_id_counter += 1;
                let pod = Pod::new(pod_id);
                self.pods[i] = Some(pod);
                self.active_pod_count += 1;
                return pod_id;
            }
        }
        0
    }

    pub fn delete_pod(&mut self, pod_id: u32) -> bool {
        for i in 0..MAX_PODS {
            if let Some(pod) = self.pods[i] {
                if pod.pod_id == pod_id {
                    self.pods[i] = None;
                    self.active_pod_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn create_container(
        &mut self,
        pod_id: u32,
        image_id: u32,
        cpu: u16,
        memory: u32,
    ) -> u32 {
        for i in 0..MAX_CONTAINERS {
            if self.containers[i].is_none() {
                let container_id = self.container_id_counter;
                self.container_id_counter += 1;

                let container = Container::new(container_id, image_id, cpu, memory);
                self.containers[i] = Some(container);
                self.active_container_count += 1;

                for j in 0..MAX_PODS {
                    if let Some(mut pod) = self.pods[j] {
                        if pod.pod_id == pod_id && pod.can_add_container() {
                            pod.container_count += 1;
                            self.pods[j] = Some(pod);
                            break;
                        }
                    }
                }

                return container_id;
            }
        }
        0
    }

    pub fn transition_container_state(&mut self, container_id: u32, new_state: ContainerState) -> bool {
        for i in 0..MAX_CONTAINERS {
            if let Some(mut container) = self.containers[i] {
                if container.container_id == container_id {
                    if container.validate_state_transition(new_state) {
                        container.state = new_state;
                        self.containers[i] = Some(container);
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn get_container(&self, container_id: u32) -> Option<Container> {
        for i in 0..MAX_CONTAINERS {
            if let Some(container) = self.containers[i] {
                if container.container_id == container_id {
                    return Some(container);
                }
            }
        }
        None
    }

    pub fn delete_container(&mut self, container_id: u32) -> bool {
        for i in 0..MAX_CONTAINERS {
            if let Some(container) = self.containers[i] {
                if container.container_id == container_id {
                    if container.state == ContainerState::Terminated
                        || container.state == ContainerState::Stopped
                    {
                        self.active_container_count -= 1;
                        self.containers[i] = None;
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn set_restart_policy(&mut self, container_id: u32, policy: RestartPolicy) -> bool {
        for i in 0..MAX_CONTAINERS {
            if let Some(mut container) = self.containers[i] {
                if container.container_id == container_id {
                    container.restart_policy = policy;
                    self.containers[i] = Some(container);
                    return true;
                }
            }
        }
        false
    }

    pub fn create_health_check(&mut self, _container_id: u32) -> u32 {
        for i in 0..MAX_HEALTH_CHECKS {
            if self.health_checks[i].is_none() {
                let check_id = self.health_check_counter;
                self.health_check_counter += 1;
                let health_check = HealthCheck::new(check_id);
                self.health_checks[i] = Some(health_check);
                return check_id;
            }
        }
        0
    }

    pub fn record_health_check_result(&mut self, check_id: u32, is_healthy: bool) -> bool {
        for i in 0..MAX_HEALTH_CHECKS {
            if let Some(mut hc) = self.health_checks[i] {
                if hc.check_id == check_id {
                    if is_healthy {
                        hc.consecutive_failures = 0;
                    } else {
                        hc.consecutive_failures = hc.consecutive_failures.saturating_add(1);
                    }
                    self.health_checks[i] = Some(hc);
                    return true;
                }
            }
        }
        false
    }

    pub fn get_active_container_count(&self) -> u32 {
        self.active_container_count
    }

    pub fn get_active_pod_count(&self) -> u32 {
        self.active_pod_count
    }

    pub fn get_running_container_count(&self) -> u32 {
        let mut count = 0;
        for i in 0..MAX_CONTAINERS {
            if let Some(container) = self.containers[i] {
                if container.state == ContainerState::Running {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn get_failed_container_count(&self) -> u32 {
        let mut count = 0;
        for i in 0..MAX_CONTAINERS {
            if let Some(container) = self.containers[i] {
                if container.state == ContainerState::Failed {
                    count += 1;
                }
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_image() {
        let mut orchestrator = ContainerOrchestrator::new();
        let image_id = orchestrator.register_image(500);
        assert!(image_id > 0);
    }

    #[test]
    fn test_create_pod() {
        let mut orchestrator = ContainerOrchestrator::new();
        let pod_id = orchestrator.create_pod();
        assert!(pod_id > 0);
        assert_eq!(orchestrator.get_active_pod_count(), 1);
    }

    #[test]
    fn test_create_container() {
        let mut orchestrator = ContainerOrchestrator::new();
        let image_id = orchestrator.register_image(500);
        let pod_id = orchestrator.create_pod();
        let container_id = orchestrator.create_container(pod_id, image_id, 2, 512);

        assert!(container_id > 0);
        assert_eq!(orchestrator.get_active_container_count(), 1);
    }

    #[test]
    fn test_container_state_transitions() {
        let mut orchestrator = ContainerOrchestrator::new();
        let image_id = orchestrator.register_image(500);
        let pod_id = orchestrator.create_pod();
        let container_id = orchestrator.create_container(pod_id, image_id, 2, 512);

        assert!(orchestrator.transition_container_state(container_id, ContainerState::Starting));
        assert!(orchestrator.transition_container_state(container_id, ContainerState::Running));

        let container = orchestrator.get_container(container_id);
        assert!(container.is_some());
        assert_eq!(container.unwrap().state, ContainerState::Running);
    }

    #[test]
    fn test_restart_policy() {
        let mut orchestrator = ContainerOrchestrator::new();
        let image_id = orchestrator.register_image(500);
        let pod_id = orchestrator.create_pod();
        let container_id = orchestrator.create_container(pod_id, image_id, 2, 512);

        assert!(orchestrator.set_restart_policy(container_id, RestartPolicy::Always));

        let container = orchestrator.get_container(container_id);
        assert!(container.is_some());
        assert_eq!(container.unwrap().restart_policy, RestartPolicy::Always);
    }

    #[test]
    fn test_health_check() {
        let mut orchestrator = ContainerOrchestrator::new();
        let check_id = orchestrator.create_health_check(1);

        assert!(check_id > 0);
        assert!(orchestrator.record_health_check_result(check_id, true));
        assert!(orchestrator.record_health_check_result(check_id, false));
    }

    #[test]
    fn test_running_container_count() {
        let mut orchestrator = ContainerOrchestrator::new();
        let image_id = orchestrator.register_image(500);
        let pod_id = orchestrator.create_pod();
        let container_id1 = orchestrator.create_container(pod_id, image_id, 2, 512);
        let container_id2 = orchestrator.create_container(pod_id, image_id, 2, 512);

        orchestrator.transition_container_state(container_id1, ContainerState::Starting);
        orchestrator.transition_container_state(container_id1, ContainerState::Running);
        orchestrator.transition_container_state(container_id2, ContainerState::Starting);
        orchestrator.transition_container_state(container_id2, ContainerState::Running);

        assert_eq!(orchestrator.get_running_container_count(), 2);
    }

    #[test]
    fn test_delete_container() {
        let mut orchestrator = ContainerOrchestrator::new();
        let image_id = orchestrator.register_image(500);
        let pod_id = orchestrator.create_pod();
        let container_id = orchestrator.create_container(pod_id, image_id, 2, 512);

        orchestrator.transition_container_state(container_id, ContainerState::Starting);
        orchestrator.transition_container_state(container_id, ContainerState::Stopped);
        assert!(orchestrator.delete_container(container_id));
        assert_eq!(orchestrator.get_active_container_count(), 0);
    }

    #[test]
    fn test_pod_container_limit() {
        let mut orchestrator = ContainerOrchestrator::new();
        let image_id = orchestrator.register_image(500);
        let pod_id = orchestrator.create_pod();

        for _ in 0..4 {
            orchestrator.create_container(pod_id, image_id, 2, 512);
        }

        assert_eq!(orchestrator.get_active_container_count(), 4);
    }
}
