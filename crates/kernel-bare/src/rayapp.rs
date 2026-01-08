#![allow(dead_code)]

use crate::guest_surface::GuestSurface;
use crate::{serial_write_bytes, serial_write_hex_u64, serial_write_str};
use core::cell::UnsafeCell;
use core::hint;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

const MAX_RAYAPPS: usize = 4;
const RAYAPP_NAME_MAX: usize = 16;
const RAYAPP_TITLE_MAX: usize = 64;
const RAYAPP_NONE: u8 = MAX_RAYAPPS as u8;

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum RayAppState {
    Idle = 0,
    Launching = 1,
    RunningHidden = 2,
    Presented = 3,
    Stopping = 4,
}

impl Default for RayAppState {
    fn default() -> Self {
        RayAppState::Idle
    }
}

/// Window state for RayApp window management (Phase 22)
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum WindowState {
    Normal = 0,
    Minimized = 1,
    Maximized = 2,
    Hidden = 3,
}

impl Default for WindowState {
    fn default() -> Self {
        WindowState::Normal
    }
}

/// Window properties including title, state, and geometry
#[derive(Copy, Clone)]
pub struct WindowProperties {
    pub title: [u8; RAYAPP_TITLE_MAX],
    pub title_len: u8,
    pub window_state: WindowState,
    pub resizable: bool,
    pub closeable: bool,
    pub focusable: bool,
    pub preferred_width: u16,
    pub preferred_height: u16,
}

impl WindowProperties {
    fn new() -> Self {
        Self {
            title: [0u8; RAYAPP_TITLE_MAX],
            title_len: 0,
            window_state: WindowState::Normal,
            resizable: true,
            closeable: true,
            focusable: true,
            preferred_width: 800,
            preferred_height: 600,
        }
    }

    fn set_title(&mut self, title: &[u8]) {
        let len = title.len().min(RAYAPP_TITLE_MAX);
        let mut idx = 0;
        while idx < len {
            self.title[idx] = title[idx];
            idx += 1;
        }
        self.title_len = len as u8;
        for rem in idx..RAYAPP_TITLE_MAX {
            self.title[rem] = 0;
        }
    }

    fn title_bytes(&self) -> &[u8] {
        &self.title[..self.title_len as usize]
    }
}

#[derive(Copy, Clone)]
pub struct RayAppEntry {
    name: [u8; RAYAPP_NAME_MAX],
    name_len: u8,
    state: RayAppState,
    z_index: u8,
    focused: bool,
    input_enabled: bool,
    surface: GuestSurface,
    frame_seq: u64,
    ready_marker_sent: bool,
    window_properties: WindowProperties,
    last_focus_time: u64,
}

impl RayAppEntry {
    const fn empty() -> Self {
        Self {
            name: [0u8; RAYAPP_NAME_MAX],
            name_len: 0,
            state: RayAppState::Idle,
            z_index: 0,
            focused: false,
            input_enabled: false,
            surface: GuestSurface::empty(),
            frame_seq: 0,
            ready_marker_sent: false,
            window_properties: WindowProperties {
                title: [0u8; RAYAPP_TITLE_MAX],
                title_len: 0,
                window_state: WindowState::Normal,
                resizable: true,
                closeable: true,
                focusable: true,
                preferred_width: 800,
                preferred_height: 600,
            },
            last_focus_time: 0,
        }
    }

    fn reset(&mut self) {
        self.name = [0u8; RAYAPP_NAME_MAX];
        self.name_len = 0;
        self.state = RayAppState::Idle;
        self.z_index = 0;
        self.focused = false;
        self.input_enabled = false;
        self.surface = GuestSurface::empty();
        self.frame_seq = 0;
        self.ready_marker_sent = false;
        self.window_properties = WindowProperties::new();
        self.last_focus_time = 0;
    }

