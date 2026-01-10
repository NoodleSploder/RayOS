// ===== RayOS Service Lifecycle Module (Phase 9B Task 2) =====
// Advanced service management, lifecycle hooks, health monitoring
// Extends init.rs with comprehensive service infrastructure


// ===== Service Lifecycle Constants =====

const MAX_HOOK_NAME: usize = 32;
const MAX_ENV_VARS: usize = 16;
const MAX_ENV_VALUE: usize = 128;
const MAX_RESOURCE_LIMITS: usize = 8;
const MAX_CAPABILITIES: usize = 16;
const MAX_LOG_ENTRIES: usize = 64;
const MAX_HEALTH_CHECKS: usize = 8;

// ===== Service Type Classification =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ServiceType {
    /// Simple one-shot service (run once and exit)
    OneShot,
    /// Long-running daemon process
    Daemon,
    /// Forking daemon (parent exits, child continues)
    Forking,
    /// Notify-style daemon (signals ready via sd_notify)
    Notify,
    /// D-Bus activated service
    Dbus,
    /// Socket-activated service
    Socket,
    /// Idle service (runs when system is idle)
    Idle,
}

impl ServiceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceType::OneShot => "oneshot",
            ServiceType::Daemon => "daemon",
            ServiceType::Forking => "forking",
            ServiceType::Notify => "notify",
            ServiceType::Dbus => "dbus",
            ServiceType::Socket => "socket",
            ServiceType::Idle => "idle",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "oneshot" => Some(ServiceType::OneShot),
            "daemon" | "simple" => Some(ServiceType::Daemon),
            "forking" => Some(ServiceType::Forking),
            "notify" => Some(ServiceType::Notify),
            "dbus" => Some(ServiceType::Dbus),
            "socket" => Some(ServiceType::Socket),
            "idle" => Some(ServiceType::Idle),
            _ => None,
        }
    }
}

// ===== Service Restart Policy =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RestartPolicy {
    /// Never restart
    No,
    /// Always restart
    Always,
    /// Restart only on failure (non-zero exit)
    OnFailure,
    /// Restart only on abnormal exit (signal, timeout, watchdog)
    OnAbnormal,
    /// Restart on abort
    OnAbort,
    /// Restart on success (zero exit)
    OnSuccess,
    /// Restart on watchdog timeout
    OnWatchdog,
}

impl RestartPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            RestartPolicy::No => "no",
            RestartPolicy::Always => "always",
            RestartPolicy::OnFailure => "on-failure",
            RestartPolicy::OnAbnormal => "on-abnormal",
            RestartPolicy::OnAbort => "on-abort",
            RestartPolicy::OnSuccess => "on-success",
            RestartPolicy::OnWatchdog => "on-watchdog",
        }
    }

    pub fn should_restart(&self, exit_code: i32, was_signaled: bool, watchdog_timeout: bool) -> bool {
        match self {
            RestartPolicy::No => false,
            RestartPolicy::Always => true,
            RestartPolicy::OnFailure => exit_code != 0 || was_signaled,
            RestartPolicy::OnAbnormal => was_signaled || watchdog_timeout,
            RestartPolicy::OnAbort => was_signaled,
            RestartPolicy::OnSuccess => exit_code == 0 && !was_signaled,
            RestartPolicy::OnWatchdog => watchdog_timeout,
        }
    }
}

// ===== Resource Limit Types =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ResourceType {
    /// Maximum CPU time (seconds)
    CpuTime,
    /// Maximum file size (bytes)
    FileSize,
    /// Maximum data segment size (bytes)
    DataSize,
    /// Maximum stack size (bytes)
    StackSize,
    /// Maximum core file size (bytes)
    CoreSize,
    /// Maximum resident set size (bytes)
    Rss,
    /// Maximum number of processes
    NumProcs,
    /// Maximum number of open files
    NumFiles,
    /// Maximum locked memory (bytes)
    MemLock,
    /// Maximum address space (bytes)
    AddressSpace,
    /// Maximum file locks
    FileLocks,
    /// Maximum pending signals
    SigPending,
    /// Maximum message queue size (bytes)
    MsgQueue,
    /// Maximum nice priority
    Nice,
    /// Maximum real-time priority
    RtPrio,
    /// Maximum real-time timeout (microseconds)
    RtTime,
}

#[derive(Debug, Copy, Clone)]
pub struct ResourceLimit {
    pub resource: ResourceType,
    pub soft_limit: u64,
    pub hard_limit: u64,
}

impl ResourceLimit {
    pub fn new(resource: ResourceType, soft: u64, hard: u64) -> Self {
        ResourceLimit {
            resource,
            soft_limit: soft,
            hard_limit: hard,
        }
    }

    pub fn unlimited(resource: ResourceType) -> Self {
        ResourceLimit {
            resource,
            soft_limit: u64::MAX,
            hard_limit: u64::MAX,
        }
    }
}

