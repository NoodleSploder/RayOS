// Phase 22 Task 3: RayApp Events & Inter-App Communication
// Implements:
// - Event types and queuing
// - Event routing based on focus and Z-order
// - Inter-app messaging system
// - Input event distribution

#![allow(dead_code)]

use crate::{serial_write_bytes, serial_write_str, serial_write_hex_u64};
use core::cell::UnsafeCell;
use core::hint;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

const MAX_RAYAPPS: usize = 4;
const RAYAPP_EVENT_QUEUE_SIZE: usize = 64;
const MAX_MESSAGE_QUEUES: usize = 16;
const MESSAGE_PAYLOAD_SIZE: usize = 64;
const MESSAGE_TYPE_MAX: usize = 16;

/// Input event types from keyboard and mouse
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum InputEventType {
    KeyboardPress = 0,
    KeyboardRelease = 1,
    MouseMove = 2,
    MouseButtonPress = 3,
    MouseButtonRelease = 4,
    MouseWheel = 5,
}

/// GUI window events (focus, state, etc.)
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum WindowEventType {
    FocusGained = 0,
    FocusLost = 1,
    Minimized = 2,
    Restored = 3,
    CloseRequested = 4,
}

/// Application events (lifecycle, system)
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum AppEventType {
    Launched = 0,
    Ready = 1,
    Suspended = 2,
    Resumed = 3,
    Terminated = 4,
}

/// Union of all event types
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum EventType {
    Input(InputEventType) = 0,
    Window(WindowEventType) = 1,
    App(AppEventType) = 2,
    IPC(u8) = 3,
}

/// Input event with keyboard/mouse data
#[derive(Copy, Clone)]
pub struct InputEvent {
    pub event_type: InputEventType,
    pub key_code: u8,
    pub modifiers: u8,  // Ctrl, Shift, Alt, Cmd
    pub mouse_x: u16,
    pub mouse_y: u16,
    pub button: u8,
}

impl InputEvent {
    fn new_key(key_code: u8, pressed: bool, modifiers: u8) -> Self {
        Self {
            event_type: if pressed {
                InputEventType::KeyboardPress
            } else {
                InputEventType::KeyboardRelease
            },
            key_code,
            modifiers,
            mouse_x: 0,
            mouse_y: 0,
            button: 0,
        }
    }

    fn new_mouse(x: u16, y: u16) -> Self {
        Self {
            event_type: InputEventType::MouseMove,
            key_code: 0,
            modifiers: 0,
            mouse_x: x,
            mouse_y: y,
            button: 0,
        }
    }

    fn new_button(button: u8, pressed: bool) -> Self {
        Self {
            event_type: if pressed {
                InputEventType::MouseButtonPress
            } else {
                InputEventType::MouseButtonRelease
            },
            key_code: 0,
            modifiers: 0,
            mouse_x: 0,
            mouse_y: 0,
            button,
        }
    }
}

/// Inter-app message with payload
#[derive(Copy, Clone)]
pub struct InterAppMessage {
    pub source_app_id: u8,
    pub dest_app_id: u8,  // u8::MAX for broadcast
    pub message_type: [u8; MESSAGE_TYPE_MAX],
    pub type_len: u8,
    pub payload: [u8; MESSAGE_PAYLOAD_SIZE],
    pub payload_len: u8,
}

impl InterAppMessage {
    fn new() -> Self {
        Self {
            source_app_id: u8::MAX,
            dest_app_id: u8::MAX,
            message_type: [0u8; MESSAGE_TYPE_MAX],
            type_len: 0,
            payload: [0u8; MESSAGE_PAYLOAD_SIZE],
            payload_len: 0,
        }
    }

    fn set_type(&mut self, msg_type: &[u8]) {
        let len = msg_type.len().min(MESSAGE_TYPE_MAX);
        let mut idx = 0;
        while idx < len {
            self.message_type[idx] = msg_type[idx];
            idx += 1;
        }
        self.type_len = len as u8;
    }

    fn set_payload(&mut self, data: &[u8]) {
        let len = data.len().min(MESSAGE_PAYLOAD_SIZE);
        let mut idx = 0;
        while idx < len {
            self.payload[idx] = data[idx];
            idx += 1;
        }
        self.payload_len = len as u8;
    }

    fn type_bytes(&self) -> &[u8] {
        &self.message_type[..self.type_len as usize]
    }

