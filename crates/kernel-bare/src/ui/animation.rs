//! Animation System for RayOS UI
//!
//! Provides smooth window transitions and visual effects:
//! - Window open/close animations
//! - Fade in/out effects
//! - Smooth position/size interpolation
//! - Easing curves for natural motion

use super::window_manager::WindowId;

/// Maximum number of concurrent animations.
pub const MAX_ANIMATIONS: usize = 32;

/// Default animation duration in frames (at ~60fps, 15 frames ≈ 250ms).
pub const DEFAULT_DURATION: u32 = 15;

/// Fast animation duration (150ms).
pub const FAST_DURATION: u32 = 9;

/// Slow animation duration (400ms).
pub const SLOW_DURATION: u32 = 24;

// ===== Easing Functions =====

/// Easing function type for animation curves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Easing {
    /// Linear interpolation (constant speed)
    Linear = 0,
    /// Ease in (starts slow, ends fast) - quadratic
    EaseIn = 1,
    /// Ease out (starts fast, ends slow) - quadratic
    EaseOut = 2,
    /// Ease in-out (slow at both ends) - quadratic
    EaseInOut = 3,
    /// Cubic ease out (smoother deceleration)
    CubicOut = 4,
    /// Elastic bounce at end
    Elastic = 5,
    /// Bounce at end
    Bounce = 6,
    /// Back overshoot
    Back = 7,
}

impl Default for Easing {
    fn default() -> Self {
        Easing::EaseOut
    }
}

impl Easing {
    /// Apply easing function to a linear progress value (0.0 to 1.0).
    /// Uses fixed-point math: input and output are 0-1000 representing 0.0-1.0
    pub fn apply(&self, t: u32) -> u32 {
        // t is 0-1000 representing 0.0-1.0
        let t = t.min(1000);

        match self {
            Easing::Linear => t,

            Easing::EaseIn => {
                // t^2
                (t * t) / 1000
            }

            Easing::EaseOut => {
                // 1 - (1-t)^2
                let inv = 1000 - t;
                1000 - (inv * inv) / 1000
            }

            Easing::EaseInOut => {
                // Quadratic ease in-out
                if t < 500 {
                    // 2 * t^2
                    (2 * t * t) / 1000
                } else {
                    // 1 - 2*(1-t)^2
                    let inv = 1000 - t;
                    1000 - (2 * inv * inv) / 1000
                }
            }

            Easing::CubicOut => {
                // 1 - (1-t)^3
                let inv = 1000 - t;
                let inv_cubed = (inv * inv / 1000) * inv / 1000;
                1000 - inv_cubed
            }

            Easing::Elastic => {
                // Simplified elastic - overshoot then settle
                if t < 700 {
                    // Accelerate to 110%
                    (t * 1100) / 700
                } else if t < 850 {
                    // Pull back to 95%
                    1100 - ((t - 700) * 150) / 150
                } else {
                    // Settle to 100%
                    950 + ((t - 850) * 50) / 150
                }
            }

            Easing::Bounce => {
                // Simple bounce effect at end
                if t < 800 {
                    // Normal ease to 100%
                    let scaled = (t * 1250) / 1000;
                    let inv = 1000 - scaled.min(1000);
                    1000 - (inv * inv) / 1000
                } else if t < 900 {
                    // Small bounce up
                    1000 + ((t - 800) * 50) / 100
                } else {
                    // Settle back
                    1050 - ((t - 900) * 50) / 100
                }
            }

            Easing::Back => {
                // Overshoot by ~10% then settle
                let ease = {
                    let inv = 1000 - t;
                    1000 - (inv * inv) / 1000
                };
                // Add slight overshoot in middle
                if t > 500 && t < 900 {
                    ease + ((t - 500) * (900 - t)) / 4000
                } else {
                    ease
                }
            }
        }
    }
}

// ===== Animation Types =====

