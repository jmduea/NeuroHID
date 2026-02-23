//! Domain type wrappers exposed to Python.
//!
//! Types that are complex or have many fields use serde-based `from_dict`/
//! `to_dict` conversion. Simpler types expose fields directly.

use numpy::{PyArray1, PyArray2, PyArrayMethods};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use neurohid_types::Timestamp;

// ---------------------------------------------------------------------------
// SystemConfig — serde round-trip (too many nested fields to map individually)
// ---------------------------------------------------------------------------

/// Top-level runtime configuration.
///
/// Use `SystemConfig.from_dict(d)` to construct from a Python dict,
/// and `config.to_dict()` to serialize back.
#[pyclass(name = "SystemConfig", skip_from_py_object)]
#[derive(Clone)]
pub struct PySystemConfig {
    pub inner: neurohid_types::config::SystemConfig,
}

#[pymethods]
impl PySystemConfig {
    /// Construct a `SystemConfig` from a Python dict (JSON-compatible).
    #[staticmethod]
    fn from_dict(dict: &Bound<'_, PyDict>) -> PyResult<Self> {
        let json_mod = PyModule::import(dict.py(), "json")?;
        let json_str: String = json_mod.call_method1("dumps", (dict,))?.extract()?;
        let inner: neurohid_types::config::SystemConfig =
            serde_json::from_str(&json_str).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid SystemConfig: {e}"))
            })?;
        Ok(Self { inner })
    }

    /// Construct a default `SystemConfig`.
    #[staticmethod]
    fn default() -> Self {
        Self {
            inner: neurohid_types::config::SystemConfig::default(),
        }
    }

    /// Serialize to a Python dict.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let json_str = serde_json::to_string(&self.inner).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "failed to serialize SystemConfig: {e}"
            ))
        })?;
        let json_mod = PyModule::import(py, "json")?;
        json_mod.call_method1("loads", (json_str,))
    }

    fn __repr__(&self) -> String {
        format!("SystemConfig({:?})", self.inner)
    }
}

// ---------------------------------------------------------------------------
// Newtype wrappers: ProfileId, DeviceId, ChannelId
// ---------------------------------------------------------------------------

/// Identifies a decoder profile.
#[pyclass(name = "ProfileId", skip_from_py_object)]
#[derive(Clone)]
pub struct PyProfileId {
    pub inner: neurohid_types::profile::ProfileId,
}

#[pymethods]
impl PyProfileId {
    #[new]
    fn new(id: String) -> Self {
        Self {
            inner: neurohid_types::profile::ProfileId(id),
        }
    }

    fn __str__(&self) -> &str {
        &self.inner.0
    }

    fn __repr__(&self) -> String {
        format!("ProfileId('{}')", self.inner.0)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.0.hash(&mut hasher);
        hasher.finish()
    }
}

/// Identifies a biosensor device.
#[pyclass(name = "DeviceId", skip_from_py_object)]
#[derive(Clone)]
pub struct PyDeviceId {
    pub inner: neurohid_types::device::DeviceId,
}

#[pymethods]
impl PyDeviceId {
    #[new]
    fn new(id: String) -> Self {
        Self {
            inner: neurohid_types::device::DeviceId(id),
        }
    }

    fn __str__(&self) -> &str {
        &self.inner.0
    }

    fn __repr__(&self) -> String {
        format!("DeviceId('{}')", self.inner.0)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.0.hash(&mut hasher);
        hasher.finish()
    }
}

/// Identifies an EEG channel.
#[pyclass(name = "ChannelId", skip_from_py_object)]
#[derive(Clone)]
pub struct PyChannelId {
    pub inner: neurohid_types::signal::ChannelId,
}

#[pymethods]
impl PyChannelId {
    #[new]
    fn new(id: String) -> Self {
        Self {
            inner: neurohid_types::signal::ChannelId(id),
        }
    }

    fn __str__(&self) -> &str {
        &self.inner.0
    }

    fn __repr__(&self) -> String {
        format!("ChannelId('{}')", self.inner.0)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.0.hash(&mut hasher);
        hasher.finish()
    }
}

// ---------------------------------------------------------------------------
// Device types: DeviceType, ConnectionState, DeviceInfo, DeviceStatus,
// ConnectionSettings, DeviceChannelConfig, DiscoveredStream
// ---------------------------------------------------------------------------

/// Device type/model.
#[pyclass(name = "DeviceType", eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyDeviceType {
    pub inner: neurohid_types::device::DeviceType,
}

#[pymethods]
impl PyDeviceType {
    #[staticmethod]
    fn open_bci_cyton() -> Self {
        Self {
            inner: neurohid_types::device::DeviceType::OpenBCICyton,
        }
    }
    #[staticmethod]
    fn open_bci_ganglion() -> Self {
        Self {
            inner: neurohid_types::device::DeviceType::OpenBCIGanglion,
        }
    }
    #[staticmethod]
    fn mock() -> Self {
        Self {
            inner: neurohid_types::device::DeviceType::Mock,
        }
    }
    #[staticmethod]
    fn unknown(name: String) -> Self {
        Self {
            inner: neurohid_types::device::DeviceType::Unknown(name),
        }
    }

    fn expected_channel_count(&self) -> Option<usize> {
        self.inner.expected_channel_count()
    }

    fn expected_sampling_rate(&self) -> Option<f32> {
        self.inner.expected_sampling_rate()
    }

    #[getter]
    fn name(&self) -> String {
        format!("{:?}", self.inner)
    }

    fn __repr__(&self) -> String {
        format!("DeviceType.{:?}", self.inner)
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }
}

/// Device connection state.
#[pyclass(name = "ConnectionState", eq, skip_from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub struct PyConnectionState {
    pub inner: neurohid_types::device::ConnectionState,
}

#[pymethods]
impl PyConnectionState {
    #[staticmethod]
    fn disconnected() -> Self {
        Self {
            inner: neurohid_types::device::ConnectionState::Disconnected,
        }
    }
    #[staticmethod]
    fn connecting() -> Self {
        Self {
            inner: neurohid_types::device::ConnectionState::Connecting,
        }
    }
    #[staticmethod]
    fn connected() -> Self {
        Self {
            inner: neurohid_types::device::ConnectionState::Connected,
        }
    }
    #[staticmethod]
    fn connection_lost() -> Self {
        Self {
            inner: neurohid_types::device::ConnectionState::ConnectionLost,
        }
    }
    #[staticmethod]
    fn error() -> Self {
        Self {
            inner: neurohid_types::device::ConnectionState::Error,
        }
    }

    fn is_usable(&self) -> bool {
        self.inner.is_usable()
    }

    fn is_transitioning(&self) -> bool {
        self.inner.is_transitioning()
    }

    #[getter]
    fn name(&self) -> String {
        format!("{:?}", self.inner)
    }

    fn __repr__(&self) -> String {
        format!("ConnectionState.{:?}", self.inner)
    }
}

/// Information about a discovered or connected device.
#[pyclass(name = "DeviceInfo", skip_from_py_object)]
#[derive(Clone)]
pub struct PyDeviceInfo {
    pub inner: neurohid_types::device::DeviceInfo,
}

#[pymethods]
impl PyDeviceInfo {
    #[getter]
    fn id(&self) -> PyDeviceId {
        PyDeviceId {
            inner: self.inner.id.clone(),
        }
    }

    #[getter]
    fn device_type(&self) -> PyDeviceType {
        PyDeviceType {
            inner: self.inner.device_type.clone(),
        }
    }

