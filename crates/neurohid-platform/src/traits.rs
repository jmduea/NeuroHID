//! # Platform Traits
//!
//! This module defines the abstractions for platform-specific operations.
//! The goal is to provide a uniform interface that works across Linux, Windows,
//! and macOS while handling the significant differences in how each platform
//! approaches input simulation and system queries.
//!
//! ## Platform Differences: A Brief Overview
//!
//! Understanding why we need this abstraction layer requires knowing a bit about
//! how each platform handles input:
//!
//! ### Linux
//! Linux uses the `uinput` kernel module to create virtual input devices. When we
//! create a virtual mouse, it appears to the system as a real hardware device.
//! This is very clean but requires appropriate permissions (user must be in the
//! `input` group or have udev rules configured).
//!
//! ### Windows
//! Windows uses the `SendInput` API to inject input events into the input queue.
//! This is straightforward but can be blocked by applications running with higher
//! privileges (admin apps won't receive input from non-admin injectors).
//!
//! ### macOS
//! macOS uses the Quartz Event Services API (`CGEvent`). This requires the
//! "Accessibility" permission, which users must explicitly grant in System
//! Preferences. Without this permission, input simulation silently fails.
//!
//! ## Design Principles
//!
//! 1. **Fail clearly**: If permissions are missing, tell the user exactly what
//!    to do to fix it, rather than silently failing.
//!
//! 2. **Minimal surface area**: Only expose what we actually need. We're not
//!    building a general-purpose input library.
//!
//! 3. **Synchronous core**: The actual input emission is synchronous and fast.
//!    Async wrappers can be added at higher levels if needed.

use neurohid_types::{
    action::{Key, MouseButton, MouseMovement},
    error::Result,
    observation::{CursorState, ScreenInfo},
};

/// The core trait for platform-specific operations.
///
/// This trait provides everything we need to:
/// 1. Emit input events (mouse movement, clicks, key presses)
/// 2. Query system state (cursor position, screen info)
/// 3. Check permissions and capabilities
///
/// ## Thread Safety
///
/// Implementations must be `Send` but are NOT required to be `Sync`. Input
/// emission typically needs to happen from a single thread (the platform's
/// "main" thread in some cases), so we don't enforce `Sync`. The service
/// architecture handles this by having a dedicated input thread.
///
/// ## Error Handling
///
/// All methods that can fail return `Result<T>`. The errors include hints
/// for users about how to resolve permission issues.
pub trait Platform: Send {
    // =========================================================================
    // Capability Checks
    // =========================================================================

    /// Returns the name of this platform implementation (for logging/debugging).
    fn platform_name(&self) -> &'static str;

    /// Checks if input simulation is available and permitted.
    ///
    /// This should verify:
    /// - Required APIs/kernel modules are available
    /// - Necessary permissions are granted
    /// - No blocking conditions exist (e.g., secure input mode on macOS)
    ///
    /// # Returns
    ///
    /// `Ok(())` if input simulation will work, or a descriptive error explaining
    /// what's missing and how to fix it.
    fn check_input_permissions(&self) -> Result<()>;

    /// Checks if screen/cursor queries are available.
    ///
    /// This is usually less restricted than input simulation, but may still
    /// require certain permissions on some platforms.
    fn check_query_permissions(&self) -> Result<()>;

    // =========================================================================
    // Input Emission
    // =========================================================================

    /// Moves the mouse cursor by a relative amount.
    ///
    /// # Arguments
    ///
    /// * `movement` - The relative movement in pixels (dx, dy).
    ///   Positive dx moves right, positive dy moves down.
    ///
    /// # Notes
    ///
    /// The movement is relative to the current cursor position. The actual
    /// pixel distance may be affected by system mouse acceleration settings
    /// (which we don't try to compensate for - that would be fighting the user's
    /// preferences).
    fn emit_mouse_move(&mut self, movement: MouseMovement) -> Result<()>;

    /// Moves the mouse cursor to an absolute screen position.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate in pixels from the left edge of the primary display
    /// * `y` - Y coordinate in pixels from the top edge of the primary display
    ///
    /// # Notes
    ///
    /// For multi-monitor setups, coordinates may extend beyond the primary
    /// display's dimensions. Negative coordinates are valid if there are
    /// monitors to the left of or above the primary display.
    fn emit_mouse_move_absolute(&mut self, x: i32, y: i32) -> Result<()>;

