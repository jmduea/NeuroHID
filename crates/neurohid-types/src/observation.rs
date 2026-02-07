//! # Observation Types
//!
//! Types defining the observation space - everything the decoder can "see"
//! when making decisions. This includes biosignal features, cursor state,
//! and optional enhanced observations (screen content, eye tracking, etc.).
//!
//! ## Design Philosophy
//!
//! The observation space is modular: users can opt into enhanced observations
//! for better performance, or use minimal observations for privacy.

use crate::{FeatureVector, Timestamp};
use serde::{Deserialize, Serialize};

/// A complete observation at a point in time.
/// This is what gets passed to the decoder for action selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Timestamp when this observation was assembled
    pub timestamp: Timestamp,

    /// Processed biosignal features (always present)
    pub signal_features: FeatureVector,

    /// Current cursor state (always present for mouse control)
    pub cursor: CursorState,

    /// Screen information
    pub screen: ScreenInfo,

    /// Enhanced observations (optional, user opt-in)
    pub enhanced: Option<EnhancedObservation>,
}

impl Observation {
    /// Get the total observation dimension (for neural network input sizing)
    pub fn total_dim(&self) -> usize {
        let mut dim = self.signal_features.dim();
        dim += CursorState::dim();
        dim += ScreenInfo::dim();

        if let Some(enhanced) = &self.enhanced {
            dim += enhanced.dim();
        }

        dim
    }

    /// Convert to a flat vector (for neural network input)
    pub fn to_vector(&self) -> Vec<f32> {
        let mut vec = Vec::with_capacity(self.total_dim());

        vec.extend(&self.signal_features.values);
        vec.extend(self.cursor.to_vector());
        vec.extend(self.screen.to_vector());

        if let Some(enhanced) = &self.enhanced {
            vec.extend(enhanced.to_vector());
        }

        vec
    }
}

/// Current state of the cursor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CursorState {
    /// Normalized X position (0.0 = left edge, 1.0 = right edge)
    pub x: f32,

    /// Normalized Y position (0.0 = top edge, 1.0 = bottom edge)
    pub y: f32,

    /// Velocity in X direction (normalized units per second)
    pub velocity_x: f32,

    /// Velocity in Y direction (normalized units per second)
    pub velocity_y: f32,

    /// Whether a mouse button is currently held
    pub button_held: bool,
}

impl CursorState {
    /// Create a cursor state at the center of the screen
    pub fn centered() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            velocity_x: 0.0,
            velocity_y: 0.0,
            button_held: false,
        }
    }

    /// Get the observation dimension for cursor state
    pub fn dim() -> usize {
        5 // x, y, vx, vy, button_held
    }

    /// Convert to a vector for neural network input
    pub fn to_vector(&self) -> Vec<f32> {
        vec![
            self.x,
            self.y,
            self.velocity_x,
            self.velocity_y,
            if self.button_held { 1.0 } else { 0.0 },
        ]
    }

    /// Distance from cursor to a normalized point
    pub fn distance_to(&self, target_x: f32, target_y: f32) -> f32 {
        let dx = self.x - target_x;
        let dy = self.y - target_y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Check if cursor is at screen edge
    pub fn at_edge(&self, threshold: f32) -> bool {
        self.x < threshold
            || self.x > (1.0 - threshold)
            || self.y < threshold
            || self.y > (1.0 - threshold)
    }
}

/// Information about the screen/display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenInfo {
    /// Screen width in pixels
    pub width: u32,

    /// Screen height in pixels
    pub height: u32,

    /// Index of the active monitor (for multi-monitor setups)
    pub active_monitor: u32,

    /// Total number of monitors
    pub monitor_count: u32,
}

impl ScreenInfo {
    /// Get the observation dimension for screen info
    pub fn dim() -> usize {
        2 // We only include normalized aspect ratio info
    }