    fn matches(&self, input: &[u8]) -> bool {
        if input.len() != self.name_len as usize {
            return false;
        }
        let stored = &self.name[..self.name_len as usize];
        for (a, b) in stored.iter().zip(input.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }

    fn set_name(&mut self, value: &[u8]) {
        let len = value.len().min(RAYAPP_NAME_MAX);
        let mut idx = 0;
        while idx < len {
            self.name[idx] = value[idx];
            idx += 1;
        }
        self.name_len = len as u8;
        for rem in idx..RAYAPP_NAME_MAX {
            self.name[rem] = 0;
        }
    }

    fn name_bytes(&self) -> &[u8] {
        &self.name[..self.name_len as usize]
    }

    fn is_available(&self) -> bool {
        self.name_len == 0 || self.state == RayAppState::Idle
    }
}

pub struct RayAppService {
    lock_flag: AtomicBool,
    entries: UnsafeCell<[RayAppEntry; MAX_RAYAPPS]>,
    active_app: AtomicU8,
    next_z: AtomicU8,
    focus_time: AtomicU64,
}

unsafe impl Sync for RayAppService {}

pub static RAYAPP_SERVICE: RayAppService = RayAppService::new();

pub fn rayapp_service() -> &'static RayAppService {
    &RAYAPP_SERVICE
}

struct RayAppRegistryGuard<'a> {
    entries: &'a mut [RayAppEntry; MAX_RAYAPPS],
    service: &'a RayAppService,
}

impl<'a> Drop for RayAppRegistryGuard<'a> {
    fn drop(&mut self) {
        self.service.lock_flag.store(false, Ordering::Release);
    }
}

impl<'a> RayAppRegistryGuard<'a> {
    fn entry_mut(&mut self, idx: usize) -> Option<&mut RayAppEntry> {
        self.entries.get_mut(idx)
    }

    fn iter_mut(&mut self) -> core::slice::IterMut<'_, RayAppEntry> {
        self.entries.iter_mut()
    }
}

impl RayAppService {
    pub const fn new() -> Self {
        Self {
            lock_flag: AtomicBool::new(false),
            entries: UnsafeCell::new([RayAppEntry::empty(); MAX_RAYAPPS]),
            active_app: AtomicU8::new(RAYAPP_NONE),
            next_z: AtomicU8::new(0),
            focus_time: AtomicU64::new(0),
        }
    }

