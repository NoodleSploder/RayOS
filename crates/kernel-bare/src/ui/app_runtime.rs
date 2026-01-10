//! RayApp Runtime for RayOS UI
//!
//! App lifecycle management, registry, and launcher with sandboxing hooks.
//! Provides cooperative scheduling, IPC, and capability enforcement.
//!
//! # Overview
//!
//! The App Runtime is responsible for:
//! - Managing app lifecycle (launch, suspend, resume, terminate)
//! - Registering and tracking running apps
//! - Allocating frame budgets for cooperative scheduling
//! - Enforcing capability-based sandboxing
//! - Inter-app communication via message channels
//!
//! # Markers
//!
//! - `RAYOS_APP:LAUNCHED` - App successfully launched
//! - `RAYOS_APP:RUNNING` - App is actively running
//! - `RAYOS_APP:SUSPENDED` - App suspended (backgrounded)
//! - `RAYOS_APP:TERMINATED` - App terminated
//! - `RAYOS_APP:IPC` - Inter-app message sent/received

use super::window_manager::WindowId;
use super::surface_manager::SurfaceId;

// ============================================================================
// Constants
// ============================================================================

/// Maximum running apps.
pub const MAX_APPS: usize = 8;

/// Maximum queued IPC messages per app.
pub const MAX_IPC_QUEUE: usize = 64;

/// Maximum capabilities per app.
pub const MAX_CAPABILITIES: usize = 16;

/// Maximum lifecycle hooks per app.
pub const MAX_LIFECYCLE_HOOKS: usize = 8;

/// Frame budget in microseconds (16.6ms = 60fps).
pub const DEFAULT_FRAME_BUDGET_US: u64 = 16_666;

/// Invalid app ID.
pub const APP_ID_NONE: AppId = 0;

// ============================================================================
// App ID
// ============================================================================

/// Unique app instance identifier.
pub type AppId = u32;

/// Counter for generating unique app IDs.
static mut NEXT_APP_ID: AppId = 1;

/// Generate a new unique app ID.
fn next_app_id() -> AppId {
    // SAFETY: Single-threaded kernel context
    unsafe {
        let id = NEXT_APP_ID;
        NEXT_APP_ID = NEXT_APP_ID.wrapping_add(1);
        if NEXT_APP_ID == 0 {
            NEXT_APP_ID = 1;
        }
        id
    }
}

// ============================================================================
// App State
// ============================================================================

/// App lifecycle state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum AppState {
    /// Not initialized.
    None = 0,
    /// Loading resources.
    Loading = 1,
    /// Actively running.
    Running = 2,
    /// Suspended in background.
    Suspended = 3,
    /// Terminating.
    Terminating = 4,
    /// Terminated (resources freed).
    Terminated = 5,
    /// Error state.
    Error = 6,
}

impl Default for AppState {
    fn default() -> Self {
        AppState::None
    }
}

impl AppState {
    /// Check if app can receive input.
    pub fn can_receive_input(&self) -> bool {
        matches!(self, AppState::Running)
    }

    /// Check if app is active (loading or running).
    pub fn is_active(&self) -> bool {
        matches!(self, AppState::Loading | AppState::Running)
    }

    /// Check if app is finished.
    pub fn is_finished(&self) -> bool {
        matches!(self, AppState::Terminated | AppState::Error)
    }
}

// ============================================================================
// App Capabilities
// ============================================================================

/// App capability flags.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Capability {
    /// No capability.
    None = 0,
    /// File system read access.
    FileRead = 1,
    /// File system write access.
    FileWrite = 2,
    /// Network access.
    Network = 3,
    /// Clipboard read.
    ClipboardRead = 4,
    /// Clipboard write.
    ClipboardWrite = 5,
    /// Audio playback.
    AudioPlay = 6,
    /// Audio recording.
    AudioRecord = 7,
    /// Camera access.
    Camera = 8,
    /// Notifications.
    Notifications = 9,
    /// Background execution.
    Background = 10,
    /// System settings access.
    Settings = 11,
    /// Inter-app communication.
    Ipc = 12,
    /// GPU acceleration.
    Gpu = 13,
    /// USB access.
    Usb = 14,
    /// Bluetooth access.
    Bluetooth = 15,
}

