//! Domain type wrappers exposed to Python.
//!
//! Types that are complex or have many fields use serde-based `from_dict`/
//! `to_dict` conversion. Simpler types expose fields directly.

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
        let json_str: String = json_mod
            .call_method1("dumps", (dict,))?
            .extract()?;
        let inner: neurohid_types::config::SystemConfig =
            serde_json::from_str(&json_str).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "invalid SystemConfig: {e}"
                ))
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

    #[getter]
    fn values(&self) -> Vec<f32> {
        self.inner.values.clone()
    }

    #[getter]
    fn quality(&self) -> Option<Vec<f32>> {
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
    #[getter]
    fn values(&self) -> Vec<f32> {
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
// RuntimeEvent — serde round-trip
// ---------------------------------------------------------------------------

/// A runtime event from the broadcast stream.
#[pyclass(name = "RuntimeEvent", skip_from_py_object)]
#[derive(Clone)]
pub struct PyRuntimeEvent {
    pub inner: neurohid_ipc::RuntimeEvent,
}

#[pymethods]
impl PyRuntimeEvent {
    /// The event variant name (e.g. "sample", "action_emitted", "snapshot").
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

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        serde_to_pydict(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!("RuntimeEvent(type='{}')", self.event_type())
    }
}

// ---------------------------------------------------------------------------
// Helper: serde → Python dict
// ---------------------------------------------------------------------------

fn serde_to_pydict<'py, T: ::serde::Serialize>(
    py: Python<'py>,
    value: &T,
) -> PyResult<Bound<'py, PyAny>> {
    let json_str = serde_json::to_string(value).map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("serialization failed: {e}"))
    })?;
    let json_mod = PyModule::import(py, "json")?;
    json_mod.call_method1("loads", (json_str,))
}

/// Register type classes on the module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySystemConfig>()?;
    m.add_class::<PyProfileId>()?;
    m.add_class::<PyDeviceId>()?;
    m.add_class::<PyChannelId>()?;
    m.add_class::<PySample>()?;
    m.add_class::<PyFeatureVector>()?;
    m.add_class::<PyAction>()?;
    m.add_class::<PyStreamMarker>()?;
    m.add_class::<PyControlSnapshot>()?;
    m.add_class::<PyTrainerSnapshot>()?;
    m.add_class::<PyRuntimeEvent>()?;
    Ok(())
}