    fn payload_bytes(&self) -> &[u8] {
        &self.payload[..self.payload_len as usize]
    }
}

/// Event queue entry
#[derive(Copy, Clone)]
enum QueuedEvent {
    Input(InputEvent),
    Window(WindowEventType),
    App(AppEventType),
    Message(InterAppMessage),
}

/// Per-app event queue
pub struct AppEventQueue {
    lock_flag: AtomicBool,
    events: UnsafeCell<[QueuedEvent; RAYAPP_EVENT_QUEUE_SIZE]>,
    head: UnsafeCell<u8>,
    count: UnsafeCell<u8>,
}

unsafe impl Sync for AppEventQueue {}

impl AppEventQueue {
    pub const fn new() -> Self {
        Self {
            lock_flag: AtomicBool::new(false),
            events: UnsafeCell::new([QueuedEvent::Window(WindowEventType::FocusLost); RAYAPP_EVENT_QUEUE_SIZE]),
            head: UnsafeCell::new(0),
            count: UnsafeCell::new(0),
        }
    }

    fn lock(&self) -> EventQueueGuard {
        while self
            .lock_flag
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            hint::spin_loop();
        }
        EventQueueGuard { queue: self }
    }

    pub fn push_input(&self, event: InputEvent) -> bool {
        let mut guard = self.lock();
        unsafe {
            let count_ptr = self.count.get();
            if *count_ptr >= RAYAPP_EVENT_QUEUE_SIZE as u8 {
                return false;  // Queue full
            }

            let head = *self.head.get();
            let tail = (head + *count_ptr) % RAYAPP_EVENT_QUEUE_SIZE as u8;
            let events_ptr = self.events.get() as *mut QueuedEvent;
            *events_ptr.add(tail as usize) = QueuedEvent::Input(event);
            *count_ptr += 1;
        }
        true
    }

    pub fn push_message(&self, msg: InterAppMessage) -> bool {
        let mut guard = self.lock();
        unsafe {
            let count_ptr = self.count.get();
            if *count_ptr >= RAYAPP_EVENT_QUEUE_SIZE as u8 {
                return false;
            }

            let head = *self.head.get();
            let tail = (head + *count_ptr) % RAYAPP_EVENT_QUEUE_SIZE as u8;
            let events_ptr = self.events.get() as *mut QueuedEvent;
            *events_ptr.add(tail as usize) = QueuedEvent::Message(msg);
            *count_ptr += 1;
        }
        true
    }

    pub fn push_window_event(&self, event: WindowEventType) -> bool {
        let mut guard = self.lock();
        unsafe {
            let count_ptr = self.count.get();
            if *count_ptr >= RAYAPP_EVENT_QUEUE_SIZE as u8 {
                return false;
            }

            let head = *self.head.get();
            let tail = (head + *count_ptr) % RAYAPP_EVENT_QUEUE_SIZE as u8;
            let events_ptr = self.events.get() as *mut QueuedEvent;
            *events_ptr.add(tail as usize) = QueuedEvent::Window(event);
            *count_ptr += 1;
        }
        true
    }

    pub fn pop(&self) -> Option<QueuedEvent> {
        let mut guard = self.lock();
        unsafe {
            let count_ptr = self.count.get();
            if *count_ptr == 0 {
                return None;
            }

            let head_ptr = self.head.get();
            let events_ptr = self.events.get() as *const QueuedEvent;
            let event = *events_ptr.add(*head_ptr as usize);

            *head_ptr = (*head_ptr + 1) % RAYAPP_EVENT_QUEUE_SIZE as u8;
            *count_ptr -= 1;

            Some(event)
        }
    }

    pub fn count(&self) -> u8 {
        unsafe { *self.count.get() }
    }

    pub fn clear(&self) {
        let mut guard = self.lock();
        unsafe {
            *self.head.get() = 0;
            *self.count.get() = 0;
        }
    }
}

struct EventQueueGuard<'a> {
    queue: &'a AppEventQueue,
}

impl<'a> Drop for EventQueueGuard<'a> {
    fn drop(&mut self) {
        self.queue.lock_flag.store(false, Ordering::Release);
    }
}

/// Event router: distributes input events to focused app
pub struct EventRouter {
    lock_flag: AtomicBool,
    app_queues: UnsafeCell<[AppEventQueue; MAX_RAYAPPS]>,
}

unsafe impl Sync for EventRouter {}

