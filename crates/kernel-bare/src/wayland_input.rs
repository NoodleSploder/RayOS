// ===== Phase 23 Task 4: Wayland Input Devices & Events =====
// Implements wl_keyboard, wl_pointer, wl_touch, wl_seat
// Provides input event delivery with focus management

use core::fmt::Write;

// Input device limits
const MAX_KEYBOARDS: usize = 4;
const MAX_POINTERS: usize = 4;
const MAX_TOUCH_DEVICES: usize = 4;
const MAX_SEATS: usize = 1;
const MAX_TOUCH_POINTS: usize = 10;

// Key codes
const KEY_ESCAPE: u32 = 1;
const KEY_RETURN: u32 = 28;
const KEY_SHIFT_L: u32 = 42;
const KEY_SHIFT_R: u32 = 54;
const KEY_CTRL_L: u32 = 29;
const KEY_CTRL_R: u32 = 97;
const KEY_ALT_L: u32 = 56;
const KEY_ALT_R: u32 = 100;
const KEY_SUPER_L: u32 = 125;
const KEY_SUPER_R: u32 = 126;

// Modifier flags
const MODIFIER_SHIFT: u32 = 0x01;
const MODIFIER_CTRL: u32 = 0x02;
const MODIFIER_ALT: u32 = 0x04;
const MODIFIER_SUPER: u32 = 0x08;

// Button codes (left=1, middle=2, right=3, wheel_up=4, wheel_down=5)
const BUTTON_LEFT: u32 = 1;
const BUTTON_MIDDLE: u32 = 2;
const BUTTON_RIGHT: u32 = 3;

// Axis constants (vertical=0, horizontal=1)
const AXIS_VERTICAL: u32 = 0;
const AXIS_HORIZONTAL: u32 = 1;

/// Touch point state
#[derive(Clone, Copy)]
pub struct TouchPoint {
    id: i32,
    x: i32,
    y: i32,
    pressure: u32,
    active: bool,
}

impl TouchPoint {
    fn new(id: i32, x: i32, y: i32) -> Self {
        TouchPoint {
            id,
            x,
            y,
            pressure: 255,
            active: true,
        }
    }

    pub fn get_id(&self) -> i32 {
        self.id
    }

    pub fn get_position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn update_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn release(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// Wayland Touch Device
#[derive(Clone, Copy)]
pub struct WaylandTouch {
    id: u32,
    surface_id: Option<u32>,
    touch_points: [Option<TouchPoint>; MAX_TOUCH_POINTS],
    point_count: usize,
    in_use: bool,
}

impl WaylandTouch {
    const UNINIT: Self = WaylandTouch {
        id: 0,
        surface_id: None,
        touch_points: [None; MAX_TOUCH_POINTS],
        point_count: 0,
        in_use: false,
    };

    fn new(id: u32) -> Self {
        WaylandTouch {
            id,
            surface_id: None,
            touch_points: [None; MAX_TOUCH_POINTS],
            point_count: 0,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn send_down(&mut self, surface_id: u32, touch_id: i32, x: i32, y: i32) -> Result<(), &'static str> {
        if self.point_count >= MAX_TOUCH_POINTS {
            return Err("touch point limit exceeded");
        }

        self.surface_id = Some(surface_id);
        let point = TouchPoint::new(touch_id, x, y);
        self.touch_points[self.point_count] = Some(point);
        self.point_count += 1;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_TOUCH:DOWN] touch_id={} x={} y={}\n", touch_id, x, y)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_up(&mut self, touch_id: i32) -> Result<(), &'static str> {
        for point in self.touch_points.iter_mut() {
            if let Some(p) = point {
                if p.id == touch_id {
                    p.release();
                    return Ok(());
                }
            }
        }
        Err("touch point not found")
    }

    pub fn send_motion(&mut self, touch_id: i32, x: i32, y: i32) -> Result<(), &'static str> {
        for point in self.touch_points.iter_mut() {
            if let Some(p) = point {
                if p.id == touch_id {
                    p.update_position(x, y);
                    return Ok(());
                }
            }
        }
        Err("touch point not found")
    }

    pub fn send_frame(&self) {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_TOUCH:FRAME] point_count={}\n", self.point_count)
            ).ok() {
                // Marker emitted
            }
        }
    }

    pub fn send_cancel(&mut self) {
        self.touch_points = [None; MAX_TOUCH_POINTS];
        self.point_count = 0;
    }

    pub fn get_surface_id(&self) -> Option<u32> {
        self.surface_id
    }
}