// ===== Environment Variable =====

#[derive(Copy, Clone)]
pub struct EnvVar {
    name: [u8; MAX_HOOK_NAME],
    name_len: usize,
    value: [u8; MAX_ENV_VALUE],
    value_len: usize,
}

impl EnvVar {
    pub fn new() -> Self {
        EnvVar {
            name: [0u8; MAX_HOOK_NAME],
            name_len: 0,
            value: [0u8; MAX_ENV_VALUE],
            value_len: 0,
        }
    }

    pub fn set(&mut self, name: &str, value: &str) {
        let nlen = core::cmp::min(name.len(), MAX_HOOK_NAME - 1);
        for i in 0..nlen {
            self.name[i] = name.as_bytes()[i];
        }
        self.name_len = nlen;

        let vlen = core::cmp::min(value.len(), MAX_ENV_VALUE - 1);
        for i in 0..vlen {
            self.value[i] = value.as_bytes()[i];
        }
        self.value_len = vlen;
    }

    pub fn name(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.name[..self.name_len]) }
    }

    pub fn value(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.value[..self.value_len]) }
    }
}

// ===== Capability Set =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Capability {
    /// CAP_CHOWN - Make arbitrary changes to file UIDs and GIDs
    Chown,
    /// CAP_DAC_OVERRIDE - Bypass file read, write, and execute permission checks
    DacOverride,
    /// CAP_DAC_READ_SEARCH - Bypass file read permission checks
    DacReadSearch,
    /// CAP_FOWNER - Bypass permission checks on operations requiring matching file owner
    Fowner,
    /// CAP_FSETID - Don't clear set-user-ID and set-group-ID mode bits
    Fsetid,
    /// CAP_KILL - Bypass permission checks for sending signals
    Kill,
    /// CAP_SETGID - Make arbitrary manipulations of process GIDs
    Setgid,
    /// CAP_SETUID - Make arbitrary manipulations of process UIDs
    Setuid,
    /// CAP_SETPCAP - Modify process capabilities
    Setpcap,
    /// CAP_NET_BIND_SERVICE - Bind a socket to internet domain privileged ports
    NetBindService,
    /// CAP_NET_BROADCAST - Make socket broadcasts
    NetBroadcast,
    /// CAP_NET_ADMIN - Perform various network-related operations
    NetAdmin,
    /// CAP_NET_RAW - Use RAW and PACKET sockets
    NetRaw,
    /// CAP_SYS_ADMIN - Perform a range of system administration operations
    SysAdmin,
    /// CAP_SYS_BOOT - Use reboot and kexec_load
    SysBoot,
    /// CAP_SYS_PTRACE - Trace arbitrary processes using ptrace
    SysPtrace,
}

#[derive(Copy, Clone)]
pub struct CapabilitySet {
    caps: [bool; 32],  // Enough for all Linux capabilities
}

impl CapabilitySet {
    pub fn new() -> Self {
        CapabilitySet { caps: [false; 32] }
    }

    pub fn all() -> Self {
        CapabilitySet { caps: [true; 32] }
    }

    pub fn add(&mut self, cap: Capability) {
        self.caps[cap as usize] = true;
    }

    pub fn remove(&mut self, cap: Capability) {
        self.caps[cap as usize] = false;
    }

    pub fn has(&self, cap: Capability) -> bool {
        self.caps[cap as usize]
    }

    pub fn clear(&mut self) {
        self.caps = [false; 32];
    }
}

// ===== Health Check Definition =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HealthCheckType {
    /// Check if process is alive
    ProcessAlive,
    /// Check TCP port is listening
    TcpPort,
    /// Check Unix socket is available
    UnixSocket,
    /// Check file exists
    FileExists,
    /// Execute command and check exit code
    Command,
    /// Check HTTP endpoint
    HttpGet,
}

#[derive(Copy, Clone)]
pub struct HealthCheck {
    pub check_type: HealthCheckType,
    pub interval_secs: u32,
    pub timeout_secs: u32,
    pub retries: u32,
    pub target_port: u16,
    target_path: [u8; 64],
    target_path_len: usize,
    pub enabled: bool,
}

impl HealthCheck {
    pub fn new(check_type: HealthCheckType) -> Self {
        HealthCheck {
            check_type,
            interval_secs: 30,
            timeout_secs: 5,
            retries: 3,
            target_port: 0,
            target_path: [0u8; 64],
            target_path_len: 0,
            enabled: true,
        }
    }

    pub fn set_path(&mut self, path: &str) {
        let len = core::cmp::min(path.len(), 63);
        for i in 0..len {
            self.target_path[i] = path.as_bytes()[i];
        }
        self.target_path_len = len;
    }