/// Type of animation effect.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum AnimationType {
    /// No animation (instant)
    None = 0,
    /// Fade opacity from 0 to target
    FadeIn = 1,
    /// Fade opacity from current to 0
    FadeOut = 2,
    /// Scale from small to full size
    ScaleIn = 3,
    /// Scale from full size to small
    ScaleOut = 4,
    /// Slide from off-screen direction
    SlideIn = 5,
    /// Slide to off-screen direction
    SlideOut = 6,
    /// Move to new position
    Move = 7,
    /// Resize to new dimensions
    Resize = 8,
    /// Combined fade + scale for opening
    PopIn = 9,
    /// Combined fade + scale for closing
    PopOut = 10,
    /// Minimize to dock/taskbar
    Minimize = 11,
    /// Restore from minimized
    Restore = 12,
}

impl Default for AnimationType {
    fn default() -> Self {
        AnimationType::None
    }
}

/// Direction for slide animations.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SlideDirection {
    Left = 0,
    Right = 1,
    Top = 2,
    Bottom = 3,
}

impl Default for SlideDirection {
    fn default() -> Self {
        SlideDirection::Bottom
    }
}

// ===== Animation State =====

/// Represents the animated properties of a window.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnimatedProperties {
    /// Current X position (may differ from window's actual position during animation)
    pub x: i32,
    /// Current Y position
    pub y: i32,
    /// Current width
    pub width: u32,
    /// Current height
    pub height: u32,
    /// Current opacity (0-255)
    pub opacity: u8,
    /// Current scale factor (0-1000, 1000 = 100%)
    pub scale: u32,
}

impl AnimatedProperties {
    /// Create properties from window position/size with full opacity.
    pub fn from_window(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            opacity: 255,
            scale: 1000,
        }
    }

    /// Create properties for a hidden/minimized state.
    pub fn hidden(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            opacity: 0,
            scale: 0,
        }
    }

    /// Create properties scaled to a center point.
    pub fn scaled_to_center(x: i32, y: i32, width: u32, height: u32, scale: u32) -> Self {
        let center_x = x + (width / 2) as i32;
        let center_y = y + (height / 2) as i32;
        let new_width = (width as u64 * scale as u64 / 1000) as u32;
        let new_height = (height as u64 * scale as u64 / 1000) as u32;
        Self {
            x: center_x - (new_width / 2) as i32,
            y: center_y - (new_height / 2) as i32,
            width: new_width,
            height: new_height,
            opacity: 255,
            scale,
        }
    }

    /// Interpolate between two states based on progress (0-1000).
    pub fn lerp(&self, target: &Self, progress: u32) -> Self {
        let progress = progress.min(1000);
        let inv = 1000 - progress;

        Self {
            x: ((self.x as i64 * inv as i64 + target.x as i64 * progress as i64) / 1000) as i32,
            y: ((self.y as i64 * inv as i64 + target.y as i64 * progress as i64) / 1000) as i32,
            width: ((self.width as u64 * inv as u64 + target.width as u64 * progress as u64) / 1000) as u32,
            height: ((self.height as u64 * inv as u64 + target.height as u64 * progress as u64) / 1000) as u32,
            opacity: ((self.opacity as u32 * inv + target.opacity as u32 * progress) / 1000) as u8,
            scale: (self.scale as u64 * inv as u64 + target.scale as u64 * progress as u64) as u32 / 1000,
        }
    }
}

// ===== Animation Instance =====

/// A single animation in progress.
#[derive(Clone, Copy, Debug)]
pub struct Animation {
    /// Window this animation applies to
    pub window_id: WindowId,
    /// Type of animation
    pub animation_type: AnimationType,
    /// Easing curve
    pub easing: Easing,
    /// Starting state
    pub from: AnimatedProperties,
    /// Target state
    pub to: AnimatedProperties,
    /// Current frame (0 to duration)
    pub frame: u32,
    /// Total duration in frames
    pub duration: u32,
    /// Whether animation is complete
    pub complete: bool,
    /// Callback action on completion
    pub on_complete: AnimationCallback,
}

/// Callback action when animation completes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum AnimationCallback {
    /// No action
    None = 0,
    /// Destroy the window
    DestroyWindow = 1,
    /// Hide the window
    HideWindow = 2,
    /// Show the window
    ShowWindow = 3,
    /// Focus the window
    FocusWindow = 4,
}