/// Capability set for an app.
#[derive(Clone, Copy, Default)]
pub struct CapabilitySet {
    /// Bitmask of granted capabilities.
    mask: u32,
}

impl CapabilitySet {
    /// Create an empty capability set.
    pub const fn empty() -> Self {
        Self { mask: 0 }
    }

    /// Create a capability set with all capabilities.
    pub const fn all() -> Self {
        Self { mask: 0xFFFF }
    }

    /// Grant a capability.
    pub fn grant(&mut self, cap: Capability) {
        self.mask |= 1 << (cap as u8);
    }

    /// Revoke a capability.
    pub fn revoke(&mut self, cap: Capability) {
        self.mask &= !(1 << (cap as u8));
    }

    /// Check if a capability is granted.
    pub fn has(&self, cap: Capability) -> bool {
        (self.mask & (1 << (cap as u8))) != 0
    }

    /// Get raw mask.
    pub fn mask(&self) -> u32 {
        self.mask
    }

    /// Create from raw mask.
    pub fn from_mask(mask: u32) -> Self {
        Self { mask }
    }
}

// ============================================================================
// App Descriptor
// ============================================================================

/// Static app descriptor (loaded from manifest).
#[derive(Clone, Copy)]
pub struct AppDescriptor {
    /// App name (null-terminated).
    pub name: [u8; 32],
    /// App identifier (reverse-domain, null-terminated).
    pub identifier: [u8; 64],
    /// App version.
    pub version: (u16, u16, u16),
    /// Required capabilities.
    pub capabilities: CapabilitySet,
    /// Minimum frame budget in microseconds.
    pub min_frame_budget: u64,
    /// Whether app supports background execution.
    pub background_capable: bool,
    /// Whether app is a system app.
    pub system_app: bool,
}

impl AppDescriptor {
    /// Create an empty descriptor.
    pub const fn empty() -> Self {
        Self {
            name: [0u8; 32],
            identifier: [0u8; 64],
            version: (0, 0, 0),
            capabilities: CapabilitySet::empty(),
            min_frame_budget: DEFAULT_FRAME_BUDGET_US,
            background_capable: false,
            system_app: false,
        }
    }

    /// Create a descriptor with name.
    pub fn new(name: &str, identifier: &str) -> Self {
        let mut desc = Self::empty();

        // Copy name
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len().min(31);
        desc.name[..name_len].copy_from_slice(&name_bytes[..name_len]);

        // Copy identifier
        let id_bytes = identifier.as_bytes();
        let id_len = id_bytes.len().min(63);
        desc.identifier[..id_len].copy_from_slice(&id_bytes[..id_len]);

        desc
    }

    /// Get name as string.
    pub fn name_str(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }

    /// Get identifier as string.
    pub fn identifier_str(&self) -> &str {
        let len = self.identifier.iter().position(|&b| b == 0).unwrap_or(64);
        core::str::from_utf8(&self.identifier[..len]).unwrap_or("")
    }
}

// ============================================================================
// App Instance
// ============================================================================

/// Running app instance.
#[derive(Clone, Copy)]
pub struct AppInstance {
    /// Unique instance ID.
    pub id: AppId,
    /// App descriptor.
    pub descriptor: AppDescriptor,
    /// Current state.
    pub state: AppState,
    /// Main window ID.
    pub window_id: WindowId,
    /// Associated surface ID (for VM apps).
    pub surface_id: SurfaceId,
    /// Granted capabilities (may be subset of requested).
    pub capabilities: CapabilitySet,
    /// Frame budget allocated.
    pub frame_budget: u64,
    /// CPU time used this frame.
    pub frame_time_used: u64,
    /// Total ticks executed.
    pub tick_count: u64,
    /// Launch timestamp.
    pub launch_time: u64,
    /// Last activity timestamp.
    pub last_active: u64,
    /// Error code if in error state.
    pub error_code: u32,
    /// Whether app is visible.
    pub visible: bool,
}

impl AppInstance {
    /// Create an empty instance.
    pub const fn empty() -> Self {
        Self {
            id: APP_ID_NONE,
            descriptor: AppDescriptor::empty(),
            state: AppState::None,
            window_id: 0,
            surface_id: 0,
            capabilities: CapabilitySet::empty(),
            frame_budget: DEFAULT_FRAME_BUDGET_US,
            frame_time_used: 0,
            tick_count: 0,
            launch_time: 0,
            last_active: 0,
            error_code: 0,
            visible: false,
        }
    }

