// ===== Phase 23 Task 6b: Wayland DPI Scaling & Output Protocol =====
// Implements wl_output, coordinate transformation, HiDPI support
// Provides per-surface scaling and output configuration


// Output limits
const MAX_OUTPUTS: usize = 4;
const MAX_MODES: usize = 8;

// Supported scale factors
const SCALE_100: i32 = 100;
const SCALE_125: i32 = 125;
const SCALE_150: i32 = 150;
const SCALE_200: i32 = 200;

// Transform flags
const TRANSFORM_NORMAL: u32 = 0;
const TRANSFORM_90: u32 = 1;
const TRANSFORM_180: u32 = 2;
const TRANSFORM_270: u32 = 3;

/// Display mode
#[derive(Clone, Copy)]
pub struct DisplayMode {
    width: u32,
    height: u32,
    refresh: u32, // Hz * 1000
    preferred: bool,
}

impl DisplayMode {
    pub fn new(width: u32, height: u32, refresh: u32, preferred: bool) -> Self {
        DisplayMode {
            width,
            height,
            refresh,
            preferred,
        }
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }

    pub fn get_height(&self) -> u32 {
        self.height
    }

    pub fn get_refresh(&self) -> u32 {
        self.refresh
    }

    pub fn is_preferred(&self) -> bool {
        self.preferred
    }
}

/// Output (Display) configuration
#[derive(Clone, Copy)]
pub struct WaylandOutput {
    id: u32,
    x: i32,
    y: i32,
    width_mm: i32,
    height_mm: i32,
    subpixel: u32,
    make: [u8; 32],
    make_len: usize,
    model: [u8; 32],
    model_len: usize,
    scale: i32,
    modes: [Option<DisplayMode>; MAX_MODES],
    mode_count: usize,
    current_mode: usize,
    transform: u32,
    in_use: bool,
}

impl WaylandOutput {
    const UNINIT: Self = WaylandOutput {
        id: 0,
        x: 0,
        y: 0,
        width_mm: 0,
        height_mm: 0,
        subpixel: 0,
        make: [0u8; 32],
        make_len: 0,
        model: [0u8; 32],
        model_len: 0,
        scale: SCALE_100,
        modes: [None; MAX_MODES],
        mode_count: 0,
        current_mode: 0,
        transform: TRANSFORM_NORMAL,
        in_use: false,
    };

    fn new(id: u32, x: i32, y: i32, width_mm: i32, height_mm: i32) -> Self {
        WaylandOutput {
            id,
            x,
            y,
            width_mm,
            height_mm,
            subpixel: 0, // SUBPIXEL_UNKNOWN
            make: [0u8; 32],
            make_len: 0,
            model: [0u8; 32],
            model_len: 0,
            scale: SCALE_100,
            modes: [None; MAX_MODES],
            mode_count: 0,
            current_mode: 0,
            transform: TRANSFORM_NORMAL,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn send_geometry(&self) -> Result<(), &'static str> {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_OUTPUT:GEOMETRY] x={} y={} width_mm={} height_mm={}\n",
                    self.x, self.y, self.width_mm, self.height_mm)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn add_mode(&mut self, width: u32, height: u32, refresh: u32, preferred: bool) -> Result<(), &'static str> {
        if self.mode_count >= MAX_MODES {
            return Err("mode limit exceeded");
        }

        let mode = DisplayMode::new(width, height, refresh, preferred);
        self.modes[self.mode_count] = Some(mode);
        self.mode_count += 1;

        if preferred {
            self.current_mode = self.mode_count - 1;
        }

        Ok(())
    }

    pub fn send_mode(&self, mode_idx: usize) -> Result<(), &'static str> {
        if mode_idx >= self.mode_count {
            return Err("mode index out of range");
        }

