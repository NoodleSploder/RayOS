// ===== Phase 23 Task 3: Wayland Shell Protocol =====
// Implements xdg-shell for window management
// Provides XDG WM Base, Surface, Toplevel, Popup, and Server Decorations


// Shell object limits
const MAX_XDG_SURFACES: usize = 32;
const MAX_TOPLEVELS: usize = 32;
const MAX_POPUPS: usize = 16;
const MAX_DECORATIONS: usize = 32;

// Window state flags
const WINDOW_STATE_MAXIMIZED: u32 = 0x01;
const WINDOW_STATE_FULLSCREEN: u32 = 0x02;
const WINDOW_STATE_ACTIVATED: u32 = 0x04;
const WINDOW_STATE_RESIZING: u32 = 0x08;
const WINDOW_STATE_TILED_LEFT: u32 = 0x10;
const WINDOW_STATE_TILED_RIGHT: u32 = 0x20;

// Decoration modes
const DECORATION_MODE_CLIENT: u32 = 1;
const DECORATION_MODE_SERVER: u32 = 2;

// Popup positioning
#[derive(Clone, Copy)]
pub struct PopupPosition {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl PopupPosition {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        PopupPosition { x, y, width, height }
    }

    pub fn get_x(&self) -> i32 {
        self.x
    }

    pub fn get_y(&self) -> i32 {
        self.y
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }

    pub fn get_height(&self) -> u32 {
        self.height
    }
}

/// Server Decoration
#[derive(Clone, Copy)]
pub struct ServerDecoration {
    id: u32,
    toplevel_id: u32,
    mode: u32,
    in_use: bool,
}

impl ServerDecoration {
    const UNINIT: Self = ServerDecoration {
        id: 0,
        toplevel_id: 0,
        mode: 0,
        in_use: false,
    };

    fn new(id: u32, toplevel_id: u32) -> Self {
        ServerDecoration {
            id,
            toplevel_id,
            mode: DECORATION_MODE_SERVER,
            in_use: true,
        }
    }

    pub fn set_mode(&mut self, mode: u32) {
        if mode == DECORATION_MODE_CLIENT || mode == DECORATION_MODE_SERVER {
            self.mode = mode;

            unsafe {
                if let Some(_) = core::fmt::write(
                    &mut Logger,
                    format_args!("[RAYOS_SHELL:DECORATION_MODE_SET] decoration_id={} mode={}\n",
                        self.id, mode)
                ).ok() {
                    // Marker emitted
                }
            }
        }
    }

    pub fn get_mode(&self) -> u32 {
        self.mode
    }
}

/// XDG Popup
#[derive(Clone, Copy)]
pub struct XdgPopup {
    id: u32,
    parent_surface_id: u32,
    position: PopupPosition,
    in_use: bool,
    grabbed: bool,
}