    pub fn path(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.target_path[..self.target_path_len]) }
    }

    pub fn tcp_port(port: u16) -> Self {
        let mut check = HealthCheck::new(HealthCheckType::TcpPort);
        check.target_port = port;
        check
    }

    pub fn http_get(port: u16, path: &str) -> Self {
        let mut check = HealthCheck::new(HealthCheckType::HttpGet);
        check.target_port = port;
        check.set_path(path);
        check
    }
}

// ===== Service Log Entry =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LogLevel {
    Emergency,
    Alert,
    Critical,
    Error,
    Warning,
    Notice,
    Info,
    Debug,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Emergency => "EMERG",
            LogLevel::Alert => "ALERT",
            LogLevel::Critical => "CRIT",
            LogLevel::Error => "ERROR",
            LogLevel::Warning => "WARN",
            LogLevel::Notice => "NOTE",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
        }
    }

    pub fn from_priority(pri: u32) -> Self {
        match pri {
            0 => LogLevel::Emergency,
            1 => LogLevel::Alert,
            2 => LogLevel::Critical,
            3 => LogLevel::Error,
            4 => LogLevel::Warning,
            5 => LogLevel::Notice,
            6 => LogLevel::Info,
            _ => LogLevel::Debug,
        }
    }
}

#[derive(Copy, Clone)]
pub struct LogEntry {
    pub timestamp: u64,
    pub level: LogLevel,
    message: [u8; 128],
    message_len: usize,
    pub service_id: u32,
}

impl LogEntry {
    pub fn new(service_id: u32, level: LogLevel, msg: &str) -> Self {
        let mut entry = LogEntry {
            timestamp: 0,  // Would be set by system time
            level,
            message: [0u8; 128],
            message_len: 0,
            service_id,
        };
        let len = core::cmp::min(msg.len(), 127);
        for i in 0..len {
            entry.message[i] = msg.as_bytes()[i];
        }
        entry.message_len = len;
        entry
    }

    pub fn message(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.message[..self.message_len]) }
    }
}

// ===== Service Log Buffer =====

pub struct ServiceLog {
    entries: [LogEntry; MAX_LOG_ENTRIES],
    count: usize,
    write_index: usize,
}

impl ServiceLog {
    pub fn new() -> Self {
        ServiceLog {
            entries: [LogEntry::new(0, LogLevel::Info, ""); MAX_LOG_ENTRIES],
            count: 0,
            write_index: 0,
        }
    }

    pub fn append(&mut self, entry: LogEntry) {
        self.entries[self.write_index] = entry;
        self.write_index = (self.write_index + 1) % MAX_LOG_ENTRIES;
        if self.count < MAX_LOG_ENTRIES {
            self.count += 1;
        }
    }

    pub fn latest(&self, n: usize) -> impl Iterator<Item = &LogEntry> {
        let _start = if self.count >= n {
            (self.write_index + MAX_LOG_ENTRIES - n) % MAX_LOG_ENTRIES
        } else {
            0
        };
        let count = core::cmp::min(n, self.count);
        self.entries[..count].iter()
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn clear(&mut self) {
        self.count = 0;
        self.write_index = 0;
    }
}

// ===== Service Lifecycle Hooks =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LifecyclePhase {
    /// Before service starts
    PreStart,
    /// Service is starting
    Start,
    /// After service has started
    PostStart,
    /// Service reload requested
    Reload,
    /// Before service stops
    PreStop,
    /// Service is stopping
    Stop,
    /// After service has stopped
    PostStop,
}

#[derive(Copy, Clone)]
pub struct LifecycleHook {
    pub phase: LifecyclePhase,
    command: [u8; 128],
    command_len: usize,
    pub timeout_secs: u32,
    pub enabled: bool,
}

impl LifecycleHook {
    pub fn new(phase: LifecyclePhase) -> Self {
        LifecycleHook {
            phase,
            command: [0u8; 128],
            command_len: 0,
            timeout_secs: 30,
            enabled: false,
        }
    }

    pub fn set_command(&mut self, cmd: &str) {
        let len = core::cmp::min(cmd.len(), 127);
        for i in 0..len {
            self.command[i] = cmd.as_bytes()[i];
        }
        self.command_len = len;
        self.enabled = true;
    }

    pub fn command(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.command[..self.command_len]) }
    }
}

// ===== Extended Service Definition =====

#[derive(Copy, Clone)]
pub struct ServiceDefinition {
    // Identity
    pub service_id: u32,
    name: [u8; 32],
    name_len: usize,
    description: [u8; 128],
    description_len: usize,

    // Type and behavior
    pub service_type: ServiceType,
    pub restart_policy: RestartPolicy,
    pub restart_delay_secs: u32,
    pub start_limit_interval: u32,
    pub start_limit_burst: u32,

    // Execution context
    pub user_id: u32,
    pub group_id: u32,
    working_dir: [u8; 64],
    working_dir_len: usize,

    // Environment
    env_vars: [EnvVar; MAX_ENV_VARS],
    env_count: usize,