    #[getter]
    fn name(&self) -> Option<&str> {
        self.inner.name.as_deref()
    }

    #[getter]
    fn firmware_version(&self) -> Option<&str> {
        self.inner.firmware_version.as_deref()
    }

    #[getter]
    fn battery_percent(&self) -> Option<u8> {
        self.inner.battery_percent
    }

    #[getter]
    fn source_id(&self) -> Option<&str> {
        self.inner.source_id.as_deref()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "DeviceInfo(id='{}', type={:?})",
            self.inner.id.0, self.inner.device_type
        )
    }
}

/// Overall device status.
#[pyclass(name = "DeviceStatus", skip_from_py_object)]
#[derive(Clone)]
pub struct PyDeviceStatus {
    pub inner: neurohid_types::device::DeviceStatus,
}

#[pymethods]
impl PyDeviceStatus {
    #[getter]
    fn device_id(&self) -> PyDeviceId {
        PyDeviceId {
            inner: self.inner.device_id.clone(),
        }
    }

    #[getter]
    fn connection_state(&self) -> PyConnectionState {
        PyConnectionState {
            inner: self.inner.connection_state,
        }
    }

    #[getter]
    fn is_streaming(&self) -> bool {
        self.inner.is_streaming
    }

    #[getter]
    fn samples_received(&self) -> u64 {
        self.inner.samples_received
    }

    #[getter]
    fn samples_dropped(&self) -> u64 {
        self.inner.samples_dropped
    }

    #[getter]
    fn battery_percent(&self) -> Option<u8> {
        self.inner.battery_percent
    }

    #[getter]
    fn channel_quality<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyArray1<f32>>> {
        self.inner
            .channel_quality
            .as_deref()
            .map(|q| PyArray1::from_slice(py, q))
    }

    #[getter]
    fn message(&self) -> Option<&str> {
        self.inner.message.as_deref()
    }

    fn drop_rate(&self) -> f32 {
        self.inner.drop_rate()
    }

    fn average_quality(&self) -> Option<f32> {
        self.inner.average_quality()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "DeviceStatus(id='{}', state={:?}, streaming={})",
            self.inner.device_id.0, self.inner.connection_state, self.inner.is_streaming
        )
    }
}

/// Connection behavior settings.
#[pyclass(name = "ConnectionSettings", skip_from_py_object)]
#[derive(Clone)]
pub struct PyConnectionSettings {
    pub inner: neurohid_types::device::ConnectionSettings,
}

#[pymethods]
impl PyConnectionSettings {
    #[new]
    fn new() -> Self {
        Self {
            inner: neurohid_types::device::ConnectionSettings::default(),
        }
    }

    #[getter]
    fn auto_reconnect(&self) -> bool {
        self.inner.auto_reconnect
    }
    #[getter]
    fn max_reconnect_attempts(&self) -> u32 {
        self.inner.max_reconnect_attempts
    }
    #[getter]
    fn reconnect_delay_ms(&self) -> u64 {
        self.inner.reconnect_delay_ms
    }
    #[getter]
    fn connection_timeout_ms(&self) -> u64 {
        self.inner.connection_timeout_ms
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "ConnectionSettings(auto_reconnect={}, timeout={}ms)",
            self.inner.auto_reconnect, self.inner.connection_timeout_ms
        )
    }
}

/// A discovered LSL stream.
#[pyclass(name = "DiscoveredStream", skip_from_py_object)]
#[derive(Clone)]
pub struct PyDiscoveredStream {
    pub inner: neurohid_types::device::DiscoveredStream,
}

#[pymethods]
impl PyDiscoveredStream {
    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }
    #[getter]
    fn stream_type(&self) -> &str {
        &self.inner.stream_type
    }
    #[getter]
    fn channel_count(&self) -> i32 {
        self.inner.channel_count
    }
    #[getter]
    fn sample_rate(&self) -> f64 {
        self.inner.sample_rate
    }
    #[getter]
    fn connected(&self) -> bool {
        self.inner.connected
    }
    #[getter]
    fn battery_percent(&self) -> Option<u8> {
        self.inner.battery_percent
    }
    #[getter]
    fn source_id(&self) -> Option<&str> {
        self.inner.source_id.as_deref()
    }
    #[getter]
    fn effective_sample_rate_hz(&self) -> Option<f64> {
        self.inner.effective_sample_rate_hz
    }
    #[getter]
    fn samples_received(&self) -> Option<u64> {
        self.inner.samples_received
    }
    #[getter]
    fn samples_dropped(&self) -> Option<u64> {
        self.inner.samples_dropped
    }
    #[getter]
    fn drop_rate_pct(&self) -> Option<f32> {
        self.inner.drop_rate_pct
    }
    #[getter]
    fn integrity_state(&self) -> Option<&str> {
        self.inner.integrity_state.as_deref()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "DiscoveredStream(id='{}', name='{}', type='{}', connected={})",
            self.inner.id, self.inner.name, self.inner.stream_type, self.inner.connected
        )
    }
}

// ---------------------------------------------------------------------------
// Sample
// ---------------------------------------------------------------------------

/// A raw multi-channel EEG sample.
#[pyclass(name = "Sample", skip_from_py_object)]
#[derive(Clone)]
pub struct PySample {
    pub inner: neurohid_types::signal::Sample,
}

#[pymethods]
impl PySample {
    #[getter]
    fn source_id(&self) -> Option<&str> {
        self.inner.source_id.as_deref()
    }

    #[getter]
    fn system_timestamp(&self) -> Timestamp {
        self.inner.system_timestamp
    }

    #[getter]
    fn device_timestamp(&self) -> Option<Timestamp> {
        self.inner.device_timestamp
    }

    #[getter]
    fn sequence_number(&self) -> Option<u64> {
        self.inner.sequence_number
    }

    /// Channel values as a numpy array (single memcpy, no Python float boxing).
    #[getter]
    fn values<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
        PyArray1::from_slice(py, &self.inner.values)
    }

    /// Channel values as a Python list (backward-compat).
    fn values_list(&self) -> Vec<f32> {
        self.inner.values.clone()
    }

    /// Quality indicators as a numpy array, or ``None``.
    #[getter]
    fn quality<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyArray1<f32>>> {
        self.inner
            .quality
            .as_deref()
            .map(|q| PyArray1::from_slice(py, q))
    }

    /// Quality indicators as a Python list (backward-compat).
    fn quality_list(&self) -> Option<Vec<f32>> {
        self.inner.quality.clone()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "Sample(ts={}, channels={})",
            self.inner.system_timestamp,
            self.inner.values.len()
        )
    }
}

// ---------------------------------------------------------------------------
// FeatureVector
// ---------------------------------------------------------------------------

/// Extracted signal features for one processing window.
#[pyclass(name = "FeatureVector", skip_from_py_object)]
#[derive(Clone)]
pub struct PyFeatureVector {
    pub inner: neurohid_types::signal::FeatureVector,
}

#[pymethods]
impl PyFeatureVector {
    /// Feature values as a numpy array (single memcpy, no Python float boxing).
    #[getter]
    fn values<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
        PyArray1::from_slice(py, &self.inner.values)
    }

    /// Feature values as a Python list (backward-compat).
    fn values_list(&self) -> Vec<f32> {
        self.inner.values.clone()
    }

    #[getter]
    fn timestamp(&self) -> Timestamp {
        self.inner.timestamp
    }

    #[getter]
    fn stream_id(&self) -> Option<&str> {
        self.inner.stream_id.as_deref()
    }

    #[getter]
    fn window_start_us(&self) -> Option<Timestamp> {
        self.inner.window_start_us
    }

    #[getter]
    fn window_end_us(&self) -> Option<Timestamp> {
        self.inner.window_end_us
    }

    #[getter]
    fn labels(&self) -> Option<Vec<String>> {
        self.inner.labels.clone()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "FeatureVector(ts={}, dims={})",
            self.inner.timestamp,
            self.inner.values.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Key enum
// ---------------------------------------------------------------------------

/// Keyboard key identifier.
#[pyclass(name = "Key", eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyKey {
    pub inner: neurohid_types::action::Key,
}