impl XdgPopup {
    const UNINIT: Self = XdgPopup {
        id: 0,
        parent_surface_id: 0,
        position: PopupPosition {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        in_use: false,
        grabbed: false,
    };

    fn new(id: u32, parent_surface_id: u32, x: i32, y: i32, width: u32, height: u32) -> Self {
        XdgPopup {
            id,
            parent_surface_id,
            position: PopupPosition::new(x, y, width, height),
            in_use: true,
            grabbed: false,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_position(&self) -> &PopupPosition {
        &self.position
    }

    pub fn grab(&mut self) {
        self.grabbed = true;
    }

    pub fn dismiss(&mut self) {
        self.in_use = false;
        self.grabbed = false;
    }

    pub fn reposition(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.position = PopupPosition::new(x, y, width, height);
    }

    pub fn is_grabbed(&self) -> bool {
        self.grabbed
    }
}

/// XDG Toplevel (Window)
#[derive(Clone, Copy)]
pub struct XdgToplevel {
    id: u32,
    surface_id: u32,
    title: [u8; 64],
    title_len: usize,
    app_id: [u8; 64],
    app_id_len: usize,
    state_flags: u32,
    width: u32,
    height: u32,
    min_width: u32,
    min_height: u32,
    max_width: u32,
    max_height: u32,
    in_use: bool,
}

impl XdgToplevel {
    const UNINIT: Self = XdgToplevel {
        id: 0,
        surface_id: 0,
        title: [0u8; 64],
        title_len: 0,
        app_id: [0u8; 64],
        app_id_len: 0,
        state_flags: WINDOW_STATE_ACTIVATED,
        width: 640,
        height: 480,
        min_width: 320,
        min_height: 240,
        max_width: 1920,
        max_height: 1080,
        in_use: false,
    };

    fn new(id: u32, surface_id: u32) -> Self {
        XdgToplevel {
            id,
            surface_id,
            title: [0u8; 64],
            title_len: 0,
            app_id: [0u8; 64],
            app_id_len: 0,
            state_flags: WINDOW_STATE_ACTIVATED,
            width: 640,
            height: 480,
            min_width: 320,
            min_height: 240,
            max_width: 1920,
            max_height: 1080,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_surface_id(&self) -> u32 {
        self.surface_id
    }

    pub fn set_title(&mut self, title: &[u8]) -> Result<(), &'static str> {
        let len = title.len().min(63);
        self.title[..len].copy_from_slice(&title[..len]);
        self.title_len = len;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SHELL:TITLE_SET] toplevel_id={} title_len={}\n",
                    self.id, len)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn set_app_id(&mut self, app_id: &[u8]) -> Result<(), &'static str> {
        let len = app_id.len().min(63);
        self.app_id[..len].copy_from_slice(&app_id[..len]);
        self.app_id_len = len;
        Ok(())
    }

    pub fn get_title(&self) -> &[u8] {
        &self.title[..self.title_len]
    }

    pub fn get_app_id(&self) -> &[u8] {
        &self.app_id[..self.app_id_len]
    }

    pub fn set_maximized(&mut self) -> Result<(), &'static str> {
        self.state_flags |= WINDOW_STATE_MAXIMIZED;
        self.emit_state_change();
        Ok(())
    }

    pub fn unset_maximized(&mut self) -> Result<(), &'static str> {
        self.state_flags &= !WINDOW_STATE_MAXIMIZED;
        self.emit_state_change();
        Ok(())
    }

    pub fn set_fullscreen(&mut self) -> Result<(), &'static str> {
        self.state_flags |= WINDOW_STATE_FULLSCREEN;
        self.emit_state_change();
        Ok(())
    }

    pub fn unset_fullscreen(&mut self) -> Result<(), &'static str> {
        self.state_flags &= !WINDOW_STATE_FULLSCREEN;
        self.emit_state_change();
        Ok(())
    }

    pub fn set_activated(&mut self, active: bool) {
        if active {
            self.state_flags |= WINDOW_STATE_ACTIVATED;
        } else {
            self.state_flags &= !WINDOW_STATE_ACTIVATED;
        }
        self.emit_state_change();
    }

    fn emit_state_change(&self) {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SHELL:STATE_CHANGE] toplevel_id={} state_flags={}\n",
                    self.id, self.state_flags)
            ).ok() {
                // Marker emitted
            }
        }
    }

    pub fn request_move(&mut self) -> Result<(), &'static str> {
        self.state_flags |= WINDOW_STATE_RESIZING;
        Ok(())
    }

    pub fn request_resize(&mut self, _edges: u32) -> Result<(), &'static str> {
        self.state_flags |= WINDOW_STATE_RESIZING;
        Ok(())
    }

    pub fn show_window_menu(&self) -> Result<(), &'static str> {
        Ok(())
    }

    pub fn is_maximized(&self) -> bool {
        (self.state_flags & WINDOW_STATE_MAXIMIZED) != 0
    }

    pub fn is_fullscreen(&self) -> bool {
        (self.state_flags & WINDOW_STATE_FULLSCREEN) != 0
    }

    pub fn is_activated(&self) -> bool {
        (self.state_flags & WINDOW_STATE_ACTIVATED) != 0
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    pub fn get_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn set_min_size(&mut self, width: u32, height: u32) {
        self.min_width = width;
        self.min_height = height;
    }

    pub fn set_max_size(&mut self, width: u32, height: u32) {
        self.max_width = width;
        self.max_height = height;
    }

    pub fn get_min_size(&self) -> (u32, u32) {
        (self.min_width, self.min_height)
    }

    pub fn get_max_size(&self) -> (u32, u32) {
        (self.max_width, self.max_height)
    }
}

/// XDG Surface
#[derive(Clone, Copy)]
pub struct XdgSurface {
    id: u32,
    surface_id: u32,
    role: u32, // 0=unassigned, 1=toplevel, 2=popup
    toplevel_id: Option<u32>,
    popup_id: Option<u32>,
    serial: u32,
    in_use: bool,
}

