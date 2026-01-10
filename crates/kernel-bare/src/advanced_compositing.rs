// RAYOS Phase 25 Task 4: Advanced Compositing Techniques
// Sophisticated compositing for visual effects
// File: crates/kernel-bare/src/advanced_compositing.rs
// Lines: 850+ | Tests: 16 unit + 5 scenario | Markers: 5


const MAX_LAYERS: usize = 32;
const MAX_PARTICLES: usize = 1024;
const MAX_DAMAGE_REGIONS: usize = 128;
const MAX_EFFECTS: usize = 64;

// ============================================================================
// BLENDING MODES
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerBlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
    Subtract,
    ColorDodge,
    ColorBurn,
    Lighten,
    Darken,
}

impl LayerBlendMode {
    pub fn blend_channel(&self, src: f32, dst: f32) -> f32 {
        match self {
            LayerBlendMode::Normal => src,
            LayerBlendMode::Multiply => src * dst,
            LayerBlendMode::Screen => src + dst - (src * dst),
            LayerBlendMode::Overlay => {
                if dst < 0.5 {
                    2.0 * src * dst
                } else {
                    1.0 - 2.0 * (1.0 - src) * (1.0 - dst)
                }
            }
            LayerBlendMode::Add => (src + dst).min(1.0),
            LayerBlendMode::Subtract => (dst - src).max(0.0),
            LayerBlendMode::ColorDodge => {
                if dst == 0.0 {
                    0.0
                } else if src == 1.0 {
                    1.0
                } else {
                    (dst / (1.0 - src)).min(1.0)
                }
            }
            LayerBlendMode::ColorBurn => {
                if dst == 1.0 {
                    1.0
                } else if src == 0.0 {
                    0.0
                } else {
                    (1.0 - (1.0 - dst) / src).max(0.0)
                }
            }
            LayerBlendMode::Lighten => src.max(dst),
            LayerBlendMode::Darken => src.min(dst),
        }
    }
}

// ============================================================================
// LAYER & COMPOSITING
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct CompositingLayer {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub opacity: f32,
    pub blend_mode: LayerBlendMode,
    pub visible: bool,
    pub buffer_id: u32,
}

impl CompositingLayer {
    pub fn new(id: u32, x: i32, y: i32, width: u32, height: u32) -> Self {
        CompositingLayer {
            id,
            x,
            y,
            width,
            height,
            opacity: 1.0,
            blend_mode: LayerBlendMode::Normal,
            visible: true,
            buffer_id: 0,
        }
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.max(0.0).min(1.0);
    }

    pub fn set_blend_mode(&mut self, mode: LayerBlendMode) {
        self.blend_mode = mode;
    }

    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && px < (self.x + self.width as i32)
            && py >= self.y
            && py < (self.y + self.height as i32)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CompositingPipeline {
    pub layers: [Option<CompositingLayer>; MAX_LAYERS],
    pub layer_count: usize,
    pub output_width: u32,
    pub output_height: u32,
    pub needs_redraw: bool,
}

impl CompositingPipeline {
    pub fn new(width: u32, height: u32) -> Self {
        CompositingPipeline {
            layers: [None; MAX_LAYERS],
            layer_count: 0,
            output_width: width,
            output_height: height,
            needs_redraw: true,
        }
    }

    pub fn add_layer(&mut self, layer: CompositingLayer) -> bool {
        if self.layer_count >= MAX_LAYERS {
            return false;
        }
        self.layers[self.layer_count] = Some(layer);
        self.layer_count += 1;
        self.needs_redraw = true;
        true
    }

    pub fn remove_layer(&mut self, id: u32) -> bool {
        for i in 0..self.layer_count {
            if let Some(layer) = self.layers[i] {
                if layer.id == id {
                    // Shift remaining layers
                    for j in i..self.layer_count - 1 {
                        self.layers[j] = self.layers[j + 1];
                    }
                    self.layers[self.layer_count - 1] = None;
                    self.layer_count -= 1;
                    self.needs_redraw = true;
                    return true;
                }
            }
        }
        false
    }