#[pymethods]
impl PyKey {
    // Named constructors for each variant
    #[staticmethod]
    fn arrow_up() -> Self {
        Self {
            inner: neurohid_types::action::Key::ArrowUp,
        }
    }
    #[staticmethod]
    fn arrow_down() -> Self {
        Self {
            inner: neurohid_types::action::Key::ArrowDown,
        }
    }
    #[staticmethod]
    fn arrow_left() -> Self {
        Self {
            inner: neurohid_types::action::Key::ArrowLeft,
        }
    }
    #[staticmethod]
    fn arrow_right() -> Self {
        Self {
            inner: neurohid_types::action::Key::ArrowRight,
        }
    }
    #[staticmethod]
    fn enter() -> Self {
        Self {
            inner: neurohid_types::action::Key::Enter,
        }
    }
    #[staticmethod]
    fn space() -> Self {
        Self {
            inner: neurohid_types::action::Key::Space,
        }
    }
    #[staticmethod]
    fn escape() -> Self {
        Self {
            inner: neurohid_types::action::Key::Escape,
        }
    }
    #[staticmethod]
    fn backspace() -> Self {
        Self {
            inner: neurohid_types::action::Key::Backspace,
        }
    }
    #[staticmethod]
    fn tab() -> Self {
        Self {
            inner: neurohid_types::action::Key::Tab,
        }
    }
    #[staticmethod]
    fn shift() -> Self {
        Self {
            inner: neurohid_types::action::Key::Shift,
        }
    }
    #[staticmethod]
    fn control() -> Self {
        Self {
            inner: neurohid_types::action::Key::Control,
        }
    }
    #[staticmethod]
    fn alt() -> Self {
        Self {
            inner: neurohid_types::action::Key::Alt,
        }
    }
    #[staticmethod]
    fn meta() -> Self {
        Self {
            inner: neurohid_types::action::Key::Meta,
        }
    }
    #[staticmethod]
    fn letter(c: char) -> Self {
        Self {
            inner: neurohid_types::action::Key::Letter(c),
        }
    }
    #[staticmethod]
    fn number(n: u8) -> Self {
        Self {
            inner: neurohid_types::action::Key::Number(n),
        }
    }
    #[staticmethod]
    fn function(n: u8) -> Self {
        Self {
            inner: neurohid_types::action::Key::Function(n),
        }
    }

    /// Check if this is an arrow key.
    fn is_arrow(&self) -> bool {
        self.inner.is_arrow()
    }

    /// Check if this is a modifier key.
    fn is_modifier(&self) -> bool {
        self.inner.is_modifier()
    }

    /// The variant name as a string.
    #[getter]
    fn name(&self) -> String {
        format!("{:?}", self.inner)
    }

    fn __repr__(&self) -> String {
        format!("Key.{:?}", self.inner)
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }
}

// ---------------------------------------------------------------------------
// MouseButton enum
// ---------------------------------------------------------------------------

/// Mouse button identifier.
#[pyclass(name = "MouseButton", eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyMouseButton {
    pub inner: neurohid_types::action::MouseButton,
}

#[pymethods]
impl PyMouseButton {
    #[staticmethod]
    fn left() -> Self {
        Self {
            inner: neurohid_types::action::MouseButton::Left,
        }
    }
    #[staticmethod]
    fn right() -> Self {
        Self {
            inner: neurohid_types::action::MouseButton::Right,
        }
    }
    #[staticmethod]
    fn middle() -> Self {
        Self {
            inner: neurohid_types::action::MouseButton::Middle,
        }
    }
    #[staticmethod]
    fn extra(n: u8) -> Self {
        Self {
            inner: neurohid_types::action::MouseButton::Extra(n),
        }
    }

    #[getter]
    fn name(&self) -> String {
        format!("{:?}", self.inner)
    }

    fn __repr__(&self) -> String {
        format!("MouseButton.{:?}", self.inner)
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }
}

// ---------------------------------------------------------------------------
// MouseMovement
// ---------------------------------------------------------------------------

/// Relative mouse movement.
#[pyclass(name = "MouseMovement", skip_from_py_object)]
#[derive(Clone)]
pub struct PyMouseMovement {
    pub inner: neurohid_types::action::MouseMovement,
}

#[pymethods]
impl PyMouseMovement {
    #[new]
    fn new(dx: f32, dy: f32) -> Self {
        Self {
            inner: neurohid_types::action::MouseMovement { dx, dy },
        }
    }

    #[getter]
    fn dx(&self) -> f32 {
        self.inner.dx
    }
    #[getter]
    fn dy(&self) -> f32 {
        self.inner.dy
    }

    fn magnitude(&self) -> f32 {
        self.inner.magnitude()
    }

    fn direction(&self) -> f32 {
        self.inner.direction()
    }

    fn __repr__(&self) -> String {
        format!("MouseMovement(dx={}, dy={})", self.inner.dx, self.inner.dy)
    }
}

// ---------------------------------------------------------------------------
// ScrollMovement
// ---------------------------------------------------------------------------

/// Scroll wheel movement.
#[pyclass(name = "ScrollMovement", skip_from_py_object)]
#[derive(Clone)]
pub struct PyScrollMovement {
    pub inner: neurohid_types::action::ScrollMovement,
}

#[pymethods]
impl PyScrollMovement {
    #[new]
    fn new(dx: f32, dy: f32) -> Self {
        Self {
            inner: neurohid_types::action::ScrollMovement { dx, dy },
        }
    }

    #[getter]
    fn dx(&self) -> f32 {
        self.inner.dx
    }
    #[getter]
    fn dy(&self) -> f32 {
        self.inner.dy
    }

    fn __repr__(&self) -> String {
        format!("ScrollMovement(dx={}, dy={})", self.inner.dx, self.inner.dy)
    }
}

// ---------------------------------------------------------------------------
// MouseButtonEvent
// ---------------------------------------------------------------------------

/// A mouse button state change event.
#[pyclass(name = "MouseButtonEvent", skip_from_py_object)]
#[derive(Clone)]
pub struct PyMouseButtonEvent {
    pub inner: neurohid_types::action::MouseButtonEvent,
}

#[pymethods]
impl PyMouseButtonEvent {
    #[new]
    fn new(button: &PyMouseButton, pressed: bool) -> Self {
        Self {
            inner: neurohid_types::action::MouseButtonEvent {
                button: button.inner,
                pressed,
            },
        }
    }

    #[getter]
    fn button(&self) -> PyMouseButton {
        PyMouseButton {
            inner: self.inner.button,
        }
    }

    #[getter]
    fn pressed(&self) -> bool {
        self.inner.pressed
    }

    fn __repr__(&self) -> String {
        format!(
            "MouseButtonEvent({:?}, pressed={})",
            self.inner.button, self.inner.pressed
        )
    }
}

// ---------------------------------------------------------------------------
// KeyEvent
// ---------------------------------------------------------------------------

/// A keyboard key state change event.
#[pyclass(name = "KeyEvent", skip_from_py_object)]
#[derive(Clone)]
pub struct PyKeyEvent {
    pub inner: neurohid_types::action::KeyEvent,
}