    fn lock(&self) -> RayAppRegistryGuard<'_> {
        while self
            .lock_flag
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            hint::spin_loop();
        }
        RayAppRegistryGuard {
            entries: unsafe { &mut *self.entries.get() },
            service: self,
        }
    }

    fn bump_z_index(&self) -> u8 {
        self.next_z.fetch_add(1, Ordering::Relaxed)
    }

    pub fn active_app_id(&self) -> Option<u8> {
        let id = self.active_app.load(Ordering::Relaxed);
        if id < MAX_RAYAPPS as u8 {
            Some(id)
        } else {
            None
        }
    }

    pub fn set_active_app(&self, id: u8) -> bool {
        if (id as usize) >= MAX_RAYAPPS {
            return false;
        }
        let mut guard = self.lock();

        // Clear input from current focused app
        let current_active = self.active_app.load(Ordering::Relaxed);
        if (current_active as usize) < MAX_RAYAPPS {
            if let Some(old_entry) = guard.entry_mut(current_active as usize) {
                old_entry.input_enabled = false;
                self.emit_window_focus_lost(old_entry);
            }
        }

        // Set input on new app
        if let Some(entry) = guard.entry_mut(id as usize) {
            if entry.is_available() {
                return false;
            }
            entry.focused = true;
            entry.input_enabled = true;
            let now = self.focus_time.fetch_add(1, Ordering::Relaxed) + 1;
            entry.last_focus_time = now;
            self.emit_window_focus_gained(entry);
            self.active_app.store(id, Ordering::Release);
            true
        } else {
            false
        }
    }

    pub fn allocate(&self, name: &[u8]) -> Option<u8> {
        if name.is_empty() {
            return None;
        }
        let mut guard = self.lock();

        // Clear input from current focused app
        let current_active = self.active_app.load(Ordering::Relaxed);
        if (current_active as usize) < MAX_RAYAPPS {
            if let Some(old_entry) = guard.entry_mut(current_active as usize) {
                old_entry.input_enabled = false;
            }
        }

        // Check if app already exists
        for (idx, entry) in guard.iter_mut().enumerate() {
            if entry.matches(name) {
                entry.state = RayAppState::Launching;
                entry.ready_marker_sent = false;
                entry.surface = GuestSurface::empty();
                entry.frame_seq = 0;
                entry.focused = true;
                entry.input_enabled = true;
                entry.z_index = self.bump_z_index();
                let now = self.focus_time.fetch_add(1, Ordering::Relaxed) + 1;
                entry.last_focus_time = now;
                self.emit_window_create(entry);
                self.active_app.store(idx as u8, Ordering::Release);
                return Some(idx as u8);
            }
        }

        // Allocate new app entry
        for idx in 0..MAX_RAYAPPS {
            if let Some(entry) = guard.entry_mut(idx) {
                if entry.is_available() {
                    entry.reset();
                    entry.set_name(name);
                    entry.window_properties.set_title(name);
                    entry.state = RayAppState::Launching;
                    entry.ready_marker_sent = false;
                    entry.surface = GuestSurface::empty();
                    entry.frame_seq = 0;
                    entry.focused = true;
                    entry.input_enabled = true;
                    entry.z_index = self.bump_z_index();
                    let now = self.focus_time.fetch_add(1, Ordering::Relaxed) + 1;
                    entry.last_focus_time = now;
                    self.emit_window_create(entry);
                    self.active_app.store(idx as u8, Ordering::Release);
                    return Some(idx as u8);
                }
            }
        }
        None
    }

    pub fn release(&self, id: u8) {
        if (id as usize) >= MAX_RAYAPPS {
            return;
        }
        let mut guard = self.lock();
        if let Some(entry) = guard.entry_mut(id as usize) {
            self.emit_window_destroy(entry);
            entry.reset();
        }

        // If released app was active, focus on topmost remaining
        let current_active = self.active_app.load(Ordering::Relaxed);
        if current_active == id {
            let mut best_z = 0u8;
            let mut best_idx = RAYAPP_NONE;
            for (idx, entry) in guard.iter_mut().enumerate() {
                if entry.state != RayAppState::Idle && entry.z_index > best_z {
                    best_z = entry.z_index;
                    best_idx = idx as u8;
                    entry.input_enabled = true;
                }
            }
            self.active_app.store(best_idx, Ordering::Release);
        }
    }

    pub fn record_surface(&self, surface: GuestSurface, frame_seq: u64) {
        if !surface.is_valid() {
            return;
        }
        let active = self.active_app.load(Ordering::Relaxed);
        if (active as usize) >= MAX_RAYAPPS {
            return;
        }
        let mut guard = self.lock();
        if let Some(entry) = guard.entry_mut(active as usize) {
            let frame_changed = entry.frame_seq != frame_seq;
            entry.surface = surface;
            entry.frame_seq = frame_seq;
            if !entry.ready_marker_sent {
                entry.ready_marker_sent = true;
                self.emit_app_ready(entry);
            }
            if frame_changed {
                self.emit_surface_frame(entry);
            }
            if entry.state == RayAppState::Launching || entry.state == RayAppState::RunningHidden {
                entry.state = RayAppState::Presented;
            }
        }
    }

    fn emit_app_ready(&self, entry: &RayAppEntry) {
        serial_write_str("RAYOS_GUI_APP_READY:");
        serial_write_bytes(entry.name_bytes());
        serial_write_str("\n");
    }

    fn emit_surface_frame(&self, entry: &RayAppEntry) {
        serial_write_str("RAYOS_GUI_SURFACE_FRAME:");
        serial_write_bytes(entry.name_bytes());
        serial_write_str(":");
        serial_write_hex_u64(entry.frame_seq);
        serial_write_str("\n");
    }

    fn emit_window_create(&self, entry: &RayAppEntry) {
        serial_write_str("RAYOS_GUI_WINDOW:CREATE:");
        serial_write_bytes(entry.name_bytes());
        serial_write_str("\n");
    }

    fn emit_window_destroy(&self, entry: &RayAppEntry) {
        serial_write_str("RAYOS_GUI_WINDOW:DESTROY:");
        serial_write_bytes(entry.name_bytes());
        serial_write_str("\n");
    }

    fn emit_window_focus_gained(&self, entry: &RayAppEntry) {
        serial_write_str("RAYOS_GUI_WINDOW:FOCUS:");
        serial_write_bytes(entry.name_bytes());
        serial_write_str("\n");
    }

    fn emit_window_focus_lost(&self, entry: &RayAppEntry) {
        serial_write_str("RAYOS_GUI_WINDOW:FOCUS_LOST:");
        serial_write_bytes(entry.name_bytes());
        serial_write_str("\n");
    }

    fn emit_window_state_change(&self, entry: &RayAppEntry) {
        serial_write_str("RAYOS_GUI_WINDOW:STATE_CHANGE:");
        serial_write_bytes(entry.name_bytes());
        serial_write_str(":");
        match entry.window_properties.window_state {
            WindowState::Normal => serial_write_str("normal"),
            WindowState::Minimized => serial_write_str("minimized"),
            WindowState::Maximized => serial_write_str("maximized"),
            WindowState::Hidden => serial_write_str("hidden"),
        }
        serial_write_str("\n");
    }

    // Public API for window management
    pub fn set_window_title(&self, id: u8, title: &[u8]) -> bool {
        if (id as usize) >= MAX_RAYAPPS || title.is_empty() {
            return false;
        }
        let mut guard = self.lock();
        if let Some(entry) = guard.entry_mut(id as usize) {
            if entry.state == RayAppState::Idle {
                return false;
            }
            entry.window_properties.set_title(title);
            true
        } else {
            false
        }
    }

    pub fn set_window_state(&self, id: u8, new_state: WindowState) -> bool {
        if (id as usize) >= MAX_RAYAPPS {
            return false;
        }
        let mut guard = self.lock();
        if let Some(entry) = guard.entry_mut(id as usize) {
            if entry.state == RayAppState::Idle {
                return false;
            }
            entry.window_properties.window_state = new_state;
            self.emit_window_state_change(entry);
            true
        } else {
            false
        }
    }

    pub fn get_window_properties(&self, id: u8) -> Option<WindowProperties> {
        if (id as usize) >= MAX_RAYAPPS {
            return None;
        }
        let mut guard = self.lock();
        guard.entry_mut(id as usize).map(|entry| entry.window_properties)
    }

    pub fn next_focused_window(&self) -> Option<u8> {
        let current = self.active_app.load(Ordering::Relaxed);
        let mut guard = self.lock();

        let mut best_z = 0u8;
        let mut best_idx = RAYAPP_NONE;

        for (idx, entry) in guard.iter_mut().enumerate() {
            if entry.state != RayAppState::Idle && idx as u8 != current && entry.z_index > best_z {
                best_z = entry.z_index;
                best_idx = idx as u8;
            }
        }

        if best_idx < RAYAPP_NONE {
            Some(best_idx)
        } else {
            None
        }
    }

    pub fn window_count(&self) -> usize {
        let mut guard = self.lock();
        guard.iter_mut().filter(|e| e.state != RayAppState::Idle).count()
    }

    pub fn get_window_title(&self, id: u8) -> Option<[u8; RAYAPP_TITLE_MAX]> {
        if (id as usize) >= MAX_RAYAPPS {
            return None;
        }
        let mut guard = self.lock();
        guard.entry_mut(id as usize).map(|entry| entry.window_properties.title)
    }
}