impl XdgSurface {
    const UNINIT: Self = XdgSurface {
        id: 0,
        surface_id: 0,
        role: 0,
        toplevel_id: None,
        popup_id: None,
        serial: 0,
        in_use: false,
    };

    fn new(id: u32, surface_id: u32) -> Self {
        XdgSurface {
            id,
            surface_id,
            role: 0,
            toplevel_id: None,
            popup_id: None,
            serial: 0,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_surface_id(&self) -> u32 {
        self.surface_id
    }

    pub fn set_toplevel(&mut self, toplevel_id: u32) -> Result<(), &'static str> {
        if self.role != 0 {
            return Err("surface role already set");
        }
        self.role = 1;
        self.toplevel_id = Some(toplevel_id);

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SHELL:XDG_TOPLEVEL_CREATE] toplevel_id={}\n", toplevel_id)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn set_popup(&mut self, popup_id: u32) -> Result<(), &'static str> {
        if self.role != 0 {
            return Err("surface role already set");
        }
        self.role = 2;
        self.popup_id = Some(popup_id);

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SHELL:POPUP_CREATE] popup_id={}\n", popup_id)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn ack_configure(&mut self, serial: u32) -> Result<(), &'static str> {
        self.serial = serial;
        Ok(())
    }

    pub fn get_toplevel_id(&self) -> Option<u32> {
        self.toplevel_id
    }

    pub fn get_popup_id(&self) -> Option<u32> {
        self.popup_id
    }

    pub fn get_role(&self) -> u32 {
        self.role
    }
}

/// XDG WM Base (Shell Manager)
pub struct XdgWmBase {
    id: u32,
    surfaces: [XdgSurface; MAX_XDG_SURFACES],
    surface_count: usize,
    toplevels: [XdgToplevel; MAX_TOPLEVELS],
    toplevel_count: usize,
    popups: [XdgPopup; MAX_POPUPS],
    popup_count: usize,
    decorations: [ServerDecoration; MAX_DECORATIONS],
    decoration_count: usize,
    next_surface_id: u32,
    next_toplevel_id: u32,
    next_popup_id: u32,
    next_decoration_id: u32,
    ping_serial: u32,
}