    // Resource limits
    limits: [ResourceLimit; MAX_RESOURCE_LIMITS],
    limit_count: usize,

    // Security
    pub capabilities: CapabilitySet,
    pub no_new_privileges: bool,
    pub protect_system: bool,
    pub protect_home: bool,
    pub private_tmp: bool,
    pub private_devices: bool,

    // Health checks
    health_checks: [HealthCheck; MAX_HEALTH_CHECKS],
    health_check_count: usize,

    // Lifecycle hooks
    pub pre_start: LifecycleHook,
    pub post_start: LifecycleHook,
    pub pre_stop: LifecycleHook,
    pub post_stop: LifecycleHook,

    // Watchdog
    pub watchdog_secs: u32,
    pub watchdog_signal: i32,

    // Cgroup settings
    pub memory_max: u64,
    pub cpu_quota: u32,  // Percentage * 100
    pub io_weight: u32,
}

impl ServiceDefinition {
    pub fn new(id: u32) -> Self {
        ServiceDefinition {
            service_id: id,
            name: [0u8; 32],
            name_len: 0,
            description: [0u8; 128],
            description_len: 0,
            service_type: ServiceType::Daemon,
            restart_policy: RestartPolicy::OnFailure,
            restart_delay_secs: 5,
            start_limit_interval: 60,
            start_limit_burst: 5,
            user_id: 0,
            group_id: 0,
            working_dir: [0u8; 64],
            working_dir_len: 0,
            env_vars: [EnvVar::new(); MAX_ENV_VARS],
            env_count: 0,
            limits: [ResourceLimit::unlimited(ResourceType::CpuTime); MAX_RESOURCE_LIMITS],
            limit_count: 0,
            capabilities: CapabilitySet::new(),
            no_new_privileges: true,
            protect_system: true,
            protect_home: true,
            private_tmp: true,
            private_devices: true,
            health_checks: [HealthCheck::new(HealthCheckType::ProcessAlive); MAX_HEALTH_CHECKS],
            health_check_count: 0,
            pre_start: LifecycleHook::new(LifecyclePhase::PreStart),
            post_start: LifecycleHook::new(LifecyclePhase::PostStart),
            pre_stop: LifecycleHook::new(LifecyclePhase::PreStop),
            post_stop: LifecycleHook::new(LifecyclePhase::PostStop),
            watchdog_secs: 0,
            watchdog_signal: 9,  // SIGKILL
            memory_max: 0,
            cpu_quota: 0,
            io_weight: 100,
        }
    }

    pub fn set_name(&mut self, name: &str) {
        let len = core::cmp::min(name.len(), 31);
        for i in 0..len {
            self.name[i] = name.as_bytes()[i];
        }
        self.name_len = len;
    }

    pub fn name(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.name[..self.name_len]) }
    }

    pub fn set_description(&mut self, desc: &str) {
        let len = core::cmp::min(desc.len(), 127);
        for i in 0..len {
            self.description[i] = desc.as_bytes()[i];
        }
        self.description_len = len;
    }

    pub fn description(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.description[..self.description_len]) }
    }

    pub fn set_working_dir(&mut self, dir: &str) {
        let len = core::cmp::min(dir.len(), 63);
        for i in 0..len {
            self.working_dir[i] = dir.as_bytes()[i];
        }
        self.working_dir_len = len;
    }

    pub fn working_dir(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.working_dir[..self.working_dir_len]) }
    }

    pub fn add_env(&mut self, name: &str, value: &str) -> bool {
        if self.env_count >= MAX_ENV_VARS {
            return false;
        }
        self.env_vars[self.env_count].set(name, value);
        self.env_count += 1;
        true
    }

    pub fn add_limit(&mut self, limit: ResourceLimit) -> bool {
        if self.limit_count >= MAX_RESOURCE_LIMITS {
            return false;
        }
        self.limits[self.limit_count] = limit;
        self.limit_count += 1;
        true
    }

    pub fn add_health_check(&mut self, check: HealthCheck) -> bool {
        if self.health_check_count >= MAX_HEALTH_CHECKS {
            return false;
        }
        self.health_checks[self.health_check_count] = check;
        self.health_check_count += 1;
        true
    }
}

// ===== Service Runtime State =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuntimeState {
    /// Service is not loaded
    Inactive,
    /// Service is activating
    Activating,
    /// Service is active and running
    Active,
    /// Service is reloading
    Reloading,
    /// Service is deactivating
    Deactivating,
    /// Service failed
    Failed,
    /// Service is in maintenance mode
    Maintenance,
}

