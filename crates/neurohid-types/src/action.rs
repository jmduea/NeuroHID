//! # Action Types
//!
//! Types representing HID output actions: mouse movements, clicks, and key presses.
//! These are the actions the decoder can output to control the computer.
//!
//! ## Design Notes
//!
//! Actions are designed to be:
//! - Serializable (for IPC with Python ML layer)
//! - Composable (multiple actions can be combined)
//! - Platform-agnostic (platform layer translates to native calls)

use crate::Timestamp;
use serde::{Deserialize, Serialize};

/// A complete action the decoder can output.
/// This may contain multiple sub-actions (e.g., move mouse AND click).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// Timestamp when this action was decided
    pub timestamp: Timestamp,

    /// Mouse-related actions
    pub mouse: Option<MouseAction>,

    /// Keyboard-related actions
    pub keyboard: Option<KeyAction>,

    /// Confidence score for this action (0.0 to 1.0)
    /// Higher values mean the decoder is more certain
    pub confidence: f32,

    /// Stable decision identifier used to correlate runtime events
    /// (decision -> ErrP window -> ErrP result -> training episode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<String>,
}

impl Action {
    /// Create an empty action (no-op)
    pub fn none() -> Self {
        Self {
            timestamp: crate::now_micros(),
            mouse: None,
            keyboard: None,
            confidence: 1.0,
            decision_id: None,
        }
    }

    /// Create a mouse-only action
    pub fn mouse(action: MouseAction) -> Self {
        Self {
            timestamp: crate::now_micros(),
            mouse: Some(action),
            keyboard: None,
            confidence: 1.0,
            decision_id: None,
        }
    }

    /// Create a keyboard-only action
    pub fn key(action: KeyAction) -> Self {
        Self {
            timestamp: crate::now_micros(),
            mouse: None,
            keyboard: Some(action),
            confidence: 1.0,
            decision_id: None,
        }
    }

    /// Set the confidence for this action
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Check if this is a no-op action
    pub fn is_none(&self) -> bool {
        self.mouse.is_none() && self.keyboard.is_none()
    }
}

/// Mouse-related actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseAction {
    /// Relative movement (in normalized units, will be scaled by sensitivity)
    pub movement: Option<MouseMovement>,

    /// Button press/release events
    pub buttons: Vec<MouseButtonEvent>,

    /// Scroll wheel movement
    pub scroll: Option<ScrollMovement>,
}

impl MouseAction {
    /// Create a movement-only action
    pub fn move_relative(dx: f32, dy: f32) -> Self {
        Self {
            movement: Some(MouseMovement { dx, dy }),
            buttons: Vec::new(),
            scroll: None,
        }
    }

    /// Create a click action
    pub fn click(button: MouseButton) -> Self {
        Self {
            movement: None,
            buttons: vec![
                MouseButtonEvent {
                    button,
                    pressed: true,
                },
                MouseButtonEvent {
                    button,
                    pressed: false,
                },
            ],
            scroll: None,
        }
    }

    /// Create a button press (without release)
    pub fn press(button: MouseButton) -> Self {
        Self {
            movement: None,
            buttons: vec![MouseButtonEvent {
                button,
                pressed: true,
            }],
            scroll: None,
        }
    }

    /// Create a button release
    pub fn release(button: MouseButton) -> Self {
        Self {
            movement: None,
            buttons: vec![MouseButtonEvent {
                button,
                pressed: false,
            }],
            scroll: None,
        }
    }

    /// Create a scroll action
    pub fn scroll(dx: f32, dy: f32) -> Self {
        Self {
            movement: None,
            buttons: Vec::new(),
            scroll: Some(ScrollMovement { dx, dy }),
        }
    }
}

/// Relative mouse movement.
/// Values are in normalized units; the platform layer will apply sensitivity scaling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MouseMovement {
    /// Horizontal movement (positive = right)
    pub dx: f32,
    /// Vertical movement (positive = down, following screen coordinates)
    pub dy: f32,
}

impl MouseMovement {
    /// Get the magnitude of the movement
    pub fn magnitude(&self) -> f32 {
        (self.dx * self.dx + self.dy * self.dy).sqrt()
    }

    /// Get the direction in radians (0 = right, PI/2 = down)
    pub fn direction(&self) -> f32 {
        self.dy.atan2(self.dx)
    }
}

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// Additional buttons (e.g., side buttons)
    Extra(u8),
}

/// A mouse button state change event.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MouseButtonEvent {
    pub button: MouseButton,
    /// true = pressed, false = released
    pub pressed: bool,
}