#[pymethods]
impl PyKeyEvent {
    #[new]
    fn new(key: &PyKey, pressed: bool) -> Self {
        Self {
            inner: neurohid_types::action::KeyEvent {
                key: key.inner,
                pressed,
            },
        }
    }

    #[getter]
    fn key(&self) -> PyKey {
        PyKey {
            inner: self.inner.key,
        }
    }

    #[getter]
    fn pressed(&self) -> bool {
        self.inner.pressed
    }

    fn __repr__(&self) -> String {
        format!(
            "KeyEvent({:?}, pressed={})",
            self.inner.key, self.inner.pressed
        )
    }
}

// ---------------------------------------------------------------------------
// MouseAction
// ---------------------------------------------------------------------------

/// Mouse-related actions: movement, buttons, and scroll.
#[pyclass(name = "MouseAction", skip_from_py_object)]
#[derive(Clone)]
pub struct PyMouseAction {
    pub inner: neurohid_types::action::MouseAction,
}

#[pymethods]
impl PyMouseAction {
    /// Create a movement-only action.
    #[staticmethod]
    fn move_relative(dx: f32, dy: f32) -> Self {
        Self {
            inner: neurohid_types::action::MouseAction::move_relative(dx, dy),
        }
    }

    /// Create a click action (press + release).
    #[staticmethod]
    fn click(button: &PyMouseButton) -> Self {
        Self {
            inner: neurohid_types::action::MouseAction::click(button.inner),
        }
    }

    /// Create a button press (without release).
    #[staticmethod]
    fn press(button: &PyMouseButton) -> Self {
        Self {
            inner: neurohid_types::action::MouseAction::press(button.inner),
        }
    }

    /// Create a button release.
    #[staticmethod]
    fn release(button: &PyMouseButton) -> Self {
        Self {
            inner: neurohid_types::action::MouseAction::release(button.inner),
        }
    }

    /// Create a scroll action.
    #[staticmethod]
    fn scroll(dx: f32, dy: f32) -> Self {
        Self {
            inner: neurohid_types::action::MouseAction::scroll(dx, dy),
        }
    }

    /// The movement component, or None.
    #[getter]
    fn movement(&self) -> Option<PyMouseMovement> {
        self.inner.movement.map(|m| PyMouseMovement { inner: m })
    }

    /// Button events as a list.
    #[getter]
    fn buttons(&self) -> Vec<PyMouseButtonEvent> {
        self.inner
            .buttons
            .iter()
            .map(|b| PyMouseButtonEvent { inner: *b })
            .collect()
    }

    /// The scroll component, or None.
    #[getter]
    fn scroll_movement(&self) -> Option<PyScrollMovement> {
        self.inner.scroll.map(|s| PyScrollMovement { inner: s })
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        let mut parts = Vec::new();
        if self.inner.movement.is_some() {
            parts.push("movement");
        }
        if !self.inner.buttons.is_empty() {
            parts.push("buttons");
        }
        if self.inner.scroll.is_some() {
            parts.push("scroll");
        }
        format!("MouseAction({})", parts.join("+"))
    }
}

// ---------------------------------------------------------------------------
// KeyAction
// ---------------------------------------------------------------------------

/// Keyboard-related actions.
#[pyclass(name = "KeyAction", skip_from_py_object)]
#[derive(Clone)]
pub struct PyKeyAction {
    pub inner: neurohid_types::action::KeyAction,
}

#[pymethods]
impl PyKeyAction {
    /// Create a key tap (press + release).
    #[staticmethod]
    fn tap(key: &PyKey) -> Self {
        Self {
            inner: neurohid_types::action::KeyAction::tap(key.inner),
        }
    }

    /// Create a key press (without release).
    #[staticmethod]
    fn press(key: &PyKey) -> Self {
        Self {
            inner: neurohid_types::action::KeyAction::press(key.inner),
        }
    }

    /// Create a key release.
    #[staticmethod]
    fn release(key: &PyKey) -> Self {
        Self {
            inner: neurohid_types::action::KeyAction::release(key.inner),
        }
    }

    /// The key events.
    #[getter]
    fn events(&self) -> Vec<PyKeyEvent> {
        self.inner
            .events
            .iter()
            .map(|e| PyKeyEvent { inner: *e })
            .collect()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!("KeyAction(events={})", self.inner.events.len())
    }
}

// ---------------------------------------------------------------------------
// ActionSpace
// ---------------------------------------------------------------------------

/// Defines what actions are available to the decoder for RL training.
#[pyclass(name = "ActionSpace", skip_from_py_object)]
#[derive(Clone)]
pub struct PyActionSpace {
    pub inner: neurohid_types::action::ActionSpace,
}

#[pymethods]
impl PyActionSpace {
    /// Default action space (mouse + arrow keys).
    #[new]
    fn new() -> Self {
        Self {
            inner: neurohid_types::action::ActionSpace::default(),
        }
    }

    /// Mouse-only action space.
    #[staticmethod]
    fn mouse_only() -> Self {
        Self {
            inner: neurohid_types::action::ActionSpace::mouse_only(),
        }
    }

    /// Arrow-keys-only action space (discrete control).
    #[staticmethod]
    fn arrows_only() -> Self {
        Self {
            inner: neurohid_types::action::ActionSpace::arrows_only(),
        }
    }

    #[getter]
    fn mouse_movement(&self) -> bool {
        self.inner.mouse_movement
    }
    #[getter]
    fn mouse_scroll(&self) -> bool {
        self.inner.mouse_scroll
    }
    #[getter]
    fn movement_sensitivity(&self) -> f32 {
        self.inner.movement_sensitivity
    }
    #[getter]
    fn confidence_threshold(&self) -> f32 {
        self.inner.confidence_threshold
    }

    #[getter]
    fn mouse_buttons(&self) -> Vec<PyMouseButton> {
        self.inner
            .mouse_buttons
            .iter()
            .map(|b| PyMouseButton { inner: *b })
            .collect()
    }

    #[getter]
    fn keys(&self) -> Vec<PyKey> {
        self.inner
            .keys
            .iter()
            .map(|k| PyKey { inner: *k })
            .collect()
    }

    /// Total number of discrete actions (for discrete action space RL).
    fn discrete_action_count(&self) -> usize {
        self.inner.discrete_action_count()
    }

    /// Continuous action dimension (for continuous action space RL).
    fn continuous_action_dim(&self) -> usize {
        self.inner.continuous_action_dim()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "ActionSpace(discrete={}, continuous={})",
            self.inner.discrete_action_count(),
            self.inner.continuous_action_dim()
        )
    }
}

// ---------------------------------------------------------------------------
// Action
// ---------------------------------------------------------------------------

/// A decoded HID output action.
#[pyclass(name = "Action", skip_from_py_object)]
#[derive(Clone)]
pub struct PyAction {
    pub inner: neurohid_types::action::Action,
}

#[pymethods]
impl PyAction {
    /// Create an empty (no-op) action.
    #[staticmethod]
    fn none() -> Self {
        Self {
            inner: neurohid_types::action::Action::none(),
        }
    }

    /// Create a mouse-only action.
    #[staticmethod]
    fn from_mouse(action: &PyMouseAction) -> Self {
        Self {
            inner: neurohid_types::action::Action::mouse(action.inner.clone()),
        }
    }

    /// Create a keyboard-only action.
    #[staticmethod]
    fn from_key(action: &PyKeyAction) -> Self {
        Self {
            inner: neurohid_types::action::Action::key(action.inner.clone()),
        }
    }