#[derive(Copy, Clone)]
pub struct ServiceRuntime {
    pub service_id: u32,
    pub state: RuntimeState,
    pub main_pid: u32,
    pub control_pid: u32,
    pub start_timestamp: u64,
    pub exit_timestamp: u64,
    pub exit_code: i32,
    pub exit_signal: i32,
    pub restart_count: u32,
    pub failure_count: u32,
    pub last_health_check: u64,
    pub health_check_failures: u32,
    pub memory_current: u64,
    pub cpu_usage: u32,  // Percentage * 100
}

impl ServiceRuntime {
    pub fn new(id: u32) -> Self {
        ServiceRuntime {
            service_id: id,
            state: RuntimeState::Inactive,
            main_pid: 0,
            control_pid: 0,
            start_timestamp: 0,
            exit_timestamp: 0,
            exit_code: 0,
            exit_signal: 0,
            restart_count: 0,
            failure_count: 0,
            last_health_check: 0,
            health_check_failures: 0,
            memory_current: 0,
            cpu_usage: 0,
        }
    }

    pub fn uptime(&self, current_time: u64) -> u64 {
        if self.state == RuntimeState::Active && self.start_timestamp > 0 {
            current_time.saturating_sub(self.start_timestamp)
        } else {
            0
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.state == RuntimeState::Active && self.health_check_failures == 0
    }
}

// ===== Dependency Graph =====

const MAX_DEPS: usize = 8;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DependencyType {
    /// Required for this service to start
    Requires,
    /// Wanted but not required
    Wants,
    /// Conflicts with this service
    Conflicts,
    /// Start after this service
    After,
    /// Start before this service
    Before,
    /// Bind lifecycle to this service
    BindsTo,
    /// Part of this target/group
    PartOf,
}

#[derive(Copy, Clone)]
pub struct Dependency {
    pub target_id: u32,
    pub dep_type: DependencyType,
}

#[derive(Copy, Clone)]
pub struct DependencyGraph {
    deps: [Dependency; MAX_DEPS],
    count: usize,
}

impl DependencyGraph {
    pub fn new() -> Self {
        DependencyGraph {
            deps: [Dependency { target_id: 0, dep_type: DependencyType::Wants }; MAX_DEPS],
            count: 0,
        }
    }

    pub fn add(&mut self, target_id: u32, dep_type: DependencyType) -> bool {
        if self.count >= MAX_DEPS {
            return false;
        }
        self.deps[self.count] = Dependency { target_id, dep_type };
        self.count += 1;
        true
    }

    pub fn get_requires(&self) -> impl Iterator<Item = u32> + '_ {
        self.deps[..self.count]
            .iter()
            .filter(|d| d.dep_type == DependencyType::Requires)
            .map(|d| d.target_id)
    }

    pub fn get_after(&self) -> impl Iterator<Item = u32> + '_ {
        self.deps[..self.count]
            .iter()
            .filter(|d| d.dep_type == DependencyType::After)
            .map(|d| d.target_id)
    }

    pub fn conflicts_with(&self, service_id: u32) -> bool {
        self.deps[..self.count]
            .iter()
            .any(|d| d.dep_type == DependencyType::Conflicts && d.target_id == service_id)
    }
}

// ===== Service Unit (Complete Service with Definition + Runtime) =====

#[derive(Copy, Clone)]
pub struct ServiceUnit {
    pub definition: ServiceDefinition,
    pub runtime: ServiceRuntime,
    pub dependencies: DependencyGraph,
    pub enabled: bool,
    pub masked: bool,
}

impl ServiceUnit {
    pub fn new(id: u32) -> Self {
        ServiceUnit {
            definition: ServiceDefinition::new(id),
            runtime: ServiceRuntime::new(id),
            dependencies: DependencyGraph::new(),
            enabled: false,
            masked: false,
        }
    }

    pub fn can_start(&self, running_services: &[u32]) -> bool {
        if self.masked {
            return false;
        }

        // Check all required dependencies are running
        for req_id in self.dependencies.get_requires() {
            if !running_services.contains(&req_id) {
                return false;
            }
        }

        // Check no conflicts
        for &svc_id in running_services {
            if self.dependencies.conflicts_with(svc_id) {
                return false;
            }
        }

        true
    }
}

// ===== Socket Activation =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SocketType {
    Stream,   // TCP
    Datagram, // UDP
    SeqPacket,
    Raw,
}

#[derive(Copy, Clone)]
pub struct SocketDefinition {
    pub socket_type: SocketType,
    pub listen_port: u16,
    listen_path: [u8; 64],
    listen_path_len: usize,
    pub backlog: u32,
    pub accept: bool,
    pub pass_credentials: bool,
    pub service_id: u32,  // Service to activate
}

impl SocketDefinition {
    pub fn tcp(port: u16, service_id: u32) -> Self {
        SocketDefinition {
            socket_type: SocketType::Stream,
            listen_port: port,
            listen_path: [0u8; 64],
            listen_path_len: 0,
            backlog: 128,
            accept: true,
            pass_credentials: false,
            service_id,
        }
    }

