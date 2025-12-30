//! Context Manager - Audio-Visual Fusion
//!
//! Combines data from multiple sensors:
//! - Gaze tracking (from Phase 2 Cortex)
//! - Audio transcription
//! - Visual object detection
//! - Deictic reference resolution

use crate::types::*;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;

/// Context manager handles sensor fusion and deictic resolution
pub struct ContextManager {
    _config: IntentConfig,
    gaze_history: Arc<RwLock<VecDeque<GazeContext>>>,
    audio_buffer: Arc<RwLock<VecDeque<AudioContext>>>,
    visual_objects: Arc<RwLock<Vec<VisualObject>>>,
    filesystem_context: Arc<RwLock<Option<FilesystemContext>>>,
    max_history_size: usize,
    history_duration: Duration,
}

impl ContextManager {
    /// Create new context manager
    pub fn new(config: IntentConfig) -> Self {
        Self {
            _config: config,
            gaze_history: Arc::new(RwLock::new(VecDeque::new())),
            audio_buffer: Arc::new(RwLock::new(VecDeque::new())),
            visual_objects: Arc::new(RwLock::new(Vec::new())),
            filesystem_context: Arc::new(RwLock::new(None)),
            max_history_size: 100,
            history_duration: Duration::from_secs(30),  // 30 second context window
        }
    }

    /// Update gaze context from eye tracker
    pub fn update_gaze(&self, position: (f32, f32), focused_object: Option<String>) {
        let gaze = GazeContext {
            position,
            focused_object,
            timestamp: Instant::now(),
        };

        let mut history = self.gaze_history.write();
        history.push_back(gaze);

        // Clean old entries
        self.clean_history(&mut history);
    }

    /// Update audio context from microphone
    pub fn update_audio(&self, transcript: String, raw_audio: Vec<f32>) {
        let audio = AudioContext {
            transcript,
            raw_audio,
            timestamp: Instant::now(),
        };

        let mut buffer = self.audio_buffer.write();
        buffer.push_back(audio);

        // Clean old entries
        self.clean_history(&mut buffer);
    }

    /// Update visual objects from screen analysis
    pub fn update_visual_objects(&self, objects: Vec<VisualObject>) {
        let mut visual = self.visual_objects.write();
        *visual = objects;
    }

    /// Update filesystem context
    pub fn update_filesystem(&self, current_dir: PathBuf, open_files: Vec<PathBuf>, recent_files: Vec<PathBuf>) {
        let context = FilesystemContext {
            current_directory: current_dir,
            open_files,
            recent_files,
        };

        let mut fs = self.filesystem_context.write();
        *fs = Some(context);
    }

    /// Build complete context for intent parsing
    pub fn build_context(&self, system: SystemContext) -> Context {
        let gaze = self.get_latest_gaze();
        let audio = self.get_latest_audio();
        let visual_objects = self.visual_objects.read().clone();
        let filesystem = self.filesystem_context.read().clone();

        Context {
            gaze,
            audio,
            visual_objects,
            application: self.detect_application(),
            filesystem,
            system,
        }
    }

    /// Resolve deictic reference ("that", "this", "it")
    pub fn resolve_deictic(&self, reference: &str) -> Option<Target> {
        match reference.to_lowercase().as_str() {
            "that" | "this" => self.resolve_from_gaze(),
            "it" => self.resolve_from_history(),
            _ => None,
        }
    }

    /// Resolve "that"/"this" from current gaze
    fn resolve_from_gaze(&self) -> Option<Target> {
        let gaze = self.get_latest_gaze()?;

        Some(Target::Deictic {
            gaze_position: Some(gaze.position),
            object_id: gaze.focused_object,
        })
    }

    /// Resolve "it" from conversation history
    fn resolve_from_history(&self) -> Option<Target> {
        // Look at recent audio context for previous references
        let audio_buffer = self.audio_buffer.read();

        // Simple heuristic: find last mentioned filename
        for audio in audio_buffer.iter().rev() {
            if let Some(filename) = self.extract_filename(&audio.transcript) {
                return Some(Target::Direct {
                    path: PathBuf::from(filename),
                });
            }
        }

        // Fallback to gaze
        self.resolve_from_gaze()
    }