    /// Set the confidence for this action.
    fn with_confidence(&self, confidence: f32) -> Self {
        Self {
            inner: self.inner.clone().with_confidence(confidence),
        }
    }

    /// Check if this is a no-op action.
    fn is_none(&self) -> bool {
        self.inner.is_none()
    }

    #[getter]
    fn timestamp(&self) -> Timestamp {
        self.inner.timestamp
    }

    #[getter]
    fn confidence(&self) -> f32 {
        self.inner.confidence
    }

    #[getter]
    fn decision_id(&self) -> Option<&str> {
        self.inner.decision_id.as_deref()
    }

    #[getter]
    fn has_mouse(&self) -> bool {
        self.inner.mouse.is_some()
    }

    #[getter]
    fn has_keyboard(&self) -> bool {
        self.inner.keyboard.is_some()
    }

    /// Extract the mouse action component, or None.
    fn mouse(&self) -> Option<PyMouseAction> {
        self.inner.mouse.as_ref().map(|m| PyMouseAction {
            inner: m.clone(),
        })
    }

    /// Extract the keyboard action component, or None.
    fn keyboard(&self) -> Option<PyKeyAction> {
        self.inner.keyboard.as_ref().map(|k| PyKeyAction {
            inner: k.clone(),
        })
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        let kind = match (&self.inner.mouse, &self.inner.keyboard) {
            (Some(_), Some(_)) => "mouse+keyboard",
            (Some(_), None) => "mouse",
            (None, Some(_)) => "keyboard",
            (None, None) => "noop",
        };
        format!("Action(ts={}, kind={kind})", self.inner.timestamp)
    }
}

// ---------------------------------------------------------------------------
// StreamMarker
// ---------------------------------------------------------------------------

/// A timestamped event marker in the signal stream.
#[pyclass(name = "StreamMarker", skip_from_py_object)]
#[derive(Clone)]
pub struct PyStreamMarker {
    pub inner: neurohid_types::event::StreamMarker,
}

#[pymethods]
impl PyStreamMarker {
    #[getter]
    fn timestamp(&self) -> Timestamp {
        self.inner.timestamp
    }

    #[getter]
    fn source_id(&self) -> Option<&str> {
        self.inner.source_id.as_deref()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "StreamMarker(ts={}, type={:?})",
            self.inner.timestamp, self.inner.marker_type
        )
    }
}

// ---------------------------------------------------------------------------
// ControlSnapshot / TrainerSnapshot — serde round-trip (many fields)
// ---------------------------------------------------------------------------

/// Point-in-time runtime state snapshot.
#[pyclass(name = "ControlSnapshot", skip_from_py_object)]
#[derive(Clone)]
pub struct PyControlSnapshot {
    pub inner: neurohid_types::control::ControlSnapshot,
}

#[pymethods]
impl PyControlSnapshot {
    #[getter]
    fn running(&self) -> bool {
        self.inner.running
    }

    #[getter]
    fn uptime_secs(&self) -> u64 {
        self.inner.uptime_secs
    }

    #[getter]
    fn calibration_mode(&self) -> bool {
        self.inner.calibration_mode
    }

    #[getter]
    fn output_enabled(&self) -> bool {
        self.inner.output_enabled
    }

    #[getter]
    fn profile_ready(&self) -> bool {
        self.inner.profile_ready
    }

    #[getter]
    fn decoder_ready(&self) -> bool {
        self.inner.decoder_ready
    }

    #[getter]
    fn device_connected(&self) -> bool {
        self.inner.device_connected
    }

    #[getter]
    fn signal_quality(&self) -> f32 {
        self.inner.signal_quality
    }

    #[getter]
    fn actions_emitted(&self) -> u64 {
        self.inner.actions_emitted
    }

    #[getter]
    fn errors_detected(&self) -> u64 {
        self.inner.errors_detected
    }

    #[getter]
    fn learning_enabled(&self) -> bool {
        self.inner.learning_enabled
    }

    #[getter]
    fn ml_bridge_connected(&self) -> bool {
        self.inner.ml_bridge_connected
    }

    #[getter]
    fn discovered_streams(&self) -> Vec<PyDiscoveredStream> {
        self.inner
            .discovered_streams
            .iter()
            .cloned()
            .map(|s| PyDiscoveredStream { inner: s })
            .collect()
    }

    #[getter]
    fn recording_active(&self) -> bool {
        self.inner.recording_active
    }

    #[getter]
    fn current_session_id(&self) -> Option<&str> {
        self.inner.current_session_id.as_deref()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "ControlSnapshot(running={}, uptime={}s)",
            self.inner.running, self.inner.uptime_secs
        )
    }
}

/// Trainer bridge status snapshot.
#[pyclass(name = "TrainerSnapshot", skip_from_py_object)]
#[derive(Clone)]
pub struct PyTrainerSnapshot {
    pub inner: neurohid_types::control::TrainerSnapshot,
}

#[pymethods]
impl PyTrainerSnapshot {
    #[getter]
    fn trainer_connected(&self) -> bool {
        self.inner.trainer_connected
    }

    #[getter]
    fn trainer_state(&self) -> &str {
        &self.inner.trainer_state
    }

    #[getter]
    fn replay_size(&self) -> u64 {
        self.inner.replay_size
    }

    #[getter]
    fn training_step(&self) -> u64 {
        self.inner.training_step
    }

    #[getter]
    fn last_error(&self) -> Option<&str> {
        self.inner.last_error.as_deref()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "TrainerSnapshot(state='{}', step={})",
            self.inner.trainer_state, self.inner.training_step
        )
    }
}

// ---------------------------------------------------------------------------
// DecisionEvent — typed wrapper with numpy access
// ---------------------------------------------------------------------------

/// A runtime decision event with numpy-backed feature values.
///
/// Access ``feature_values`` as a numpy array instead of going through JSON.
#[pyclass(name = "DecisionEvent", skip_from_py_object)]
#[derive(Clone)]
pub struct PyDecisionEvent {
    pub inner: neurohid_ipc::DecisionEvent,
}

#[pymethods]
impl PyDecisionEvent {
    #[getter]
    fn decision_id(&self) -> &str {
        &self.inner.decision_id
    }

    #[getter]
    fn timestamp_us(&self) -> Timestamp {
        self.inner.timestamp_us
    }

    /// Feature values as a numpy array (single memcpy).
    #[getter]
    fn feature_values<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
        PyArray1::from_slice(py, &self.inner.feature_values)
    }

    #[getter]
    fn decoder_confidence(&self) -> f32 {
        self.inner.decoder_confidence
    }

    #[getter]
    fn signal_quality(&self) -> f32 {
        self.inner.signal_quality
    }

    #[getter]
    fn action(&self) -> PyAction {
        PyAction {
            inner: self.inner.action.clone(),
        }
    }

    #[getter]
    fn decoder_model_version(&self) -> Option<&str> {
        self.inner.decoder_model_version.as_deref()
    }

    #[getter]
    fn stream_id(&self) -> Option<&str> {
        self.inner.stream_id.as_deref()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "DecisionEvent(id='{}', confidence={:.3})",
            self.inner.decision_id, self.inner.decoder_confidence
        )
    }
}

// ---------------------------------------------------------------------------
// ErrpWindow — typed wrapper with numpy 2D array
// ---------------------------------------------------------------------------