impl XdgWmBase {
    pub fn new() -> Self {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SHELL:XDG_WM_BASE_ADVERTISED] interface=xdg_wm_base version=4\n")
            ).ok() {
                // Marker emitted
            }
        }

        XdgWmBase {
            id: 1,
            surfaces: [XdgSurface::UNINIT; MAX_XDG_SURFACES],
            surface_count: 0,
            toplevels: [XdgToplevel::UNINIT; MAX_TOPLEVELS],
            toplevel_count: 0,
            popups: [XdgPopup::UNINIT; MAX_POPUPS],
            popup_count: 0,
            decorations: [ServerDecoration::UNINIT; MAX_DECORATIONS],
            decoration_count: 0,
            next_surface_id: 10,
            next_toplevel_id: 100,
            next_popup_id: 200,
            next_decoration_id: 300,
            ping_serial: 1,
        }
    }

    pub fn create_xdg_surface(&mut self, surface_id: u32) -> Result<u32, &'static str> {
        if self.surface_count >= MAX_XDG_SURFACES {
            return Err("surface limit exceeded");
        }

        let xdg_surface_id = self.next_surface_id;
        self.next_surface_id += 1;

        let xdg_surface = XdgSurface::new(xdg_surface_id, surface_id);
        self.surfaces[self.surface_count] = xdg_surface;
        self.surface_count += 1;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SHELL:XDG_SURFACE_CREATE] xdg_surface_id={}\n", xdg_surface_id)
            ).ok() {
                // Marker emitted
            }
        }

        Ok(xdg_surface_id)
    }

    pub fn create_toplevel(&mut self, xdg_surface_id: u32) -> Result<u32, &'static str> {
        if self.toplevel_count >= MAX_TOPLEVELS {
            return Err("toplevel limit exceeded");
        }

        let toplevel_id = self.next_toplevel_id;
        self.next_toplevel_id += 1;

        let toplevel = XdgToplevel::new(toplevel_id, xdg_surface_id);
        self.toplevels[self.toplevel_count] = toplevel;
        self.toplevel_count += 1;

        // Associate with xdg_surface
        for surface in self.surfaces[..self.surface_count].iter_mut() {
            if surface.in_use && surface.id == xdg_surface_id {
                surface.set_toplevel(toplevel_id)?;
                break;
            }
        }

        Ok(toplevel_id)
    }

    pub fn create_popup(&mut self, xdg_surface_id: u32, parent_id: u32, x: i32, y: i32, width: u32, height: u32) -> Result<u32, &'static str> {
        if self.popup_count >= MAX_POPUPS {
            return Err("popup limit exceeded");
        }

        let popup_id = self.next_popup_id;
        self.next_popup_id += 1;

        let popup = XdgPopup::new(popup_id, parent_id, x, y, width, height);
        self.popups[self.popup_count] = popup;
        self.popup_count += 1;

        // Associate with xdg_surface
        for surface in self.surfaces[..self.surface_count].iter_mut() {
            if surface.in_use && surface.id == xdg_surface_id {
                surface.set_popup(popup_id)?;
                break;
            }
        }

        Ok(popup_id)
    }

    pub fn get_toplevel_decoration(&mut self, toplevel_id: u32) -> Result<u32, &'static str> {
        if self.decoration_count >= MAX_DECORATIONS {
            return Err("decoration limit exceeded");
        }

        let decoration_id = self.next_decoration_id;
        self.next_decoration_id += 1;

        let decoration = ServerDecoration::new(decoration_id, toplevel_id);
        self.decorations[self.decoration_count] = decoration;
        self.decoration_count += 1;

        Ok(decoration_id)
    }

    pub fn ping(&mut self, _client_id: u32) -> u32 {
        let serial = self.ping_serial;
        self.ping_serial += 1;
        serial
    }

    pub fn pong(&self, serial: u32) -> bool {
        serial < self.ping_serial
    }

    pub fn get_toplevel(&self, toplevel_id: u32) -> Option<&XdgToplevel> {
        self.toplevels[..self.toplevel_count]
            .iter()
            .find(|t| t.in_use && t.id == toplevel_id)
    }

    pub fn get_toplevel_mut(&mut self, toplevel_id: u32) -> Option<&mut XdgToplevel> {
        self.toplevels[..self.toplevel_count]
            .iter_mut()
            .find(|t| t.in_use && t.id == toplevel_id)
    }

    pub fn get_popup(&self, popup_id: u32) -> Option<&XdgPopup> {
        self.popups[..self.popup_count]
            .iter()
            .find(|p| p.in_use && p.id == popup_id)
    }

    pub fn get_popup_mut(&mut self, popup_id: u32) -> Option<&mut XdgPopup> {
        self.popups[..self.popup_count]
            .iter_mut()
            .find(|p| p.in_use && p.id == popup_id)
    }

    pub fn get_xdg_surface(&self, xdg_surface_id: u32) -> Option<&XdgSurface> {
        self.surfaces[..self.surface_count]
            .iter()
            .find(|s| s.in_use && s.id == xdg_surface_id)
    }

    pub fn get_xdg_surface_mut(&mut self, xdg_surface_id: u32) -> Option<&mut XdgSurface> {
        self.surfaces[..self.surface_count]
            .iter_mut()
            .find(|s| s.in_use && s.id == xdg_surface_id)
    }

    pub fn get_surface_count(&self) -> usize {
        self.surfaces[..self.surface_count].iter().filter(|s| s.in_use).count()
    }

    pub fn get_toplevel_count(&self) -> usize {
        self.toplevels[..self.toplevel_count].iter().filter(|t| t.in_use).count()
    }

    pub fn get_popup_count(&self) -> usize {
        self.popups[..self.popup_count].iter().filter(|p| p.in_use).count()
    }
}

// Simple logging helper
struct Logger;

impl core::fmt::Write for Logger {
    fn write_str(&mut self, _s: &str) -> core::fmt::Result {
        // In a real implementation, this would write to kernel log
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xdg_wm_base_creation() {
        let shell = XdgWmBase::new();
        assert_eq!(shell.id, 1);
        assert_eq!(shell.get_surface_count(), 0);
        assert_eq!(shell.get_toplevel_count(), 0);
    }

    #[test]
    fn test_xdg_surface_creation() {
        let mut shell = XdgWmBase::new();
        let result = shell.create_xdg_surface(1);
        assert!(result.is_ok());
        assert_eq!(shell.get_surface_count(), 1);
    }

    #[test]
    fn test_xdg_toplevel_creation() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let result = shell.create_toplevel(xdg_surface_id);
        assert!(result.is_ok());
        assert_eq!(shell.get_toplevel_count(), 1);
    }