    /// Convert to a vector for neural network input
    pub fn to_vector(&self) -> Vec<f32> {
        // Provide aspect ratio as normalized feature
        let aspect = self.width as f32 / self.height as f32;
        // Normalize typical aspects (0.5 to 3.0 range) to roughly 0-1
        let normalized_aspect = (aspect - 0.5) / 2.5;

        vec![
            normalized_aspect,
            (self.monitor_count as f32 - 1.0) / 3.0, // Normalize monitor count
        ]
    }

    /// Convert pixel coordinates to normalized coordinates
    pub fn normalize(&self, pixel_x: i32, pixel_y: i32) -> (f32, f32) {
        (
            pixel_x as f32 / self.width as f32,
            pixel_y as f32 / self.height as f32,
        )
    }

    /// Convert normalized coordinates to pixel coordinates
    pub fn denormalize(&self, norm_x: f32, norm_y: f32) -> (i32, i32) {
        (
            (norm_x * self.width as f32) as i32,
            (norm_y * self.height as f32) as i32,
        )
    }
}

/// Enhanced observations that require additional permissions/hardware.
/// These are optional and user must opt-in for privacy reasons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedObservation {
    /// Eye tracking data (requires webcam access)
    pub eye_tracking: Option<EyeTrackingState>,

    /// UI element information (requires accessibility API access)
    pub ui_elements: Option<UIElementInfo>,

    /// Active window information (requires window enumeration access)
    pub active_window: Option<ActiveWindowInfo>,

    /// Screen content embedding (requires screen capture access)
    pub screen_embedding: Option<ScreenEmbedding>,
}

impl EnhancedObservation {
    /// Get the total dimension of enabled enhanced observations
    pub fn dim(&self) -> usize {
        let mut dim = 0;

        if let Some(_eye) = &self.eye_tracking {
            dim += EyeTrackingState::dim();
        }
        if let Some(_ui) = &self.ui_elements {
            dim += UIElementInfo::dim();
        }
        if let Some(_window) = &self.active_window {
            dim += ActiveWindowInfo::dim();
        }
        if let Some(screen) = &self.screen_embedding {
            dim += screen.dim();
        }

        dim
    }

    /// Convert to a vector for neural network input
    pub fn to_vector(&self) -> Vec<f32> {
        let mut vec = Vec::new();

        if let Some(eye) = &self.eye_tracking {
            vec.extend(eye.to_vector());
        }
        if let Some(ui) = &self.ui_elements {
            vec.extend(ui.to_vector());
        }
        if let Some(window) = &self.active_window {
            vec.extend(window.to_vector());
        }
        if let Some(screen) = &self.screen_embedding {
            vec.extend(&screen.embedding);
        }

        vec
    }
}

/// Eye tracking state from webcam-based gaze estimation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EyeTrackingState {
    /// Estimated gaze X position (normalized, 0.0 = left, 1.0 = right)
    pub gaze_x: f32,

    /// Estimated gaze Y position (normalized, 0.0 = top, 1.0 = bottom)
    pub gaze_y: f32,

    /// Confidence in the gaze estimate (0.0 = no confidence, 1.0 = high confidence)
    pub confidence: f32,

    /// Whether eyes are detected
    pub eyes_detected: bool,

    /// Blink state (true if currently blinking)
    pub blinking: bool,

    /// How long the current fixation has lasted (seconds)
    pub fixation_duration: f32,
}

impl EyeTrackingState {
    pub fn dim() -> usize {
        6
    }

    pub fn to_vector(&self) -> Vec<f32> {
        vec![
            self.gaze_x,
            self.gaze_y,
            self.confidence,
            if self.eyes_detected { 1.0 } else { 0.0 },
            if self.blinking { 1.0 } else { 0.0 },
            self.fixation_duration.min(5.0) / 5.0, // Normalize to 0-1
        ]
    }
}