/// An `ErrP` analysis window with numpy-backed channel data.
///
/// ``channel_data`` returns a 2D numpy array of shape ``(channels, samples)``
/// instead of nested Python lists.
#[pyclass(name = "ErrpWindow", skip_from_py_object)]
#[derive(Clone)]
pub struct PyErrpWindow {
    pub inner: neurohid_ipc::ErrpWindow,
}

#[pymethods]
impl PyErrpWindow {
    #[getter]
    fn decision_id(&self) -> &str {
        &self.inner.decision_id
    }

    #[getter]
    fn action_timestamp_us(&self) -> Timestamp {
        self.inner.action_timestamp_us
    }

    #[getter]
    fn window_start_us(&self) -> Timestamp {
        self.inner.window_start_us
    }

    #[getter]
    fn window_end_us(&self) -> Timestamp {
        self.inner.window_end_us
    }

    #[getter]
    fn sample_rate_hz(&self) -> f32 {
        self.inner.sample_rate_hz
    }

    #[getter]
    fn channel_labels(&self) -> Vec<String> {
        self.inner.channel_labels.clone()
    }

    /// Channel data as a 2D numpy array of shape ``(channels, samples)``.
    ///
    /// Each row is one channel's time-series segment. Single memcpy into
    /// a contiguous numpy buffer.
    #[getter]
    fn channel_data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let rows = self.inner.channel_data.len();
        if rows == 0 {
            return PyArray2::from_vec2(py, &[]).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "empty array construction failed: {e}"
                ))
            });
        }
        let cols = self.inner.channel_data[0].len();
        // Flatten row-major for numpy (C-contiguous).
        let flat: Vec<f32> = self
            .inner
            .channel_data
            .iter()
            .flat_map(|row| row.iter().copied())
            .collect();
        PyArray1::from_vec(py, flat).reshape([rows, cols])
    }

    #[getter]
    fn signal_quality(&self) -> f32 {
        self.inner.signal_quality
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        let channels = self.inner.channel_data.len();
        let samples = self.inner.channel_data.first().map_or(0, Vec::len);
        format!(
            "ErrpWindow(id='{}', shape=({channels}, {samples}))",
            self.inner.decision_id
        )
    }
}

// ---------------------------------------------------------------------------
// RuntimeEvent — serde round-trip + typed accessors
// ---------------------------------------------------------------------------

/// A runtime event from the broadcast stream.
///
/// Use ``event_type`` to determine the variant, then use typed accessors
/// like ``sample()``, ``feature()``, ``decision_event()``, ``errp_window()``
/// to extract inner data with numpy-backed arrays.
#[pyclass(name = "RuntimeEvent", skip_from_py_object)]
#[derive(Clone)]
pub struct PyRuntimeEvent {
    pub inner: neurohid_ipc::RuntimeEvent,
}

#[pymethods]
impl PyRuntimeEvent {
    /// The event variant name (e.g. "sample", "`action_emitted`", "snapshot").
    #[getter]
    fn event_type(&self) -> &str {
        match &self.inner {
            neurohid_ipc::RuntimeEvent::Snapshot { .. } => "snapshot",
            neurohid_ipc::RuntimeEvent::TrainerSnapshot { .. } => "trainer_snapshot",
            neurohid_ipc::RuntimeEvent::TrainerStatus { .. } => "trainer_status",
            neurohid_ipc::RuntimeEvent::RuntimeTelemetry { .. } => "runtime_telemetry",
            neurohid_ipc::RuntimeEvent::Sample { .. } => "sample",
            neurohid_ipc::RuntimeEvent::FeatureFrame { .. } => "feature_frame",
            neurohid_ipc::RuntimeEvent::ActionEmitted { .. } => "action_emitted",
            neurohid_ipc::RuntimeEvent::Marker { .. } => "marker",
            neurohid_ipc::RuntimeEvent::ObservationFrame { .. } => "observation_frame",
            neurohid_ipc::RuntimeEvent::DecisionEvent { .. } => "decision_event",
            neurohid_ipc::RuntimeEvent::ErrpWindow { .. } => "errp_window",
            neurohid_ipc::RuntimeEvent::ErrpResult { .. } => "errp_result",
            neurohid_ipc::RuntimeEvent::IntegrityIssue { .. } => "integrity_issue",
            neurohid_ipc::RuntimeEvent::Lifecycle { .. } => "lifecycle",
            neurohid_ipc::RuntimeEvent::BackpressureDrop { .. } => "backpressure_drop",
            neurohid_ipc::RuntimeEvent::Capabilities { .. } => "capabilities",
        }
    }

    /// Extract the inner ``Sample`` if this is a sample event, else ``None``.
    fn sample(&self) -> Option<PySample> {
        match &self.inner {
            neurohid_ipc::RuntimeEvent::Sample { sample } => Some(PySample {
                inner: sample.clone(),
            }),
            _ => None,
        }
    }

    /// Extract the inner ``FeatureVector`` if this is a `feature_frame` event.
    fn feature(&self) -> Option<PyFeatureVector> {
        match &self.inner {
            neurohid_ipc::RuntimeEvent::FeatureFrame { feature } => Some(PyFeatureVector {
                inner: feature.clone(),
            }),
            _ => None,
        }
    }

    /// Extract the inner ``Action`` if this is an `action_emitted` event.
    fn action(&self) -> Option<PyAction> {
        match &self.inner {
            neurohid_ipc::RuntimeEvent::ActionEmitted { action } => Some(PyAction {
                inner: action.clone(),
            }),
            _ => None,
        }
    }

    /// Extract the inner ``DecisionEvent`` with numpy feature values.
    fn decision_event(&self) -> Option<PyDecisionEvent> {
        match &self.inner {
            neurohid_ipc::RuntimeEvent::DecisionEvent { event } => Some(PyDecisionEvent {
                inner: event.clone(),
            }),
            _ => None,
        }
    }

    /// Extract the inner ``ErrpWindow`` with numpy channel data.
    fn errp_window(&self) -> Option<PyErrpWindow> {
        match &self.inner {
            neurohid_ipc::RuntimeEvent::ErrpWindow { window } => Some(PyErrpWindow {
                inner: window.clone(),
            }),
            _ => None,
        }
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!("RuntimeEvent(type='{}')", self.event_type())
    }
}

// ---------------------------------------------------------------------------
// Observation — complete ML input state
// ---------------------------------------------------------------------------

/// Complete observation at a point in time (decoder input).
#[pyclass(name = "Observation", skip_from_py_object)]
#[derive(Clone)]
pub struct PyObservation {
    pub inner: neurohid_types::observation::Observation,
}

#[pymethods]
impl PyObservation {
    #[getter]
    fn timestamp(&self) -> i64 {
        self.inner.timestamp
    }

    #[getter]
    fn signal_features(&self) -> PyFeatureVector {
        PyFeatureVector {
            inner: self.inner.signal_features.clone(),
        }
    }

    #[getter]
    fn cursor(&self) -> crate::platform::PyCursorState {
        crate::platform::PyCursorState {
            inner: self.inner.cursor,
        }
    }

    #[getter]
    fn screen(&self) -> crate::platform::PyScreenInfo {
        crate::platform::PyScreenInfo {
            inner: self.inner.screen.clone(),
        }
    }

    fn total_dim(&self) -> usize {
        self.inner.total_dim()
    }

    /// Flat feature vector for neural network input.
    fn to_vector<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
        PyArray1::from_vec(py, self.inner.to_vector())
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "Observation(dim={}, timestamp={})",
            self.inner.total_dim(),
            self.inner.timestamp
        )
    }
}

// ---------------------------------------------------------------------------
// Model types — ModelManifest, NormalizationStats
// ---------------------------------------------------------------------------

