//! Python bindings for the `neurohid-ipc` external-mode client.
//!
//! These bindings let Python connect to a *running* NeuroHID service process
//! over IPC (local socket or TCP loopback), rather than embedding the runtime
//! in-process.

use std::sync::Arc;

use neurohid_ipc::client::IpcClient;
use neurohid_ipc::protocol::{IpcConfig, IpcTransport};
use pyo3::prelude::*;

use crate::tokio_runtime;

/// IPC transport kind.
#[pyclass(name = "IpcTransport", eq, skip_from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub struct PyIpcTransport {
    inner: IpcTransport,
}

#[pymethods]
impl PyIpcTransport {
    /// Cross-platform local socket (Unix domain / Windows named pipe).
    #[staticmethod]
    fn local_socket() -> Self {
        Self {
            inner: IpcTransport::LocalSocket,
        }
    }
    /// TCP `127.0.0.1` fallback.
    #[staticmethod]
    fn tcp_loopback() -> Self {
        Self {
            inner: IpcTransport::TcpLoopback,
        }
    }

    fn __repr__(&self) -> String {
        match self.inner {
            IpcTransport::LocalSocket => "IpcTransport.LocalSocket".to_owned(),
            IpcTransport::TcpLoopback => "IpcTransport.TcpLoopback".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// IpcConfig
// ---------------------------------------------------------------------------

/// IPC endpoint configuration.
#[pyclass(name = "IpcConfig", from_py_object)]
#[derive(Clone)]
pub struct PyIpcConfig {
    pub inner: IpcConfig,
}

#[pymethods]
impl PyIpcConfig {
    /// Create a config with defaults (local socket, standard endpoint).
    #[new]
    fn new() -> Self {
        Self {
            inner: IpcConfig::default(),
        }
    }

    /// Default config for runtime event streaming.
    #[staticmethod]
    fn runtime_stream_default() -> Self {
        Self {
            inner: IpcConfig::runtime_stream_default(),
        }
    }

    /// Default config for control requests.
    #[staticmethod]
    fn control_default() -> Self {
        Self {
            inner: IpcConfig::control_default(),
        }
    }

    #[getter]
    fn endpoint(&self) -> &str {
        &self.inner.endpoint
    }

    #[getter]
    fn connect_timeout_ms(&self) -> u64 {
        self.inner.connect_timeout_ms
    }

    #[getter]
    fn auto_reconnect(&self) -> bool {
        self.inner.auto_reconnect
    }

    fn __repr__(&self) -> String {
        format!(
            "IpcConfig(endpoint='{}', auto_reconnect={})",
            self.inner.endpoint, self.inner.auto_reconnect
        )
    }
}

// ---------------------------------------------------------------------------
// IpcClient
// ---------------------------------------------------------------------------

/// External-mode IPC client for connecting to a running NeuroHID service.
///
/// ```python
/// client = IpcClient(IpcConfig.control_default())
/// await client.connect()
/// response = await client.send_control({"command": "RescanStreams"})
/// await client.disconnect()
/// ```
#[pyclass(name = "IpcClient")]
pub struct PyIpcClient {
    inner: Arc<tokio::sync::Mutex<IpcClient>>,
}

#[pymethods]
impl PyIpcClient {
    #[new]
    fn new(config: PyIpcConfig) -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(IpcClient::new(config.inner))),
        }
    }

    /// Connect to the NeuroHID service endpoint.
    fn connect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut guard = client.lock().await;
            guard.connect().await.map_err(crate::errors::to_py_err)
        })
    }

    fn is_connected(&self) -> bool {
        self.inner.blocking_lock().is_connected()
    }

    /// Disconnect from the service.
    fn disconnect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut guard = client.lock().await;
            guard.disconnect().await.map_err(crate::errors::to_py_err)
        })
    }

    /// Send a control request (as a JSON string) and return the response JSON.
    fn send_control<'py>(
        &self,
        py: Python<'py>,
        request_json: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let request: neurohid_types::control::ControlRequest =
                serde_json::from_str(&request_json).map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "invalid control request JSON: {e}"
                    ))
                })?;
            let mut guard = client.lock().await;
            let resp = guard
                .send_control_request(request, "python", 0)
                .await
                .map_err(crate::errors::to_py_err)?;
            serde_json::to_string(&resp)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    fn __repr__(&self) -> String {
        let connected = self.inner.blocking_lock().is_connected();
        format!("IpcClient(connected={connected})")
    }
}

// ---------------------------------------------------------------------------
// Blocking helper
// ---------------------------------------------------------------------------

/// One-shot blocking control request (no connect/disconnect lifecycle).
///
/// Connects, sends the request, receives the response, and disconnects.
/// Runs the async I/O on the shared tokio runtime.
#[pyfunction]
fn send_control_request_blocking(
    config: PyIpcConfig,
    request_json: String,
) -> PyResult<String> {
    let request: neurohid_types::control::ControlRequest =
        serde_json::from_str(&request_json).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid control request JSON: {e}"))
        })?;

    let resp = tokio_runtime()
        .block_on(neurohid_ipc::client::send_control_request_once(
            config.inner,
            request,
            "python",
            0,
        ))
        .map_err(crate::errors::to_py_err)?;

    serde_json::to_string(&resp)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyIpcTransport>()?;
    m.add_class::<PyIpcConfig>()?;
    m.add_class::<PyIpcClient>()?;
    m.add_function(wrap_pyfunction!(send_control_request_blocking, m)?)?;
    Ok(())
}