/// Wayland Pointer (Mouse)
#[derive(Clone, Copy)]
pub struct WaylandPointer {
    id: u32,
    surface_id: Option<u32>,
    x: i32,
    y: i32,
    button_state: u32,
    in_use: bool,
}

impl WaylandPointer {
    const UNINIT: Self = WaylandPointer {
        id: 0,
        surface_id: None,
        x: 0,
        y: 0,
        button_state: 0,
        in_use: false,
    };

    fn new(id: u32) -> Self {
        WaylandPointer {
            id,
            surface_id: None,
            x: 0,
            y: 0,
            button_state: 0,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn send_enter(&mut self, surface_id: u32, x: i32, y: i32) -> Result<(), &'static str> {
        self.surface_id = Some(surface_id);
        self.x = x;
        self.y = y;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_POINTER:ENTER] surface_id={}\n", surface_id)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_leave(&mut self) -> Result<(), &'static str> {
        self.surface_id = None;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_POINTER:LEAVE] motion_complete\n")
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_motion(&mut self, x: i32, y: i32) -> Result<(), &'static str> {
        self.x = x;
        self.y = y;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_POINTER:MOTION] x={} y={}\n", x, y)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_button(&mut self, button: u32, pressed: bool) -> Result<(), &'static str> {
        if pressed {
            self.button_state |= 1 << button;
        } else {
            self.button_state &= !(1 << button);
        }

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_POINTER:BUTTON] button={} pressed={}\n", button, pressed)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_axis(&self, axis: u32, value: i32) -> Result<(), &'static str> {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_POINTER:AXIS] axis={} value={}\n", axis, value)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn set_cursor(&self, _cursor_name: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }

    pub fn get_position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn get_surface_id(&self) -> Option<u32> {
        self.surface_id
    }

    pub fn is_button_pressed(&self, button: u32) -> bool {
        (self.button_state & (1 << button)) != 0
    }
}

/// Wayland Keyboard
#[derive(Clone, Copy)]
pub struct WaylandKeyboard {
    id: u32,
    surface_id: Option<u32>,
    modifiers: u32,
    repeat_rate: u32,
    repeat_delay: u32,
    in_use: bool,
}

impl WaylandKeyboard {
    const UNINIT: Self = WaylandKeyboard {
        id: 0,
        surface_id: None,
        modifiers: 0,
        repeat_rate: 25,
        repeat_delay: 660,
        in_use: false,
    };

