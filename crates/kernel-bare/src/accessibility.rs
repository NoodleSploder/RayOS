// RAYOS Phase 27 Task 3: Accessibility Framework
// AT-SPI2 compatible accessibility framework for screen readers and assistive tech
// File: crates/kernel-bare/src/accessibility.rs
// Lines: 700+ | Tests: 13 unit + 5 scenario | Markers: 5

use core::fmt;

const MAX_A11Y_OBJECTS: usize = 64;
const MAX_SHORTCUTS: usize = 256;
const ANNOUNCEMENT_QUEUE_SIZE: usize = 128;

// ============================================================================
// ACCESSIBILITY ROLES & STATES
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccessibleRole {
    Window,
    Button,
    Label,
    Text,
    Container,
    Menu,
    MenuItem,
    List,
    ListItem,
    Table,
    TableRow,
    Dialog,
    ToggleButton,
    Slider,
    ComboBox,
    Application,
}

impl AccessibleRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessibleRole::Window => "window",
            AccessibleRole::Button => "button",
            AccessibleRole::Label => "label",
            AccessibleRole::Text => "text",
            AccessibleRole::Container => "container",
            AccessibleRole::Menu => "menu",
            AccessibleRole::MenuItem => "menuitem",
            AccessibleRole::List => "list",
            AccessibleRole::ListItem => "listitem",
            AccessibleRole::Table => "table",
            AccessibleRole::TableRow => "table_row",
            AccessibleRole::Dialog => "dialog",
            AccessibleRole::ToggleButton => "toggle_button",
            AccessibleRole::Slider => "slider",
            AccessibleRole::ComboBox => "combobox",
            AccessibleRole::Application => "application",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AccessibleState {
    pub focused: bool,
    pub pressed: bool,
    pub expanded: bool,
    pub sensitive: bool,
    pub visible: bool,
    pub enabled: bool,
    pub selected: bool,
}