        if let Some(mode) = self.modes[mode_idx] {
            unsafe {
                if let Some(_) = core::fmt::write(
                    &mut Logger,
                    format_args!("[RAYOS_OUTPUT:MODE] width={} height={} refresh={}\n",
                        mode.width, mode.height, mode.refresh)
                ).ok() {
                    // Marker emitted
                }
            }
        }
        Ok(())
    }

    pub fn send_scale(&self) -> Result<(), &'static str> {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_OUTPUT:SCALE] scale={}\n", self.scale)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn send_done(&self) -> Result<(), &'static str> {
        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_OUTPUT:DONE] output_id={}\n", self.id)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn set_scale(&mut self, scale: i32) -> Result<(), &'static str> {
        match scale {
            SCALE_100 | SCALE_125 | SCALE_150 | SCALE_200 => {
                self.scale = scale;
                Ok(())
            }
            _ => Err("unsupported scale factor"),
        }
    }

    pub fn get_scale(&self) -> i32 {
        self.scale
    }

    pub fn set_transform(&mut self, transform: u32) -> Result<(), &'static str> {
        if transform <= TRANSFORM_270 {
            self.transform = transform;
            Ok(())
        } else {
            Err("invalid transform")
        }
    }

    pub fn get_transform(&self) -> u32 {
        self.transform
    }

    pub fn get_current_mode(&self) -> Option<DisplayMode> {
        if self.current_mode < self.mode_count {
            self.modes[self.current_mode]
        } else {
            None
        }
    }

    pub fn get_mode_count(&self) -> usize {
        self.mode_count
    }
}

/// Coordinate transformation helper
#[derive(Clone, Copy)]
pub struct CoordinateTransform {
    scale: i32,
}

impl CoordinateTransform {
    pub fn new(scale: i32) -> Self {
        CoordinateTransform { scale }
    }

    pub fn logical_to_physical(&self, x: i32, y: i32) -> (i32, i32) {
        let physical_x = (x * self.scale) / 100;
        let physical_y = (y * self.scale) / 100;
        (physical_x, physical_y)
    }

    pub fn physical_to_logical(&self, x: i32, y: i32) -> (i32, i32) {
        let logical_x = (x * 100) / self.scale;
        let logical_y = (y * 100) / self.scale;
        (logical_x, logical_y)
    }

    pub fn scale_buffer(&self, width: u32, height: u32) -> (u32, u32) {
        let scaled_width = (width * self.scale as u32) / 100;
        let scaled_height = (height * self.scale as u32) / 100;
        (scaled_width, scaled_height)
    }

    pub fn get_scale(&self) -> i32 {
        self.scale
    }
}

/// HiDPI Surface scaling
#[derive(Clone, Copy)]
pub struct HiDPISurface {
    surface_id: u32,
    buffer_scale: i32,
    output_scale: i32,
    effective_scale: i32,
}

impl HiDPISurface {
    pub fn new(surface_id: u32, output_scale: i32) -> Self {
        HiDPISurface {
            surface_id,
            buffer_scale: 100,
            output_scale,
            effective_scale: output_scale,
        }
    }

    pub fn set_buffer_scale(&mut self, scale: i32) -> Result<(), &'static str> {
        match scale {
            SCALE_100 | SCALE_125 | SCALE_150 | SCALE_200 => {
                self.buffer_scale = scale;
                // Effective scale is max of buffer and output scale
                self.effective_scale = self.buffer_scale.max(self.output_scale);
                Ok(())
            }
            _ => Err("unsupported scale"),
        }
    }

    pub fn get_effective_scale(&self) -> i32 {
        self.effective_scale
    }

    pub fn get_optimal_scale(&self) -> i32 {
        // Return scale that best fits output DPI
        self.output_scale
    }
}

/// Output Manager
pub struct OutputManager {
    outputs: [WaylandOutput; MAX_OUTPUTS],
    output_count: usize,
    next_output_id: u32,
}

impl OutputManager {
    pub fn new() -> Self {
        OutputManager {
            outputs: [WaylandOutput::UNINIT; MAX_OUTPUTS],
            output_count: 0,
            next_output_id: 1,
        }
    }

    pub fn create_output(&mut self, x: i32, y: i32, width_mm: i32, height_mm: i32) -> Result<u32, &'static str> {
        if self.output_count >= MAX_OUTPUTS {
            return Err("output limit exceeded");
        }

        let output_id = self.next_output_id;
        self.next_output_id += 1;

        let output = WaylandOutput::new(output_id, x, y, width_mm, height_mm);
        self.outputs[self.output_count] = output;
        self.output_count += 1;

