//! # Linux Platform Implementation
//!
//! This module provides the Linux-specific implementation of the Platform trait.
//! Linux is actually the cleanest platform for input simulation because of the
//! `uinput` kernel module, which lets us create virtual input devices that are
//! indistinguishable from real hardware to the rest of the system.
//!
//! ## How uinput Works
//!
//! The Linux kernel provides a special device at `/dev/uinput` that allows
//! userspace programs to create virtual input devices. When we write to this
//! device, we're telling the kernel "pretend this input event came from a
//! real keyboard/mouse." The beauty of this approach is that the events go
//! through the normal input subsystem, so they work with every application
//! and window manager.
//!
//! ## Permissions
//!
//! By default, `/dev/uinput` is owned by `root:root` with mode `0600`, so it
//! is only accessible by root. You need **both** a udev rule (to assign the
//! device to the `input` group) **and** group membership:
//!
//! 1. **Create a udev rule** to grant group access:
//!    Create `/etc/udev/rules.d/99-neurohid.rules`:
//!    ```text
//!    KERNEL=="uinput", GROUP="input", MODE="0660"
//!    ```
//!    Then run `sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput`
//!
//! 2. **Add your user to the `input` group**:
//!    ```bash
//!    sudo usermod -a -G input $USER
//!    ```
//!    Then log out and back in for the group change to take effect.
//!
//! ## Wayland vs X11
//!
//! This implementation works on both X11 and Wayland because uinput operates
//! at the kernel level, below the display server. This is a significant
//! advantage over approaches that try to inject events at the X11 level
//! (which don't work on Wayland).

use neurohid_types::{
    action::{Key, MouseButton, MouseMovement},
    error::{PlatformError, Result},
    observation::ScreenInfo,
};

use crate::traits::{PermissionHint, Platform};

/// Linux platform implementation using uinput for input simulation.
///
/// This struct manages a connection to the uinput device and provides methods
/// for emitting input events. It also handles cursor position and screen
/// information queries.
pub struct LinuxPlatform {
    // We'll use the `enigo` crate for the actual input simulation
    // It handles the uinput setup internally
    enigo: enigo::Enigo,
}

impl LinuxPlatform {
    /// Creates a new Linux platform instance.
    ///
    /// This initializes the connection to uinput. If uinput is not accessible,
    /// this will return an error with instructions for fixing permissions.
    pub fn new() -> Result<Self> {
        // Try to create an Enigo instance
        // This will fail if we don't have uinput access
        let enigo = enigo::Enigo::new(&enigo::Settings::default()).map_err(|e| {
            PlatformError::PermissionDenied {
                hint: format!(
                    "Cannot initialize input simulation: {}. {}",
                    e,
                    PermissionHint::linux_uinput().instructions.join(" ")
                ),
            }
        })?;

        Ok(Self { enigo })
    }

    /// Checks if uinput is accessible.
    ///
    /// Since `LinuxPlatform::new()` already opens `/dev/uinput` via enigo,
    /// a successful construction proves we have access. We only check that
    /// the device node still exists (hasn't been hot-removed) rather than
    /// opening a second file handle, which can fail under restrictive
    /// security modules (e.g. AppArmor in WSL2) even though enigo's
    /// existing handle works fine.
    fn check_uinput_access() -> Result<()> {
        use std::path::Path;

        let uinput_path = Path::new("/dev/uinput");

        if !uinput_path.exists() {
            return Err(PlatformError::HidNotAvailable.into());
        }

        // If we reach this point, new() already succeeded, meaning enigo
        // has a working handle to /dev/uinput. No need to re-open it.
        Ok(())
    }
}

impl Platform for LinuxPlatform {
    fn platform_name(&self) -> &'static str {
        "Linux (uinput)"
    }

    fn check_input_permissions(&self) -> Result<()> {
        Self::check_uinput_access()
    }

    fn check_query_permissions(&self) -> Result<()> {
        // Screen queries don't require special permissions on Linux
        Ok(())
    }

    fn emit_mouse_move(&mut self, movement: MouseMovement) -> Result<()> {
        use enigo::{Coordinate, Mouse};

        self.enigo
            .move_mouse(movement.dx as i32, movement.dy as i32, Coordinate::Rel)
            .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;

        Ok(())
    }

    fn emit_mouse_move_absolute(&mut self, x: i32, y: i32) -> Result<()> {
        use enigo::{Coordinate, Mouse};

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
                return Err(PlatformError::NotSupported(
                    format!("Extra mouse button {} not supported (only 0=Back, 1=Forward)", n),
                )
                .into())
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
                return Err(PlatformError::NotSupported(
                    format!("Extra mouse button {} not supported (only 0=Back, 1=Forward)", n),
                )
                .into())
            }
        };

        self.enigo
            .button(btn, Direction::Release)
            .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;

        Ok(())
    }

    fn emit_scroll(&mut self, dx: f32, dy: f32) -> Result<()> {
        use enigo::{Axis, Mouse};

        // Enigo expects integer scroll amounts
        // Most systems treat 1 unit as one "click" of the scroll wheel
        if dy.abs() > 0.1 {
            self.enigo
                .scroll(dy as i32, Axis::Vertical)
                .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;
        }

        if dx.abs() > 0.1 {
            self.enigo
                .scroll(dx as i32, Axis::Horizontal)
                .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;
        }

        Ok(())
    }

    fn emit_key_press(&mut self, key: Key) -> Result<()> {
        use enigo::{Direction, Keyboard};

        let enigo_key = key_to_enigo(key)?;

        self.enigo
            .key(enigo_key, Direction::Press)
            .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;

        Ok(())
    }

    fn emit_key_release(&mut self, key: Key) -> Result<()> {
        use enigo::{Direction, Keyboard};

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
        Key::Meta => EKey::Meta,
        Key::Letter(c) => EKey::Unicode(c),
        Key::Number(n) if n <= 9 => EKey::Unicode((b'0' + n) as char),
        Key::Function(n) if (1..=12).contains(&n) => match n {
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
                .into())
            }
        },
        _ => {
            return Err(PlatformError::NotSupported(format!("Key {:?} not supported", key)).into())
        }
    };

    Ok(enigo_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_conversion() {
        // Test that our key conversions work
        assert!(key_to_enigo(Key::ArrowUp).is_ok());
        assert!(key_to_enigo(Key::ArrowDown).is_ok());
        assert!(key_to_enigo(Key::Letter('a')).is_ok());
    }
}