    /// Presses a mouse button (without releasing).
    ///
    /// # Arguments
    ///
    /// * `button` - Which button to press
    ///
    /// # Notes
    ///
    /// The button remains pressed until `emit_mouse_release` is called.
    /// For a simple click, call `emit_mouse_click` instead.
    fn emit_mouse_press(&mut self, button: MouseButton) -> Result<()>;

    /// Releases a mouse button.
    ///
    /// # Arguments
    ///
    /// * `button` - Which button to release
    fn emit_mouse_release(&mut self, button: MouseButton) -> Result<()>;

    /// Performs a complete mouse click (press then release).
    ///
    /// # Arguments
    ///
    /// * `button` - Which button to click
    ///
    /// # Notes
    ///
    /// This is equivalent to calling `emit_mouse_press` followed by
    /// `emit_mouse_release`, but may be implemented more efficiently
    /// on some platforms.
    fn emit_mouse_click(&mut self, button: MouseButton) -> Result<()> {
        // Default implementation; platforms can override if they have
        // a more efficient combined operation
        self.emit_mouse_press(button)?;
        self.emit_mouse_release(button)?;
        Ok(())
    }

    /// Performs a scroll wheel action.
    ///
    /// # Arguments
    ///
    /// * `dx` - Horizontal scroll amount (positive = right)
    /// * `dy` - Vertical scroll amount (positive = down, but UI convention
    ///   often inverts this so positive scrolls content up)
    fn emit_scroll(&mut self, dx: f32, dy: f32) -> Result<()>;

    /// Presses a key (without releasing).
    ///
    /// # Arguments
    ///
    /// * `key` - Which key to press
    fn emit_key_press(&mut self, key: Key) -> Result<()>;

    /// Releases a key.
    ///
    /// # Arguments
    ///
    /// * `key` - Which key to release
    fn emit_key_release(&mut self, key: Key) -> Result<()>;

    /// Performs a complete key tap (press then release).
    ///
    /// # Arguments
    ///
    /// * `key` - Which key to tap
    fn emit_key_tap(&mut self, key: Key) -> Result<()> {
        // Default implementation
        self.emit_key_press(key)?;
        self.emit_key_release(key)?;
        Ok(())
    }

    // =========================================================================
    // State Queries
    // =========================================================================

    /// Gets the current cursor position in screen coordinates.
    ///
    /// # Returns
    ///
    /// A tuple of (x, y) pixel coordinates. For multi-monitor setups, this is
    /// in the virtual screen coordinate space (which may include negative values
    /// if monitors are positioned left of or above the primary display).
    fn get_cursor_position(&self) -> Result<(i32, i32)>;

    /// Gets information about the screen/display configuration.
    ///
    /// # Returns
    ///
    /// A `ScreenInfo` struct containing screen dimensions and monitor information.
    fn get_screen_info(&self) -> Result<ScreenInfo>;

    /// Gets the complete cursor state (position + velocity + button state).
    ///
    /// # Arguments
    ///
    /// * `prev_state` - The previous cursor state (used to compute velocity)
    /// * `dt_seconds` - Time since the previous state was captured
    ///
    /// # Returns
    ///
    /// A complete `CursorState` with normalized position and computed velocity.
    fn get_cursor_state(
        &self,
        prev_state: Option<&CursorState>,
        dt_seconds: f32,
    ) -> Result<CursorState> {
        // Default implementation that works for all platforms
        let (x, y) = self.get_cursor_position()?;
        let screen = self.get_screen_info()?;

        // Normalize coordinates to [0, 1]
        let norm_x = x as f32 / screen.width as f32;
        let norm_y = y as f32 / screen.height as f32;

        // Compute velocity if we have a previous state
        let (velocity_x, velocity_y) = if let Some(prev) = prev_state {
            if dt_seconds > 0.0 {
                (
                    (norm_x - prev.x) / dt_seconds,
                    (norm_y - prev.y) / dt_seconds,
                )
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };

        Ok(CursorState {
            x: norm_x,
            y: norm_y,
            velocity_x,
            velocity_y,
            // Button state cannot be queried reliably across platforms from
            // the Platform trait alone. Callers (e.g., the action executor)
            // should track press/release state from emit_mouse_press/release
            // calls instead of relying on this field.
            button_held: false,
        })
    }
}

/// Factory function to create the appropriate platform implementation.
///
/// This function detects the current platform and returns the appropriate
/// implementation. It's the main entry point for getting a Platform instance.
///
/// # Returns
///
/// A boxed Platform implementation for the current OS.
///
/// # Errors
///
/// Returns an error if the platform is not supported or if initialization fails.
///
/// # Example
///
/// ```ignore
/// let platform = create_platform()?;
/// platform.check_input_permissions()?;
/// platform.emit_mouse_move(MouseMovement { dx: 10.0, dy: 5.0 })?;
/// ```
pub fn create_platform() -> Result<Box<dyn Platform>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(super::linux::LinuxPlatform::new()?))
    }

    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(super::windows::WindowsPlatform::new()?))
    }

    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(super::macos::MacOSPlatform::new()?))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(PlatformError::NotSupported("Current OS is not supported".to_string()).into())
    }
}

