//! Python wrappers for the managed runtime.
//!
//! `PyRuntimeBuilder` → `PyRuntimeHandle` lifecycle mirrors the Rust embedder
//! API in `neurohid_core::runtime`.

use std::sync::Arc;

use numpy::{PyArray1, PyArrayMethods};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tokio::sync::Mutex;

use neurohid_core::runtime::{RuntimeBuilder, RuntimeCommand, RuntimeHandle, RuntimeIpcHandle};
use neurohid_ipc::IpcEnvelope;

use crate::errors::to_py_err;
use crate::storage::PyProfileStore;
use crate::streams::{
    PyActionStream, PyEventStream, PyFeatureStream, PyMarkerStream, PySampleStream,
};
use crate::types::{
    PyControlSnapshot, PyDecisionEvent, PyErrpWindow, PyProfileId, PySystemConfig,
    PyTrainerSnapshot,
};

// ---------------------------------------------------------------------------
// RuntimeBuilder
// ---------------------------------------------------------------------------

/// Builder for a managed `NeuroHID` runtime instance.
///
/// ```python
/// builder = RuntimeBuilder(SystemConfig.default())
/// runtime = await builder.start()
/// ```
#[pyclass(name = "RuntimeBuilder")]
pub struct PyRuntimeBuilder {
    config: neurohid_types::config::SystemConfig,
    profile_store: Option<neurohid_storage::ProfileStore>,
    profile_id: Option<neurohid_types::profile::ProfileId>,
    replay_path: Option<std::path::PathBuf>,
}

#[pymethods]
impl PyRuntimeBuilder {
    /// Create a builder from a `SystemConfig`.
    #[new]
    fn new(config: &PySystemConfig) -> Self {
        Self {
            config: config.inner.clone(),
            profile_store: None,
            profile_id: None,
            replay_path: None,
        }
    }

    /// Attach an initialized profile store.
    fn with_profile_store<'a>(
        mut slf: PyRefMut<'a, Self>,
        store: &PyProfileStore,
    ) -> PyRefMut<'a, Self> {
        slf.profile_store = Some(store.inner.clone());
        slf
    }

    /// Select the active profile.
    fn with_profile_id<'a>(
        mut slf: PyRefMut<'a, Self>,
        profile_id: &PyProfileId,
    ) -> PyRefMut<'a, Self> {
        slf.profile_id = Some(profile_id.inner.clone());
        slf
    }

    /// Use a session folder as the sample source (replay mode).
    fn with_replay_path<'a>(mut slf: PyRefMut<'a, Self>, path: &str) -> PyRefMut<'a, Self> {
        slf.replay_path = Some(std::path::PathBuf::from(path));
        slf
    }

    /// Start the runtime (async). Returns a `RuntimeHandle`.
    fn start<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let mut builder = RuntimeBuilder::new(self.config.clone());
        if let Some(store) = &self.profile_store {
            builder = builder.with_profile_store(store.clone());
        }
        if let Some(id) = &self.profile_id {
            builder = builder.with_profile_id(id.clone());
        }
        if let Some(path) = &self.replay_path {
            builder = builder.with_replay_path(path.clone());
        }

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let handle = builder.start().await.map_err(to_py_err)?;
            Ok(PyRuntimeHandle::from_handle(handle))
        })
    }
}

// ---------------------------------------------------------------------------
// RuntimeHandle
// ---------------------------------------------------------------------------

/// Live handle to a running managed runtime.
///
/// Provides synchronous getters (snapshot, `is_alive`), async commands,
/// stream subscriptions, and trainer bridge methods.
#[pyclass(name = "RuntimeHandle")]
pub struct PyRuntimeHandle {
    /// The underlying non-Clone handle, consumed by `wait()`.
    inner: Arc<Mutex<Option<RuntimeHandle>>>,
    /// Cloneable IPC facade for subscriptions & commands.
    ipc: RuntimeIpcHandle,
}

impl PyRuntimeHandle {
    fn from_handle(handle: RuntimeHandle) -> Self {
        let ipc = handle.ipc_handle();
        Self {
            inner: Arc::new(Mutex::new(Some(handle))),
            ipc,
        }
    }
}

#[pymethods]
impl PyRuntimeHandle {
    // -- Synchronous getters ------------------------------------------------

    /// Check whether the runtime is still alive.
    fn is_alive(&self) -> bool {
        // Try to read the inner handle synchronously. If we can't lock it
        // (unlikely), assume alive.
        let Ok(guard) = self.inner.try_lock() else {
            return true;
        };
        match &*guard {
            Some(h) => h.is_alive(),
            None => false,
        }
    }

