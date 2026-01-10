//! Example Apps for RayOS
//!
//! These example apps demonstrate how to use the App SDK.

use super::app_sdk::{App, AppContext, AppDescriptor, AppEvent, MouseButton};

// ============================================================================
// Counter App - Simple state management example
// ============================================================================

/// A simple counter app demonstrating state and click handling.
pub struct CounterApp {
    count: i32,
    button_hovered: bool,
    button_pressed: bool,
}

impl CounterApp {
    pub const DESCRIPTOR: AppDescriptor = AppDescriptor::new(b"Counter", b"1.0.0")
        .with_author(b"RayOS Team")
        .with_description(b"A simple counter app demonstrating the App SDK")
        .with_app_id(b"com.rayos.counter")
        .with_size(300, 200)
        .with_min_size(200, 150);

    pub fn new() -> Self {
        Self {
            count: 0,
            button_hovered: false,
            button_pressed: false,
        }
    }
}

impl App for CounterApp {
    fn descriptor() -> AppDescriptor {
        Self::DESCRIPTOR
    }

    fn on_init(&mut self, _ctx: &mut AppContext) {
        // Nothing to initialize
    }

    fn on_frame(&mut self, ctx: &mut AppContext) {
        // Clear background
        ctx.clear(0x2A2A4E);

        // Draw title
        ctx.draw_text(20, 20, b"Counter App", 0xFFFFFF);

        // Draw count value
        let mut count_buf = [0u8; 32];
        let count_str = format_count(self.count, &mut count_buf);
        ctx.draw_text(120, 80, count_str, 0x88CCFF);

        // Draw button
        let btn_color = if self.button_pressed {
            0x224488
        } else if self.button_hovered {
            0x446699
        } else {
            0x335577
        };

        ctx.fill_rect(100, 120, 100, 40, btn_color);
        ctx.draw_rect(100, 120, 100, 40, 0x6688AA);
        ctx.draw_text(120, 132, b"Increment", 0xFFFFFF);

        // Draw instructions
        ctx.draw_text(20, 170, b"Click button to increment", 0x888888);
    }

    fn on_event(&mut self, ctx: &mut AppContext, event: AppEvent) {
        match event {
            AppEvent::MouseMove { x, y } => {
                self.button_hovered = ctx.point_in_rect(x, y, 100, 120, 100, 40);
            }
            AppEvent::MouseDown { x, y, button: MouseButton::Left } => {
                if ctx.point_in_rect(x, y, 100, 120, 100, 40) {
                    self.button_pressed = true;
                }
            }
            AppEvent::MouseUp { x, y, button: MouseButton::Left } => {
                if self.button_pressed && ctx.point_in_rect(x, y, 100, 120, 100, 40) {
                    self.count += 1;
                }
                self.button_pressed = false;
            }
            AppEvent::CloseRequested => {
                ctx.close();
            }
            _ => {}
        }
    }
}

fn format_count(n: i32, buf: &mut [u8]) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }

    let negative = n < 0;
    let mut val = if negative { -n as u32 } else { n as u32 };
    let _pos = 0;

    // Count digits
    let mut temp = val;
    let mut digits = 0;
    while temp > 0 {
        digits += 1;
        temp /= 10;
    }

    let start = if negative { 1 } else { 0 };
    let len = start + digits;

    if len > buf.len() {
        return b"???";
    }

    // Write digits in reverse
    for i in (start..len).rev() {
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
    }

    if negative {
        buf[0] = b'-';
    }

    &buf[..len]
}

// ============================================================================
// About App - Shows system information
// ============================================================================

/// An "About RayOS" app showing system information.
pub struct AboutApp;

impl AboutApp {
    pub const DESCRIPTOR: AppDescriptor = AppDescriptor::new(b"About RayOS", b"1.0.0")
        .with_author(b"RayOS Team")
        .with_description(b"System information and about dialog")
        .with_app_id(b"com.rayos.about")
        .with_size(350, 250)
        .with_min_size(300, 200);

    pub fn new() -> Self {
        Self
    }
}

impl App for AboutApp {
    fn descriptor() -> AppDescriptor {
        Self::DESCRIPTOR
    }

    fn on_init(&mut self, _ctx: &mut AppContext) {}

    fn on_frame(&mut self, ctx: &mut AppContext) {
        ctx.clear(0x252545);

        // Logo area
        ctx.fill_rect(20, 20, 64, 64, 0x4466AA);
        ctx.draw_text(28, 40, b"RayOS", 0xFFFFFF);

        // System info
        let mut y = 30;
        ctx.draw_text(100, y, b"RayOS", 0xFFFFFF);
        y += 20;
        ctx.draw_text(100, y, b"Version 0.1.0 (January 2026)", 0x88CCFF);
        y += 20;
        ctx.draw_text(100, y, b"A hypervisor-native OS", 0xAAAAAA);

        // Details section
        y = 110;
        ctx.draw_text(20, y, b"Architecture: x86_64", 0xCCCCCC);
        y += 18;
        ctx.draw_text(20, y, b"UI Framework: Native", 0xCCCCCC);
        y += 18;
        ctx.draw_text(20, y, b"Kernel: RayOS Kernel", 0xCCCCCC);
        y += 18;
        ctx.draw_text(20, y, b"VMM: Integrated Hypervisor", 0xCCCCCC);

        // Footer
        ctx.draw_text(20, 220, b"Copyright 2026 RayOS Project", 0x666666);
    }