/// Extension trait providing additional convenience methods for platforms.
///
/// These methods are built on top of the core `Platform` trait and work
/// uniformly across all platform implementations.
pub trait PlatformExt: Platform {
    /// Performs a double-click at the current cursor position.
    ///
    /// # Arguments
    ///
    /// * `button` - Which button to double-click
    fn emit_double_click(&mut self, button: MouseButton) -> Result<()> {
        self.emit_mouse_click(button)?;
        // Small delay between clicks (most systems require this to register as double-click)
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.emit_mouse_click(button)?;
        Ok(())
    }

    /// Moves the cursor to a position and clicks.
    ///
    /// # Arguments
    ///
    /// * `x` - Target X coordinate
    /// * `y` - Target Y coordinate
    /// * `button` - Which button to click
    fn move_and_click(&mut self, x: i32, y: i32, button: MouseButton) -> Result<()> {
        self.emit_mouse_move_absolute(x, y)?;
        std::thread::sleep(std::time::Duration::from_millis(10));
        self.emit_mouse_click(button)?;
        Ok(())
    }

    /// Checks if the cursor is within the screen bounds.
    fn is_cursor_in_bounds(&self) -> Result<bool> {
        let (x, y) = self.get_cursor_position()?;
        let screen = self.get_screen_info()?;

        Ok(x >= 0 && x < screen.width as i32 && y >= 0 && y < screen.height as i32)
    }
}

// Blanket implementation of PlatformExt for all Platform implementors
impl<T: Platform + ?Sized> PlatformExt for T {}

/// Configuration for platform initialization.
///
/// This allows customizing platform behavior where supported.
#[derive(Debug, Clone, Default)]
pub struct PlatformConfig {
    /// Whether to attempt to request necessary permissions if missing.
    /// On macOS, this can trigger the accessibility permission dialog.
    pub request_permissions: bool,

    /// Whether to use high-precision mouse movement where available.
    /// This may have higher CPU cost but smoother movement.
    pub high_precision_mouse: bool,

    /// Whether to print platform-specific setup hints on permission errors.
    pub verbose_errors: bool,
}

/// Information about why input permissions might be missing and how to fix them.
#[derive(Debug, Clone)]
pub struct PermissionHint {
    /// A brief description of the issue
    pub message: String,

    /// Detailed instructions for resolving the issue
    pub instructions: Vec<String>,

    /// A command the user could run, if applicable
    pub suggested_command: Option<String>,
}

impl PermissionHint {
    /// Creates a Linux-specific permission hint for uinput access.
    pub fn linux_uinput() -> Self {
        Self {
            message: "Cannot access /dev/uinput for input simulation".to_string(),
            instructions: vec![
                "Create a udev rule to grant your user access to /dev/uinput:".to_string(),
                "".to_string(),
                "  sudo bash -c 'echo \"KERNEL==\\\"uinput\\\", GROUP=\\\"input\\\", MODE=\\\"0660\\\"\" > /etc/udev/rules.d/99-neurohid.rules'".to_string(),
                "  sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput".to_string(),
                "".to_string(),
                "Then ensure your user is in the 'input' group:".to_string(),
                "  sudo usermod -a -G input $USER".to_string(),
                "".to_string(),
                "Log out and back in for group changes to take effect.".to_string(),
            ],
            suggested_command: Some(
                "sudo bash -c 'echo \"KERNEL==\\\"uinput\\\", GROUP=\\\"input\\\", MODE=\\\"0660\\\"\" > /etc/udev/rules.d/99-neurohid.rules' && sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput".to_string()
            ),
        }
    }