    /// Read a non-blocking runtime snapshot.
    fn snapshot(&self) -> PyControlSnapshot {
        PyControlSnapshot {
            inner: self.ipc.snapshot(),
        }
    }

    /// Read trainer bridge snapshot.
    fn trainer_snapshot(&self) -> PyTrainerSnapshot {
        PyTrainerSnapshot {
            inner: self.ipc.trainer_snapshot(),
        }
    }

    // -- Commands -----------------------------------------------------------

    /// Send a command string to the runtime.
    ///
    /// Supported commands (pass as string): "stop", "`rescan_streams`",
    /// "`reload_model`", "`promote_candidate_model`", "`ml_bridge_reconnect`".
    ///
    /// For commands with parameters, use the dict overload.
    #[pyo3(signature = (command, **kwargs))]
    fn command(&self, command: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let cmd = parse_runtime_command(command, kwargs)?;
        self.ipc.command(cmd).map_err(to_py_err)
    }

    // -- Async methods ------------------------------------------------------

    /// Dispatch a control request and return the response as a JSON string (async).
    ///
    /// The Python caller should `json.loads()` the result to get a dict.
    fn dispatch_control<'py>(
        &self,
        py: Python<'py>,
        request_dict: &Bound<'py, PyDict>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let json_mod = PyModule::import(py, "json")?;
        let json_str: String = json_mod.call_method1("dumps", (request_dict,))?.extract()?;
        let request: neurohid_types::control::ControlRequest = serde_json::from_str(&json_str)
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid ControlRequest: {e}"))
            })?;
        let ipc = self.ipc.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let response = ipc.dispatch_control_request(request).await;
            serde_json::to_string(&response).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "response serialization failed: {e}"
                ))
            })
        })
    }

    /// Blocking variant of `dispatch_control` for synchronous Python callers.
    ///
    /// Takes a JSON string (the serialized `ControlRequest`), blocks on the
    /// embedded tokio runtime, and returns the JSON response string.
    fn dispatch_control_sync(&self, request_json: &str) -> PyResult<String> {
        let request: neurohid_types::control::ControlRequest = serde_json::from_str(request_json)
            .map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid ControlRequest: {e}"))
        })?;
        let ipc = self.ipc.clone();
        let response = crate::tokio_runtime()
            .block_on(async move { ipc.dispatch_control_request(request).await });
        serde_json::to_string(&response).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("response serialization failed: {e}"))
        })
    }

    /// Graceful shutdown: sends Stop and waits for termination (async).
    fn shutdown<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let ipc = self.ipc.clone();
        let inner = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let _ = ipc.command(RuntimeCommand::Stop);
            let handle = {
                let mut guard = inner.lock().await;
                guard.take()
            };
            if let Some(h) = handle {
                h.wait().await.map_err(to_py_err)?;
            }
            Ok(())
        })
    }

    /// Wait for runtime termination (async). Consumes the handle.
    fn wait<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let handle = {
                let mut guard = inner.lock().await;
                guard.take()
            };
            if let Some(h) = handle {
                h.wait().await.map_err(to_py_err)?;
            }
            Ok(())
        })
    }

    // -- Stream subscriptions (async iterators) -----------------------------

    /// Subscribe to live raw sample stream.
    fn subscribe_samples(&self) -> PySampleStream {
        PySampleStream::new(self.ipc.subscribe_samples())
    }

    /// Subscribe to live feature vector stream.
    fn subscribe_features(&self) -> PyFeatureStream {
        PyFeatureStream::new(self.ipc.subscribe_features())
    }

    /// Subscribe to live decoded action stream.
    fn subscribe_actions(&self) -> PyActionStream {
        PyActionStream::new(self.ipc.subscribe_actions())
    }

    /// Subscribe to stream marker/event annotations.
    fn subscribe_markers(&self) -> PyMarkerStream {
        PyMarkerStream::new(self.ipc.subscribe_markers())
    }

    /// Subscribe to runtime events (all types).
    fn subscribe_events(&self) -> PyEventStream {
        PyEventStream::new(self.ipc.subscribe_runtime_bridge_events())
    }

    // -- Batched stream subscriptions (numpy) -------------------------------

    /// Collect ``batch_size`` samples and return as a 2D numpy array.
    ///
    /// Returns shape ``(batch_size, channels)`` with dtype ``float32``.
    /// Blocks (async) until the batch is full or the stream closes.
    fn recv_sample_batch<'py>(
        &self,
        py: Python<'py>,
        batch_size: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut rx = self.ipc.subscribe_samples();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut rows: Vec<Vec<f32>> = Vec::with_capacity(batch_size);
            while rows.len() < batch_size {
                match rx.recv().await {
                    Ok(sample) => rows.push(sample.values),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            if rows.is_empty() {
                return Err(pyo3::exceptions::PyStopAsyncIteration::new_err(
                    "stream closed before any samples received",
                ));
            }
            let cols = rows[0].len();
            let flat: Vec<f32> = rows.iter().flat_map(|r| r.iter().copied()).collect();
            let n_rows = rows.len();
            Python::try_attach(|py| {
                let arr = PyArray1::from_vec(py, flat)
                    .reshape([n_rows, cols])
                    .map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "batch reshape failed: {e}"
                        ))
                    })?;
                Ok(arr.into_any().unbind())
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Python interpreter not available")
            })?
        })
    }

    /// Collect ``batch_size`` feature vectors and return as a 2D numpy array.
    ///
    /// Returns shape ``(batch_size, dims)`` with dtype ``float32``.
    fn recv_feature_batch<'py>(
        &self,
        py: Python<'py>,
        batch_size: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut rx = self.ipc.subscribe_features();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut rows: Vec<Vec<f32>> = Vec::with_capacity(batch_size);
            while rows.len() < batch_size {
                match rx.recv().await {
                    Ok(fv) => rows.push(fv.values),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            if rows.is_empty() {
                return Err(pyo3::exceptions::PyStopAsyncIteration::new_err(
                    "stream closed before any features received",
                ));
            }
            let cols = rows[0].len();
            let flat: Vec<f32> = rows.iter().flat_map(|r| r.iter().copied()).collect();
            let n_rows = rows.len();
            Python::try_attach(|py| {
                let arr = PyArray1::from_vec(py, flat)
                    .reshape([n_rows, cols])
                    .map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "batch reshape failed: {e}"
                        ))
                    })?;
                Ok(arr.into_any().unbind())
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Python interpreter not available")
            })?
        })
    }

    // -- Trainer bridge methods ---------------------------------------------

    /// Notify the runtime that a trainer has connected (async).
    fn trainer_connect<'py>(
        &self,
        py: Python<'py>,
        session_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let ipc = self.ipc.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            ipc.trainer_connected(session_id).await.map_err(to_py_err)?;
            Ok(())
        })
    }

    /// Send a trainer-stream envelope dict to the runtime (async).
    fn trainer_send<'py>(
        &self,
        py: Python<'py>,
        envelope_dict: &Bound<'py, PyDict>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let json_mod = PyModule::import(py, "json")?;
        let json_str: String = json_mod
            .call_method1("dumps", (envelope_dict,))?
            .extract()?;
        let envelope: IpcEnvelope = serde_json::from_str(&json_str).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid IpcEnvelope: {e}"))
        })?;
        let ipc = self.ipc.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            ipc.trainer_send_envelope(envelope)
                .await
                .map_err(to_py_err)?;
            Ok(())
        })
    }

    /// Receive one trainer-stream envelope from the runtime (async).
    /// Returns a JSON string, or `None` if the channel is closed.
    /// The Python caller should `json.loads()` the result to get a dict.
    ///
    /// Prefer ``trainer_recv_typed`` for numeric-heavy payloads to avoid
    /// JSON round-trips.
    fn trainer_recv<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let ipc = self.ipc.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let maybe_envelope = ipc.recv_trainer_envelope().await;
            match maybe_envelope {
                Some(envelope) => {
                    let json_str = serde_json::to_string(&envelope).map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "envelope serialization failed: {e}"
                        ))
                    })?;
                    Ok(Some(json_str))
                }
                None => Ok(None),
            }
        })
    }

    /// Receive one trainer-stream envelope and return a Python dict with
    /// typed payload extraction.
    ///
    /// Returns a dict with keys: ``v``, ``channel``, ``msg_type``, ``seq``,
    /// ``sent_at_us``, ``session_id``, ``request_id``, ``payload``.
    ///
    /// When ``msg_type`` is ``"decision_event"`` or ``"errp_window"``, the
    /// ``payload`` value is a typed ``DecisionEvent`` or ``ErrpWindow``
    /// object with numpy-backed arrays. For other message types the payload
    /// is a plain Python dict.
    ///
    /// Returns ``None`` if the channel is closed.
    fn trainer_recv_typed<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let ipc = self.ipc.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let maybe_envelope = ipc.recv_trainer_envelope().await;
            match maybe_envelope {
                Some(envelope) => Python::try_attach(|py| {
                    let dict = pyo3::types::PyDict::new(py);
                    dict.set_item("v", envelope.v)?;
                    dict.set_item(
                        "channel",
                        serde_json::to_string(&envelope.channel).map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "channel serialize: {e}"
                            ))
                        })?,
                    )?;
                    dict.set_item("msg_type", &envelope.msg_type)?;
                    dict.set_item("seq", envelope.seq)?;
                    dict.set_item("sent_at_us", envelope.sent_at_us)?;
                    dict.set_item("session_id", &envelope.session_id)?;
                    dict.set_item("request_id", &envelope.request_id)?;

                    // Typed payload extraction for hot-path message types.
                    let payload: Py<PyAny> = match envelope.msg_type.as_str() {
                        "decision_event" => {
                            let event: neurohid_ipc::DecisionEvent =
                                serde_json::from_value(envelope.payload).map_err(|e| {
                                    pyo3::exceptions::PyValueError::new_err(format!(
                                        "decision_event decode: {e}"
                                    ))
                                })?;
                            PyDecisionEvent { inner: event }
                                .into_pyobject(py)?
                                .unbind()
                                .into()
                        }
                        "errp_window" => {
                            let window: neurohid_ipc::ErrpWindow =
                                serde_json::from_value(envelope.payload).map_err(|e| {
                                    pyo3::exceptions::PyValueError::new_err(format!(
                                        "errp_window decode: {e}"
                                    ))
                                })?;
                            PyErrpWindow { inner: window }
                                .into_pyobject(py)?
                                .unbind()
                                .into()
                        }
                        _ => {
                            // Fallback: convert payload serde_json::Value → Python dict
                            pythonize::pythonize(py, &envelope.payload)
                                .map_err(|e| {
                                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                                        "payload pythonize: {e}"
                                    ))
                                })?
                                .unbind()
                        }
                    };
                    dict.set_item("payload", payload)?;
                    Ok(Some(dict.unbind()))
                })
                .ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Python interpreter not available")
                })?,
                None => Ok(None),
            }
        })
    }

    /// Notify the runtime that the trainer has disconnected (async).
    fn trainer_disconnect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let ipc = self.ipc.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            ipc.trainer_disconnected().await.map_err(to_py_err)?;
            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        let alive = self.is_alive();
        format!("RuntimeHandle(alive={alive})")
    }
}