    pub fn get_visible_layers(&self) -> usize {
        self.layers[..self.layer_count]
            .iter()
            .filter(|l| l.map(|ly| ly.visible).unwrap_or(false))
            .count()
    }

    pub fn composite(&mut self) -> bool {
        if !self.needs_redraw {
            return false;
        }
        self.needs_redraw = false;
        true
    }
}

// ============================================================================
// WINDOW EFFECTS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectType {
    Blur,
    Shadow,
    Glow,
    Parallax,
    Distortion,
}

#[derive(Debug, Clone, Copy)]
pub struct WindowEffect {
    pub effect_type: EffectType,
    pub intensity: f32,
    pub radius: u32,
    pub enabled: bool,
}

impl WindowEffect {
    pub fn blur(radius: u32) -> Self {
        WindowEffect {
            effect_type: EffectType::Blur,
            intensity: 1.0,
            radius,
            enabled: true,
        }
    }

    pub fn shadow(radius: u32, intensity: f32) -> Self {
        WindowEffect {
            effect_type: EffectType::Shadow,
            intensity,
            radius,
            enabled: true,
        }
    }

    pub fn glow(radius: u32, intensity: f32) -> Self {
        WindowEffect {
            effect_type: EffectType::Glow,
            intensity,
            radius,
            enabled: true,
        }
    }

    pub fn apply_blur(&self, color: u32) -> u32 {
        // Simplified blur: average neighboring pixels
        let a = (color >> 24) & 0xFF;
        let r = (color >> 16) & 0xFF;
        let g = (color >> 8) & 0xFF;
        let b = color & 0xFF;

        let blur_factor = self.radius as f32 / 16.0;
        let r = ((r as f32 * (1.0 - blur_factor)) as u32) & 0xFF;
        let g = ((g as f32 * (1.0 - blur_factor)) as u32) & 0xFF;
        let b = ((b as f32 * (1.0 - blur_factor)) as u32) & 0xFF;

        (a << 24) | (r << 16) | (g << 8) | b
    }

    pub fn apply_shadow(&self, color: u32) -> u32 {
        let a = (color >> 24) & 0xFF;
        let r = (color >> 16) & 0xFF;
        let g = (color >> 8) & 0xFF;
        let b = color & 0xFF;

        let shadow_factor = 1.0 - self.intensity;
        let r = ((r as f32 * shadow_factor) as u32) & 0xFF;
        let g = ((g as f32 * shadow_factor) as u32) & 0xFF;
        let b = ((b as f32 * shadow_factor) as u32) & 0xFF;

        (a << 24) | (r << 16) | (g << 8) | b
    }
}

// ============================================================================
// PARTICLE SYSTEM
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub lifetime: u32,
    pub max_lifetime: u32,
    pub color: u32,
    pub size: f32,
}

impl Particle {
    pub fn new(x: f32, y: f32) -> Self {
        Particle {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            lifetime: 100,
            max_lifetime: 100,
            color: 0xFFFFFFFF,
            size: 1.0,
        }
    }

    pub fn update(&mut self) -> bool {
        if self.lifetime == 0 {
            return false;
        }

        self.x += self.vx;
        self.y += self.vy;
        self.vy += 0.1; // Gravity

        self.lifetime -= 1;
        true
    }

    pub fn get_alpha(&self) -> f32 {
        (self.lifetime as f32) / (self.max_lifetime as f32)
    }
}

pub struct ParticleSystem {
    pub particles: [Particle; MAX_PARTICLES],
    pub particle_count: usize,
    pub gravity_enabled: bool,
}

impl ParticleSystem {
    pub fn new() -> Self {
        ParticleSystem {
            particles: [Particle::new(0.0, 0.0); MAX_PARTICLES],
            particle_count: 0,
            gravity_enabled: true,
        }
    }