    fn new(id: u32) -> Self {
        WaylandKeyboard {
            id,
            surface_id: None,
            modifiers: 0,
            repeat_rate: 25,
            repeat_delay: 660,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn send_keymap(&self) -> Result<(), &'static str> {
        Ok(())
    }

    pub fn send_enter(&mut self, surface_id: u32) -> Result<(), &'static str> {
        self.surface_id = Some(surface_id);

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_KEYBOARD:ENTER] surface_id={}\n", surface_id)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_leave(&mut self) -> Result<(), &'static str> {
        self.surface_id = None;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_KEYBOARD:LEAVE] focus_lost\n")
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_key(&self, keycode: u32, pressed: bool) -> Result<(), &'static str> {
        let mut modifiers = self.modifiers;
        match keycode {
            KEY_SHIFT_L | KEY_SHIFT_R => {
                if pressed {
                    modifiers |= MODIFIER_SHIFT;
                } else {
                    modifiers &= !MODIFIER_SHIFT;
                }
            },
            KEY_CTRL_L | KEY_CTRL_R => {
                if pressed {
                    modifiers |= MODIFIER_CTRL;
                } else {
                    modifiers &= !MODIFIER_CTRL;
                }
            },
            KEY_ALT_L | KEY_ALT_R => {
                if pressed {
                    modifiers |= MODIFIER_ALT;
                } else {
                    modifiers &= !MODIFIER_ALT;
                }
            },
            KEY_SUPER_L | KEY_SUPER_R => {
                if pressed {
                    modifiers |= MODIFIER_SUPER;
                } else {
                    modifiers &= !MODIFIER_SUPER;
                }
            },
            _ => {},
        }

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_KEYBOARD:KEY] keycode={} pressed={}\n", keycode, pressed)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_modifiers(&mut self, modifiers: u32) -> Result<(), &'static str> {
        self.modifiers = modifiers;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_KEYBOARD:MODIFIERS] modifiers={}\n", modifiers)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn get_surface_id(&self) -> Option<u32> {
        self.surface_id
    }

    pub fn get_modifiers(&self) -> u32 {
        self.modifiers
    }

    pub fn set_repeat(&mut self, rate: u32, delay: u32) {
        self.repeat_rate = rate;
        self.repeat_delay = delay;
    }
}

/// Wayland Seat (Input Hub)
pub struct WaylandSeat {
    id: u32,
    keyboards: [WaylandKeyboard; MAX_KEYBOARDS],
    keyboard_count: usize,
    pointers: [WaylandPointer; MAX_POINTERS],
    pointer_count: usize,
    touch_devices: [WaylandTouch; MAX_TOUCH_DEVICES],
    touch_count: usize,
    focused_surface: Option<u32>,
    next_keyboard_id: u32,
    next_pointer_id: u32,
    next_touch_id: u32,
}