    /// Check if instance is valid.
    pub fn is_valid(&self) -> bool {
        self.id != APP_ID_NONE && !self.state.is_finished()
    }

    /// Check if app can use more frame budget.
    pub fn has_budget(&self) -> bool {
        self.frame_time_used < self.frame_budget
    }

    /// Consume frame budget.
    pub fn consume_budget(&mut self, time_us: u64) {
        self.frame_time_used = self.frame_time_used.saturating_add(time_us);
    }

    /// Reset frame budget for new frame.
    pub fn reset_budget(&mut self) {
        self.frame_time_used = 0;
    }
}

// ============================================================================
// App Registry
// ============================================================================

/// Registry of running apps.
pub struct AppRegistry {
    /// Running app instances.
    apps: [AppInstance; MAX_APPS],
    /// Number of running apps.
    count: usize,
    /// Currently focused app.
    focused_app: AppId,
    /// Total apps launched since boot.
    total_launched: u64,
    /// Total apps terminated since boot.
    total_terminated: u64,
}

impl AppRegistry {
    /// Create a new empty registry.
    pub const fn new() -> Self {
        Self {
            apps: [AppInstance::empty(); MAX_APPS],
            count: 0,
            focused_app: APP_ID_NONE,
            total_launched: 0,
            total_terminated: 0,
        }
    }

    /// Register a new app instance.
    pub fn register(&mut self, instance: AppInstance) -> Option<AppId> {
        if self.count >= MAX_APPS {
            return None;
        }

        // Find empty slot
        for slot in &mut self.apps {
            if slot.id == APP_ID_NONE || slot.state.is_finished() {
                *slot = instance;
                self.count += 1;
                self.total_launched += 1;
                // RAYOS_APP:LAUNCHED
                return Some(instance.id);
            }
        }
        None
    }

    /// Get an app by ID.
    pub fn get(&self, app_id: AppId) -> Option<&AppInstance> {
        self.apps.iter().find(|a| a.id == app_id && a.is_valid())
    }

    /// Get an app mutably by ID.
    pub fn get_mut(&mut self, app_id: AppId) -> Option<&mut AppInstance> {
        self.apps.iter_mut().find(|a| a.id == app_id && a.is_valid())
    }

    /// Get app by window ID.
    pub fn get_by_window(&self, window_id: WindowId) -> Option<&AppInstance> {
        self.apps.iter().find(|a| a.window_id == window_id && a.is_valid())
    }

    /// Remove an app from the registry.
    pub fn remove(&mut self, app_id: AppId) -> bool {
        for app in &mut self.apps {
            if app.id == app_id {
                app.state = AppState::Terminated;
                app.id = APP_ID_NONE;
                if self.count > 0 {
                    self.count -= 1;
                }
                self.total_terminated += 1;
                // RAYOS_APP:TERMINATED
                return true;
            }
        }
        false
    }

    /// Set the focused app.
    pub fn set_focus(&mut self, app_id: AppId) {
        self.focused_app = app_id;
    }

    /// Get the focused app.
    pub fn focused(&self) -> AppId {
        self.focused_app
    }

    /// Get number of running apps.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Check if registry is full.
    pub fn is_full(&self) -> bool {
        self.count >= MAX_APPS
    }

    /// Iterate over running apps.
    pub fn iter(&self) -> impl Iterator<Item = &AppInstance> {
        self.apps.iter().filter(|a| a.is_valid())
    }

    /// Get statistics.
    pub fn stats(&self) -> (usize, u64, u64) {
        (self.count, self.total_launched, self.total_terminated)
    }
}

// ============================================================================
// App Scheduler
// ============================================================================

/// Cooperative scheduler for apps.
pub struct AppScheduler {
    /// Current app being scheduled.
    current_app: AppId,
    /// Round-robin index.
    schedule_index: usize,
    /// Total frame budget available.
    total_budget: u64,
    /// Budget consumed this frame.
    budget_consumed: u64,
    /// Frame tick counter.
    frame_number: u64,
}