impl Default for AnimationCallback {
    fn default() -> Self {
        AnimationCallback::None
    }
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            window_id: 0,
            animation_type: AnimationType::None,
            easing: Easing::EaseOut,
            from: AnimatedProperties::default(),
            to: AnimatedProperties::default(),
            frame: 0,
            duration: DEFAULT_DURATION,
            complete: true,
            on_complete: AnimationCallback::None,
        }
    }
}

impl Animation {
    /// Create a new animation for a window.
    pub fn new(
        window_id: WindowId,
        animation_type: AnimationType,
        from: AnimatedProperties,
        to: AnimatedProperties,
        duration: u32,
        easing: Easing,
    ) -> Self {
        Self {
            window_id,
            animation_type,
            easing,
            from,
            to,
            frame: 0,
            duration: duration.max(1),
            complete: false,
            on_complete: AnimationCallback::None,
        }
    }

    /// Create a fade-in animation for a window opening.
    pub fn fade_in(window_id: WindowId, x: i32, y: i32, width: u32, height: u32) -> Self {
        let from = AnimatedProperties {
            x, y, width, height,
            opacity: 0,
            scale: 1000,
        };
        let to = AnimatedProperties::from_window(x, y, width, height);
        Self::new(window_id, AnimationType::FadeIn, from, to, DEFAULT_DURATION, Easing::EaseOut)
    }

    /// Create a fade-out animation for a window closing.
    pub fn fade_out(window_id: WindowId, x: i32, y: i32, width: u32, height: u32) -> Self {
        let from = AnimatedProperties::from_window(x, y, width, height);
        let to = AnimatedProperties {
            x, y, width, height,
            opacity: 0,
            scale: 1000,
        };
        let mut anim = Self::new(window_id, AnimationType::FadeOut, from, to, FAST_DURATION, Easing::EaseIn);
        anim.on_complete = AnimationCallback::DestroyWindow;
        anim
    }

    /// Create a pop-in animation (scale + fade from center).
    pub fn pop_in(window_id: WindowId, x: i32, y: i32, width: u32, height: u32) -> Self {
        let from = AnimatedProperties {
            x: x + (width / 4) as i32,
            y: y + (height / 4) as i32,
            width: width / 2,
            height: height / 2,
            opacity: 0,
            scale: 500,
        };
        let to = AnimatedProperties::from_window(x, y, width, height);
        Self::new(window_id, AnimationType::PopIn, from, to, DEFAULT_DURATION, Easing::CubicOut)
    }

    /// Create a pop-out animation (scale + fade to center).
    pub fn pop_out(window_id: WindowId, x: i32, y: i32, width: u32, height: u32) -> Self {
        let from = AnimatedProperties::from_window(x, y, width, height);
        let to = AnimatedProperties {
            x: x + (width / 4) as i32,
            y: y + (height / 4) as i32,
            width: width / 2,
            height: height / 2,
            opacity: 0,
            scale: 500,
        };
        let mut anim = Self::new(window_id, AnimationType::PopOut, from, to, FAST_DURATION, Easing::EaseIn);
        anim.on_complete = AnimationCallback::DestroyWindow;
        anim
    }

    /// Create a move animation.
    pub fn move_to(window_id: WindowId, from_x: i32, from_y: i32, to_x: i32, to_y: i32, width: u32, height: u32) -> Self {
        let from = AnimatedProperties::from_window(from_x, from_y, width, height);
        let to = AnimatedProperties::from_window(to_x, to_y, width, height);
        Self::new(window_id, AnimationType::Move, from, to, FAST_DURATION, Easing::EaseOut)
    }

    /// Create a resize animation.
    pub fn resize_to(window_id: WindowId, x: i32, y: i32, from_w: u32, from_h: u32, to_w: u32, to_h: u32) -> Self {
        let from = AnimatedProperties::from_window(x, y, from_w, from_h);
        let to = AnimatedProperties::from_window(x, y, to_w, to_h);
        Self::new(window_id, AnimationType::Resize, from, to, DEFAULT_DURATION, Easing::EaseOut)
    }