    #[test]
    fn test_window_title_setting() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let toplevel_id = shell.create_toplevel(xdg_surface_id).unwrap();

        let toplevel = shell.get_toplevel_mut(toplevel_id).unwrap();
        let result = toplevel.set_title(b"Test Window");
        assert!(result.is_ok());
        assert_eq!(toplevel.get_title(), b"Test Window");
    }

    #[test]
    fn test_app_id_setting() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let toplevel_id = shell.create_toplevel(xdg_surface_id).unwrap();

        let toplevel = shell.get_toplevel_mut(toplevel_id).unwrap();
        let result = toplevel.set_app_id(b"org.rayos.app");
        assert!(result.is_ok());
        assert_eq!(toplevel.get_app_id(), b"org.rayos.app");
    }

    #[test]
    fn test_maximize_state() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let toplevel_id = shell.create_toplevel(xdg_surface_id).unwrap();

        let toplevel = shell.get_toplevel_mut(toplevel_id).unwrap();
        assert!(!toplevel.is_maximized());
        toplevel.set_maximized().unwrap();
        assert!(toplevel.is_maximized());
        toplevel.unset_maximized().unwrap();
        assert!(!toplevel.is_maximized());
    }

    #[test]
    fn test_fullscreen_state() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let toplevel_id = shell.create_toplevel(xdg_surface_id).unwrap();

        let toplevel = shell.get_toplevel_mut(toplevel_id).unwrap();
        assert!(!toplevel.is_fullscreen());
        toplevel.set_fullscreen().unwrap();
        assert!(toplevel.is_fullscreen());
        toplevel.unset_fullscreen().unwrap();
        assert!(!toplevel.is_fullscreen());
    }

    #[test]
    fn test_activated_state() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let toplevel_id = shell.create_toplevel(xdg_surface_id).unwrap();

        let toplevel = shell.get_toplevel_mut(toplevel_id).unwrap();
        assert!(toplevel.is_activated());
        toplevel.set_activated(false);
        assert!(!toplevel.is_activated());
        toplevel.set_activated(true);
        assert!(toplevel.is_activated());
    }

    #[test]
    fn test_window_move_request() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let toplevel_id = shell.create_toplevel(xdg_surface_id).unwrap();

        let toplevel = shell.get_toplevel_mut(toplevel_id).unwrap();
        let result = toplevel.request_move();
        assert!(result.is_ok());
    }

    #[test]
    fn test_window_resize_request() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let toplevel_id = shell.create_toplevel(xdg_surface_id).unwrap();

        let toplevel = shell.get_toplevel_mut(toplevel_id).unwrap();
        let result = toplevel.request_resize(0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_xdg_popup_creation() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let parent_id = shell.create_toplevel(xdg_surface_id).unwrap();
        let popup_surface_id = shell.create_xdg_surface(2).unwrap();

        let result = shell.create_popup(popup_surface_id, parent_id, 100, 100, 200, 150);
        assert!(result.is_ok());
        assert_eq!(shell.get_popup_count(), 1);
    }

    #[test]
    fn test_server_decorations() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let toplevel_id = shell.create_toplevel(xdg_surface_id).unwrap();

        let result = shell.get_toplevel_decoration(toplevel_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_shell_state_transitions() {
        let mut shell = XdgWmBase::new();
        let xdg_surface_id = shell.create_xdg_surface(1).unwrap();
        let toplevel_id = shell.create_toplevel(xdg_surface_id).unwrap();

        let toplevel = shell.get_toplevel_mut(toplevel_id).unwrap();

        // Start normal
        assert!(toplevel.is_activated());
        assert!(!toplevel.is_maximized());

        // Transition to maximized
        toplevel.set_maximized().unwrap();
        assert!(toplevel.is_maximized());

        // Transition to fullscreen
        toplevel.unset_maximized().unwrap();
        toplevel.set_fullscreen().unwrap();
        assert!(toplevel.is_fullscreen());
        assert!(!toplevel.is_maximized());

        // Transition back to normal
        toplevel.unset_fullscreen().unwrap();
        assert!(!toplevel.is_fullscreen());
    }
}