impl AppScheduler {
    /// Create a new scheduler.
    pub const fn new() -> Self {
        Self {
            current_app: APP_ID_NONE,
            schedule_index: 0,
            total_budget: DEFAULT_FRAME_BUDGET_US,
            budget_consumed: 0,
            frame_number: 0,
        }
    }

    /// Set total frame budget.
    pub fn set_budget(&mut self, budget_us: u64) {
        self.total_budget = budget_us;
    }

    /// Start a new frame.
    pub fn begin_frame(&mut self) {
        self.budget_consumed = 0;
        self.frame_number += 1;
    }

    /// Get next app to schedule.
    pub fn next_app(&mut self, registry: &mut AppRegistry) -> Option<AppId> {
        if registry.count() == 0 {
            return None;
        }

        // Round-robin scheduling
        let start_index = self.schedule_index;
        loop {
            self.schedule_index = (self.schedule_index + 1) % MAX_APPS;

            let app = &mut registry.apps[self.schedule_index];
            if app.is_valid() && app.state == AppState::Running && app.has_budget() {
                self.current_app = app.id;
                return Some(app.id);
            }

            if self.schedule_index == start_index {
                break;
            }
        }
        None
    }

    /// Record time used by current app.
    pub fn record_time(&mut self, registry: &mut AppRegistry, time_us: u64) {
        if let Some(app) = registry.get_mut(self.current_app) {
            app.consume_budget(time_us);
            app.tick_count += 1;
        }
        self.budget_consumed += time_us;
    }

    /// Check if frame budget is exhausted.
    pub fn budget_exhausted(&self) -> bool {
        self.budget_consumed >= self.total_budget
    }

    /// Reset all app budgets for new frame.
    pub fn reset_budgets(&mut self, registry: &mut AppRegistry) {
        for app in &mut registry.apps {
            if app.is_valid() {
                app.reset_budget();
            }
        }
    }

    /// Get current frame number.
    pub fn frame_number(&self) -> u64 {
        self.frame_number
    }
}

// ============================================================================
// App Sandbox
// ============================================================================

/// Sandbox for capability enforcement.
pub struct AppSandbox {
    /// App ID this sandbox is for.
    app_id: AppId,
    /// Granted capabilities.
    capabilities: CapabilitySet,
    /// Number of capability violations.
    violations: u32,
    /// Last violation capability.
    last_violation: Capability,
}

impl AppSandbox {
    /// Create a new sandbox.
    pub const fn new() -> Self {
        Self {
            app_id: APP_ID_NONE,
            capabilities: CapabilitySet::empty(),
            violations: 0,
            last_violation: Capability::None,
        }
    }

    /// Initialize sandbox for an app.
    pub fn init(&mut self, app_id: AppId, capabilities: CapabilitySet) {
        self.app_id = app_id;
        self.capabilities = capabilities;
        self.violations = 0;
        self.last_violation = Capability::None;
    }

    /// Check if a capability is allowed.
    pub fn check(&mut self, cap: Capability) -> bool {
        if self.capabilities.has(cap) {
            true
        } else {
            self.violations += 1;
            self.last_violation = cap;
            false
        }
    }

    /// Require a capability (panics if not granted).
    pub fn require(&mut self, cap: Capability) -> Result<(), SandboxError> {
        if self.check(cap) {
            Ok(())
        } else {
            Err(SandboxError::CapabilityDenied(cap))
        }
    }

    /// Get violation count.
    pub fn violations(&self) -> u32 {
        self.violations
    }
}

/// Sandbox error types.
#[derive(Clone, Copy, Debug)]
pub enum SandboxError {
    /// Capability denied.
    CapabilityDenied(Capability),
    /// Resource limit exceeded.
    ResourceLimit,
    /// Invalid operation.
    InvalidOperation,
}

// ============================================================================
// IPC Message
// ============================================================================

/// IPC message type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum IpcMessageType {
    /// No message.
    None = 0,
    /// Generic data message.
    Data = 1,
    /// Request message.
    Request = 2,
    /// Response message.
    Response = 3,
    /// Event notification.
    Event = 4,
    /// Clipboard data.
    Clipboard = 5,
    /// Drag-drop data.
    DragDrop = 6,
}

