//! # Windows Platform Implementation
//!
//! This module provides the Windows-specific implementation of the Platform trait.
//! Windows uses the `SendInput` Win32 API for input simulation, which injects
//! events directly into the input queue.
//!
//! ## How SendInput Works
//!
//! Unlike Linux's uinput (which creates a virtual device), Windows' SendInput
//! directly injects input events into the system's input queue. This is simpler
//! but has some limitations:
//!
//! - Events injected by a non-elevated process may not reach elevated (admin) windows
//! - Some games with anti-cheat software may block or detect injected input
//! - User Interface Privilege Isolation (UIPI) can block cross-elevation input
//!
//! ## Permissions
//!
//! In most cases, no special permissions are needed on Windows. However:
//!
//! - If NeuroHID runs as a normal user, it can only send input to apps running
//!   at the same or lower privilege level
//! - To send input to admin applications, NeuroHID itself must run as admin
//! - Some security software may flag input injection as suspicious
//!
//! ## Input Types
//!
//! Windows distinguishes between several input types:
//!
//! - **Keyboard**: Uses `KEYBDINPUT` structure with virtual key codes
//! - **Mouse**: Uses `MOUSEINPUT` structure with coordinates and button flags
//! - **Hardware**: Low-level hardware events (we don't use this)
//!
//! This implementation uses the high-level enigo crate, which wraps SendInput
//! with a safe Rust interface.

use neurohid_types::{
    action::{Key, MouseButton, MouseMovement},
    error::{PlatformError, Result},
    observation::ScreenInfo,
};

use crate::traits::Platform;

/// Windows platform implementation using SendInput for input simulation.
pub struct WindowsPlatform {
    enigo: enigo::Enigo,
}

impl WindowsPlatform {
    /// Creates a new Windows platform instance that simulates input.
    pub fn new() -> Result<Self> {
        let enigo = enigo::Enigo::new(&enigo::Settings::default())
            .map_err(|_e| PlatformError::HidNotAvailable)?;

        Ok(Self { enigo })
    }
}

impl Platform for WindowsPlatform {
    fn platform_name(&self) -> &'static str {
        "Windows (SendInput)"
    }

    fn check_input_permissions(&self) -> Result<()> {
        // On Windows, we can almost always send input
        // The main issue is elevated apps, which we can't easily detect
        // For now, just return success and let failures happen at runtime
        Ok(())
    }

    fn check_query_permissions(&self) -> Result<()> {
        // Screen queries don't require special permissions on Windows
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
                return Err(PlatformError::NotSupported(format!(
                    "Extra mouse button {} not supported (only 0=Back, 1=Forward)",
                    n
                ))
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
                return Err(PlatformError::NotSupported(format!(
                    "Extra mouse button {} not supported (only 0=Back, 1=Forward)",
                    n
                ))
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

        if dy.abs() > 0.1 {
            // Windows scroll is in units of WHEEL_DELTA (120)
            // We scale accordingly
            let clicks = (dy * 3.0) as i32;
            self.enigo
                .scroll(clicks, Axis::Vertical)
                .map_err(|e| PlatformError::InputEmissionFailed(e.to_string()))?;
        }

        if dx.abs() > 0.1 {
            let clicks = (dx * 3.0) as i32;
            self.enigo
                .scroll(clicks, Axis::Horizontal)
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
        // On Windows, we can use GetCursorPos from the Win32 API
        // For now, use a placeholder - this will be implemented with the windows crate

        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::POINT;
            use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

            let mut point = POINT { x: 0, y: 0 };
            unsafe {
                GetCursorPos(&mut point)
                    .map_err(|e| PlatformError::CursorQueryFailed(e.to_string()))?;
            }
            Ok((point.x, point.y))
        }

        #[cfg(not(target_os = "windows"))]
        Err(PlatformError::NotSupported("Not on Windows".to_string()).into())
    }

    fn get_screen_info(&self) -> Result<ScreenInfo> {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                GetSystemMetrics, SM_CMONITORS, SM_CXSCREEN, SM_CYSCREEN,
            };

            unsafe {
                let width = GetSystemMetrics(SM_CXSCREEN) as u32;
                let height = GetSystemMetrics(SM_CYSCREEN) as u32;
                let monitor_count = GetSystemMetrics(SM_CMONITORS) as u32;

                Ok(ScreenInfo {
                    width,
                    height,
                    active_monitor: 0,
                    monitor_count,
                })
            }
        }

        #[cfg(not(target_os = "windows"))]
        Ok(ScreenInfo {
            width: 1920,
            height: 1080,
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