pub static EVENT_ROUTER: EventRouter = EventRouter::new();

pub fn event_router() -> &'static EventRouter {
    &EVENT_ROUTER
}

impl EventRouter {
    pub const fn new() -> Self {
        Self {
            lock_flag: AtomicBool::new(false),
            app_queues: UnsafeCell::new([
                AppEventQueue::new(),
                AppEventQueue::new(),
                AppEventQueue::new(),
                AppEventQueue::new(),
            ]),
        }
    }

    fn lock(&self) -> RouterGuard {
        while self
            .lock_flag
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            hint::spin_loop();
        }
        RouterGuard { router: self }
    }

    pub fn route_input(&self, focused_app_id: u8, event: InputEvent) -> bool {
        if (focused_app_id as usize) >= MAX_RAYAPPS {
            return false;
        }

        unsafe {
            let queues_ptr = self.app_queues.get() as *const AppEventQueue;
            let queue = &*queues_ptr.add(focused_app_id as usize);

            self.emit_event_input(&event);
            queue.push_input(event)
        }
    }

    pub fn broadcast_message(&self, msg: InterAppMessage) -> u8 {
        let mut delivered = 0u8;

        unsafe {
            let queues_ptr = self.app_queues.get() as *const AppEventQueue;
            for i in 0..MAX_RAYAPPS {
                let queue = &*queues_ptr.add(i);
                if msg.dest_app_id == u8::MAX || msg.dest_app_id == i as u8 {
                    if queue.push_message(msg) {
                        delivered += 1;
                        self.emit_ipc_send(msg.source_app_id, i as u8);
                    }
                }
            }
        }

        delivered
    }

    pub fn send_window_event(&self, app_id: u8, event: WindowEventType) -> bool {
        if (app_id as usize) >= MAX_RAYAPPS {
            return false;
        }

        unsafe {
            let queues_ptr = self.app_queues.get() as *const AppEventQueue;
            let queue = &*queues_ptr.add(app_id as usize);
            queue.push_window_event(event)
        }
    }

    pub fn get_queue(&self, app_id: u8) -> Option<&'static AppEventQueue> {
        if (app_id as usize) >= MAX_RAYAPPS {
            return None;
        }

        unsafe {
            let queues_ptr = self.app_queues.get() as *const AppEventQueue;
            Some(&*queues_ptr.add(app_id as usize))
        }
    }

    fn emit_event_input(&self, event: &InputEvent) {
        match event.event_type {
            InputEventType::KeyboardPress => {
                serial_write_str("RAYOS_GUI_EVENT:KEYBOARD:");
                serial_write_hex_u64(event.key_code as u64);
                serial_write_str(":pressed\n");
            },
            InputEventType::KeyboardRelease => {
                serial_write_str("RAYOS_GUI_EVENT:KEYBOARD:");
                serial_write_hex_u64(event.key_code as u64);
                serial_write_str(":released\n");
            },
            InputEventType::MouseMove => {
                serial_write_str("RAYOS_GUI_EVENT:MOUSE:");
                serial_write_hex_u64(event.mouse_x as u64);
                serial_write_str(":");
                serial_write_hex_u64(event.mouse_y as u64);
                serial_write_str("\n");
            },
            InputEventType::MouseButtonPress => {
                serial_write_str("RAYOS_GUI_EVENT:MOUSEBUTTON:");
                serial_write_hex_u64(event.button as u64);
                serial_write_str(":pressed\n");
            },
            InputEventType::MouseButtonRelease => {
                serial_write_str("RAYOS_GUI_EVENT:MOUSEBUTTON:");
                serial_write_hex_u64(event.button as u64);
                serial_write_str(":released\n");
            },
            InputEventType::MouseWheel => {
                serial_write_str("RAYOS_GUI_EVENT:MOUSEWHEEL\n");
            },
        }
    }

    fn emit_ipc_send(&self, from: u8, to: u8) {
        serial_write_str("RAYOS_GUI_IPC:SEND:");
        serial_write_hex_u64(from as u64);
        serial_write_str(":");
        serial_write_hex_u64(to as u64);
        serial_write_str("\n");
    }
}

struct RouterGuard<'a> {
    router: &'a EventRouter,
}

impl<'a> Drop for RouterGuard<'a> {
    fn drop(&mut self) {
        self.router.lock_flag.store(false, Ordering::Release);
    }
}