/// IPC message between apps.
#[derive(Clone, Copy)]
pub struct IpcMessage {
    /// Message type.
    pub msg_type: IpcMessageType,
    /// Sender app ID.
    pub sender: AppId,
    /// Receiver app ID.
    pub receiver: AppId,
    /// Message sequence number.
    pub sequence: u32,
    /// Timestamp.
    pub timestamp: u64,
    /// Payload size.
    pub payload_len: usize,
    /// Payload data (small messages inline).
    pub payload: [u8; 64],
}

impl IpcMessage {
    /// Create an empty message.
    pub const fn empty() -> Self {
        Self {
            msg_type: IpcMessageType::None,
            sender: APP_ID_NONE,
            receiver: APP_ID_NONE,
            sequence: 0,
            timestamp: 0,
            payload_len: 0,
            payload: [0u8; 64],
        }
    }

    /// Create a new data message.
    pub fn data(sender: AppId, receiver: AppId, data: &[u8]) -> Self {
        let mut msg = Self::empty();
        msg.msg_type = IpcMessageType::Data;
        msg.sender = sender;
        msg.receiver = receiver;

        let copy_len = data.len().min(64);
        msg.payload[..copy_len].copy_from_slice(&data[..copy_len]);
        msg.payload_len = copy_len;

        msg
    }

    /// Get payload as bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        &self.payload[..self.payload_len]
    }
}

// ============================================================================
// IPC Channel
// ============================================================================

/// IPC message queue for an app.
pub struct IpcQueue {
    /// Messages.
    messages: [IpcMessage; MAX_IPC_QUEUE],
    /// Queue head.
    head: usize,
    /// Queue tail.
    tail: usize,
    /// Messages received total.
    received_count: u64,
    /// Messages dropped (queue full).
    dropped_count: u64,
}

impl IpcQueue {
    /// Create a new queue.
    pub const fn new() -> Self {
        Self {
            messages: [IpcMessage::empty(); MAX_IPC_QUEUE],
            head: 0,
            tail: 0,
            received_count: 0,
            dropped_count: 0,
        }
    }

    /// Enqueue a message.
    pub fn enqueue(&mut self, msg: IpcMessage) -> bool {
        let next_tail = (self.tail + 1) % MAX_IPC_QUEUE;
        if next_tail == self.head {
            self.dropped_count += 1;
            return false;
        }
        self.messages[self.tail] = msg;
        self.tail = next_tail;
        self.received_count += 1;
        // RAYOS_APP:IPC
        true
    }

    /// Dequeue a message.
    pub fn dequeue(&mut self) -> Option<IpcMessage> {
        if self.head == self.tail {
            return None;
        }
        let msg = self.messages[self.head];
        self.head = (self.head + 1) % MAX_IPC_QUEUE;
        Some(msg)
    }

    /// Peek at next message without removing.
    pub fn peek(&self) -> Option<&IpcMessage> {
        if self.head == self.tail {
            return None;
        }
        Some(&self.messages[self.head])
    }

    /// Get queue length.
    pub fn len(&self) -> usize {
        if self.tail >= self.head {
            self.tail - self.head
        } else {
            MAX_IPC_QUEUE - self.head + self.tail
        }
    }

    /// Check if queue is empty.
    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    /// Clear the queue.
    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
    }

    /// Get statistics.
    pub fn stats(&self) -> (u64, u64) {
        (self.received_count, self.dropped_count)
    }
}

// ============================================================================
// App IPC Router
// ============================================================================

/// Routes IPC messages between apps.
pub struct IpcRouter {
    /// Per-app IPC queues.
    queues: [IpcQueue; MAX_APPS],
    /// App ID to queue index mapping.
    app_to_queue: [AppId; MAX_APPS],
    /// Sequence counter.
    sequence: u32,
    /// Timestamp counter.
    timestamp: u64,
}

impl IpcRouter {
    /// Create a new IPC router.
    pub const fn new() -> Self {
        // Manual array init to avoid Copy requirement
        const EMPTY_QUEUE: IpcQueue = IpcQueue::new();
        Self {
            queues: [EMPTY_QUEUE; MAX_APPS],
            app_to_queue: [APP_ID_NONE; MAX_APPS],
            sequence: 0,
            timestamp: 0,
        }
    }