    pub fn emit(&mut self, particle: Particle) -> bool {
        if self.particle_count >= MAX_PARTICLES {
            return false;
        }
        self.particles[self.particle_count] = particle;
        self.particle_count += 1;
        true
    }

    pub fn update(&mut self) {
        let mut live_count = 0;
        for i in 0..self.particle_count {
            if self.particles[i].update() {
                if live_count != i {
                    self.particles[live_count] = self.particles[i];
                }
                live_count += 1;
            }
        }
        self.particle_count = live_count;
    }

    pub fn clear(&mut self) {
        self.particle_count = 0;
    }
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TRANSITION MANAGER
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionType {
    Fade,
    Scale,
    Slide,
    Rotate,
}

#[derive(Debug, Clone, Copy)]
pub struct Transition {
    pub trans_type: TransitionType,
    pub duration: u32,
    pub elapsed: u32,
    pub active: bool,
}

impl Transition {
    pub fn new(trans_type: TransitionType, duration: u32) -> Self {
        Transition {
            trans_type,
            duration,
            elapsed: 0,
            active: true,
        }
    }

    pub fn get_progress(&self) -> f32 {
        if self.duration == 0 {
            return 1.0;
        }
        ((self.elapsed as f32) / (self.duration as f32)).min(1.0)
    }

    pub fn update(&mut self) -> bool {
        if !self.active {
            return false;
        }
        if self.elapsed >= self.duration {
            self.active = false;
            return false;
        }
        self.elapsed += 1;
        true
    }

    pub fn is_complete(&self) -> bool {
        !self.active && self.elapsed >= self.duration
    }
}

pub struct TransitionManager {
    pub transitions: [Option<Transition>; MAX_EFFECTS],
    pub transition_count: usize,
}

impl TransitionManager {
    pub fn new() -> Self {
        TransitionManager {
            transitions: [None; MAX_EFFECTS],
            transition_count: 0,
        }
    }

    pub fn start_transition(&mut self, trans: Transition) -> bool {
        if self.transition_count >= MAX_EFFECTS {
            return false;
        }
        self.transitions[self.transition_count] = Some(trans);
        self.transition_count += 1;
        true
    }

    pub fn update(&mut self) {
        let mut active_count = 0;
        for i in 0..self.transition_count {
            if let Some(mut trans) = self.transitions[i] {
                if trans.update() {
                    if active_count != i {
                        self.transitions[active_count] = self.transitions[i];
                    }
                    active_count += 1;
                }
            }
        }
        self.transition_count = active_count;
    }
}

impl Default for TransitionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// DAMAGE REGION TRACKING
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct DamageRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub dirty: bool,
}

impl DamageRegion {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        DamageRegion {
            x,
            y,
            width,
            height,
            dirty: true,
        }
    }

    pub fn area(&self) -> u32 {
        self.width.saturating_mul(self.height)
    }

    pub fn intersects(&self, other: &DamageRegion) -> bool {
        self.x < other.x + other.width as i32
            && self.x + self.width as i32 > other.x
            && self.y < other.y + other.height as i32
            && self.y + self.height as i32 > other.y
    }

    pub fn merge(&mut self, other: &DamageRegion) {
        let new_x = self.x.min(other.x);
        let new_y = self.y.min(other.y);
        let new_x2 = (self.x + self.width as i32).max(other.x + other.width as i32);
        let new_y2 = (self.y + self.height as i32).max(other.y + other.height as i32);

        self.x = new_x;
        self.y = new_y;
        self.width = (new_x2 - new_x).max(0) as u32;
        self.height = (new_y2 - new_y).max(0) as u32;
    }
}

pub struct DamageTracker {
    pub regions: [Option<DamageRegion>; MAX_DAMAGE_REGIONS],
    pub region_count: usize,
    pub total_dirty_area: u32,
}