    /// Creates a macOS-specific permission hint for accessibility access.
    pub fn macos_accessibility() -> Self {
        Self {
            message: "Accessibility permission required for input simulation".to_string(),
            instructions: vec![
                "Grant accessibility permission to NeuroHID:".to_string(),
                "1. Open System Preferences > Security & Privacy > Privacy".to_string(),
                "2. Select 'Accessibility' in the sidebar".to_string(),
                "3. Click the lock icon and enter your password".to_string(),
                "4. Add NeuroHID to the list and enable the checkbox".to_string(),
                "5. Restart NeuroHID".to_string(),
            ],
            suggested_command: None,
        }
    }

    /// Creates a Windows-specific hint (usually fewer permission issues).
    pub fn windows_admin() -> Self {
        Self {
            message: "Input simulation may not work with elevated applications".to_string(),
            instructions: vec![
                "Some applications running as Administrator may not receive".to_string(),
                "simulated input from NeuroHID. If you encounter issues:".to_string(),
                "1. Try running NeuroHID as Administrator, or".to_string(),
                "2. Run the target application without elevation".to_string(),
            ],
            suggested_command: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurohid_types::{
        action::{Key, MouseButton, MouseMovement},
        error::Result,
        observation::{CursorState, ScreenInfo},
    };

    /// A mock platform with configurable cursor/screen state and call recording.
    struct MockPlatform {
        cursor_x: i32,
        cursor_y: i32,
        screen_width: u32,
        screen_height: u32,
        calls: Vec<String>,
    }

    impl MockPlatform {
        fn new(cursor_x: i32, cursor_y: i32, screen_width: u32, screen_height: u32) -> Self {
            Self {
                cursor_x,
                cursor_y,
                screen_width,
                screen_height,
                calls: Vec::new(),
            }
        }
    }

    impl Platform for MockPlatform {
        fn platform_name(&self) -> &'static str {
            "mock"
        }

        fn check_input_permissions(&self) -> Result<()> {
            Ok(())
        }

        fn check_query_permissions(&self) -> Result<()> {
            Ok(())
        }

        fn emit_mouse_move(&mut self, movement: MouseMovement) -> Result<()> {
            self.calls
                .push(format!("mouse_move({}, {})", movement.dx, movement.dy));
            Ok(())
        }

        fn emit_mouse_move_absolute(&mut self, x: i32, y: i32) -> Result<()> {
            self.calls.push(format!("mouse_move_absolute({x}, {y})"));
            self.cursor_x = x;
            self.cursor_y = y;
            Ok(())
        }

        fn emit_mouse_press(&mut self, button: MouseButton) -> Result<()> {
            self.calls.push(format!("mouse_press({button:?})"));
            Ok(())
        }

        fn emit_mouse_release(&mut self, button: MouseButton) -> Result<()> {
            self.calls.push(format!("mouse_release({button:?})"));
            Ok(())
        }

        fn emit_scroll(&mut self, dx: f32, dy: f32) -> Result<()> {
            self.calls.push(format!("scroll({dx}, {dy})"));
            Ok(())
        }

        fn emit_key_press(&mut self, key: Key) -> Result<()> {
            self.calls.push(format!("key_press({key:?})"));
            Ok(())
        }

        fn emit_key_release(&mut self, key: Key) -> Result<()> {
            self.calls.push(format!("key_release({key:?})"));
            Ok(())
        }

        fn get_cursor_position(&self) -> Result<(i32, i32)> {
            Ok((self.cursor_x, self.cursor_y))
        }

        fn get_screen_info(&self) -> Result<ScreenInfo> {
            Ok(ScreenInfo {
                width: self.screen_width,
                height: self.screen_height,
                active_monitor: 0,
                monitor_count: 1,
            })
        }
    }

    // ── get_cursor_state tests ──────────────────────────────────────────

    #[test]
    fn cursor_state_velocity_from_position_change() {
        let platform = MockPlatform::new(200, 400, 1000, 1000);
        let prev = CursorState {
            x: 0.1, // 100/1000
            y: 0.2, // 200/1000
            velocity_x: 0.0,
            velocity_y: 0.0,
            button_held: false,
        };

        let state = platform.get_cursor_state(Some(&prev), 1.0).unwrap();

        // norm_x = 200/1000 = 0.2, velocity_x = (0.2 - 0.1) / 1.0 = 0.1
        assert!((state.velocity_x - 0.1).abs() < 1e-5);
        // norm_y = 400/1000 = 0.4, velocity_y = (0.4 - 0.2) / 1.0 = 0.2
        assert!((state.velocity_y - 0.2).abs() < 1e-5);
    }

    #[test]
    fn cursor_state_no_previous_gives_zero_velocity() {
        let platform = MockPlatform::new(500, 500, 1000, 1000);

        let state = platform.get_cursor_state(None, 1.0).unwrap();

        assert!((state.velocity_x).abs() < 1e-5);
        assert!((state.velocity_y).abs() < 1e-5);
        assert!((state.x - 0.5).abs() < 1e-5);
        assert!((state.y - 0.5).abs() < 1e-5);
    }

    #[test]
    fn cursor_state_zero_dt_gives_zero_velocity() {
        let platform = MockPlatform::new(200, 400, 1000, 1000);
        let prev = CursorState {
            x: 0.1,
            y: 0.2,
            velocity_x: 5.0,
            velocity_y: 5.0,
            button_held: false,
        };

        let state = platform.get_cursor_state(Some(&prev), 0.0).unwrap();

        assert!((state.velocity_x).abs() < 1e-5);
        assert!((state.velocity_y).abs() < 1e-5);
    }

    // ── PlatformExt tests ───────────────────────────────────────────────

    #[test]
    fn emit_double_click_records_two_press_release_pairs() {
        let mut platform = MockPlatform::new(0, 0, 1920, 1080);

        platform.emit_double_click(MouseButton::Left).unwrap();

        assert_eq!(
            platform.calls,
            vec![
                "mouse_press(Left)",
                "mouse_release(Left)",
                "mouse_press(Left)",
                "mouse_release(Left)",
            ]
        );
    }

    #[test]
    fn move_and_click_records_move_then_click() {
        let mut platform = MockPlatform::new(0, 0, 1920, 1080);

        platform
            .move_and_click(100, 200, MouseButton::Left)
            .unwrap();

        assert_eq!(
            platform.calls,
            vec![
                "mouse_move_absolute(100, 200)",
                "mouse_press(Left)",
                "mouse_release(Left)",
            ]
        );
    }

    #[test]
    fn is_cursor_in_bounds_inside() {
        let platform = MockPlatform::new(500, 500, 1920, 1080);
        assert!(platform.is_cursor_in_bounds().unwrap());
    }

    #[test]
    fn is_cursor_in_bounds_at_edge_width() {
        // x == width is out of bounds (valid range is 0..width-1)
        let platform = MockPlatform::new(1920, 500, 1920, 1080);
        assert!(!platform.is_cursor_in_bounds().unwrap());
    }

    #[test]
    fn is_cursor_in_bounds_at_edge_height() {
        let platform = MockPlatform::new(500, 1080, 1920, 1080);
        assert!(!platform.is_cursor_in_bounds().unwrap());
    }

    #[test]
    fn is_cursor_in_bounds_negative_coords() {
        let platform = MockPlatform::new(-1, 500, 1920, 1080);
        assert!(!platform.is_cursor_in_bounds().unwrap());

        let platform = MockPlatform::new(500, -1, 1920, 1080);
        assert!(!platform.is_cursor_in_bounds().unwrap());
    }

    // ── PermissionHint constructor tests ────────────────────────────────

    #[test]
    fn permission_hint_linux_uinput() {
        let hint = PermissionHint::linux_uinput();
        assert!(
            hint.message.contains("uinput"),
            "expected 'uinput' in message: {}",
            hint.message
        );
        assert!(hint.suggested_command.is_some());
    }

    #[test]
    fn permission_hint_macos_accessibility() {
        let hint = PermissionHint::macos_accessibility();
        assert!(
            hint.message.contains("Accessibility"),
            "expected 'Accessibility' in message: {}",
            hint.message
        );
    }

    #[test]
    fn permission_hint_windows_admin() {
        let hint = PermissionHint::windows_admin();
        assert!(
            hint.message.contains("elevated"),
            "expected 'elevated' in message: {}",
            hint.message
        );
    }
}