    /// Register an app for IPC.
    pub fn register_app(&mut self, app_id: AppId) -> bool {
        for i in 0..MAX_APPS {
            if self.app_to_queue[i] == APP_ID_NONE {
                self.app_to_queue[i] = app_id;
                self.queues[i].clear();
                return true;
            }
        }
        false
    }

    /// Unregister an app.
    pub fn unregister_app(&mut self, app_id: AppId) {
        for i in 0..MAX_APPS {
            if self.app_to_queue[i] == app_id {
                self.app_to_queue[i] = APP_ID_NONE;
                self.queues[i].clear();
                return;
            }
        }
    }

    /// Find queue index for an app.
    fn queue_index(&self, app_id: AppId) -> Option<usize> {
        self.app_to_queue.iter().position(|&id| id == app_id)
    }

    /// Send a message.
    pub fn send(&mut self, mut msg: IpcMessage) -> bool {
        let receiver_idx = match self.queue_index(msg.receiver) {
            Some(idx) => idx,
            None => return false,
        };

        self.sequence += 1;
        self.timestamp += 1;
        msg.sequence = self.sequence;
        msg.timestamp = self.timestamp;

        self.queues[receiver_idx].enqueue(msg)
    }

    /// Receive a message for an app.
    pub fn receive(&mut self, app_id: AppId) -> Option<IpcMessage> {
        let idx = self.queue_index(app_id)?;
        self.queues[idx].dequeue()
    }

    /// Check if app has pending messages.
    pub fn has_messages(&self, app_id: AppId) -> bool {
        if let Some(idx) = self.queue_index(app_id) {
            !self.queues[idx].is_empty()
        } else {
            false
        }
    }

    /// Get pending message count for an app.
    pub fn pending_count(&self, app_id: AppId) -> usize {
        if let Some(idx) = self.queue_index(app_id) {
            self.queues[idx].len()
        } else {
            0
        }
    }
}

// ============================================================================
// Lifecycle Hooks
// ============================================================================

/// Lifecycle hook type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum LifecycleHookType {
    /// Before app launches.
    PreLaunch = 0,
    /// After app initialization.
    PostInit = 1,
    /// Before suspend.
    PreSuspend = 2,
    /// After resume.
    PostResume = 3,
    /// Before termination.
    PreTerminate = 4,
    /// After termination.
    PostTerminate = 5,
}

/// Lifecycle hook callback signature.
pub type LifecycleHookFn = fn(app_id: AppId, hook_type: LifecycleHookType);

/// Lifecycle hook entry.
#[derive(Clone, Copy)]
pub struct LifecycleHook {
    /// Hook type.
    pub hook_type: LifecycleHookType,
    /// Callback function.
    pub callback: Option<LifecycleHookFn>,
    /// Enabled flag.
    pub enabled: bool,
}

impl LifecycleHook {
    /// Create an empty hook.
    pub const fn empty() -> Self {
        Self {
            hook_type: LifecycleHookType::PreLaunch,
            callback: None,
            enabled: false,
        }
    }
}

// ============================================================================
// App Launcher
// ============================================================================

/// Launches and manages app instances.
pub struct AppLauncher {
    /// App registry.
    registry: AppRegistry,
    /// App scheduler.
    scheduler: AppScheduler,
    /// IPC router.
    ipc_router: IpcRouter,
    /// Lifecycle hooks.
    hooks: [LifecycleHook; MAX_LIFECYCLE_HOOKS],
    /// Number of hooks.
    hook_count: usize,
    /// Current timestamp.
    timestamp: u64,
}

impl AppLauncher {
    /// Create a new app launcher.
    pub const fn new() -> Self {
        Self {
            registry: AppRegistry::new(),
            scheduler: AppScheduler::new(),
            ipc_router: IpcRouter::new(),
            hooks: [LifecycleHook::empty(); MAX_LIFECYCLE_HOOKS],
            hook_count: 0,
            timestamp: 0,
        }
    }