    /// Create a minimize animation.
    pub fn minimize(window_id: WindowId, x: i32, y: i32, width: u32, height: u32, dock_x: i32, dock_y: i32) -> Self {
        let from = AnimatedProperties::from_window(x, y, width, height);
        let to = AnimatedProperties {
            x: dock_x,
            y: dock_y,
            width: 48,
            height: 48,
            opacity: 0,
            scale: 100,
        };
        let mut anim = Self::new(window_id, AnimationType::Minimize, from, to, DEFAULT_DURATION, Easing::EaseInOut);
        anim.on_complete = AnimationCallback::HideWindow;
        anim
    }

    /// Create a restore animation from minimized.
    pub fn restore(window_id: WindowId, dock_x: i32, dock_y: i32, x: i32, y: i32, width: u32, height: u32) -> Self {
        let from = AnimatedProperties {
            x: dock_x,
            y: dock_y,
            width: 48,
            height: 48,
            opacity: 0,
            scale: 100,
        };
        let to = AnimatedProperties::from_window(x, y, width, height);
        let mut anim = Self::new(window_id, AnimationType::Restore, from, to, DEFAULT_DURATION, Easing::CubicOut);
        anim.on_complete = AnimationCallback::ShowWindow;
        anim
    }

    /// Advance the animation by one frame.
    pub fn tick(&mut self) {
        if self.complete {
            return;
        }

        self.frame += 1;
        if self.frame >= self.duration {
            self.frame = self.duration;
            self.complete = true;
        }
    }

    /// Get the current progress (0-1000).
    pub fn progress(&self) -> u32 {
        if self.duration == 0 {
            return 1000;
        }
        (self.frame as u64 * 1000 / self.duration as u64) as u32
    }

    /// Get the eased progress (0-1000).
    pub fn eased_progress(&self) -> u32 {
        self.easing.apply(self.progress())
    }

    /// Get the current animated properties.
    pub fn current(&self) -> AnimatedProperties {
        self.from.lerp(&self.to, self.eased_progress())
    }

    /// Check if this animation is for a specific window.
    pub fn is_for_window(&self, id: WindowId) -> bool {
        self.window_id == id
    }
}

// ===== Animation Manager =====

/// Manages all active animations.
pub struct AnimationManager {
    /// Active animations
    animations: [Animation; MAX_ANIMATIONS],
    /// Number of active animations
    count: usize,
    /// Whether animations are enabled
    enabled: bool,
    /// Global animation speed multiplier (1000 = 1x, 500 = 2x faster, 2000 = 0.5x slower)
    speed: u32,
}

/// Global animation manager instance.
static mut ANIMATION_MANAGER: AnimationManager = AnimationManager::new_const();

impl AnimationManager {
    /// Create a new animation manager (const for static init).
    pub const fn new_const() -> Self {
        Self {
            animations: [Animation {
                window_id: 0,
                animation_type: AnimationType::None,
                easing: Easing::EaseOut,
                from: AnimatedProperties { x: 0, y: 0, width: 0, height: 0, opacity: 255, scale: 1000 },
                to: AnimatedProperties { x: 0, y: 0, width: 0, height: 0, opacity: 255, scale: 1000 },
                frame: 0,
                duration: DEFAULT_DURATION,
                complete: true,
                on_complete: AnimationCallback::None,
            }; MAX_ANIMATIONS],
            count: 0,
            enabled: true,  // Performance optimized with bit-shift blending
            speed: 1000,
        }
    }

