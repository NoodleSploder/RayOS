// ===== RayOS Init System Module (Phase 9B Task 2) =====
// System services, service manager, process initialization
// PID 1 (init process) - System Services & Dependencies

// ===== Service System Constants =====

const MAX_SERVICES: usize = 32;
const MAX_SERVICE_NAME: usize = 32;
const MAX_SERVICE_DEPS: usize = 8;
const MAX_RUNLEVELS: usize = 6;  // 0=shutdown, 1=single-user, 2-5=multi-user, 6=reboot

// ===== Service Status Enumeration =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
    Unknown,
}

impl ServiceState {
    fn as_str(&self) -> &'static str {
        match self {
            ServiceState::Stopped => "stopped",
            ServiceState::Starting => "starting",
            ServiceState::Running => "running",
            ServiceState::Stopping => "stopping",
            ServiceState::Failed => "failed",
            ServiceState::Unknown => "unknown",
        }
    }
}

// ===== Runlevel Type =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Runlevel {
    Shutdown = 0,      // Halt/shutdown
    SingleUser = 1,    // Single-user mode
    MultiUser2 = 2,    // Multi-user, no NFS
    MultiUser3 = 3,    // Multi-user with networking
    MultiUser4 = 4,    // Multi-user, user-defined
    MultiUser5 = 5,    // Multi-user with X11
    Reboot = 6,        // Reboot
}

impl Runlevel {
    fn from_u32(val: u32) -> Option<Self> {
        match val {
            0 => Some(Runlevel::Shutdown),
            1 => Some(Runlevel::SingleUser),
            2 => Some(Runlevel::MultiUser2),
            3 => Some(Runlevel::MultiUser3),
            4 => Some(Runlevel::MultiUser4),
            5 => Some(Runlevel::MultiUser5),
            6 => Some(Runlevel::Reboot),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Runlevel::Shutdown => "shutdown/halt",
            Runlevel::SingleUser => "single-user",
            Runlevel::MultiUser2 => "multi-user (no NFS)",
            Runlevel::MultiUser3 => "multi-user (networking)",
            Runlevel::MultiUser4 => "multi-user (user-defined)",
            Runlevel::MultiUser5 => "multi-user (X11)",
            Runlevel::Reboot => "reboot",
        }
    }
}

// ===== Service Definition Structure =====

#[derive(Copy, Clone)]
pub struct Service {
    // Service metadata
    name: [u8; MAX_SERVICE_NAME],
    name_len: usize,

    // Service state
    state: ServiceState,
    pid: u32,

    // Service configuration
    runlevel: u32,                                    // Bitmask of runlevels (1 << runlevel)
    priority: i32,                                   // Start order priority
    dependencies: [u32; MAX_SERVICE_DEPS],           // Service IDs this depends on
    dependency_count: usize,

    // Service behavior
    auto_restart: bool,                              // Auto-restart on failure
    respawn_timeout: u32,                            // Seconds to wait before restart
    failure_count: u32,                              // Number of consecutive failures
}

impl Service {
    pub fn new() -> Self {
        Service {
            name: [0u8; MAX_SERVICE_NAME],
            name_len: 0,
            state: ServiceState::Stopped,
            pid: 0,
            runlevel: 0,
            priority: 0,
            dependencies: [0u32; MAX_SERVICE_DEPS],
            dependency_count: 0,
            auto_restart: false,
            respawn_timeout: 5,
            failure_count: 0,
        }
    }

    pub fn set_name(&mut self, name: &str) {
        let len = core::cmp::min(name.len(), MAX_SERVICE_NAME - 1);
        for i in 0..len {
            self.name[i] = name.as_bytes()[i];
        }
        self.name_len = len;
    }

    pub fn get_name(&self) -> &str {
        unsafe {
            core::str::from_utf8_unchecked(&self.name[..self.name_len])
        }
    }

    pub fn add_dependency(&mut self, dep_id: u32) -> bool {
        if self.dependency_count >= MAX_SERVICE_DEPS {
            return false;
        }
        self.dependencies[self.dependency_count] = dep_id;
        self.dependency_count += 1;
        true
    }

    pub fn set_runlevels(&mut self, mask: u32) {
        self.runlevel = mask;
    }
}

// ===== Service Manager =====

pub struct ServiceManager {
    services: [Service; MAX_SERVICES],
    service_count: usize,
    current_runlevel: Runlevel,
    next_runlevel: Option<Runlevel>,
}

impl ServiceManager {
    pub fn new() -> Self {
        ServiceManager {
            services: [Service::new(); MAX_SERVICES],
            service_count: 0,
            current_runlevel: Runlevel::MultiUser3,
            next_runlevel: None,
        }
    }

