//! Python bindings for the `neurohid-platform` HID output layer.

use std::sync::Mutex;

use neurohid_platform::traits::{PlatformConfig, PlatformExt};
use pyo3::prelude::*;

use crate::types::{PyKey, PyMouseButton, PyMouseMovement};

/// Convert platform `Error` → `PyErr`.
fn plat_err(e: neurohid_types::error::Error) -> PyErr {
    crate::errors::to_py_err(e)
}

// ---------------------------------------------------------------------------
// PlatformConfig
// ---------------------------------------------------------------------------

/// Platform configuration.
#[pyclass(name = "PlatformConfig", skip_from_py_object)]
#[derive(Clone)]
pub struct PyPlatformConfig {
    pub inner: PlatformConfig,
}

#[pymethods]
impl PyPlatformConfig {
    #[new]
    #[pyo3(signature = (
        request_permissions = false,
        high_precision_mouse = false,
        verbose_errors = false,
    ))]
    fn new(request_permissions: bool, high_precision_mouse: bool, verbose_errors: bool) -> Self {
        Self {
            inner: PlatformConfig {
                request_permissions,
                high_precision_mouse,
                verbose_errors,
            },
        }
    }

    #[getter]
    fn request_permissions(&self) -> bool {
        self.inner.request_permissions
    }
    #[getter]
    fn high_precision_mouse(&self) -> bool {
        self.inner.high_precision_mouse
    }
    #[getter]
    fn verbose_errors(&self) -> bool {
        self.inner.verbose_errors
    }

    fn __repr__(&self) -> String {
        format!(
            "PlatformConfig(permissions={}, high_precision={}, verbose={})",
            self.inner.request_permissions,
            self.inner.high_precision_mouse,
            self.inner.verbose_errors
        )
    }
}

// ---------------------------------------------------------------------------
// PermissionHint
// ---------------------------------------------------------------------------

/// Platform-specific permission hint for users.
#[pyclass(name = "PermissionHint", skip_from_py_object)]
#[derive(Clone)]
pub struct PyPermissionHint {
    inner: neurohid_platform::traits::PermissionHint,
}

#[pymethods]
impl PyPermissionHint {
    #[staticmethod]
    fn linux_uinput() -> Self {
        Self {
            inner: neurohid_platform::traits::PermissionHint::linux_uinput(),
        }
    }
    #[staticmethod]
    fn macos_accessibility() -> Self {
        Self {
            inner: neurohid_platform::traits::PermissionHint::macos_accessibility(),
        }
    }
    #[staticmethod]
    fn windows_admin() -> Self {
        Self {
            inner: neurohid_platform::traits::PermissionHint::windows_admin(),
        }
    }

    #[getter]
    fn message(&self) -> &str {
        &self.inner.message
    }
    #[getter]
    fn instructions(&self) -> Vec<String> {
        self.inner.instructions.clone()
    }
    #[getter]
    fn suggested_command(&self) -> Option<&str> {
        self.inner.suggested_command.as_deref()
    }

    fn __repr__(&self) -> String {
        format!("PermissionHint('{}')", self.inner.message)
    }
}

// ---------------------------------------------------------------------------
// ScreenInfo
// ---------------------------------------------------------------------------

/// Display information.
#[pyclass(name = "ScreenInfo", skip_from_py_object)]
#[derive(Clone)]
pub struct PyScreenInfo {
    pub inner: neurohid_types::observation::ScreenInfo,
}

#[pymethods]
impl PyScreenInfo {
    #[getter]
    fn width(&self) -> u32 {
        self.inner.width
    }
    #[getter]
    fn height(&self) -> u32 {
        self.inner.height
    }
    #[getter]
    fn active_monitor(&self) -> u32 {
        self.inner.active_monitor
    }
    #[getter]
    fn monitor_count(&self) -> u32 {
        self.inner.monitor_count
    }

    fn __repr__(&self) -> String {
        format!(
            "ScreenInfo({}x{}, monitor={}/{})",
            self.inner.width, self.inner.height, self.inner.active_monitor, self.inner.monitor_count
        )
    }
}

// ---------------------------------------------------------------------------
// CursorState
// ---------------------------------------------------------------------------

/// Normalized cursor state for ML observation.
#[pyclass(name = "CursorState", skip_from_py_object)]
#[derive(Clone)]
pub struct PyCursorState {
    pub inner: neurohid_types::observation::CursorState,
}

#[pymethods]
impl PyCursorState {
    #[staticmethod]
    fn centered() -> Self {
        Self {
            inner: neurohid_types::observation::CursorState::centered(),
        }
    }

    #[getter]
    fn x(&self) -> f32 {
        self.inner.x
    }
    #[getter]
    fn y(&self) -> f32 {
        self.inner.y
    }
    #[getter]
    fn velocity_x(&self) -> f32 {
        self.inner.velocity_x
    }
    #[getter]
    fn velocity_y(&self) -> f32 {
        self.inner.velocity_y
    }
    #[getter]
    fn button_held(&self) -> bool {
        self.inner.button_held
    }

    /// Observation dimension (always 5).
    #[staticmethod]
    fn dim() -> usize {
        neurohid_types::observation::CursorState::dim()
    }