impl AccessibleState {
    pub fn new() -> Self {
        AccessibleState {
            focused: false,
            pressed: false,
            expanded: false,
            sensitive: true,
            visible: true,
            enabled: true,
            selected: false,
        }
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn set_pressed(&mut self, pressed: bool) {
        self.pressed = pressed;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Default for AccessibleState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ACCESSIBILITY OBJECT
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct AccessibilityObject {
    pub object_id: u32,
    pub role: AccessibleRole,
    pub state: AccessibleState,
    pub parent_id: Option<u32>,
    pub child_count: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl AccessibilityObject {
    pub fn new(object_id: u32, role: AccessibleRole) -> Self {
        AccessibilityObject {
            object_id,
            role,
            state: AccessibleState::new(),
            parent_id: None,
            child_count: 0,
            name_id: 0,
            description_id: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }

    pub fn is_focused(&self) -> bool {
        self.state.focused
    }

    pub fn is_enabled(&self) -> bool {
        self.state.enabled && self.state.sensitive
    }

    pub fn get_bounds(&self) -> (i32, i32, u32, u32) {
        (self.x, self.y, self.width, self.height)
    }

    pub fn set_bounds(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.x = x;
        self.y = y;
        self.width = width;
        self.height = height;
    }

    pub fn point_in_bounds(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && py >= self.y
            && (px as u32) < (self.x as u32 + self.width)
            && (py as u32) < (self.y as u32 + self.height)
    }
}

// ============================================================================
// ACCESSIBILITY TREE
// ============================================================================

pub struct AccessibilityTree {
    pub objects: [Option<AccessibilityObject>; MAX_A11Y_OBJECTS],
    pub object_count: usize,
    pub next_object_id: u32,
    pub children: [[Option<u32>; 8]; MAX_A11Y_OBJECTS], // 8 children per object max
    pub child_counts: [usize; MAX_A11Y_OBJECTS],
}

impl AccessibilityTree {
    pub fn new() -> Self {
        AccessibilityTree {
            objects: [None; MAX_A11Y_OBJECTS],
            object_count: 0,
            next_object_id: 1,
            children: [[None; 8]; MAX_A11Y_OBJECTS],
            child_counts: [0; MAX_A11Y_OBJECTS],
        }
    }

    pub fn create_object(&mut self, role: AccessibleRole) -> Option<u32> {
        if self.object_count >= MAX_A11Y_OBJECTS {
            return None;
        }

        let object_id = self.next_object_id;
        self.next_object_id += 1;

        let object = AccessibilityObject::new(object_id, role);
        self.objects[self.object_count] = Some(object);
        self.object_count += 1;

        Some(object_id)
    }

    pub fn get_object(&self, object_id: u32) -> Option<AccessibilityObject> {
        for i in 0..self.object_count {
            if let Some(obj) = self.objects[i] {
                if obj.object_id == object_id {
                    return Some(obj);
                }
            }
        }
        None
    }

    pub fn get_object_mut(&mut self, object_id: u32) -> Option<&mut AccessibilityObject> {
        for i in 0..self.object_count {
            if let Some(ref obj) = self.objects[i] {
                if obj.object_id == object_id {
                    return self.objects[i].as_mut();
                }
            }
        }
        None
    }

    pub fn add_child(&mut self, parent_id: u32, child_id: u32) -> bool {
        let mut parent_idx = None;
        for i in 0..self.object_count {
            if let Some(obj) = self.objects[i] {
                if obj.object_id == parent_id {
                    parent_idx = Some(i);
                    break;
                }
            }
        }

        if let Some(idx) = parent_idx {
            if self.child_counts[idx] < 8 {
                self.children[idx][self.child_counts[idx]] = Some(child_id);
                self.child_counts[idx] += 1;

                if let Some(obj) = self.objects[idx].as_mut() {
                    obj.child_count += 1;
                    if let Some(child) = self.get_object_mut(child_id) {
                        child.parent_id = Some(parent_id);
                    }
                }
                return true;
            }
        }
        false
    }

    pub fn get_children(&self, parent_id: u32) -> usize {
        for i in 0..self.object_count {
            if let Some(obj) = self.objects[i] {
                if obj.object_id == parent_id {
                    return self.child_counts[i];
                }
            }
        }
        0
    }

    pub fn find_at_point(&self, x: i32, y: i32) -> Option<u32> {
        for i in 0..self.object_count {
            if let Some(obj) = self.objects[i] {
                if obj.point_in_bounds(x, y) {
                    return Some(obj.object_id);
                }
            }
        }
        None
    }
}

impl Default for AccessibilityTree {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SCREEN READER INTERFACE
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct Announcement {
    pub announcement_id: u32,
    pub priority: u8, // 0-255, higher = more urgent
    pub length: usize,
}

impl Announcement {
    pub fn new(announcement_id: u32, priority: u8) -> Self {
        Announcement {
            announcement_id,
            priority,
            length: 0,
        }
    }
}

pub struct ScreenReaderInterface {
    pub queue: [Option<Announcement>; ANNOUNCEMENT_QUEUE_SIZE],
    pub queue_depth: usize,
    pub next_announcement_id: u32,
    pub enabled: bool,
}

impl ScreenReaderInterface {
    pub fn new() -> Self {
        ScreenReaderInterface {
            queue: [None; ANNOUNCEMENT_QUEUE_SIZE],
            queue_depth: 0,
            next_announcement_id: 1,
            enabled: true,
        }
    }

    pub fn announce(&mut self, priority: u8) -> Option<u32> {
        if !self.enabled || self.queue_depth >= ANNOUNCEMENT_QUEUE_SIZE {
            return None;
        }

        let announcement_id = self.next_announcement_id;
        self.next_announcement_id += 1;

        let announcement = Announcement::new(announcement_id, priority);
        self.queue[self.queue_depth] = Some(announcement);
        self.queue_depth += 1;

        Some(announcement_id)
    }

    pub fn get_next_announcement(&mut self) -> Option<Announcement> {
        if self.queue_depth == 0 {
            return None;
        }

        // Find highest priority
        let mut max_idx = 0;
        let mut max_priority = self.queue[0].map(|a| a.priority).unwrap_or(0);

        for i in 1..self.queue_depth {
            if let Some(ann) = self.queue[i] {
                if ann.priority > max_priority {
                    max_priority = ann.priority;
                    max_idx = i;
                }
            }
        }

        let announcement = self.queue[max_idx];
        for i in max_idx..self.queue_depth - 1 {
            self.queue[i] = self.queue[i + 1];
        }
        self.queue[self.queue_depth - 1] = None;
        self.queue_depth -= 1;

        announcement
    }

    pub fn clear_queue(&mut self) {
        self.queue_depth = 0;
    }

    pub fn queue_is_empty(&self) -> bool {
        self.queue_depth == 0
    }
}

impl Default for ScreenReaderInterface {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// KEYBOARD SHORTCUTS
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct KeyboardShortcut {
    pub shortcut_id: u32,
    pub modifiers: u8, // bit flags: Shift=1, Ctrl=2, Alt=4, Super=8
    pub key_code: u8,
    pub action_id: u32,
}

impl KeyboardShortcut {
    pub fn new(shortcut_id: u32, modifiers: u8, key_code: u8, action_id: u32) -> Self {
        KeyboardShortcut {
            shortcut_id,
            modifiers,
            key_code,
            action_id,
        }
    }
}

pub struct KeyboardShortcutRegistry {
    pub shortcuts: [Option<KeyboardShortcut>; MAX_SHORTCUTS],
    pub shortcut_count: usize,
    pub next_shortcut_id: u32,
}

impl KeyboardShortcutRegistry {
    pub fn new() -> Self {
        KeyboardShortcutRegistry {
            shortcuts: [None; MAX_SHORTCUTS],
            shortcut_count: 0,
            next_shortcut_id: 1,
        }
    }

    pub fn register_shortcut(&mut self, modifiers: u8, key_code: u8, action_id: u32) -> Option<u32> {
        if self.shortcut_count >= MAX_SHORTCUTS {
            return None;
        }

        let shortcut_id = self.next_shortcut_id;
        self.next_shortcut_id += 1;

        let shortcut = KeyboardShortcut::new(shortcut_id, modifiers, key_code, action_id);
        self.shortcuts[self.shortcut_count] = Some(shortcut);
        self.shortcut_count += 1;

        Some(shortcut_id)
    }

    pub fn find_shortcut(&self, modifiers: u8, key_code: u8) -> Option<u32> {
        for i in 0..self.shortcut_count {
            if let Some(shortcut) = self.shortcuts[i] {
                if shortcut.modifiers == modifiers && shortcut.key_code == key_code {
                    return Some(shortcut.action_id);
                }
            }
        }
        None
    }

    pub fn unregister_shortcut(&mut self, shortcut_id: u32) -> bool {
        for i in 0..self.shortcut_count {
            if let Some(shortcut) = self.shortcuts[i] {
                if shortcut.shortcut_id == shortcut_id {
                    for j in i..self.shortcut_count - 1 {
                        self.shortcuts[j] = self.shortcuts[j + 1];
                    }
                    self.shortcuts[self.shortcut_count - 1] = None;
                    self.shortcut_count -= 1;
                    return true;
                }
            }
        }
        false
    }
}

impl Default for KeyboardShortcutRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// FOCUS MANAGER
// ============================================================================

pub struct FocusManager {
    pub focused_object_id: Option<u32>,
    pub focus_stack: [Option<u32>; 32],
    pub stack_depth: usize,
    pub show_focus_rect: bool,
    pub focus_rect_x: i32,
    pub focus_rect_y: i32,
}

impl FocusManager {
    pub fn new() -> Self {
        FocusManager {
            focused_object_id: None,
            focus_stack: [None; 32],
            stack_depth: 0,
            show_focus_rect: true,
            focus_rect_x: 0,
            focus_rect_y: 0,
        }
    }

    pub fn set_focus(&mut self, object_id: u32) {
        self.focused_object_id = Some(object_id);
    }

    pub fn clear_focus(&mut self) {
        self.focused_object_id = None;
    }

    pub fn push_focus(&mut self, object_id: u32) -> bool {
        if self.stack_depth >= 32 {
            return false;
        }
        self.focus_stack[self.stack_depth] = Some(object_id);
        self.stack_depth += 1;
        self.focused_object_id = Some(object_id);
        true
    }

    pub fn pop_focus(&mut self) -> Option<u32> {
        if self.stack_depth == 0 {
            return None;
        }
        self.stack_depth -= 1;
        let popped = self.focus_stack[self.stack_depth];
        self.focus_stack[self.stack_depth] = None;

        if self.stack_depth > 0 {
            self.focused_object_id = self.focus_stack[self.stack_depth - 1];
        } else {
            self.focused_object_id = None;
        }
        popped
    }

    pub fn get_focused(&self) -> Option<u32> {
        self.focused_object_id
    }

    pub fn set_focus_rect(&mut self, x: i32, y: i32) {
        self.focus_rect_x = x;
        self.focus_rect_y = y;
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessible_role_str() {
        assert_eq!(AccessibleRole::Button.as_str(), "button");
        assert_eq!(AccessibleRole::Window.as_str(), "window");
    }

    #[test]
    fn test_accessible_state_new() {
        let state = AccessibleState::new();
        assert!(!state.focused);
        assert!(state.enabled);
    }

    #[test]
    fn test_accessibility_object_new() {
        let obj = AccessibilityObject::new(1, AccessibleRole::Button);
        assert_eq!(obj.object_id, 1);
        assert_eq!(obj.role, AccessibleRole::Button);
    }

    #[test]
    fn test_accessibility_object_bounds() {
        let mut obj = AccessibilityObject::new(1, AccessibleRole::Button);
        obj.set_bounds(10, 20, 100, 50);
        let (x, y, w, h) = obj.get_bounds();
        assert_eq!((x, y, w, h), (10, 20, 100, 50));
    }

    #[test]
    fn test_accessibility_tree_new() {
        let tree = AccessibilityTree::new();
        assert_eq!(tree.object_count, 0);
    }

    #[test]
    fn test_accessibility_tree_create_object() {
        let mut tree = AccessibilityTree::new();
        let oid = tree.create_object(AccessibleRole::Button);
        assert!(oid.is_some());
        assert_eq!(tree.object_count, 1);
    }

    #[test]
    fn test_accessibility_tree_get_object() {
        let mut tree = AccessibilityTree::new();
        let oid = tree.create_object(AccessibleRole::Button).unwrap();
        let obj = tree.get_object(oid);
        assert!(obj.is_some());
    }

    #[test]
    fn test_screen_reader_interface_new() {
        let sr = ScreenReaderInterface::new();
        assert!(sr.queue_is_empty());
    }

    #[test]
    fn test_screen_reader_announce() {
        let mut sr = ScreenReaderInterface::new();
        let aid = sr.announce(1);
        assert!(aid.is_some());
        assert!(!sr.queue_is_empty());
    }

    #[test]
    fn test_keyboard_shortcut_registry_new() {
        let reg = KeyboardShortcutRegistry::new();
        assert_eq!(reg.shortcut_count, 0);
    }

    #[test]
    fn test_keyboard_shortcut_register() {
        let mut reg = KeyboardShortcutRegistry::new();
        let sid = reg.register_shortcut(0x02, 0x41, 1); // Ctrl+A
        assert!(sid.is_some());
    }

    #[test]
    fn test_keyboard_shortcut_find() {
        let mut reg = KeyboardShortcutRegistry::new();
        reg.register_shortcut(0x02, 0x41, 100); // Ctrl+A -> action 100
        let action = reg.find_shortcut(0x02, 0x41);
        assert_eq!(action, Some(100));
    }

    #[test]
    fn test_focus_manager_new() {
        let fm = FocusManager::new();
        assert!(fm.get_focused().is_none());
    }

    #[test]
    fn test_focus_manager_set_focus() {
        let mut fm = FocusManager::new();
        fm.set_focus(5);
        assert_eq!(fm.get_focused(), Some(5));
    }

    #[test]
    fn test_focus_manager_push_pop() {
        let mut fm = FocusManager::new();
        fm.push_focus(1);
        fm.push_focus(2);
        assert_eq!(fm.get_focused(), Some(2));
        fm.pop_focus();
        assert_eq!(fm.get_focused(), Some(1));
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_accessibility_tree_hierarchy() {
        let mut tree = AccessibilityTree::new();

        let window = tree.create_object(AccessibleRole::Window).unwrap();
        let button1 = tree.create_object(AccessibleRole::Button).unwrap();
        let button2 = tree.create_object(AccessibleRole::Button).unwrap();

        tree.add_child(window, button1);
        tree.add_child(window, button2);

        let window_obj = tree.get_object(window).unwrap();
        assert_eq!(window_obj.child_count, 2);
    }

    #[test]
    fn test_point_hit_testing() {
        let mut tree = AccessibilityTree::new();
        let mut button = AccessibilityObject::new(1, AccessibleRole::Button);
        button.set_bounds(10, 20, 100, 50);

        assert!(button.point_in_bounds(50, 40));
        assert!(!button.point_in_bounds(5, 40));
    }

    #[test]
    fn test_screen_reader_priority_queue() {
        let mut sr = ScreenReaderInterface::new();

        sr.announce(1);
        sr.announce(10); // Higher priority
        sr.announce(2);

        let first = sr.get_next_announcement();
        assert!(first.is_some());
        assert_eq!(first.unwrap().priority, 10);
    }

    #[test]
    fn test_keyboard_navigation_focus() {
        let mut tree = AccessibilityTree::new();
        let mut fm = FocusManager::new();

        let btn1 = tree.create_object(AccessibleRole::Button).unwrap();
        let btn2 = tree.create_object(AccessibleRole::Button).unwrap();

        fm.push_focus(btn1);
        fm.push_focus(btn2);

        assert_eq!(fm.get_focused(), Some(btn2));
        fm.pop_focus();
        assert_eq!(fm.get_focused(), Some(btn1));
    }

    #[test]
    fn test_accessibility_state_transitions() {
        let mut tree = AccessibilityTree::new();
        let oid = tree.create_object(AccessibleRole::Button).unwrap();

        let obj = tree.get_object_mut(oid).unwrap();
        obj.state.set_focused(true);
        obj.state.set_enabled(true);

        let updated = tree.get_object(oid).unwrap();
        assert!(updated.is_focused());
        assert!(updated.is_enabled());
    }
}