impl WaylandSeat {
    pub fn new() -> Self {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_INPUT:SEAT_ADVERTISED] capabilities=keyboard|pointer|touch\n")
            ).ok() {
                // Marker emitted
            }
        }

        WaylandSeat {
            id: 1,
            keyboards: [WaylandKeyboard::UNINIT; MAX_KEYBOARDS],
            keyboard_count: 0,
            pointers: [WaylandPointer::UNINIT; MAX_POINTERS],
            pointer_count: 0,
            touch_devices: [WaylandTouch::UNINIT; MAX_TOUCH_DEVICES],
            touch_count: 0,
            focused_surface: None,
            next_keyboard_id: 10,
            next_pointer_id: 100,
            next_touch_id: 200,
        }
    }

    pub fn create_keyboard(&mut self) -> Result<u32, &'static str> {
        if self.keyboard_count >= MAX_KEYBOARDS {
            return Err("keyboard limit exceeded");
        }

        let keyboard_id = self.next_keyboard_id;
        self.next_keyboard_id += 1;

        let keyboard = WaylandKeyboard::new(keyboard_id);
        self.keyboards[self.keyboard_count] = keyboard;
        self.keyboard_count += 1;

        Ok(keyboard_id)
    }

    pub fn create_pointer(&mut self) -> Result<u32, &'static str> {
        if self.pointer_count >= MAX_POINTERS {
            return Err("pointer limit exceeded");
        }

        let pointer_id = self.next_pointer_id;
        self.next_pointer_id += 1;

        let pointer = WaylandPointer::new(pointer_id);
        self.pointers[self.pointer_count] = pointer;
        self.pointer_count += 1;

        Ok(pointer_id)
    }

    pub fn create_touch(&mut self) -> Result<u32, &'static str> {
        if self.touch_count >= MAX_TOUCH_DEVICES {
            return Err("touch device limit exceeded");
        }

        let touch_id = self.next_touch_id;
        self.next_touch_id += 1;

        let touch = WaylandTouch::new(touch_id);
        self.touch_devices[self.touch_count] = touch;
        self.touch_count += 1;

        Ok(touch_id)
    }

    pub fn set_selection(&self, _data_device_id: u32) -> Result<(), &'static str> {
        Ok(())
    }

    pub fn set_focus(&mut self, surface_id: Option<u32>) -> Result<(), &'static str> {
        self.focused_surface = surface_id;

        if surface_id.is_some() {
            unsafe {
                if let Some(_) = core::fmt::write(
                    &mut Logger,
                    format_args!("[RAYOS_INPUT:FOCUS_CHANGE] surface_id={}\n", surface_id.unwrap_or(0))
                ).ok() {
                    // Marker emitted
                }
            }
        }
        Ok(())
    }

    pub fn get_focused_surface(&self) -> Option<u32> {
        self.focused_surface
    }

    pub fn get_keyboard_mut(&mut self, keyboard_id: u32) -> Option<&mut WaylandKeyboard> {
        self.keyboards[..self.keyboard_count]
            .iter_mut()
            .find(|k| k.in_use && k.id == keyboard_id)
    }

    pub fn find_keyboard(&self, keyboard_id: u32) -> Option<&WaylandKeyboard> {
        self.keyboards[..self.keyboard_count]
            .iter()
            .find(|k| k.in_use && k.id == keyboard_id)
    }

    pub fn get_pointer_mut(&mut self, pointer_id: u32) -> Option<&mut WaylandPointer> {
        self.pointers[..self.pointer_count]
            .iter_mut()
            .find(|p| p.in_use && p.id == pointer_id)
    }

    pub fn find_pointer(&self, pointer_id: u32) -> Option<&WaylandPointer> {
        self.pointers[..self.pointer_count]
            .iter()
            .find(|p| p.in_use && p.id == pointer_id)
    }

    pub fn get_touch_mut(&mut self, touch_id: u32) -> Option<&mut WaylandTouch> {
        self.touch_devices[..self.touch_count]
            .iter_mut()
            .find(|t| t.in_use && t.id == touch_id)
    }

    pub fn find_touch(&self, touch_id: u32) -> Option<&WaylandTouch> {
        self.touch_devices[..self.touch_count]
            .iter()
            .find(|t| t.in_use && t.id == touch_id)
    }

    pub fn get_keyboard_count(&self) -> usize {
        self.keyboards[..self.keyboard_count].iter().filter(|k| k.in_use).count()
    }

    pub fn get_pointer_count(&self) -> usize {
        self.pointers[..self.pointer_count].iter().filter(|p| p.in_use).count()
    }

    pub fn get_touch_count(&self) -> usize {
        self.touch_devices[..self.touch_count].iter().filter(|t| t.in_use).count()
    }
}

// Simple logging helper
struct Logger;

impl core::fmt::Write for Logger {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        // In a real implementation, this would write to kernel log
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seat_creation() {
        let seat = WaylandSeat::new();
        assert_eq!(seat.id, 1);
        assert_eq!(seat.get_keyboard_count(), 0);
        assert_eq!(seat.get_pointer_count(), 0);
        assert_eq!(seat.get_touch_count(), 0);
    }

    #[test]
    fn test_keyboard_creation() {
        let mut seat = WaylandSeat::new();
        let result = seat.create_keyboard();
        assert!(result.is_ok());
        assert_eq!(seat.get_keyboard_count(), 1);
    }

    #[test]
    fn test_pointer_creation() {
        let mut seat = WaylandSeat::new();
        let result = seat.create_pointer();
        assert!(result.is_ok());
        assert_eq!(seat.get_pointer_count(), 1);
    }

    #[test]
    fn test_touch_creation() {
        let mut seat = WaylandSeat::new();
        let result = seat.create_touch();
        assert!(result.is_ok());
        assert_eq!(seat.get_touch_count(), 1);
    }

    #[test]
    fn test_keyboard_enter_leave() {
        let mut seat = WaylandSeat::new();
        let keyboard_id = seat.create_keyboard().unwrap();
        let keyboard = seat.get_keyboard_mut(keyboard_id).unwrap();

        let result = keyboard.send_enter(1);
        assert!(result.is_ok());
        assert_eq!(keyboard.get_surface_id(), Some(1));

        let result = keyboard.send_leave();
        assert!(result.is_ok());
        assert_eq!(keyboard.get_surface_id(), None);
    }