    pub fn register_service(&mut self, name: &str) -> Option<u32> {
        if self.service_count >= MAX_SERVICES {
            return None;
        }

        let id = self.service_count as u32;
        self.services[self.service_count].set_name(name);
        self.service_count += 1;
        Some(id)
    }

    pub fn get_service_mut(&mut self, id: u32) -> Option<&mut Service> {
        if id < self.service_count as u32 {
            Some(&mut self.services[id as usize])
        } else {
            None
        }
    }

    pub fn get_service(&self, id: u32) -> Option<&Service> {
        if id < self.service_count as u32 {
            Some(&self.services[id as usize])
        } else {
            None
        }
    }

    pub fn service_count(&self) -> usize {
        self.service_count
    }

    pub fn current_runlevel(&self) -> Runlevel {
        self.current_runlevel
    }

    pub fn set_runlevel(&mut self, new_level: Runlevel) {
        self.next_runlevel = Some(new_level);
    }

    pub fn get_service_name(&self, id: u32) -> Option<&str> {
        self.get_service(id).map(|s| s.get_name())
    }

    pub fn set_service_state(&mut self, id: u32, state: ServiceState) -> bool {
        if let Some(svc) = self.get_service_mut(id) {
            svc.state = state;
            true
        } else {
            false
        }
    }

    pub fn init_default_services(&mut self) {
        // Register core system services

        // Core services (priority 10-20)
        if let Some(id) = self.register_service("sysfs") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 10;
            svc.set_runlevels(0xFF);  // All runlevels
            svc.auto_restart = true;
        }

        if let Some(id) = self.register_service("devfs") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 15;
            svc.set_runlevels(0xFF);
            svc.auto_restart = true;
            svc.add_dependency(0);  // Depends on sysfs
        }

        if let Some(id) = self.register_service("proc") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 20;
            svc.set_runlevels(0xFF);
            svc.auto_restart = true;
        }

        // Storage services (priority 30-40)
        if let Some(id) = self.register_service("storage") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 30;
            svc.set_runlevels(0xFE);  // All except shutdown
            svc.auto_restart = true;
            svc.add_dependency(1);  // Depends on devfs
        }

        if let Some(id) = self.register_service("filesystems") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 35;
            svc.set_runlevels(0xFE);
            svc.auto_restart = true;
            svc.add_dependency(4);  // Depends on storage
        }

        // Network services (priority 50-60)
        if let Some(id) = self.register_service("networking") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 50;
            svc.set_runlevels(0x3C);  // Runlevels 2-5
            svc.auto_restart = true;
            svc.add_dependency(5);  // Depends on filesystems
        }

        if let Some(id) = self.register_service("dns") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 55;
            svc.set_runlevels(0x3C);
            svc.auto_restart = true;
            svc.add_dependency(6);  // Depends on networking
        }

        // System services (priority 70-80)
        if let Some(id) = self.register_service("logging") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 70;
            svc.set_runlevels(0xFE);
            svc.auto_restart = true;
        }

        if let Some(id) = self.register_service("cron") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 75;
            svc.set_runlevels(0x3C);
            svc.auto_restart = true;
            svc.add_dependency(6);
        }

        // User services (priority 100)
        if let Some(id) = self.register_service("user-session") {
            let svc = self.get_service_mut(id).unwrap();
            svc.priority = 100;
            svc.set_runlevels(0x20);  // Runlevel 5 only
            svc.auto_restart = false;
        }
    }

    pub fn boot_services(&mut self) {
        // Start services in priority order for current runlevel
        let runlevel_mask = 1 << (self.current_runlevel as u32);

        // Simple bubble sort by priority (small number of services)
        for i in 0..self.service_count {
            for j in i+1..self.service_count {
                if self.services[i].priority > self.services[j].priority {
                    let tmp = self.services[i];
                    self.services[i] = self.services[j];
                    self.services[j] = tmp;
                }
            }
        }

        // Start services that match current runlevel
        for i in 0..self.service_count {
            let svc = &self.services[i];
            if svc.runlevel & runlevel_mask != 0 {
                // Would call service startup handler here
                // For now, mark as started
            }
        }
    }

    pub fn stop_services(&mut self) {
        // Stop services in reverse priority order
        for i in (0..self.service_count).rev() {
            let svc = &self.services[i];
            if svc.state == ServiceState::Running {
                // Would call service stop handler here
            }
        }
    }

    pub fn check_service_health(&mut self) {
        // Periodically check service health and restart if needed
        for i in 0..self.service_count {
            let svc = &mut self.services[i];
            if svc.state == ServiceState::Failed && svc.auto_restart {
                // Increment failure counter
                svc.failure_count += 1;

                // Restart if failures not exceeded
                if svc.failure_count < 5 {
                    svc.state = ServiceState::Starting;
                    // Would actually start the service here
                }
            }
        }
    }

    pub fn validate_dependencies(&self, svc_id: u32) -> bool {
        if let Some(svc) = self.get_service(svc_id) {
            // Check all dependencies are running
            for dep_id in svc.dependencies[..svc.dependency_count].iter() {
                if let Some(dep) = self.get_service(*dep_id) {
                    if dep.state != ServiceState::Running {
                        return false;
                    }
                } else {
                    return false;  // Dependency doesn't exist
                }
            }
            true
        } else {
            false
        }
    }
}