/// Feature normalization statistics (mean/std per dimension).
#[pyclass(name = "NormalizationStats", skip_from_py_object)]
#[derive(Clone)]
pub struct PyNormalizationStats {
    pub inner: neurohid_types::model::NormalizationStats,
}

#[pymethods]
impl PyNormalizationStats {
    #[new]
    fn new(mean: Vec<f32>, std: Vec<f32>) -> Self {
        Self {
            inner: neurohid_types::model::NormalizationStats { mean, std },
        }
    }

    #[getter]
    fn mean<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
        PyArray1::from_slice(py, &self.inner.mean)
    }

    #[getter]
    fn std<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
        PyArray1::from_slice(py, &self.inner.std)
    }

    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }

    fn __repr__(&self) -> String {
        format!("NormalizationStats(dim={})", self.inner.mean.len())
    }
}

/// ONNX model metadata/contract.
#[pyclass(name = "ModelManifest", skip_from_py_object)]
#[derive(Clone)]
pub struct PyModelManifest {
    pub inner: neurohid_types::model::ModelManifest,
}

#[pymethods]
impl PyModelManifest {
    #[getter]
    fn model_version(&self) -> &str {
        &self.inner.model_version
    }
    #[getter]
    fn input_dim(&self) -> usize {
        self.inner.input_dim
    }
    #[getter]
    fn feature_schema_version(&self) -> u32 {
        self.inner.feature_schema_version
    }
    #[getter]
    fn action_schema_version(&self) -> u32 {
        self.inner.action_schema_version
    }
    #[getter]
    fn normalization_stats(&self) -> PyNormalizationStats {
        PyNormalizationStats {
            inner: self.inner.normalization_stats.clone(),
        }
    }
    #[getter]
    fn trained_at(&self) -> i64 {
        self.inner.trained_at
    }

    fn validate(&self) -> PyResult<()> {
        self.inner
            .validate()
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "ModelManifest(version='{}', input_dim={})",
            self.inner.model_version, self.inner.input_dim
        )
    }
}

// ---------------------------------------------------------------------------
// Training / Learning types
// ---------------------------------------------------------------------------

/// A single training episode (feature + action + feedback).
#[pyclass(name = "TrainingEpisode", skip_from_py_object)]
#[derive(Clone)]
pub struct PyTrainingEpisode {
    pub inner: neurohid_types::learning::TrainingEpisode,
}

#[pymethods]
impl PyTrainingEpisode {
    #[getter]
    fn timestamp(&self) -> i64 {
        self.inner.timestamp
    }
    #[getter]
    fn feature_values<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
        PyArray1::from_slice(py, &self.inner.feature_values)
    }
    #[getter]
    fn action(&self) -> PyAction {
        PyAction {
            inner: self.inner.action.clone(),
        }
    }
    #[getter]
    fn decoder_confidence(&self) -> f32 {
        self.inner.decoder_confidence
    }
    #[getter]
    fn signal_quality(&self) -> f32 {
        self.inner.signal_quality
    }
    #[getter]
    fn errp_error_probability(&self) -> Option<f32> {
        self.inner.errp_error_probability
    }
    #[getter]
    fn errp_confidence(&self) -> Option<f32> {
        self.inner.errp_confidence
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "TrainingEpisode(timestamp={}, confidence={:.3})",
            self.inner.timestamp, self.inner.decoder_confidence
        )
    }
}

/// Candidate model evaluation metrics.
#[pyclass(name = "CandidateModelMetrics", skip_from_py_object)]
#[derive(Clone)]
pub struct PyCandidateModelMetrics {
    pub inner: neurohid_types::learning::CandidateModelMetrics,
}

#[pymethods]
impl PyCandidateModelMetrics {
    #[getter]
    fn holdout_sample_count(&self) -> usize {
        self.inner.holdout_sample_count
    }
    #[getter]
    fn holdout_accuracy(&self) -> f32 {
        self.inner.holdout_accuracy
    }
    #[getter]
    fn holdout_loss(&self) -> f32 {
        self.inner.holdout_loss
    }
    #[getter]
    fn decode_latency_p95_us(&self) -> u64 {
        self.inner.decode_latency_p95_us
    }
    #[getter]
    fn generated_at(&self) -> i64 {
        self.inner.generated_at
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "CandidateModelMetrics(accuracy={:.3}, loss={:.3})",
            self.inner.holdout_accuracy, self.inner.holdout_loss
        )
    }
}

// ---------------------------------------------------------------------------
// Profile types
// ---------------------------------------------------------------------------

/// Profile metadata.
#[pyclass(name = "ProfileMetadata", skip_from_py_object)]
#[derive(Clone)]
pub struct PyProfileMetadata {
    pub inner: neurohid_types::profile::ProfileMetadata,
}

#[pymethods]
impl PyProfileMetadata {
    #[getter]
    fn id(&self) -> PyProfileId {
        PyProfileId {
            inner: self.inner.id.clone(),
        }
    }
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }
    #[getter]
    fn created_at(&self) -> i64 {
        self.inner.created_at
    }
    #[getter]
    fn last_used_at(&self) -> i64 {
        self.inner.last_used_at
    }
    #[getter]
    fn last_calibrated_at(&self) -> Option<i64> {
        self.inner.last_calibrated_at
    }
    #[getter]
    fn total_usage_time_us(&self) -> i64 {
        self.inner.total_usage_time_us
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "ProfileMetadata(id='{}', name='{}')",
            self.inner.id.0, self.inner.name
        )
    }
}

// ---------------------------------------------------------------------------
// Reward / ErrP types
// ---------------------------------------------------------------------------

/// Reward signal for reinforcement learning.
#[pyclass(name = "RewardSignal", skip_from_py_object)]
#[derive(Clone)]
pub struct PyRewardSignal {
    pub inner: neurohid_types::reward::RewardSignal,
}

#[pymethods]
impl PyRewardSignal {
    #[getter]
    fn value(&self) -> f32 {
        self.inner.value
    }
    #[getter]
    fn confidence(&self) -> f32 {
        self.inner.confidence
    }
    #[getter]
    fn request_feedback(&self) -> bool {
        self.inner.request_feedback
    }
    #[getter]
    fn action_timestamp(&self) -> i64 {
        self.inner.action_timestamp
    }
    #[getter]
    fn flag(&self) -> String {
        format!("{:?}", self.inner.flag)
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "RewardSignal(value={:.3}, confidence={:.3})",
            self.inner.value, self.inner.confidence
        )
    }
}

/// ErrP detection result.
#[pyclass(name = "ErrPResult", skip_from_py_object)]
#[derive(Clone)]
pub struct PyErrPResult {
    pub inner: neurohid_types::reward::ErrPResult,
}

#[pymethods]
impl PyErrPResult {
    #[getter]
    fn action_timestamp(&self) -> i64 {
        self.inner.action_timestamp
    }
    #[getter]
    fn detection_timestamp(&self) -> i64 {
        self.inner.detection_timestamp
    }
    #[getter]
    fn error_probability(&self) -> f32 {
        self.inner.error_probability
    }
    #[getter]
    fn classification_confidence(&self) -> f32 {
        self.inner.classification_confidence
    }
    #[getter]
    fn signal_quality(&self) -> String {
        format!("{:?}", self.inner.signal_quality)
    }
    #[getter]
    fn estimated_magnitude(&self) -> Option<f32> {
        self.inner.estimated_magnitude
    }
    #[getter]
    fn detection_latency_us(&self) -> i64 {
        self.inner.detection_latency_us
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "ErrPResult(error_probability={:.3}, confidence={:.3})",
            self.inner.error_probability, self.inner.classification_confidence
        )
    }
}