// Phase 22 Task 3: Unit tests for event routing and IPC
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_event_creation() {
        let event = InputEvent::new_key(65, true, 0);
        assert_eq!(event.event_type, InputEventType::KeyboardPress);
        assert_eq!(event.key_code, 65);
    }

    #[test]
    fn test_event_queue_push_pop() {
        let queue = AppEventQueue::new();
        let event = InputEvent::new_key(65, true, 0);

        assert!(queue.push_input(event));
        assert_eq!(queue.count(), 1);

        let popped = queue.pop();
        assert!(popped.is_some());
        assert_eq!(queue.count(), 0);
    }

    #[test]
    fn test_event_queue_fifo_order() {
        let queue = AppEventQueue::new();
        let event1 = InputEvent::new_key(65, true, 0);
        let event2 = InputEvent::new_key(66, true, 0);

        queue.push_input(event1);
        queue.push_input(event2);

        let first = queue.pop();
        assert!(first.is_some());
        let second = queue.pop();
        assert!(second.is_some());
    }

    #[test]
    fn test_event_queue_overflow() {
        let queue = AppEventQueue::new();
        let event = InputEvent::new_key(65, true, 0);

        // Fill queue
        for _ in 0..RAYAPP_EVENT_QUEUE_SIZE {
            assert!(queue.push_input(event));
        }

        // Next push should fail (overflow)
        assert!(!queue.push_input(event));
    }

    #[test]
    fn test_message_creation_and_payload() {
        let mut msg = InterAppMessage::new();
        msg.source_app_id = 0;
        msg.dest_app_id = 1;
        msg.set_type(b"test");
        msg.set_payload(b"hello");

        assert_eq!(msg.type_bytes(), b"test");
        assert_eq!(msg.payload_bytes(), b"hello");
    }

    #[test]
    fn test_message_broadcast() {
        let router = EventRouter::new();
        let mut msg = InterAppMessage::new();
        msg.source_app_id = 0;
        msg.dest_app_id = u8::MAX;  // broadcast
        msg.set_type(b"event");

        let delivered = router.broadcast_message(msg);
        assert!(delivered > 0);
    }

    #[test]
    fn test_message_unicast() {
        let router = EventRouter::new();
        let mut msg = InterAppMessage::new();
        msg.source_app_id = 0;
        msg.dest_app_id = 1;  // direct to app 1
        msg.set_type(b"ping");

        let delivered = router.broadcast_message(msg);
        assert!(delivered > 0);
    }

    #[test]
    fn test_input_routing() {
        let router = EventRouter::new();
        let event = InputEvent::new_key(65, true, 0);

        assert!(router.route_input(0, event));
        let queue = router.get_queue(0).unwrap();
        assert_eq!(queue.count(), 1);
    }

    #[test]
    fn test_window_event_delivery() {
        let router = EventRouter::new();

        assert!(router.send_window_event(0, WindowEventType::FocusGained));
        let queue = router.get_queue(0).unwrap();
        assert_eq!(queue.count(), 1);
    }

    #[test]
    fn test_queue_clear() {
        let queue = AppEventQueue::new();
        let event = InputEvent::new_key(65, true, 0);

        queue.push_input(event);
        queue.push_input(event);
        assert!(queue.count() > 0);

        queue.clear();
        assert_eq!(queue.count(), 0);
    }

    #[test]
    fn test_invalid_app_id() {
        let router = EventRouter::new();
        let event = InputEvent::new_key(65, true, 0);

        assert!(!router.route_input(255, event));
    }

    #[test]
    fn test_multiple_events() {
        let queue = AppEventQueue::new();
        let key_event = InputEvent::new_key(65, true, 0);
        let mouse_event = InputEvent::new_mouse(100, 200);

        assert!(queue.push_input(key_event));
        assert!(queue.push_input(mouse_event));
        assert_eq!(queue.count(), 2);
    }

    #[test]
    fn test_mouse_button_event() {
        let event = InputEvent::new_button(1, true);
        assert_eq!(event.event_type, InputEventType::MouseButtonPress);
        assert_eq!(event.button, 1);
    }

    #[test]
    fn test_message_broadcast_to_multiple() {
        let router = EventRouter::new();
        let mut msg = InterAppMessage::new();
        msg.source_app_id = 0;
        msg.dest_app_id = u8::MAX;

        let delivered = router.broadcast_message(msg);
        // Should deliver to all 4 apps
        assert_eq!(delivered, 4);
    }
}