/// Information about UI elements near the cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIElementInfo {
    /// Whether cursor is over a clickable element
    pub over_clickable: bool,

    /// Type of element under cursor (encoded)
    pub element_type: UIElementType,

    /// Distance to nearest clickable element (normalized)
    pub distance_to_clickable: f32,

    /// Direction to nearest clickable element (radians, 0 = right)
    pub direction_to_clickable: f32,
}

impl UIElementInfo {
    pub fn dim() -> usize {
        5
    }

    pub fn to_vector(&self) -> Vec<f32> {
        vec![
            if self.over_clickable { 1.0 } else { 0.0 },
            self.element_type.to_float(),
            self.distance_to_clickable.min(1.0),
            self.direction_to_clickable / std::f32::consts::PI, // Normalize to -1 to 1
            0.0,                                                // Reserved for future use
        ]
    }
}

/// Types of UI elements (simplified for neural network consumption).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum UIElementType {
    Unknown,
    Button,
    Link,
    TextField,
    Checkbox,
    Slider,
    Menu,
    ScrollArea,
    Text,
    Image,
}

impl UIElementType {
    /// Convert to a float for neural network input
    pub fn to_float(&self) -> f32 {
        match self {
            UIElementType::Unknown => 0.0,
            UIElementType::Button => 0.1,
            UIElementType::Link => 0.2,
            UIElementType::TextField => 0.3,
            UIElementType::Checkbox => 0.4,
            UIElementType::Slider => 0.5,
            UIElementType::Menu => 0.6,
            UIElementType::ScrollArea => 0.7,
            UIElementType::Text => 0.8,
            UIElementType::Image => 0.9,
        }
    }
}

/// Information about the currently active window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveWindowInfo {
    /// Hash of the application name (for privacy - we don't store actual names)
    pub app_hash: u32,

    /// Whether this window is a browser
    pub is_browser: bool,

    /// Whether this window is a terminal/console
    pub is_terminal: bool,

    /// Whether this window is fullscreen
    pub is_fullscreen: bool,
}

impl ActiveWindowInfo {
    pub fn dim() -> usize {
        4
    }

    pub fn to_vector(&self) -> Vec<f32> {
        vec![
            (self.app_hash % 1000) as f32 / 1000.0, // Coarse app identifier
            if self.is_browser { 1.0 } else { 0.0 },
            if self.is_terminal { 1.0 } else { 0.0 },
            if self.is_fullscreen { 1.0 } else { 0.0 },
        ]
    }
}

/// Embedded representation of screen content near the cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenEmbedding {
    /// Fixed-size embedding vector (from a pre-trained encoder)
    pub embedding: Vec<f32>,
}

impl ScreenEmbedding {
    /// Default embedding dimension
    pub const DEFAULT_DIM: usize = 64;

    pub fn dim(&self) -> usize {
        self.embedding.len()
    }
}

/// Configuration for what observations to collect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationConfig {
    /// Enable eye tracking (requires webcam)
    pub eye_tracking: bool,

    /// Enable UI element detection (requires accessibility API)
    pub ui_elements: bool,

    /// Enable active window tracking
    pub active_window: bool,

    /// Enable screen content embedding (requires screen capture)
    pub screen_embedding: bool,

    /// Screen capture radius in pixels (if screen_embedding enabled)
    pub capture_radius: u32,

    /// List of app names to exclude from tracking (privacy)
    pub excluded_apps: Vec<String>,
}

impl Default for ObservationConfig {
    /// Minimal default: only required observations
    fn default() -> Self {
        Self {
            eye_tracking: false,
            ui_elements: false,
            active_window: false,
            screen_embedding: false,
            capture_radius: 100,
            excluded_apps: Vec::new(),
        }
    }
}

impl ObservationConfig {
    /// Full observation space for maximum performance
    pub fn full() -> Self {
        Self {
            eye_tracking: true,
            ui_elements: true,
            active_window: true,
            screen_embedding: true,
            capture_radius: 150,
            excluded_apps: Vec::new(),
        }
    }
}