    /// Convert to a flat feature vector `[x, y, vx, vy, button_held]`.
    fn to_vector(&self) -> Vec<f32> {
        self.inner.to_vector()
    }

    fn distance_to(&self, target_x: f32, target_y: f32) -> f32 {
        self.inner.distance_to(target_x, target_y)
    }

    fn at_edge(&self, threshold: f32) -> bool {
        self.inner.at_edge(threshold)
    }

    fn __repr__(&self) -> String {
        format!(
            "CursorState(x={:.3}, y={:.3}, vx={:.3}, vy={:.3})",
            self.inner.x, self.inner.y, self.inner.velocity_x, self.inner.velocity_y
        )
    }
}

// ---------------------------------------------------------------------------
// Platform (wrapped in Mutex for thread safety)
// ---------------------------------------------------------------------------

/// Cross-platform HID output emitter.
///
/// Wraps the OS-specific `Platform` trait implementation. Methods that emit
/// input events (`emit_*`) require mutable access; a `Mutex` is used internally
/// so the `Platform` object is safe to share across Python threads.
#[pyclass(name = "Platform")]
pub struct PyPlatform {
    inner: Mutex<Box<dyn neurohid_platform::traits::Platform>>,
}

#[pymethods]
impl PyPlatform {
    /// Create the platform backend for the current OS.
    #[new]
    fn new() -> PyResult<Self> {
        let plat = neurohid_platform::create_platform().map_err(plat_err)?;
        Ok(Self {
            inner: Mutex::new(plat),
        })
    }

    fn platform_name(&self) -> &'static str {
        self.inner.lock().expect("poisoned").platform_name()
    }

    fn check_input_permissions(&self) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .check_input_permissions()
            .map_err(plat_err)
    }

    fn check_query_permissions(&self) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .check_query_permissions()
            .map_err(plat_err)
    }

    // --- Mouse ---

    fn emit_mouse_move(&self, movement: &PyMouseMovement) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_mouse_move(movement.inner.clone())
            .map_err(plat_err)
    }

    fn emit_mouse_move_absolute(&self, x: i32, y: i32) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_mouse_move_absolute(x, y)
            .map_err(plat_err)
    }

    fn emit_mouse_press(&self, button: &PyMouseButton) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_mouse_press(button.inner)
            .map_err(plat_err)
    }

    fn emit_mouse_release(&self, button: &PyMouseButton) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_mouse_release(button.inner)
            .map_err(plat_err)
    }

    fn emit_mouse_click(&self, button: &PyMouseButton) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_mouse_click(button.inner)
            .map_err(plat_err)
    }

    fn emit_scroll(&self, dx: f32, dy: f32) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_scroll(dx, dy)
            .map_err(plat_err)
    }

    // --- Keyboard ---

    fn emit_key_press(&self, key: &PyKey) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_key_press(key.inner.clone())
            .map_err(plat_err)
    }

    fn emit_key_release(&self, key: &PyKey) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_key_release(key.inner.clone())
            .map_err(plat_err)
    }

    fn emit_key_tap(&self, key: &PyKey) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_key_tap(key.inner.clone())
            .map_err(plat_err)
    }

    // --- PlatformExt convenience ---

    fn emit_double_click(&self, button: &PyMouseButton) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .emit_double_click(button.inner)
            .map_err(plat_err)
    }

    fn move_and_click(&self, x: i32, y: i32, button: &PyMouseButton) -> PyResult<()> {
        self.inner
            .lock()
            .expect("poisoned")
            .move_and_click(x, y, button.inner)
            .map_err(plat_err)
    }

    fn is_cursor_in_bounds(&self) -> PyResult<bool> {
        self.inner
            .lock()
            .expect("poisoned")
            .is_cursor_in_bounds()
            .map_err(plat_err)
    }

    // --- Query ---

    fn get_cursor_position(&self) -> PyResult<(i32, i32)> {
        self.inner
            .lock()
            .expect("poisoned")
            .get_cursor_position()
            .map_err(plat_err)
    }

    fn get_screen_info(&self) -> PyResult<PyScreenInfo> {
        let info = self
            .inner
            .lock()
            .expect("poisoned")
            .get_screen_info()
            .map_err(plat_err)?;
        Ok(PyScreenInfo { inner: info })
    }

    fn get_cursor_state(
        &self,
        prev_state: Option<&PyCursorState>,
        dt_seconds: f32,
    ) -> PyResult<PyCursorState> {
        let state = self
            .inner
            .lock()
            .expect("poisoned")
            .get_cursor_state(prev_state.map(|s| &s.inner), dt_seconds)
            .map_err(plat_err)?;
        Ok(PyCursorState { inner: state })
    }

    fn __repr__(&self) -> String {
        let name = self.inner.lock().expect("poisoned").platform_name();
        format!("Platform('{name}')")
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPlatformConfig>()?;
    m.add_class::<PyPermissionHint>()?;
    m.add_class::<PyScreenInfo>()?;
    m.add_class::<PyCursorState>()?;
    m.add_class::<PyPlatform>()?;
    Ok(())
}