    /// Launch an app.
    pub fn launch(&mut self, descriptor: AppDescriptor, window_id: WindowId) -> Option<AppId> {
        if self.registry.is_full() {
            return None;
        }

        // Fire pre-launch hooks
        let app_id = next_app_id();
        self.fire_hooks(app_id, LifecycleHookType::PreLaunch);

        // Create instance
        let instance = AppInstance {
            id: app_id,
            descriptor,
            state: AppState::Loading,
            window_id,
            surface_id: 0,
            capabilities: descriptor.capabilities,
            frame_budget: descriptor.min_frame_budget,
            frame_time_used: 0,
            tick_count: 0,
            launch_time: self.timestamp,
            last_active: self.timestamp,
            error_code: 0,
            visible: true,
        };

        // Register
        self.registry.register(instance)?;
        self.ipc_router.register_app(app_id);

        // Transition to running
        if let Some(app) = self.registry.get_mut(app_id) {
            app.state = AppState::Running;
            // RAYOS_APP:RUNNING
        }

        // Fire post-init hooks
        self.fire_hooks(app_id, LifecycleHookType::PostInit);

        Some(app_id)
    }

    /// Terminate an app.
    pub fn terminate(&mut self, app_id: AppId) -> bool {
        // Fire pre-terminate hooks
        self.fire_hooks(app_id, LifecycleHookType::PreTerminate);

        // Update state
        if let Some(app) = self.registry.get_mut(app_id) {
            app.state = AppState::Terminating;
        }

        // Unregister from IPC
        self.ipc_router.unregister_app(app_id);

        // Remove from registry
        let result = self.registry.remove(app_id);

        // Fire post-terminate hooks
        self.fire_hooks(app_id, LifecycleHookType::PostTerminate);

        result
    }

    /// Suspend an app.
    pub fn suspend(&mut self, app_id: AppId) -> bool {
        // Check state first without holding mutable borrow
        let should_suspend = self.registry.get(app_id)
            .map(|app| app.state == AppState::Running)
            .unwrap_or(false);
        
        if should_suspend {
            self.fire_hooks(app_id, LifecycleHookType::PreSuspend);
            if let Some(app) = self.registry.get_mut(app_id) {
                app.state = AppState::Suspended;
            }
            // RAYOS_APP:SUSPENDED
            return true;
        }
        false
    }

    /// Resume an app.
    pub fn resume(&mut self, app_id: AppId) -> bool {
        // Check state first without holding mutable borrow
        let should_resume = self.registry.get(app_id)
            .map(|app| app.state == AppState::Suspended)
            .unwrap_or(false);
        
        if should_resume {
            if let Some(app) = self.registry.get_mut(app_id) {
                app.state = AppState::Running;
            }
            self.fire_hooks(app_id, LifecycleHookType::PostResume);
            // RAYOS_APP:RUNNING
            return true;
        }
        false
    }

    /// Add a lifecycle hook.
    pub fn add_hook(&mut self, hook_type: LifecycleHookType, callback: LifecycleHookFn) -> bool {
        if self.hook_count >= MAX_LIFECYCLE_HOOKS {
            return false;
        }
        self.hooks[self.hook_count] = LifecycleHook {
            hook_type,
            callback: Some(callback),
            enabled: true,
        };
        self.hook_count += 1;
        true
    }

    /// Fire hooks of a given type.
    fn fire_hooks(&self, app_id: AppId, hook_type: LifecycleHookType) {
        for hook in &self.hooks[..self.hook_count] {
            if hook.enabled && hook.hook_type == hook_type {
                if let Some(callback) = hook.callback {
                    callback(app_id, hook_type);
                }
            }
        }
    }

    /// Run one scheduler tick.
    pub fn tick(&mut self) {
        self.timestamp += 1;
        self.scheduler.begin_frame();

        // Schedule apps until budget exhausted
        while !self.scheduler.budget_exhausted() {
            if let Some(app_id) = self.scheduler.next_app(&mut self.registry) {
                // In a real implementation, this would call the app's tick function
                // and measure actual CPU time used
                self.scheduler.record_time(&mut self.registry, 100); // Placeholder
            } else {
                break;
            }
        }

        // Reset budgets for next frame
        self.scheduler.reset_budgets(&mut self.registry);
    }

    /// Get registry reference.
    pub fn registry(&self) -> &AppRegistry {
        &self.registry
    }

    /// Get registry mutable reference.
    pub fn registry_mut(&mut self) -> &mut AppRegistry {
        &mut self.registry
    }

    /// Get IPC router reference.
    pub fn ipc_router(&self) -> &IpcRouter {
        &self.ipc_router
    }