    pub fn unix(path: &str, service_id: u32) -> Self {
        let mut sock = SocketDefinition {
            socket_type: SocketType::Stream,
            listen_port: 0,
            listen_path: [0u8; 64],
            listen_path_len: 0,
            backlog: 128,
            accept: true,
            pass_credentials: true,
            service_id,
        };
        let len = core::cmp::min(path.len(), 63);
        for i in 0..len {
            sock.listen_path[i] = path.as_bytes()[i];
        }
        sock.listen_path_len = len;
        sock
    }

    pub fn path(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.listen_path[..self.listen_path_len]) }
    }
}

// ===== Timer Activation =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TimerType {
    /// Real-time (wall clock)
    Realtime,
    /// Monotonic (boot time)
    Monotonic,
    /// On specific calendar date
    Calendar,
}

#[derive(Copy, Clone)]
pub struct TimerDefinition {
    pub timer_type: TimerType,
    pub on_boot_sec: u64,
    pub on_unit_active_sec: u64,
    pub on_unit_inactive_sec: u64,
    pub accuracy_sec: u64,
    pub randomized_delay_sec: u64,
    pub persistent: bool,
    pub wake_system: bool,
    pub service_id: u32,
}

impl TimerDefinition {
    pub fn periodic(interval_secs: u64, service_id: u32) -> Self {
        TimerDefinition {
            timer_type: TimerType::Monotonic,
            on_boot_sec: 0,
            on_unit_active_sec: interval_secs,
            on_unit_inactive_sec: 0,
            accuracy_sec: 1,
            randomized_delay_sec: 0,
            persistent: false,
            wake_system: false,
            service_id,
        }
    }

    pub fn on_boot(delay_secs: u64, service_id: u32) -> Self {
        TimerDefinition {
            timer_type: TimerType::Monotonic,
            on_boot_sec: delay_secs,
            on_unit_active_sec: 0,
            on_unit_inactive_sec: 0,
            accuracy_sec: 1,
            randomized_delay_sec: 0,
            persistent: false,
            wake_system: false,
            service_id,
        }
    }
}

// ===== Service Lifecycle Manager =====

const MAX_UNITS: usize = 64;
const MAX_SOCKETS: usize = 16;
const MAX_TIMERS: usize = 16;

pub struct ServiceLifecycleManager {
    units: [ServiceUnit; MAX_UNITS],
    unit_count: usize,
    sockets: [SocketDefinition; MAX_SOCKETS],
    socket_count: usize,
    timers: [TimerDefinition; MAX_TIMERS],
    timer_count: usize,
    log: ServiceLog,
    current_time: u64,
}

impl ServiceLifecycleManager {
    pub fn new() -> Self {
        ServiceLifecycleManager {
            units: [ServiceUnit::new(0); MAX_UNITS],
            unit_count: 0,
            sockets: [SocketDefinition::tcp(0, 0); MAX_SOCKETS],
            socket_count: 0,
            timers: [TimerDefinition::periodic(0, 0); MAX_TIMERS],
            timer_count: 0,
            log: ServiceLog::new(),
            current_time: 0,
        }
    }

    pub fn register_unit(&mut self, name: &str) -> Option<u32> {
        if self.unit_count >= MAX_UNITS {
            return None;
        }
        let id = self.unit_count as u32;
        self.units[self.unit_count] = ServiceUnit::new(id);
        self.units[self.unit_count].definition.set_name(name);
        self.unit_count += 1;
        self.log.append(LogEntry::new(id, LogLevel::Info, "Unit registered"));
        Some(id)
    }

    pub fn get_unit(&self, id: u32) -> Option<&ServiceUnit> {
        if (id as usize) < self.unit_count {
            Some(&self.units[id as usize])
        } else {
            None
        }
    }

    pub fn get_unit_mut(&mut self, id: u32) -> Option<&mut ServiceUnit> {
        if (id as usize) < self.unit_count {
            Some(&mut self.units[id as usize])
        } else {
            None
        }
    }

    pub fn start_unit(&mut self, id: u32) -> Result<(), &'static str> {
        // Collect running service IDs into a fixed-size array
        let mut running = [0u32; MAX_UNITS];
        let mut running_count = 0usize;
        for i in 0..self.unit_count {
            if self.units[i].runtime.state == RuntimeState::Active {
                running[running_count] = self.units[i].definition.service_id;
                running_count += 1;
            }
        }
        let running_slice = &running[..running_count];

        // Check bounds and get initial state without holding a mutable borrow
        let idx = id as usize;
        if idx >= self.unit_count {
            return Err("Unit not found");
        }

        // Check if masked
        if self.units[idx].masked {
            return Err("Unit is masked");
        }

        // Check dependencies
        if !self.units[idx].can_start(running_slice) {
            return Err("Dependencies not met");
        }