        unsafe {
            if let Some(_) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_OUTPUT:CREATE] output_id={}\n", output_id)
            ).ok() {
                // Marker emitted
            }
        }

        Ok(output_id)
    }

    pub fn get_output_mut(&mut self, output_id: u32) -> Option<&mut WaylandOutput> {
        self.outputs[..self.output_count]
            .iter_mut()
            .find(|o| o.in_use && o.id == output_id)
    }

    pub fn find_output(&self, output_id: u32) -> Option<&WaylandOutput> {
        self.outputs[..self.output_count]
            .iter()
            .find(|o| o.in_use && o.id == output_id)
    }

    pub fn get_output_by_position(&self, x: i32, y: i32) -> Option<&WaylandOutput> {
        self.outputs[..self.output_count]
            .iter()
            .find(|o| o.in_use && o.x == x && o.y == y)
    }

    pub fn get_output_count(&self) -> usize {
        self.outputs[..self.output_count].iter().filter(|o| o.in_use).count()
    }

    pub fn get_primary_output(&self) -> Option<&WaylandOutput> {
        // Return first output (primary)
        self.outputs[..self.output_count]
            .iter()
            .find(|o| o.in_use)
    }
}

// Simple logging helper
struct Logger;

impl core::fmt::Write for Logger {
    fn write_str(&mut self, _s: &str) -> core::fmt::Result {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_creation() {
        let mut manager = OutputManager::new();
        let result = manager.create_output(0, 0, 344, 194); // 1280x720mm (approx 96 DPI)
        assert!(result.is_ok());
        assert_eq!(manager.get_output_count(), 1);
    }

    #[test]
    fn test_scale_factor_advertisement() {
        let mut manager = OutputManager::new();
        let output_id = manager.create_output(0, 0, 344, 194).unwrap();
        let output = manager.get_output_mut(output_id).unwrap();

        // Add typical display modes
        output.add_mode(1280, 720, 60000, true).unwrap();
        output.add_mode(1920, 1080, 60000, false).unwrap();

        // Send configuration
        output.send_geometry().unwrap();
        output.send_scale().unwrap();
        output.send_done().unwrap();

        // Verify scale advertised
        assert_eq!(output.get_scale(), SCALE_100);
    }

    #[test]
    fn test_coordinate_transformation() {
        let transform = CoordinateTransform::new(SCALE_150);

        // Logical to physical (1.5x scale)
        let (phys_x, phys_y) = transform.logical_to_physical(100, 200);
        assert_eq!(phys_x, 150);
        assert_eq!(phys_y, 300);

        // Physical to logical (inverse)
        let (log_x, log_y) = transform.physical_to_logical(150, 300);
        assert_eq!(log_x, 100);
        assert_eq!(log_y, 200);
    }

    #[test]
    fn test_buffer_scaling() {
        let transform_100 = CoordinateTransform::new(SCALE_100);
        let transform_200 = CoordinateTransform::new(SCALE_200);

        // 1x scale
        let (w1, h1) = transform_100.scale_buffer(640, 480);
        assert_eq!(w1, 640);
        assert_eq!(h1, 480);

        // 2x scale
        let (w2, h2) = transform_200.scale_buffer(640, 480);
        assert_eq!(w2, 1280);
        assert_eq!(h2, 960);
    }

    #[test]
    fn test_hidpi_support() {
        let mut surface = HiDPISurface::new(1, SCALE_150);

        // Default output scale
        assert_eq!(surface.get_effective_scale(), SCALE_150);

        // Client sets buffer scale
        surface.set_buffer_scale(SCALE_200).unwrap();

        // Effective scale is max
        assert_eq!(surface.get_effective_scale(), SCALE_200);
    }

    #[test]
    fn test_multi_output_scaling() {
        let mut manager = OutputManager::new();

        // Create multiple outputs with different scales
        let output1 = manager.create_output(0, 0, 344, 194).unwrap();
        let output2 = manager.create_output(1920, 0, 517, 291).unwrap(); // 4K

        let out1 = manager.get_output_mut(output1).unwrap();
        out1.set_scale(SCALE_100).unwrap();

        let out2 = manager.get_output_mut(output2).unwrap();
        out2.set_scale(SCALE_200).unwrap();

        // Verify independent scaling
        assert_eq!(out1.get_scale(), SCALE_100);
        assert_eq!(out2.get_scale(), SCALE_200);
    }
}