    /// Extract filename from text
    fn extract_filename(&self, text: &str) -> Option<String> {
        // Look for words with extensions
        for word in text.split_whitespace() {
            if word.contains('.') && !word.starts_with('.') {
                return Some(word.to_string());
            }
        }
        None
    }

    /// Get object at gaze position
    pub fn get_object_at_gaze(&self) -> Option<VisualObject> {
        let gaze = self.get_latest_gaze()?;
        let visual = self.visual_objects.read();

        // Find object containing gaze point
        visual.iter()
            .find(|obj| self.point_in_bounds(gaze.position, obj.bounds))
            .cloned()
    }

    /// Check if point is inside bounds
    fn point_in_bounds(&self, point: (f32, f32), bounds: (f32, f32, f32, f32)) -> bool {
        let (px, py) = point;
        let (x, y, w, h) = bounds;

        px >= x && px <= x + w && py >= y && py <= y + h
    }

    /// Get latest gaze context
    fn get_latest_gaze(&self) -> Option<GazeContext> {
        let history = self.gaze_history.read();
        history.back().cloned()
    }

    /// Get latest audio context
    fn get_latest_audio(&self) -> Option<AudioContext> {
        let buffer = self.audio_buffer.read();
        buffer.back().cloned()
    }

    /// Detect current application (simplified)
    fn detect_application(&self) -> Option<String> {
        // Try to detect active window on Linux via X11/Wayland
        #[cfg(target_os = "linux")]
        {
            // Try xdotool first (X11)
            if let Ok(output) = std::process::Command::new("xdotool")
                .args(["getactivewindow", "getwindowname"])
                .output()
            {
                if output.status.success() {
                    if let Ok(name) = String::from_utf8(output.stdout) {
                        let app_name = name.trim().to_string();
                        if !app_name.is_empty() {
                            return Some(app_name);
                        }
                    }
                }
            }

            // Fallback: check /proc for common applications
            if let Ok(entries) = std::fs::read_dir("/proc") {
                for entry in entries.flatten() {
                    if let Ok(exe) = std::fs::read_link(entry.path().join("exe")) {
                        let exe_name = exe.file_name()?.to_str()?.to_string();
                        // Check for common GUI apps
                        if ["code", "firefox", "chrome", "nautilus", "terminal"].iter()
                            .any(|&app| exe_name.contains(app))
                        {
                            return Some(exe_name);
                        }
                    }
                }
            }
        }

        // Fallback to environment variable
        std::env::var("RAYOS_ACTIVE_APP").ok()
    }

    /// Clean old entries from history
    fn clean_history<T>(&self, history: &mut VecDeque<T>)
    where
        T: HasTimestamp,
    {
        let now = Instant::now();

        // Remove entries older than history_duration
        while let Some(front) = history.front() {
            if now.duration_since(front.timestamp()) > self.history_duration {
                history.pop_front();
            } else {
                break;
            }
        }

        // Limit size
        while history.len() > self.max_history_size {
            history.pop_front();
        }
    }

    /// Fuse audio and visual context for command understanding
    pub fn fuse_context(&self, audio_transcript: &str) -> Context {
        let system = SystemContext {
            cpu_usage: 0.0,  // Will be filled by arbiter
            memory_usage: 0.0,
            active_tasks: 0,
        };

        let mut context = self.build_context(system);

        // If audio mentions deictic references, enhance with gaze
        if audio_transcript.contains("that") || audio_transcript.contains("this") {
            if let Some(target) = self.resolve_from_gaze() {
                // Add focused object info to context
                if let Target::Deictic { object_id, .. } = target {
                    if let Some(ref mut gaze) = context.gaze {
                        gaze.focused_object = object_id;
                    }
                }
            }
        }

        context
    }