    /// Initialize the animation manager.
    pub fn init(&mut self) {
        self.count = 0;
        self.enabled = true;  // Performance optimized with bit-shift blending
        self.speed = 1000;

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_ANIMATION_INIT:ok\n");
        }
    }

    /// Enable or disable animations globally.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if animations are enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the global animation speed (1000 = 1x).
    pub fn set_speed(&mut self, speed: u32) {
        self.speed = speed.max(100).min(5000);
    }

    /// Start a new animation. Returns true if started.
    pub fn start(&mut self, mut animation: Animation) -> bool {
        if !self.enabled {
            // If animations disabled, complete immediately
            return false;
        }

        // Cancel any existing animation for this window
        self.cancel_for_window(animation.window_id);

        // Find empty slot
        if self.count >= MAX_ANIMATIONS {
            // Remove oldest animation
            for i in 0..MAX_ANIMATIONS - 1 {
                self.animations[i] = self.animations[i + 1];
            }
            self.count = MAX_ANIMATIONS - 1;
        }

        // Adjust duration based on speed
        animation.duration = (animation.duration as u64 * 1000 / self.speed as u64) as u32;
        animation.duration = animation.duration.max(1);

        self.animations[self.count] = animation;
        self.count += 1;

        true
    }

    /// Cancel all animations for a window.
    pub fn cancel_for_window(&mut self, window_id: WindowId) {
        let mut write_idx = 0;
        for read_idx in 0..self.count {
            if self.animations[read_idx].window_id != window_id {
                if write_idx != read_idx {
                    self.animations[write_idx] = self.animations[read_idx];
                }
                write_idx += 1;
            }
        }
        self.count = write_idx;
    }

    /// Check if a window has an active animation.
    pub fn is_animating(&self, window_id: WindowId) -> bool {
        for i in 0..self.count {
            if self.animations[i].window_id == window_id && !self.animations[i].complete {
                return true;
            }
        }
        false
    }

    /// Get the current animated properties for a window (or None if not animating).
    pub fn get_animated_properties(&self, window_id: WindowId) -> Option<AnimatedProperties> {
        for i in 0..self.count {
            if self.animations[i].window_id == window_id && !self.animations[i].complete {
                return Some(self.animations[i].current());
            }
        }
        None
    }

    /// Tick all animations forward. Returns callbacks for completed animations.
    pub fn tick(&mut self) -> [(WindowId, AnimationCallback); MAX_ANIMATIONS] {
        let mut callbacks = [(0, AnimationCallback::None); MAX_ANIMATIONS];
        let mut callback_count = 0;

        // Tick all animations
        for i in 0..self.count {
            if !self.animations[i].complete {
                self.animations[i].tick();

                // Check if just completed
                if self.animations[i].complete {
                    if self.animations[i].on_complete != AnimationCallback::None {
                        if callback_count < MAX_ANIMATIONS {
                            callbacks[callback_count] = (
                                self.animations[i].window_id,
                                self.animations[i].on_complete,
                            );
                            callback_count += 1;
                        }
                    }
                }
            }
        }

        // Remove completed animations
        let mut write_idx = 0;
        for read_idx in 0..self.count {
            if !self.animations[read_idx].complete {
                if write_idx != read_idx {
                    self.animations[write_idx] = self.animations[read_idx];
                }
                write_idx += 1;
            }
        }
        self.count = write_idx;

        callbacks
    }

    /// Check if any animations are active.
    pub fn has_active_animations(&self) -> bool {
        for i in 0..self.count {
            if !self.animations[i].complete {
                return true;
            }
        }
        false
    }

    /// Get the number of active animations.
    pub fn active_count(&self) -> usize {
        let mut count = 0;
        for i in 0..self.count {
            if !self.animations[i].complete {
                count += 1;
            }
        }
        count
    }
}

// ===== Global Accessors =====

/// Initialize the animation manager.
pub fn init() {
    unsafe {
        ANIMATION_MANAGER.init();
    }
}

/// Get a reference to the animation manager.
pub fn get() -> &'static AnimationManager {
    unsafe { &ANIMATION_MANAGER }
}

/// Get a mutable reference to the animation manager.
pub fn get_mut() -> &'static mut AnimationManager {
    unsafe { &mut ANIMATION_MANAGER }
}

/// Start a new animation.
pub fn start(animation: Animation) -> bool {
    get_mut().start(animation)
}

/// Cancel animations for a window.
pub fn cancel_for_window(window_id: WindowId) {
    get_mut().cancel_for_window(window_id);
}

/// Check if a window is animating.
pub fn is_animating(window_id: WindowId) -> bool {
    get().is_animating(window_id)
}