        // Run pre-start hook
        if self.units[idx].definition.pre_start.enabled {
            // Would execute pre_start.command()
        }

        self.units[idx].runtime.state = RuntimeState::Activating;
        self.log.append(LogEntry::new(id, LogLevel::Info, "Starting"));

        // Would fork/exec the service here
        // For now, simulate successful start
        self.units[idx].runtime.state = RuntimeState::Active;
        self.units[idx].runtime.main_pid = 1000 + id;  // Simulated PID
        self.units[idx].runtime.start_timestamp = self.current_time;

        // Run post-start hook
        if self.units[idx].definition.post_start.enabled {
            // Would execute post_start.command()
        }

        self.log.append(LogEntry::new(id, LogLevel::Info, "Started"));
        Ok(())
    }

    pub fn stop_unit(&mut self, id: u32) -> Result<(), &'static str> {
        let idx = id as usize;
        if idx >= self.unit_count {
            return Err("Unit not found");
        }

        if self.units[idx].runtime.state != RuntimeState::Active {
            return Err("Unit not running");
        }

        // Run pre-stop hook
        if self.units[idx].definition.pre_stop.enabled {
            // Would execute pre_stop.command()
        }

        self.units[idx].runtime.state = RuntimeState::Deactivating;
        self.log.append(LogEntry::new(id, LogLevel::Info, "Stopping"));

        // Would send signal and wait for process to exit
        // For now, simulate successful stop
        self.units[idx].runtime.state = RuntimeState::Inactive;
        self.units[idx].runtime.exit_timestamp = self.current_time;
        self.units[idx].runtime.main_pid = 0;

        // Run post-stop hook
        if self.units[idx].definition.post_stop.enabled {
            // Would execute post_stop.command()
        }

        self.log.append(LogEntry::new(id, LogLevel::Info, "Stopped"));
        Ok(())
    }

    pub fn restart_unit(&mut self, id: u32) -> Result<(), &'static str> {
        let _ = self.stop_unit(id);
        self.start_unit(id)
    }

    pub fn reload_unit(&mut self, id: u32) -> Result<(), &'static str> {
        let idx = id as usize;
        if idx >= self.unit_count {
            return Err("Unit not found");
        }

        if self.units[idx].runtime.state != RuntimeState::Active {
            return Err("Unit not running");
        }

        self.units[idx].runtime.state = RuntimeState::Reloading;
        self.log.append(LogEntry::new(id, LogLevel::Info, "Reloading"));

        // Would send SIGHUP or execute reload command
        self.units[idx].runtime.state = RuntimeState::Active;

        self.log.append(LogEntry::new(id, LogLevel::Info, "Reloaded"));
        Ok(())
    }

    pub fn enable_unit(&mut self, id: u32) -> bool {
        let idx = id as usize;
        if idx >= self.unit_count {
            return false;
        }
        self.units[idx].enabled = true;
        self.log.append(LogEntry::new(id, LogLevel::Info, "Enabled"));
        true
    }

    pub fn disable_unit(&mut self, id: u32) -> bool {
        let idx = id as usize;
        if idx >= self.unit_count {
            return false;
        }
        self.units[idx].enabled = false;
        self.log.append(LogEntry::new(id, LogLevel::Info, "Disabled"));
        true
    }

    pub fn mask_unit(&mut self, id: u32) -> bool {
        let idx = id as usize;
        if idx >= self.unit_count {
            return false;
        }
        self.units[idx].masked = true;
        self.log.append(LogEntry::new(id, LogLevel::Info, "Masked"));
        true
    }

    pub fn unmask_unit(&mut self, id: u32) -> bool {
        let idx = id as usize;
        if idx >= self.unit_count {
            return false;
        }
        self.units[idx].masked = false;
        self.log.append(LogEntry::new(id, LogLevel::Info, "Unmasked"));
        true
    }

    pub fn register_socket(&mut self, socket: SocketDefinition) -> bool {
        if self.socket_count >= MAX_SOCKETS {
            return false;
        }
        self.sockets[self.socket_count] = socket;
        self.socket_count += 1;
        true
    }

    pub fn register_timer(&mut self, timer: TimerDefinition) -> bool {
        if self.timer_count >= MAX_TIMERS {
            return false;
        }
        self.timers[self.timer_count] = timer;
        self.timer_count += 1;
        true
    }

    pub fn check_health(&mut self) {
        for i in 0..self.unit_count {
            let unit = &mut self.units[i];
            if unit.runtime.state != RuntimeState::Active {
                continue;
            }

            // Check health checks
            for j in 0..unit.definition.health_check_count {
                let check = &unit.definition.health_checks[j];
                if !check.enabled {
                    continue;
                }

                // Would actually perform the health check
                // For now, assume healthy
                unit.runtime.last_health_check = self.current_time;
            }

            // Check watchdog
            if unit.definition.watchdog_secs > 0 {
                let watchdog_deadline = unit.runtime.start_timestamp +
                    unit.definition.watchdog_secs as u64;
                if self.current_time > watchdog_deadline {
                    // Watchdog timeout
                    unit.runtime.state = RuntimeState::Failed;
                    self.log.append(LogEntry::new(
                        unit.definition.service_id,
                        LogLevel::Error,
                        "Watchdog timeout"
                    ));
                }
            }
        }
    }

    pub fn process_restarts(&mut self) {
        for i in 0..self.unit_count {
            let unit = &mut self.units[i];
            if unit.runtime.state != RuntimeState::Failed {
                continue;
            }

            let should_restart = unit.definition.restart_policy.should_restart(
                unit.runtime.exit_code,
                unit.runtime.exit_signal != 0,
                false
            );

            if should_restart && unit.runtime.restart_count < unit.definition.start_limit_burst {
                unit.runtime.restart_count += 1;
                // Would wait restart_delay_secs then restart
                // For now, mark as activating
                unit.runtime.state = RuntimeState::Activating;
                self.log.append(LogEntry::new(
                    unit.definition.service_id,
                    LogLevel::Info,
                    "Auto-restart scheduled"
                ));
            }
        }
    }

    pub fn tick(&mut self, current_time: u64) {
        self.current_time = current_time;

        // Check timers
        for i in 0..self.timer_count {
            let timer = &self.timers[i];
            if timer.on_boot_sec > 0 && current_time >= timer.on_boot_sec {
                // Would activate service
            }
        }

        self.check_health();
        self.process_restarts();
    }

    pub fn unit_count(&self) -> usize {
        self.unit_count
    }

    pub fn running_count(&self) -> usize {
        self.units[..self.unit_count]
            .iter()
            .filter(|u| u.runtime.state == RuntimeState::Active)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.units[..self.unit_count]
            .iter()
            .filter(|u| u.runtime.state == RuntimeState::Failed)
            .count()
    }
}