/// Scroll wheel movement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScrollMovement {
    /// Horizontal scroll (positive = right)
    pub dx: f32,
    /// Vertical scroll (positive = down, but often inverted in UIs)
    pub dy: f32,
}

/// Keyboard-related actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyAction {
    /// Key events to emit
    pub events: Vec<KeyEvent>,
}

impl KeyAction {
    /// Create a key press and release (a "tap")
    pub fn tap(key: Key) -> Self {
        Self {
            events: vec![
                KeyEvent { key, pressed: true },
                KeyEvent {
                    key,
                    pressed: false,
                },
            ],
        }
    }

    /// Create a key press (without release)
    pub fn press(key: Key) -> Self {
        Self {
            events: vec![KeyEvent { key, pressed: true }],
        }
    }

    /// Create a key release
    pub fn release(key: Key) -> Self {
        Self {
            events: vec![KeyEvent {
                key,
                pressed: false,
            }],
        }
    }
}

/// A keyboard key state change event.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KeyEvent {
    pub key: Key,
    /// true = pressed, false = released
    pub pressed: bool,
}

/// Key identifiers.
/// For MVP, we only support arrow keys. This will expand over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    // Arrow keys (MVP scope)
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Common keys (future)
    Enter,
    Space,
    Escape,
    Backspace,
    Tab,

    // Modifiers (future)
    Shift,
    Control,
    Alt,
    Meta, // Windows key / Command key

    // Letters (future)
    Letter(char),

    // Numbers (future)
    Number(u8),

    // Function keys (future)
    Function(u8),
}

impl Key {
    /// Check if this is an arrow key
    pub fn is_arrow(&self) -> bool {
        matches!(
            self,
            Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
        )
    }

    /// Check if this is a modifier key
    pub fn is_modifier(&self) -> bool {
        matches!(self, Key::Shift | Key::Control | Key::Alt | Key::Meta)
    }
}

/// The action space defines what actions are available to the decoder.
/// This allows different configurations for different use cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSpace {
    /// Whether mouse movement is enabled
    pub mouse_movement: bool,

    /// Which mouse buttons are available
    pub mouse_buttons: Vec<MouseButton>,

    /// Whether scrolling is enabled
    pub mouse_scroll: bool,

    /// Which keys are available
    pub keys: Vec<Key>,

    /// Movement sensitivity scaling factor
    pub movement_sensitivity: f32,

    /// Minimum confidence threshold for executing actions
    pub confidence_threshold: f32,
}

impl Default for ActionSpace {
    /// Default action space for MVP: mouse + arrow keys
    fn default() -> Self {
        Self {
            mouse_movement: true,
            mouse_buttons: vec![MouseButton::Left, MouseButton::Right],
            mouse_scroll: false,
            keys: vec![
                Key::ArrowUp,
                Key::ArrowDown,
                Key::ArrowLeft,
                Key::ArrowRight,
            ],
            movement_sensitivity: 1.0,
            confidence_threshold: 0.5,
        }
    }
}

impl ActionSpace {
    /// Create an action space with only mouse control
    pub fn mouse_only() -> Self {
        Self {
            mouse_movement: true,
            mouse_buttons: vec![MouseButton::Left, MouseButton::Right],
            mouse_scroll: true,
            keys: Vec::new(),
            movement_sensitivity: 1.0,
            confidence_threshold: 0.5,
        }
    }

    /// Create an action space with only arrow keys (for discrete control games)
    pub fn arrows_only() -> Self {
        Self {
            mouse_movement: false,
            mouse_buttons: Vec::new(),
            mouse_scroll: false,
            keys: vec![
                Key::ArrowUp,
                Key::ArrowDown,
                Key::ArrowLeft,
                Key::ArrowRight,
            ],
            movement_sensitivity: 1.0,
            confidence_threshold: 0.5,
        }
    }

    /// Get the total number of discrete actions (for discrete action space RL)
    pub fn discrete_action_count(&self) -> usize {
        let mut count = 0;

        // Mouse buttons: each can be clicked
        count += self.mouse_buttons.len();

        // Keys: each can be pressed
        count += self.keys.len();

        // Add 1 for "no action"
        count + 1
    }

    /// Get the continuous action dimension (for continuous action space RL)
    pub fn continuous_action_dim(&self) -> usize {
        let mut dim = 0;

        // Mouse movement: 2D (dx, dy)
        if self.mouse_movement {
            dim += 2;
        }

        // Scroll: 2D (dx, dy)
        if self.mouse_scroll {
            dim += 2;
        }

        dim
    }
}