// ---------------------------------------------------------------------------
// Signal enums: FrequencyBand, FeatureType, ChannelConfig, DeviceChannelConfig
// ---------------------------------------------------------------------------

/// EEG frequency band.
#[pyclass(name = "FrequencyBand", skip_from_py_object)]
#[derive(Clone)]
pub struct PyFrequencyBand {
    pub inner: neurohid_types::signal::FrequencyBand,
}

#[pymethods]
impl PyFrequencyBand {
    #[staticmethod]
    fn delta() -> Self {
        Self {
            inner: neurohid_types::signal::FrequencyBand::Delta,
        }
    }
    #[staticmethod]
    fn theta() -> Self {
        Self {
            inner: neurohid_types::signal::FrequencyBand::Theta,
        }
    }
    #[staticmethod]
    fn alpha() -> Self {
        Self {
            inner: neurohid_types::signal::FrequencyBand::Alpha,
        }
    }
    #[staticmethod]
    fn beta() -> Self {
        Self {
            inner: neurohid_types::signal::FrequencyBand::Beta,
        }
    }
    #[staticmethod]
    fn gamma() -> Self {
        Self {
            inner: neurohid_types::signal::FrequencyBand::Gamma,
        }
    }
    #[staticmethod]
    fn custom(low_hz: u32, high_hz: u32) -> Self {
        Self {
            inner: neurohid_types::signal::FrequencyBand::Custom { low_hz, high_hz },
        }
    }

    /// Returns `(low_hz, high_hz)` range.
    fn range_hz(&self) -> (f32, f32) {
        self.inner.range_hz()
    }

    #[getter]
    fn name(&self) -> String {
        format!("{:?}", self.inner)
    }

    fn __repr__(&self) -> String {
        let (lo, hi) = self.inner.range_hz();
        format!("FrequencyBand({:?}, {lo}-{hi}Hz)", self.inner)
    }
}

/// Channel configuration.
#[pyclass(name = "ChannelConfig", skip_from_py_object)]
#[derive(Clone)]
pub struct PyChannelConfig {
    pub inner: neurohid_types::signal::ChannelConfig,
}

#[pymethods]
impl PyChannelConfig {
    #[getter]
    fn id(&self) -> PyChannelId {
        PyChannelId {
            inner: self.inner.id.clone(),
        }
    }
    #[getter]
    fn position_10_20(&self) -> Option<&str> {
        self.inner.position_10_20.as_deref()
    }
    #[getter]
    fn enabled(&self) -> bool {
        self.inner.enabled
    }
    #[getter]
    fn reference(&self) -> Option<PyChannelId> {
        self.inner
            .reference
            .as_ref()
            .map(|r| PyChannelId { inner: r.clone() })
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "ChannelConfig(id='{}', enabled={})",
            self.inner.id.0, self.inner.enabled
        )
    }
}

/// Device channel configuration.
#[pyclass(name = "DeviceChannelConfig", skip_from_py_object)]
#[derive(Clone)]
pub struct PyDeviceChannelConfig {
    pub inner: neurohid_types::signal::DeviceChannelConfig,
}

#[pymethods]
impl PyDeviceChannelConfig {
    #[getter]
    fn channels(&self) -> Vec<PyChannelConfig> {
        self.inner
            .channels
            .iter()
            .cloned()
            .map(|c| PyChannelConfig { inner: c })
            .collect()
    }
    #[getter]
    fn sampling_rate_hz(&self) -> f32 {
        self.inner.sampling_rate_hz
    }
    #[getter]
    fn resolution_bits(&self) -> u8 {
        self.inner.resolution_bits
    }

    fn enabled_channel_count(&self) -> usize {
        self.inner.enabled_channel_count()
    }

    fn sample_period_micros(&self) -> i64 {
        self.inner.sample_period_micros()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "DeviceChannelConfig(channels={}, rate={}Hz)",
            self.inner.channels.len(),
            self.inner.sampling_rate_hz
        )
    }
}

// ---------------------------------------------------------------------------
// Control types: RuntimeModeState
// ---------------------------------------------------------------------------

/// Runtime mode state.
#[pyclass(name = "RuntimeModeState", eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyRuntimeModeState {
    pub inner: neurohid_types::control::RuntimeModeState,
}

#[pymethods]
impl PyRuntimeModeState {
    #[staticmethod]
    fn full() -> Self {
        Self {
            inner: neurohid_types::control::RuntimeModeState::Full,
        }
    }
    #[staticmethod]
    fn fallback() -> Self {
        Self {
            inner: neurohid_types::control::RuntimeModeState::Fallback,
        }
    }
    #[staticmethod]
    fn degraded() -> Self {
        Self {
            inner: neurohid_types::control::RuntimeModeState::Degraded,
        }
    }

    #[getter]
    fn name(&self) -> String {
        format!("{:?}", self.inner)
    }

    fn __repr__(&self) -> String {
        format!("RuntimeModeState.{:?}", self.inner)
    }
}

// ---------------------------------------------------------------------------
// Helper: serde → Python dict
// ---------------------------------------------------------------------------

fn serde_to_pydict<'py, T: ::serde::Serialize>(
    py: Python<'py>,
    value: &T,
) -> PyResult<Bound<'py, PyAny>> {
    pythonize::pythonize(py, value).map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("serialization failed: {e}"))
    })
}

/// Register type classes on the module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySystemConfig>()?;
    m.add_class::<PyProfileId>()?;
    m.add_class::<PyDeviceId>()?;
    m.add_class::<PyChannelId>()?;
    m.add_class::<PyDeviceType>()?;
    m.add_class::<PyConnectionState>()?;
    m.add_class::<PyDeviceInfo>()?;
    m.add_class::<PyDeviceStatus>()?;
    m.add_class::<PyConnectionSettings>()?;
    m.add_class::<PyDiscoveredStream>()?;
    m.add_class::<PySample>()?;
    m.add_class::<PyFeatureVector>()?;
    m.add_class::<PyKey>()?;
    m.add_class::<PyMouseButton>()?;
    m.add_class::<PyMouseMovement>()?;
    m.add_class::<PyScrollMovement>()?;
    m.add_class::<PyMouseButtonEvent>()?;
    m.add_class::<PyKeyEvent>()?;
    m.add_class::<PyMouseAction>()?;
    m.add_class::<PyKeyAction>()?;
    m.add_class::<PyActionSpace>()?;
    m.add_class::<PyAction>()?;
    m.add_class::<PyStreamMarker>()?;
    m.add_class::<PyControlSnapshot>()?;
    m.add_class::<PyTrainerSnapshot>()?;
    m.add_class::<PyDecisionEvent>()?;
    m.add_class::<PyErrpWindow>()?;
    m.add_class::<PyRuntimeEvent>()?;
    m.add_class::<PyObservation>()?;
    m.add_class::<PyNormalizationStats>()?;
    m.add_class::<PyModelManifest>()?;
    m.add_class::<PyTrainingEpisode>()?;
    m.add_class::<PyCandidateModelMetrics>()?;
    m.add_class::<PyProfileMetadata>()?;
    m.add_class::<PyRewardSignal>()?;
    m.add_class::<PyErrPResult>()?;
    m.add_class::<PyFrequencyBand>()?;
    m.add_class::<PyChannelConfig>()?;
    m.add_class::<PyDeviceChannelConfig>()?;
    m.add_class::<PyRuntimeModeState>()?;
    Ok(())
}