    /// Get IPC router mutable reference.
    pub fn ipc_router_mut(&mut self) -> &mut IpcRouter {
        &mut self.ipc_router
    }

    /// Send an IPC message.
    pub fn send_ipc(&mut self, msg: IpcMessage) -> bool {
        self.ipc_router.send(msg)
    }

    /// Receive an IPC message.
    pub fn receive_ipc(&mut self, app_id: AppId) -> Option<IpcMessage> {
        self.ipc_router.receive(app_id)
    }
}

// ============================================================================
// Global App Runtime
// ============================================================================

/// Global app launcher instance.
static mut GLOBAL_APP_LAUNCHER: AppLauncher = AppLauncher::new();

/// Get the global app launcher.
pub fn app_launcher() -> &'static AppLauncher {
    // SAFETY: Single-threaded kernel
    unsafe { &GLOBAL_APP_LAUNCHER }
}

/// Get the global app launcher mutably.
pub fn app_launcher_mut() -> &'static mut AppLauncher {
    // SAFETY: Single-threaded kernel
    unsafe { &mut GLOBAL_APP_LAUNCHER }
}

/// Launch an app.
pub fn launch_app(descriptor: AppDescriptor, window_id: WindowId) -> Option<AppId> {
    app_launcher_mut().launch(descriptor, window_id)
}

/// Terminate an app.
pub fn terminate_app(app_id: AppId) -> bool {
    app_launcher_mut().terminate(app_id)
}

/// Get running app count.
pub fn running_app_count() -> usize {
    app_launcher().registry().count()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_set() {
        let mut caps = CapabilitySet::empty();
        assert!(!caps.has(Capability::Network));

        caps.grant(Capability::Network);
        assert!(caps.has(Capability::Network));

        caps.revoke(Capability::Network);
        assert!(!caps.has(Capability::Network));
    }

    #[test]
    fn test_app_descriptor() {
        let desc = AppDescriptor::new("Test App", "com.example.test");
        assert_eq!(desc.name_str(), "Test App");
        assert_eq!(desc.identifier_str(), "com.example.test");
    }

    #[test]
    fn test_app_state() {
        assert!(AppState::Running.can_receive_input());
        assert!(!AppState::Suspended.can_receive_input());
        assert!(AppState::Running.is_active());
        assert!(AppState::Terminated.is_finished());
    }

    #[test]
    fn test_app_registry() {
        let mut registry = AppRegistry::new();

        let instance = AppInstance {
            id: 1,
            state: AppState::Running,
            ..AppInstance::empty()
        };

        let id = registry.register(instance);
        assert!(id.is_some());
        assert_eq!(registry.count(), 1);

        let app = registry.get(1);
        assert!(app.is_some());

        registry.remove(1);
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_ipc_queue() {
        let mut queue = IpcQueue::new();
        assert!(queue.is_empty());

        let msg = IpcMessage::data(1, 2, b"hello");
        assert!(queue.enqueue(msg));
        assert_eq!(queue.len(), 1);

        let received = queue.dequeue();
        assert!(received.is_some());
        assert!(queue.is_empty());
    }

    #[test]
    fn test_ipc_router() {
        let mut router = IpcRouter::new();

        router.register_app(1);
        router.register_app(2);

        let msg = IpcMessage::data(1, 2, b"test");
        assert!(router.send(msg));
        assert!(router.has_messages(2));

        let received = router.receive(2);
        assert!(received.is_some());
        assert!(!router.has_messages(2));
    }

    #[test]
    fn test_app_sandbox() {
        let mut sandbox = AppSandbox::new();
        let mut caps = CapabilitySet::empty();
        caps.grant(Capability::Network);

        sandbox.init(1, caps);

        assert!(sandbox.check(Capability::Network));
        assert!(!sandbox.check(Capability::FileWrite));
        assert_eq!(sandbox.violations(), 1);
    }

    #[test]
    fn test_app_instance_budget() {
        let mut instance = AppInstance::empty();
        instance.frame_budget = 1000;

        assert!(instance.has_budget());

        instance.consume_budget(500);
        assert!(instance.has_budget());

        instance.consume_budget(600);
        assert!(!instance.has_budget());

        instance.reset_budget();
        assert!(instance.has_budget());
    }
}