// ===== Init Process Structure =====

pub struct InitProcess {
    pid: u32,
    state: ServiceState,
    service_manager: ServiceManager,
    boot_timestamp: u64,
}

impl InitProcess {
    pub fn new() -> Self {
        InitProcess {
            pid: 1,  // PID 1 is always init
            state: ServiceState::Starting,
            service_manager: ServiceManager::new(),
            boot_timestamp: 0,
        }
    }

    pub fn initialize(&mut self) {
        // Register default services
        self.service_manager.init_default_services();

        // Mark init as running
        self.state = ServiceState::Running;
    }

    pub fn boot(&mut self) {
        self.service_manager.boot_services();
    }

    pub fn shutdown(&mut self, final_runlevel: Runlevel) {
        self.service_manager.current_runlevel = final_runlevel;
        self.service_manager.stop_services();
        self.state = ServiceState::Stopping;
    }

    pub fn service_manager(&self) -> &ServiceManager {
        &self.service_manager
    }

    pub fn service_manager_mut(&mut self) -> &mut ServiceManager {
        &mut self.service_manager
    }

    pub fn get_pid(&self) -> u32 {
        self.pid
    }

    pub fn get_state(&self) -> ServiceState {
        self.state
    }
}

// ===== Service Display Helper =====

pub struct InitDisplay;

impl InitDisplay {
    pub fn show_services(_manager: &ServiceManager) -> &'static str {
        // This would be implemented with a write! macro to serial output
        // Placeholder for demonstration
        "Init System: Showing services"
    }

    pub fn show_runlevels() -> &'static str {
        "Init System: Showing runlevels"
    }

    pub fn show_status(_init: &InitProcess) -> &'static str {
        "Init System: Showing status"
    }
}

// ===== Global Init Instance =====

static mut INIT_PROCESS: Option<InitProcess> = None;

pub fn init_process() -> &'static mut InitProcess {
    unsafe {
        if INIT_PROCESS.is_none() {
            INIT_PROCESS = Some(InitProcess::new());
        }
        INIT_PROCESS.as_mut().unwrap()
    }
}

// ===== Init System Tests (used in shell::cmd_test) =====

pub fn test_init_system() -> bool {
    let mut manager = ServiceManager::new();
    manager.init_default_services();

    // Verify services registered
    if manager.service_count() != 9 {
        return false;
    }

    // Verify service retrieval
    if let Some(svc) = manager.get_service(0) {
        if svc.get_name() != "sysfs" {
            return false;
        }
    } else {
        return false;
    }

    // Verify dependency tracking
    if let Some(svc) = manager.get_service(1) {
        if svc.dependency_count != 1 {
            return false;
        }
        if svc.dependencies[0] != 0 {
            return false;
        }
    } else {
        return false;
    }

    // Verify runlevel operations
    if let Some(rl) = Runlevel::from_u32(3) {
        if rl != Runlevel::MultiUser3 {
            return false;
        }
    } else {
        return false;
    }

    true
}

pub fn test_init_process() -> bool {
    let mut init = InitProcess::new();
    init.initialize();

    // Verify init state
    if init.get_state() != ServiceState::Running {
        return false;
    }

    if init.get_pid() != 1 {
        return false;
    }

    // Verify service manager is initialized
    if init.service_manager().service_count() != 9 {
        return false;
    }

    true
}

pub fn test_service_states() -> bool {
    let mut manager = ServiceManager::new();
    manager.init_default_services();

    // Set service state
    if !manager.set_service_state(0, ServiceState::Running) {
        return false;
    }

    // Verify state
    if let Some(svc) = manager.get_service(0) {
        if svc.state != ServiceState::Running {
            return false;
        }
    } else {
        return false;
    }

    true
}