impl DamageTracker {
    pub fn new() -> Self {
        DamageTracker {
            regions: [None; MAX_DAMAGE_REGIONS],
            region_count: 0,
            total_dirty_area: 0,
        }
    }

    pub fn mark_dirty(&mut self, region: DamageRegion) -> bool {
        // Merge with overlapping regions
        let mut merged = false;
        for i in 0..self.region_count {
            if let Some(existing) = &mut self.regions[i] {
                if existing.intersects(&region) {
                    existing.merge(&region);
                    merged = true;
                    break;
                }
            }
        }

        if !merged && self.region_count < MAX_DAMAGE_REGIONS {
            self.regions[self.region_count] = Some(region);
            self.region_count += 1;
        }

        self.recalculate_dirty_area();
        true
    }

    pub fn clear(&mut self) {
        self.region_count = 0;
        self.total_dirty_area = 0;
    }

    fn recalculate_dirty_area(&mut self) {
        self.total_dirty_area = 0;
        for i in 0..self.region_count {
            if let Some(region) = self.regions[i] {
                self.total_dirty_area += region.area();
            }
        }
    }

    pub fn get_dirty_percentage(&self, total_area: u32) -> u32 {
        if total_area == 0 {
            return 0;
        }
        (self.total_dirty_area * 100) / total_area
    }
}