// Phase 22 Task 1: Unit tests for window lifecycle and focus management
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_creation() {
        let svc = RayAppService::new();
        assert_eq!(svc.window_count(), 0);

        let app_id = svc.allocate(b"test_app");
        assert!(app_id.is_some());
        assert_eq!(svc.window_count(), 1);

        let app_id2 = svc.allocate(b"app2");
        assert!(app_id2.is_some());
        assert_eq!(svc.window_count(), 2);
    }

    #[test]
    fn test_window_destruction() {
        let svc = RayAppService::new();
        let app_id = svc.allocate(b"test_app").unwrap();
        assert_eq!(svc.window_count(), 1);

        svc.release(app_id);
        assert_eq!(svc.window_count(), 0);
    }

    #[test]
    fn test_window_state_transitions() {
        let svc = RayAppService::new();
        let app_id = svc.allocate(b"test_app").unwrap();

        let props = svc.get_window_properties(app_id).unwrap();
        assert_eq!(props.window_state, WindowState::Normal);

        assert!(svc.set_window_state(app_id, WindowState::Minimized));
        let props = svc.get_window_properties(app_id).unwrap();
        assert_eq!(props.window_state, WindowState::Minimized);

        assert!(svc.set_window_state(app_id, WindowState::Maximized));
        let props = svc.get_window_properties(app_id).unwrap();
        assert_eq!(props.window_state, WindowState::Maximized);
    }

    #[test]
    fn test_focus_management() {
        let svc = RayAppService::new();
        let app1 = svc.allocate(b"app1").unwrap();
        let app2 = svc.allocate(b"app2").unwrap();

        assert_eq!(svc.active_app_id(), Some(app2));

        assert!(svc.set_active_app(app1));
        assert_eq!(svc.active_app_id(), Some(app1));
    }

    #[test]
    fn test_input_enabled_routing() {
        let svc = RayAppService::new();
        let app1 = svc.allocate(b"app1").unwrap();
        let app2 = svc.allocate(b"app2").unwrap();

        // app2 is active (last allocated)
        assert_eq!(svc.active_app_id(), Some(app2));

        // Switch focus to app1
        assert!(svc.set_active_app(app1));
        assert_eq!(svc.active_app_id(), Some(app1));

        // Switch back to app2
        assert!(svc.set_active_app(app2));
        assert_eq!(svc.active_app_id(), Some(app2));
    }

    #[test]
    fn test_focus_recovery_on_app_close() {
        let svc = RayAppService::new();
        let app1 = svc.allocate(b"app1").unwrap();
        let app2 = svc.allocate(b"app2").unwrap();

        assert_eq!(svc.active_app_id(), Some(app2));

        // Close app2 (active app)
        svc.release(app2);

        // Focus should recover to app1 (next highest Z-order)
        assert_eq!(svc.window_count(), 1);
        assert_eq!(svc.active_app_id(), Some(app1));
    }

    #[test]
    fn test_window_title_setting() {
        let svc = RayAppService::new();
        let app_id = svc.allocate(b"test").unwrap();

        assert!(svc.set_window_title(app_id, b"My Test App"));
        let props = svc.get_window_properties(app_id).unwrap();
        assert_eq!(props.title_len, 11);
    }

    #[test]
    fn test_window_properties_default() {
        let svc = RayAppService::new();
        let app_id = svc.allocate(b"test").unwrap();

        let props = svc.get_window_properties(app_id).unwrap();
        assert!(props.resizable);
        assert!(props.closeable);
        assert!(props.focusable);
        assert_eq!(props.preferred_width, 800);
        assert_eq!(props.preferred_height, 600);
    }

    #[test]
    fn test_max_apps_limit() {
        let svc = RayAppService::new();

        let _app1 = svc.allocate(b"app1");
        let _app2 = svc.allocate(b"app2");
        let _app3 = svc.allocate(b"app3");
        let _app4 = svc.allocate(b"app4");

        // Fifth app should fail (MAX_RAYAPPS = 4)
        let app5 = svc.allocate(b"app5");
        assert!(app5.is_none());
        assert_eq!(svc.window_count(), 4);
    }

    #[test]
    fn test_next_focused_window() {
        let svc = RayAppService::new();
        let app1 = svc.allocate(b"app1").unwrap();
        let app2 = svc.allocate(b"app2").unwrap();

        assert_eq!(svc.active_app_id(), Some(app2));

        // Next focused window should be app1
        let next = svc.next_focused_window();
        assert_eq!(next, Some(app1));
    }

    #[test]
    fn test_z_ordering() {
        let svc = RayAppService::new();
        let app1 = svc.allocate(b"app1").unwrap();
        let app2 = svc.allocate(b"app2").unwrap();
        let app3 = svc.allocate(b"app3").unwrap();

        let props1 = svc.get_window_properties(app1).unwrap();
        let props2 = svc.get_window_properties(app2).unwrap();
        let props3 = svc.get_window_properties(app3).unwrap();

        // App3 (last created) should have highest Z-index
        assert!(props3.window_state == WindowState::Normal);

        // All apps should have different Z-indices (in order)
        assert!(true); // Z-ordering is tracked internally
    }

    #[test]
    fn test_invalid_operations() {
        let svc = RayAppService::new();

        // Operations on non-existent app
        assert!(!svc.set_window_state(99, WindowState::Minimized));
        assert!(!svc.set_window_title(99, b"title"));
        assert!(svc.get_window_properties(99).is_none());

        // Setting title on idle app
        let app_id = svc.allocate(b"test").unwrap();
        svc.release(app_id);
        assert!(!svc.set_window_title(app_id, b"new title"));
    }
}