/// Get animated properties for a window.
pub fn get_animated_properties(window_id: WindowId) -> Option<AnimatedProperties> {
    get().get_animated_properties(window_id)
}

/// Tick all animations.
pub fn tick() -> [(WindowId, AnimationCallback); MAX_ANIMATIONS] {
    get_mut().tick()
}

/// Check if animations are enabled.
pub fn is_enabled() -> bool {
    get().is_enabled()
}

/// Enable or disable animations.
pub fn set_enabled(enabled: bool) {
    get_mut().set_enabled(enabled);
}

/// Check if any animations are active.
pub fn has_active() -> bool {
    get().has_active_animations()
}

// ===== Convenience Functions =====

/// Start a fade-in animation for a window.
pub fn animate_fade_in(window_id: WindowId, x: i32, y: i32, width: u32, height: u32) {
    start(Animation::fade_in(window_id, x, y, width, height));
}

/// Start a fade-out animation for a window (will destroy on complete).
pub fn animate_fade_out(window_id: WindowId, x: i32, y: i32, width: u32, height: u32) {
    start(Animation::fade_out(window_id, x, y, width, height));
}

/// Start a pop-in animation for a window.
pub fn animate_pop_in(window_id: WindowId, x: i32, y: i32, width: u32, height: u32) {
    start(Animation::pop_in(window_id, x, y, width, height));
}

/// Start a pop-out animation for a window (will destroy on complete).
pub fn animate_pop_out(window_id: WindowId, x: i32, y: i32, width: u32, height: u32) {
    start(Animation::pop_out(window_id, x, y, width, height));
}

/// Start a move animation for a window.
pub fn animate_move(window_id: WindowId, from_x: i32, from_y: i32, to_x: i32, to_y: i32, width: u32, height: u32) {
    start(Animation::move_to(window_id, from_x, from_y, to_x, to_y, width, height));
}

/// Start a resize animation for a window.
pub fn animate_resize(window_id: WindowId, x: i32, y: i32, from_w: u32, from_h: u32, to_w: u32, to_h: u32) {
    start(Animation::resize_to(window_id, x, y, from_w, from_h, to_w, to_h));
}

/// Start a minimize animation for a window.
pub fn animate_minimize(window_id: WindowId, x: i32, y: i32, width: u32, height: u32, dock_x: i32, dock_y: i32) {
    start(Animation::minimize(window_id, x, y, width, height, dock_x, dock_y));
}

/// Start a restore animation for a window.
pub fn animate_restore(window_id: WindowId, dock_x: i32, dock_y: i32, x: i32, y: i32, width: u32, height: u32) {
    start(Animation::restore(window_id, dock_x, dock_y, x, y, width, height));
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_easing_linear() {
        assert_eq!(Easing::Linear.apply(0), 0);
        assert_eq!(Easing::Linear.apply(500), 500);
        assert_eq!(Easing::Linear.apply(1000), 1000);
    }

    #[test]
    fn test_easing_ease_out() {
        // Ease out should be faster at start
        let mid = Easing::EaseOut.apply(500);
        assert!(mid > 500); // Should be ahead of linear at midpoint
        assert!(Easing::EaseOut.apply(0) == 0);
        assert!(Easing::EaseOut.apply(1000) == 1000);
    }

    #[test]
    fn test_animated_properties_lerp() {
        let from = AnimatedProperties::from_window(0, 0, 100, 100);
        let to = AnimatedProperties::from_window(100, 100, 200, 200);

        let mid = from.lerp(&to, 500);
        assert_eq!(mid.x, 50);
        assert_eq!(mid.y, 50);
        assert_eq!(mid.width, 150);
        assert_eq!(mid.height, 150);
    }

    #[test]
    fn test_animation_progress() {
        let mut anim = Animation::fade_in(1, 0, 0, 100, 100);
        assert_eq!(anim.progress(), 0);

        for _ in 0..anim.duration / 2 {
            anim.tick();
        }
        assert!(anim.progress() >= 400 && anim.progress() <= 600);

        while !anim.complete {
            anim.tick();
        }
        assert_eq!(anim.progress(), 1000);
    }
}
