#![allow(dead_code)]

use crate::guest_surface::GuestSurface;
use crate::{serial_write_bytes, serial_write_hex_u64, serial_write_str};
use core::cell::UnsafeCell;
use core::hint;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

const MAX_RAYAPPS: usize = 4;
const RAYAPP_NAME_MAX: usize = 16;
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

#[derive(Copy, Clone)]
pub struct RayAppEntry {
    name: [u8; RAYAPP_NAME_MAX],
    name_len: u8,
    state: RayAppState,
    z_index: u8,
    focused: bool,
    surface: GuestSurface,
    frame_seq: u64,
    ready_marker_sent: bool,
}

impl RayAppEntry {
    const fn empty() -> Self {
        Self {
            name: [0u8; RAYAPP_NAME_MAX],
            name_len: 0,
            state: RayAppState::Idle,
            z_index: 0,
            focused: false,
            surface: GuestSurface::empty(),
            frame_seq: 0,
            ready_marker_sent: false,
        }
    }

    fn reset(&mut self) {
        self.name = [0u8; RAYAPP_NAME_MAX];
        self.name_len = 0;
        self.state = RayAppState::Idle;
        self.z_index = 0;
        self.focused = false;
        self.surface = GuestSurface::empty();
        self.frame_seq = 0;
        self.ready_marker_sent = false;
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
        if let Some(entry) = guard.entry_mut(id as usize) {
            if entry.is_available() {
                return false;
            }
            entry.focused = true;
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
        for (idx, entry) in guard.iter_mut().enumerate() {
            if entry.matches(name) {
                entry.state = RayAppState::Launching;
                entry.ready_marker_sent = false;
                entry.surface = GuestSurface::empty();
                entry.frame_seq = 0;
                entry.focused = true;
                entry.z_index = self.bump_z_index();
                self.active_app.store(idx as u8, Ordering::Release);
                return Some(idx as u8);
            }
        }
        for idx in 0..MAX_RAYAPPS {
            if let Some(entry) = guard.entry_mut(idx) {
                if entry.is_available() {
                    entry.reset();
                    entry.set_name(name);
                    entry.state = RayAppState::Launching;
                    entry.ready_marker_sent = false;
                    entry.surface = GuestSurface::empty();
                    entry.frame_seq = 0;
                    entry.focused = true;
                    entry.z_index = self.bump_z_index();
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
            entry.reset();
        }
        self.active_app.store(RAYAPP_NONE, Ordering::Release);
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
}