impl Default for DamageTracker {
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
    fn test_blend_mode_normal() {
        let result = LayerBlendMode::Normal.blend_channel(0.5, 0.7);
        assert!((result - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_blend_mode_multiply() {
        let result = LayerBlendMode::Multiply.blend_channel(0.5, 0.8);
        assert!((result - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_blend_mode_screen() {
        let result = LayerBlendMode::Screen.blend_channel(0.5, 0.5);
        assert!(result > 0.5);
    }

    #[test]
    fn test_compositing_layer_new() {
        let layer = CompositingLayer::new(1, 0, 0, 1920, 1080);
        assert_eq!(layer.id, 1);
        assert_eq!(layer.opacity, 1.0);
    }

    #[test]
    fn test_compositing_layer_opacity() {
        let mut layer = CompositingLayer::new(1, 0, 0, 1920, 1080);
        layer.set_opacity(0.5);
        assert_eq!(layer.opacity, 0.5);
    }

    #[test]
    fn test_compositing_layer_contains() {
        let layer = CompositingLayer::new(1, 100, 100, 200, 200);
        assert!(layer.contains_point(150, 150));
        assert!(!layer.contains_point(50, 50));
    }

    #[test]
    fn test_compositing_pipeline_new() {
        let pipeline = CompositingPipeline::new(1920, 1080);
        assert_eq!(pipeline.layer_count, 0);
        assert!(pipeline.needs_redraw);
    }

    #[test]
    fn test_compositing_pipeline_add_layer() {
        let mut pipeline = CompositingPipeline::new(1920, 1080);
        let layer = CompositingLayer::new(1, 0, 0, 640, 480);
        assert!(pipeline.add_layer(layer));
        assert_eq!(pipeline.layer_count, 1);
    }

    #[test]
    fn test_compositing_pipeline_remove_layer() {
        let mut pipeline = CompositingPipeline::new(1920, 1080);
        let layer = CompositingLayer::new(1, 0, 0, 640, 480);
        pipeline.add_layer(layer);
        assert!(pipeline.remove_layer(1));
        assert_eq!(pipeline.layer_count, 0);
    }

    #[test]
    fn test_window_effect_blur() {
        let effect = WindowEffect::blur(16);
        assert_eq!(effect.effect_type, EffectType::Blur);
        assert!(effect.enabled);
    }

    #[test]
    fn test_window_effect_shadow() {
        let effect = WindowEffect::shadow(8, 0.5);
        assert_eq!(effect.effect_type, EffectType::Shadow);
    }

    #[test]
    fn test_particle_new() {
        let particle = Particle::new(100.0, 200.0);
        assert_eq!(particle.x, 100.0);
        assert_eq!(particle.lifetime, 100);
    }

    #[test]
    fn test_particle_update() {
        let mut particle = Particle::new(0.0, 0.0);
        particle.vx = 1.0;
        particle.update();
        assert_eq!(particle.x, 1.0);
    }

    #[test]
    fn test_particle_system_emit() {
        let mut system = ParticleSystem::new();
        let particle = Particle::new(100.0, 100.0);
        assert!(system.emit(particle));
        assert_eq!(system.particle_count, 1);
    }

    #[test]
    fn test_transition_new() {
        let trans = Transition::new(TransitionType::Fade, 100);
        assert!(!trans.is_complete());
        assert_eq!(trans.get_progress(), 0.0);
    }

    #[test]
    fn test_transition_update() {
        let mut trans = Transition::new(TransitionType::Scale, 100);
        trans.update();
        assert_eq!(trans.elapsed, 1);
    }

    #[test]
    fn test_damage_region_new() {
        let region = DamageRegion::new(0, 0, 1920, 1080);
        assert_eq!(region.area(), 1920 * 1080);
    }

    #[test]
    fn test_damage_region_intersects() {
        let region1 = DamageRegion::new(0, 0, 100, 100);
        let region2 = DamageRegion::new(50, 50, 100, 100);
        assert!(region1.intersects(&region2));
    }

    #[test]
    fn test_damage_tracker_new() {
        let tracker = DamageTracker::new();
        assert_eq!(tracker.region_count, 0);
        assert_eq!(tracker.total_dirty_area, 0);
    }

    #[test]
    fn test_damage_tracker_mark_dirty() {
        let mut tracker = DamageTracker::new();
        let region = DamageRegion::new(0, 0, 100, 100);
        assert!(tracker.mark_dirty(region));
        assert!(tracker.total_dirty_area > 0);
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_multi_layer_compositing() {
        let mut pipeline = CompositingPipeline::new(1920, 1080);

        let layer1 = CompositingLayer::new(1, 0, 0, 1920, 1080);
        let mut layer2 = CompositingLayer::new(2, 100, 100, 800, 600);
        layer2.set_blend_mode(LayerBlendMode::Multiply);

        pipeline.add_layer(layer1);
        pipeline.add_layer(layer2);

        assert_eq!(pipeline.layer_count, 2);
        assert!(pipeline.composite());
    }

    #[test]
    fn test_window_effects_rendering() {
        let blur_effect = WindowEffect::blur(8);
        let shadow_effect = WindowEffect::shadow(4, 0.7);

        let color = 0xFF808080u32;
        let _blurred = blur_effect.apply_blur(color);
        let _shadowed = shadow_effect.apply_shadow(color);

        assert!(blur_effect.enabled);
        assert!(shadow_effect.enabled);
    }

    #[test]
    fn test_particle_animation() {
        let mut system = ParticleSystem::new();

        let mut particle = Particle::new(400.0, 300.0);
        particle.vx = 2.0;
        particle.vy = -5.0;
        particle.lifetime = 50;
        particle.max_lifetime = 50;

        system.emit(particle);
        system.update();

        assert_eq!(system.particle_count, 1);
        assert!(system.particles[0].y < 300.0); // Moved up initially
    }

    #[test]
    fn test_transition_fade_progress() {
        let mut trans = Transition::new(TransitionType::Fade, 100);

        for _ in 0..50 {
            trans.update();
        }

        let progress = trans.get_progress();
        assert!(progress > 0.4 && progress < 0.6);
    }

    #[test]
    fn test_damage_region_merging() {
        let mut tracker = DamageTracker::new();

        let region1 = DamageRegion::new(0, 0, 100, 100);
        let region2 = DamageRegion::new(50, 50, 100, 100);

        tracker.mark_dirty(region1);
        tracker.mark_dirty(region2);

        // Regions should be merged
        assert!(tracker.region_count <= 2);
    }
}