    #[test]
    fn test_key_press_delivery() {
        let mut seat = WaylandSeat::new();
        let keyboard_id = seat.create_keyboard().unwrap();
        let keyboard = seat.get_keyboard_mut(keyboard_id).unwrap();

        let result = keyboard.send_key(KEY_RETURN, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_key_release_delivery() {
        let mut seat = WaylandSeat::new();
        let keyboard_id = seat.create_keyboard().unwrap();
        let keyboard = seat.get_keyboard_mut(keyboard_id).unwrap();

        keyboard.send_key(KEY_RETURN, true).unwrap();
        let result = keyboard.send_key(KEY_RETURN, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_modifiers_delivery() {
        let mut seat = WaylandSeat::new();
        let keyboard_id = seat.create_keyboard().unwrap();
        let keyboard = seat.get_keyboard_mut(keyboard_id).unwrap();

        let result = keyboard.send_modifiers(MODIFIER_CTRL | MODIFIER_SHIFT);
        assert!(result.is_ok());
        assert_eq!(keyboard.get_modifiers(), MODIFIER_CTRL | MODIFIER_SHIFT);
    }

    #[test]
    fn test_pointer_motion() {
        let mut seat = WaylandSeat::new();
        let pointer_id = seat.create_pointer().unwrap();
        let pointer = seat.get_pointer_mut(pointer_id).unwrap();

        let result = pointer.send_motion(100, 200);
        assert!(result.is_ok());
        assert_eq!(pointer.get_position(), (100, 200));
    }

    #[test]
    fn test_button_press_release() {
        let mut seat = WaylandSeat::new();
        let pointer_id = seat.create_pointer().unwrap();
        let pointer = seat.get_pointer_mut(pointer_id).unwrap();

        pointer.send_button(BUTTON_LEFT, true).unwrap();
        assert!(pointer.is_button_pressed(BUTTON_LEFT));

        pointer.send_button(BUTTON_LEFT, false).unwrap();
        assert!(!pointer.is_button_pressed(BUTTON_LEFT));
    }

    #[test]
    fn test_scroll_delivery() {
        let mut seat = WaylandSeat::new();
        let pointer_id = seat.create_pointer().unwrap();
        let pointer = seat.get_pointer_mut(pointer_id).unwrap();

        let result = pointer.send_axis(AXIS_VERTICAL, -5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_touch_down_up() {
        let mut seat = WaylandSeat::new();
        let touch_id = seat.create_touch().unwrap();
        let touch = seat.get_touch_mut(touch_id).unwrap();

        let result = touch.send_down(1, 0, 100, 200);
        assert!(result.is_ok());

        let result = touch.send_up(0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_focus_management() {
        let mut seat = WaylandSeat::new();
        assert_eq!(seat.get_focused_surface(), None);

        seat.set_focus(Some(1)).unwrap();
        assert_eq!(seat.get_focused_surface(), Some(1));

        seat.set_focus(Some(2)).unwrap();
        assert_eq!(seat.get_focused_surface(), Some(2));

        seat.set_focus(None).unwrap();
        assert_eq!(seat.get_focused_surface(), None);
    }

    #[test]
    fn test_multi_client_input() {
        let mut seat = WaylandSeat::new();

        // Create multiple input devices
        let keyboard_id = seat.create_keyboard().unwrap();
        let pointer_id = seat.create_pointer().unwrap();
        let touch_id = seat.create_touch().unwrap();

        // Verify they are separate
        assert_ne!(keyboard_id, pointer_id);
        assert_ne!(pointer_id, touch_id);

        // Verify counts
        assert_eq!(seat.get_keyboard_count(), 1);
        assert_eq!(seat.get_pointer_count(), 1);
        assert_eq!(seat.get_touch_count(), 1);

        // Create more keyboards for multi-client support
        let keyboard_id2 = seat.create_keyboard().unwrap();
        assert_ne!(keyboard_id, keyboard_id2);
        assert_eq!(seat.get_keyboard_count(), 2);
    }
}
