//! # macOS Platform Implementation
//!
//! This module provides the macOS-specific implementation of the Platform trait.
//! macOS uses the Quartz Event Services (CGEvent) API for input simulation,
//! which requires explicit Accessibility permissions from the user.
//!
//! ## How Quartz Events Work
//!
//! macOS provides the Core Graphics framework for creating and posting synthetic
//! input events. The process works like this:
//!
//! 1. Create a `CGEvent` with the desired event type (mouse move, key press, etc.)
//! 2. Post the event to the event tap using `CGEventPost`
//! 3. The event enters the normal input processing pipeline
//!
//! ## The Accessibility Permission Problem
//!
//! Apple takes a strict approach to security. Any app that wants to simulate
//! input MUST have explicit user permission via the Accessibility panel in
//! System Preferences. This is a deliberate friction point to prevent malware.
//!
//! The permission grant process:
//! 1. App attempts to use CGEventPost
//! 2. macOS checks if the app has Accessibility permission
//! 3. If not, the call silently fails (no error, just nothing happens!)
//! 4. User must manually add the app in System Preferences > Security & Privacy > Accessibility
//!
//! This "silent failure" behavior is particularly frustrating for users and
//! developers alike. Our implementation proactively checks for permissions and
//! provides clear instructions if they're missing.
//!
//! ## Checking Permissions
//!
//! We use `AXIsProcessTrustedWithOptions` to check if we have permission. This
//! can optionally prompt the user to grant permission (showing a dialog), but
//! the actual grant requires navigating to System Preferences.
//!
//! ## Secure Input Mode
//!
//! macOS has a "secure input" mode that's activated when you're typing in
//! password fields. During secure input, even apps with Accessibility permission
//! cannot read keyboard events (though they can still post them). This is
//! generally not a problem for NeuroHID since we're posting, not reading.

use neurohid_types::{
    action::{Key, MouseButton, MouseMovement},
    error::{PlatformError, Result},
    observation::{CursorState, ScreenInfo},
};

use crate::traits::{PermissionHint, Platform};

/// macOS platform implementation using Quartz Event Services.
pub struct MacOSPlatform {
    enigo: enigo::Enigo,
}

impl MacOSPlatform {
    /// Creates a new macOS platform instance.
    ///
    /// This initializes the Quartz event system. Unlike the initialization on
    /// other platforms, this may succeed even without Accessibility permission,
    /// but subsequent input emission will silently fail without permission.
    pub fn new() -> Result<Self> {
        let enigo = enigo::Enigo::new(&enigo::Settings::default())
            .map_err(|e| PlatformError::HidNotAvailable)?;

        Ok(Self { enigo })
    }

    /// Checks if the application has Accessibility permission.
    ///
    /// Accessibility permission is required for macOS input simulation via
    /// Quartz Events / `CGEventPost`. Without it, input events silently fail.
    /// The actual permission grant requires the user to navigate to
    /// System Settings > Privacy & Security > Accessibility.
    #[cfg(target_os = "macos")]
    fn check_accessibility_permission() -> Result<()> {
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;

        let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
        let value = CFBoolean::false_value();
        let options = CFDictionary::from_CFType_pairs(&[(key, value)]);

        // SAFETY: `AXIsProcessTrustedWithOptions` is a read-only query into the
        // TCC database.  The `CFDictionaryRef` we pass is valid for the duration
        // of the call and we do not retain any pointer afterward.
        extern "C" {
            fn AXIsProcessTrustedWithOptions(options: core_foundation::base::CFTypeRef) -> bool;
        }
        let trusted = unsafe { AXIsProcessTrustedWithOptions(options.as_CFTypeRef()) };

        if trusted {
            Ok(())
        } else {
            Err(PlatformError::PermissionDenied {
                hint: "Accessibility permission required. Grant access in \
                      System Settings > Privacy & Security > Accessibility."
                    .to_string(),
            }
            .into())
        }
    }
}