// ---------------------------------------------------------------------------
// Command parsing helper
// ---------------------------------------------------------------------------

fn parse_runtime_command(
    name: &str,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<RuntimeCommand> {
    match name {
        "start" => Ok(RuntimeCommand::Start),
        "stop" => Ok(RuntimeCommand::Stop),
        "rescan_streams" => Ok(RuntimeCommand::RescanStreams),
        "reload_model" => Ok(RuntimeCommand::ReloadModel),
        "promote_candidate_model" => Ok(RuntimeCommand::PromoteCandidateModel),
        "ml_bridge_reconnect" => Ok(RuntimeCommand::MlBridgeReconnect),
        "connect_stream" => {
            let stream_id = kwarg_str(kwargs, "stream_id")?;
            Ok(RuntimeCommand::ConnectStream { stream_id })
        }
        "disconnect_stream" => {
            let stream_id = kwarg_str(kwargs, "stream_id")?;
            Ok(RuntimeCommand::DisconnectStream { stream_id })
        }
        "toggle_calibration" => {
            let enabled = kwarg_bool(kwargs, "enabled")?;
            Ok(RuntimeCommand::ToggleCalibration { enabled })
        }
        "toggle_output" => {
            let enabled = kwarg_bool(kwargs, "enabled")?;
            Ok(RuntimeCommand::ToggleOutput { enabled })
        }
        "set_learning_enabled" => {
            let enabled = kwarg_bool(kwargs, "enabled")?;
            Ok(RuntimeCommand::SetLearningEnabled { enabled })
        }
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown command: {name}"
        ))),
    }
}

fn kwarg_str(kwargs: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<String> {
    kwargs
        .and_then(|kw| kw.get_item(key).ok().flatten())
        .map(|v| v.extract::<String>())
        .transpose()?
        .ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "missing required keyword argument: {key}"
            ))
        })
}

fn kwarg_bool(kwargs: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<bool> {
    kwargs
        .and_then(|kw| kw.get_item(key).ok().flatten())
        .map(|v| v.extract::<bool>())
        .transpose()?
        .ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "missing required keyword argument: {key}"
            ))
        })
}

/// Register runtime classes on the module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRuntimeBuilder>()?;
    m.add_class::<PyRuntimeHandle>()?;
    Ok(())
}