    fn on_event(&mut self, ctx: &mut AppContext, event: AppEvent) {
        if let AppEvent::CloseRequested = event {
            ctx.close();
        }
    }
}

// ============================================================================
// Color Picker App - More complex UI example
// ============================================================================

/// A color picker/palette app.
pub struct ColorPickerApp {
    selected_r: u8,
    selected_g: u8,
    selected_b: u8,
    dragging_slider: Option<u8>, // 0=R, 1=G, 2=B
}

impl ColorPickerApp {
    pub const DESCRIPTOR: AppDescriptor = AppDescriptor::new(b"Color Picker", b"1.0.0")
        .with_author(b"RayOS Team")
        .with_description(b"Pick and preview colors")
        .with_app_id(b"com.rayos.colorpicker")
        .with_size(400, 300);

    pub fn new() -> Self {
        Self {
            selected_r: 128,
            selected_g: 128,
            selected_b: 255,
            dragging_slider: None,
        }
    }

    fn selected_color(&self) -> u32 {
        ((self.selected_r as u32) << 16)
            | ((self.selected_g as u32) << 8)
            | (self.selected_b as u32)
    }
}

impl App for ColorPickerApp {
    fn descriptor() -> AppDescriptor {
        Self::DESCRIPTOR
    }

    fn on_init(&mut self, _ctx: &mut AppContext) {}

    fn on_frame(&mut self, ctx: &mut AppContext) {
        ctx.clear(0x1E1E2E);

        // Title
        ctx.draw_text(20, 15, b"Color Picker", 0xFFFFFF);

        // Color preview
        ctx.fill_rect(20, 45, 120, 120, self.selected_color());
        ctx.draw_rect(20, 45, 120, 120, 0x888888);

        // RGB sliders
        let slider_x = 160;
        let slider_w = 200u32;

        // Red slider
        ctx.draw_text(slider_x, 50, b"R:", 0xFF6666);
        ctx.fill_rect(slider_x + 25, 50, slider_w, 20, 0x331111);
        let r_pos = (self.selected_r as u32 * slider_w) / 255;
        ctx.fill_rect(slider_x + 25, 50, r_pos, 20, 0xFF4444);

        // Green slider
        ctx.draw_text(slider_x, 80, b"G:", 0x66FF66);
        ctx.fill_rect(slider_x + 25, 80, slider_w, 20, 0x113311);
        let g_pos = (self.selected_g as u32 * slider_w) / 255;
        ctx.fill_rect(slider_x + 25, 80, g_pos, 20, 0x44FF44);

        // Blue slider
        ctx.draw_text(slider_x, 110, b"B:", 0x6666FF);
        ctx.fill_rect(slider_x + 25, 110, slider_w, 20, 0x111133);
        let b_pos = (self.selected_b as u32 * slider_w) / 255;
        ctx.fill_rect(slider_x + 25, 110, b_pos, 20, 0x4444FF);

        // Hex value
        ctx.draw_text(20, 180, b"Hex:", 0xCCCCCC);
        let mut hex_buf = [0u8; 8];
        hex_buf[0] = b'#';
        hex_buf[1] = hex_digit(self.selected_r >> 4);
        hex_buf[2] = hex_digit(self.selected_r & 0xF);
        hex_buf[3] = hex_digit(self.selected_g >> 4);
        hex_buf[4] = hex_digit(self.selected_g & 0xF);
        hex_buf[5] = hex_digit(self.selected_b >> 4);
        hex_buf[6] = hex_digit(self.selected_b & 0xF);
        ctx.draw_text(60, 180, &hex_buf[..7], 0xFFFFFF);

        // Instructions
        ctx.draw_text(20, 270, b"Drag sliders to adjust color", 0x666666);
    }

    fn on_event(&mut self, ctx: &mut AppContext, event: AppEvent) {
        match event {
            AppEvent::MouseDown { x, y, button: MouseButton::Left } => {
                let slider_x = 185;
                let slider_w = 200;
                if x >= slider_x && x < slider_x + slider_w {
                    if y >= 50 && y < 70 {
                        self.dragging_slider = Some(0);
                        self.selected_r = (((x - slider_x) * 255) / slider_w) as u8;
                    } else if y >= 80 && y < 100 {
                        self.dragging_slider = Some(1);
                        self.selected_g = (((x - slider_x) * 255) / slider_w) as u8;
                    } else if y >= 110 && y < 130 {
                        self.dragging_slider = Some(2);
                        self.selected_b = (((x - slider_x) * 255) / slider_w) as u8;
                    }
                }
            }
            AppEvent::MouseMove { x, .. } => {
                if let Some(slider) = self.dragging_slider {
                    let slider_x = 185i32;
                    let slider_w = 200i32;
                    let val = ((x.max(slider_x).min(slider_x + slider_w) - slider_x) * 255 / slider_w) as u8;
                    match slider {
                        0 => self.selected_r = val,
                        1 => self.selected_g = val,
                        2 => self.selected_b = val,
                        _ => {}
                    }
                }
            }
            AppEvent::MouseUp { .. } => {
                self.dragging_slider = None;
            }
            AppEvent::CloseRequested => {
                ctx.close();
            }
            _ => {}
        }
    }
}

fn hex_digit(n: u8) -> u8 {
    if n < 10 {
        b'0' + n
    } else {
        b'A' + (n - 10)
    }
}