// ===== Global Lifecycle Manager =====

static mut LIFECYCLE_MANAGER: Option<ServiceLifecycleManager> = None;

pub fn lifecycle_manager() -> &'static mut ServiceLifecycleManager {
    unsafe {
        if LIFECYCLE_MANAGER.is_none() {
            LIFECYCLE_MANAGER = Some(ServiceLifecycleManager::new());
        }
        LIFECYCLE_MANAGER.as_mut().unwrap()
    }
}

// ===== Tests =====

pub fn test_service_lifecycle() -> bool {
    let mut mgr = ServiceLifecycleManager::new();

    // Register services
    let svc1 = mgr.register_unit("test-service-1");
    let svc2 = mgr.register_unit("test-service-2");

    if svc1.is_none() || svc2.is_none() {
        return false;
    }

    let id1 = svc1.unwrap();
    let id2 = svc2.unwrap();

    // Configure service 2 to depend on service 1
    if let Some(unit) = mgr.get_unit_mut(id2) {
        unit.dependencies.add(id1, DependencyType::Requires);
        unit.dependencies.add(id1, DependencyType::After);
    }

    // Try to start service 2 before service 1 - should fail
    if mgr.start_unit(id2).is_ok() {
        return false;  // Should have failed due to dependencies
    }

    // Start service 1
    if mgr.start_unit(id1).is_err() {
        return false;
    }

    // Now start service 2
    if mgr.start_unit(id2).is_err() {
        return false;
    }

    // Verify both are running
    if mgr.running_count() != 2 {
        return false;
    }

    // Stop and verify
    let _ = mgr.stop_unit(id2);
    let _ = mgr.stop_unit(id1);

    if mgr.running_count() != 0 {
        return false;
    }

    true
}

pub fn test_restart_policy() -> bool {
    // Test OnFailure policy
    let policy = RestartPolicy::OnFailure;
    if !policy.should_restart(1, false, false) {
        return false;  // Non-zero exit should trigger restart
    }
    if policy.should_restart(0, false, false) {
        return false;  // Zero exit should not trigger restart
    }

    // Test Always policy
    let policy = RestartPolicy::Always;
    if !policy.should_restart(0, false, false) {
        return false;
    }
    if !policy.should_restart(1, true, true) {
        return false;
    }

    // Test No policy
    let policy = RestartPolicy::No;
    if policy.should_restart(1, true, true) {
        return false;
    }

    true
}

pub fn test_capability_set() -> bool {
    let mut caps = CapabilitySet::new();

    caps.add(Capability::NetBindService);
    caps.add(Capability::SysAdmin);

    if !caps.has(Capability::NetBindService) {
        return false;
    }
    if !caps.has(Capability::SysAdmin) {
        return false;
    }
    if caps.has(Capability::Kill) {
        return false;
    }

    caps.remove(Capability::SysAdmin);
    if caps.has(Capability::SysAdmin) {
        return false;
    }

    true
}