impl Platform for MacOSPlatform {
    fn platform_name(&self) -> &'static str {
        "macOS (Quartz Events)"
    }

    fn check_input_permissions(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            Self::check_accessibility_permission()?;
        }

        Ok(())
    }

    fn check_query_permissions(&self) -> Result<()> {
        // Screen queries generally don't require special permissions on macOS
        // (though screen recording does, we're not doing that)
        Ok(())
    }

    fn emit_mouse_move(&mut self, movement: MouseMovement) -> Result<()> {
        use enigo::{Coordinate, Enigo, Mouse};

        self.enigo
            .move_mouse(movement.dx as i32, movement.dy as i32, Coordinate::Rel)
            .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;

        Ok(())
    }

    fn emit_mouse_move_absolute(&mut self, x: i32, y: i32) -> Result<()> {
        use enigo::{Coordinate, Enigo, Mouse};

        self.enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;

        Ok(())
    }

    fn emit_mouse_press(&mut self, button: MouseButton) -> Result<()> {
        use enigo::{Button, Direction, Mouse};

        let btn = match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
            MouseButton::Extra(0) => Button::Back,
            MouseButton::Extra(1) => Button::Forward,
            MouseButton::Extra(n) => {
                return Err(PlatformError::NotSupported(format!(
                    "Extra mouse button {} not supported (only 0=Back, 1=Forward)",
                    n
                ))
                .into());
            }
        };

        self.enigo
            .button(btn, Direction::Press)
            .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;

        Ok(())
    }

    fn emit_mouse_release(&mut self, button: MouseButton) -> Result<()> {
        use enigo::{Button, Direction, Mouse};

        let btn = match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
            MouseButton::Extra(0) => Button::Back,
            MouseButton::Extra(1) => Button::Forward,
            MouseButton::Extra(n) => {
                return Err(PlatformError::NotSupported(format!(
                    "Extra mouse button {} not supported (only 0=Back, 1=Forward)",
                    n
                ))
                .into());
            }
        };

        self.enigo
            .button(btn, Direction::Release)
            .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;

        Ok(())
    }

    fn emit_scroll(&mut self, dx: f32, dy: f32) -> Result<()> {
        use enigo::{Axis, Enigo, Mouse};

        // macOS uses "natural scrolling" by default where positive = scroll up
        // We follow screen coordinates convention (positive = down)
        // so we invert here
        if dy.abs() > 0.1 {
            self.enigo
                .scroll(-dy as i32, Axis::Vertical)
                .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;
        }

        if dx.abs() > 0.1 {
            self.enigo
                .scroll(-dx as i32, Axis::Horizontal)
                .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;
        }

        Ok(())
    }

    fn emit_key_press(&mut self, key: Key) -> Result<()> {
        use enigo::{Direction, Enigo, Keyboard};

        let enigo_key = key_to_enigo(key)?;

        self.enigo
            .key(enigo_key, Direction::Press)
            .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;

        Ok(())
    }

    fn emit_key_release(&mut self, key: Key) -> Result<()> {
        use enigo::{Direction, Enigo, Keyboard};

        let enigo_key = key_to_enigo(key)?;

        self.enigo
            .key(enigo_key, Direction::Release)
            .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;

        Ok(())
    }

    fn get_cursor_position(&self) -> Result<(i32, i32)> {
        use enigo::Mouse;

        self.enigo
            .location()
            .map_err(|e| PlatformError::CursorQueryFailed(e.to_string()).into())
    }

    fn get_screen_info(&self) -> Result<ScreenInfo> {
        use enigo::Mouse;

        let (width, height) = self.enigo.main_display().map_err(|e| {
            PlatformError::NotSupported(format!("Failed to query display info: {}", e))
        })?;

        Ok(ScreenInfo {
            width: width as u32,
            height: height as u32,
            active_monitor: 0,
            monitor_count: 1,
        })
    }
}

/// Converts our Key enum to Enigo's Key enum.
fn key_to_enigo(key: Key) -> Result<enigo::Key> {
    use enigo::Key as EKey;

    let enigo_key = match key {
        Key::ArrowUp => EKey::UpArrow,
        Key::ArrowDown => EKey::DownArrow,
        Key::ArrowLeft => EKey::LeftArrow,
        Key::ArrowRight => EKey::RightArrow,
        Key::Enter => EKey::Return,
        Key::Space => EKey::Space,
        Key::Escape => EKey::Escape,
        Key::Backspace => EKey::Backspace,
        Key::Tab => EKey::Tab,
        Key::Shift => EKey::Shift,
        Key::Control => EKey::Control,
        Key::Alt => EKey::Alt,
        Key::Meta => EKey::Meta, // Command key on Mac
        Key::Letter(c) => EKey::Unicode(c),
        Key::Number(n) if n <= 9 => EKey::Unicode((b'0' + n) as char),
        Key::Function(n) if n >= 1 && n <= 12 => match n {
            1 => EKey::F1,
            2 => EKey::F2,
            3 => EKey::F3,
            4 => EKey::F4,
            5 => EKey::F5,
            6 => EKey::F6,
            7 => EKey::F7,
            8 => EKey::F8,
            9 => EKey::F9,
            10 => EKey::F10,
            11 => EKey::F11,
            12 => EKey::F12,
            _ => {
                return Err(PlatformError::NotSupported(format!(
                    "Function key F{} not supported",
                    n
                ))
                .into());
            }
        },
        _ => {
            return Err(PlatformError::NotSupported(format!("Key {:?} not supported", key)).into());
        }
    };

    Ok(enigo_key)
}
