//! # NeuroHID Platform Abstractions
//!
//! This crate provides cross-platform abstractions for HID (Human Interface Device)
//! emulation and system queries. It's designed to hide the significant differences
//! between how Linux, Windows, and macOS handle input simulation.
//!
//! ## Why This Crate Exists
//!
//! Each operating system has its own way of simulating user input:
//!
//! - **Linux** uses the `uinput` kernel module to create virtual input devices
//! - **Windows** uses the `SendInput` Win32 API to inject input events
//! - **macOS** uses Quartz Event Services (`CGEvent`) with Accessibility permissions
//!
//! This crate provides a unified [`Platform`] trait that abstracts these differences,
//! allowing the rest of NeuroHID to work identically across all platforms.
//!
//! ## Quick Start
//!
//! ```ignore
//! use neurohid_platform::{create_platform, Platform};
//! use neurohid_types::action::{MouseMovement, MouseButton};
//!
//! // Create a platform instance (auto-detects your OS)
//! let mut platform = create_platform()?;
//!
//! // Check we have the necessary permissions
//! platform.check_input_permissions()?;
//!
//! // Move the mouse
//! platform.emit_mouse_move(MouseMovement { dx: 100.0, dy: 50.0 })?;
//!
//! // Click
//! platform.emit_mouse_click(MouseButton::Left)?;
//! ```
//!
//! ## Permission Requirements
//!
//! Each platform has different permission requirements:
//!
//! | Platform | Requirement |
//! |----------|-------------|
//! | Linux    | User in `input` group or udev rules for `/dev/uinput` |
//! | macOS    | Accessibility permission in System Preferences |
//! | Windows  | Usually none, but admin apps may not receive input |
//!
//! The [`Platform::check_input_permissions`] method will tell you exactly what's
//! needed if permissions are missing.

pub mod traits;

// Platform-specific implementations
#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

// Re-export the main types
pub use traits::{create_platform, PermissionHint, Platform, PlatformConfig, PlatformExt};

// Re-export commonly used types from neurohid-types
pub use neurohid_types::action::{Key, MouseButton, MouseMovement};
pub use neurohid_types::error::{PlatformError, Result};
pub use neurohid_types::observation::{CursorState, ScreenInfo};