    /// Get gaze heatmap for analysis
    pub fn get_gaze_heatmap(&self, grid_size: usize) -> Vec<Vec<f32>> {
        let history = self.gaze_history.read();
        let mut heatmap = vec![vec![0.0; grid_size]; grid_size];

        // Assume screen is normalized to (0,0) - (1,1)
        for gaze in history.iter() {
            let (x, y) = gaze.position;
            let grid_x = ((x / 1.0) * grid_size as f32) as usize;
            let grid_y = ((y / 1.0) * grid_size as f32) as usize;

            if grid_x < grid_size && grid_y < grid_size {
                heatmap[grid_y][grid_x] += 1.0;
            }
        }

        // Normalize
        let max_value = heatmap.iter()
            .flat_map(|row| row.iter())
            .cloned()
            .fold(0.0, f32::max);

        if max_value > 0.0 {
            for row in &mut heatmap {
                for cell in row {
                    *cell /= max_value;
                }
            }
        }

        heatmap
    }

    /// Get recent audio transcripts
    pub fn get_recent_transcripts(&self, count: usize) -> Vec<String> {
        let buffer = self.audio_buffer.read();
        buffer.iter()
            .rev()
            .take(count)
            .map(|a| a.transcript.clone())
            .collect()
    }

    /// Clear all context (for testing or reset)
    pub fn clear(&self) {
        self.gaze_history.write().clear();
        self.audio_buffer.write().clear();
        self.visual_objects.write().clear();
        *self.filesystem_context.write() = None;
    }
}

/// Trait for types with timestamps
trait HasTimestamp {
    fn timestamp(&self) -> Instant;
}

impl HasTimestamp for GazeContext {
    fn timestamp(&self) -> Instant {
        self.timestamp
    }
}

impl HasTimestamp for AudioContext {
    fn timestamp(&self) -> Instant {
        self.timestamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_gaze_update() {
        let manager = ContextManager::new(IntentConfig::default());

        manager.update_gaze((100.0, 200.0), Some("file_123".to_string()));

        let context = manager.build_context(SystemContext {
            cpu_usage: 50.0,
            memory_usage: 60.0,
            active_tasks: 10,
        });

        assert!(context.gaze.is_some());
        assert_eq!(context.gaze.unwrap().position, (100.0, 200.0));
    }

    #[test]
    fn test_deictic_resolution() {
        let manager = ContextManager::new(IntentConfig::default());

        manager.update_gaze((100.0, 200.0), Some("file_123".to_string()));

        let target = manager.resolve_deictic("that");
        assert!(target.is_some());
        assert!(matches!(target.unwrap(), Target::Deictic { .. }));
    }

    #[test]
    fn test_visual_object_detection() {
        let manager = ContextManager::new(IntentConfig::default());

        let objects = vec![
            VisualObject {
                id: "obj1".to_string(),
                object_type: "file".to_string(),
                bounds: (50.0, 50.0, 100.0, 100.0),
                properties: HashMap::new(),
            },
        ];

        manager.update_visual_objects(objects);
        manager.update_gaze((75.0, 75.0), None);  // Inside object

        let obj = manager.get_object_at_gaze();
        assert!(obj.is_some());
        assert_eq!(obj.unwrap().id, "obj1");
    }

    #[test]
    fn test_audio_fusion() {
        let manager = ContextManager::new(IntentConfig::default());

        manager.update_audio("rename that file".to_string(), vec![]);
        manager.update_gaze((100.0, 200.0), Some("file_123".to_string()));

        let context = manager.fuse_context("rename that file");

        assert!(context.gaze.is_some());
        assert!(context.audio.is_some());
    }

    #[test]
    fn test_history_cleanup() {
        let manager = ContextManager::new(IntentConfig::default());

        // Add many entries
        for i in 0..150 {
            manager.update_gaze((i as f32, i as f32), None);
        }

        let history = manager.gaze_history.read();
        assert!(history.len() <= manager.max_history_size);
    }

    #[test]
    fn test_gaze_heatmap() {
        let manager = ContextManager::new(IntentConfig::default());

        // Add concentrated gaze points
        for _ in 0..10 {
            manager.update_gaze((0.5, 0.5), None);
        }

        let heatmap = manager.get_gaze_heatmap(10);

        // Center should have high value
        assert!(heatmap[5][5] > 0.5);
    }
}
