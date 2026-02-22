//! `PyO3` Python bindings for `NeuroHID`.
//!
//! Exposes a synchronous `IpcChannel` class that replaces the `ipckit` Python
//! package for the `local_socket` transport path.  Python callers use
//! `asyncio.to_thread` to wrap blocking calls, exactly as they did with
//! `ipckit`.  The GIL is released during every blocking I/O call so free-
//! threaded Python builds benefit from true parallelism.
//!
//! # Example (Python)
//! ```python
//! from neurohid_bindings import IpcChannel
//! with IpcChannel.connect("neurohid.control.v3") as ch:
//!     ch.send_json({"v": 3, "channel": "control.rpc", ...})
//!     response = ch.recv_json()
//! ```

#![allow(clippy::used_underscore_binding)] // pyo3 macro generates these

use std::sync::Mutex;

use ipckit::AsyncLocalSocketStream;
use pyo3::exceptions::{PyConnectionError, PyOSError, PyRuntimeError, PyUnicodeDecodeError};
use pyo3::prelude::*;
use pyo3::types::PyModule;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Maximum inbound frame size (2 MiB).  Matches
/// `IpcConfig::default().max_message_size` in `neurohid-ipc`.
const MAX_FRAME_SIZE: usize = 2 * 1024 * 1024;

/// Synchronous framed-JSON IPC channel over a `NeuroHID` local socket.
///
/// Drop-in replacement for `ipckit.IpcChannel` in Python.  Framing is
/// identical to the Rust service: a 4-byte little-endian payload length
/// followed by the UTF-8 JSON body.
///
/// Supports the context-manager protocol (`with` statement).
#[pyclass(name = "IpcChannel")]
struct IpcChannel {
    /// The tokio runtime used to drive async I/O synchronously.
    ///
    /// `Runtime` is `Send + Sync` in tokio 1.x; stored outside the mutex to
    /// allow it to be borrowed immutably while the stream mutex is held.
    rt: tokio::runtime::Runtime,
    /// The underlying async stream, wrapped so it can be shared across Python
    /// threads (GIL is released during blocking calls).
    stream: Mutex<Option<AsyncLocalSocketStream>>,
}

/// Lock the stream mutex, converting a poison error into `PyRuntimeError`.
fn lock_stream(
    mutex: &Mutex<Option<AsyncLocalSocketStream>>,
) -> PyResult<std::sync::MutexGuard<'_, Option<AsyncLocalSocketStream>>> {
    mutex
        .lock()
        .map_err(|_| PyRuntimeError::new_err("stream mutex poisoned"))
}

#[pymethods]
impl IpcChannel {
    /// Connect to a `NeuroHID` local-socket endpoint and return a new channel.
    ///
    /// `endpoint` is the bare endpoint name, e.g. `"neurohid.control.v3"`.
    /// The GIL is released for the duration of the connection attempt.
    #[staticmethod]
    fn connect(py: Python<'_>, endpoint: &str) -> PyResult<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        let endpoint_owned = endpoint.to_string();
        let connect_result: Result<AsyncLocalSocketStream, String> = py.detach(|| {
            rt.block_on(AsyncLocalSocketStream::connect(&endpoint_owned))
                .map_err(|e| e.to_string())
        });
        let stream = connect_result
            .map_err(|e| PyConnectionError::new_err(format!("{endpoint_owned}: {e}")))?;

        Ok(Self {
            rt,
            stream: Mutex::new(Some(stream)),
        })
    }

    /// Send `envelope` (a Python dict) as a framed JSON message.
    ///
    /// JSON serialization uses Python's `json.dumps` to preserve exact
    /// Python type semantics.  The GIL is released during the write.
    fn send_json(&self, py: Python<'_>, envelope: &Bound<'_, PyAny>) -> PyResult<()> {
        // Serialize while holding the GIL.
        let json_str: String = {
            let json_mod = PyModule::import(py, "json")?;
            json_mod.call_method1("dumps", (envelope,))?.extract()?
        };
        let json_bytes = json_str.into_bytes();
        let payload_len = u32::try_from(json_bytes.len())
            .map_err(|_| PyOSError::new_err("IPC message exceeds 4 GiB limit"))?;

        // Release GIL for the blocking write.
        let write_result: Result<(), String> = py.detach(|| {
            let mut guard = lock_stream(&self.stream).map_err(|e| e.to_string())?;
            let Some(stream) = guard.as_mut() else {
                return Err("channel is closed".to_string());
            };
            self.rt.block_on(async {
                stream
                    .write_all(&payload_len.to_le_bytes())
                    .await
                    .map_err(|e| e.to_string())?;
                stream
                    .write_all(&json_bytes)
                    .await
                    .map_err(|e| e.to_string())?;
                stream.flush().await.map_err(|e| e.to_string())
            })
        });

        write_result.map_err(PyOSError::new_err)
    }

    /// Receive one framed JSON message and return it as a Python dict.
    ///
    /// Blocks until a complete message arrives.  The GIL is released for the
    /// duration of the read; JSON parsing happens back on the Python side.
    fn recv_json<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        // Release GIL for the blocking read.
        let read_result: Result<Vec<u8>, String> = py.detach(|| {
            let mut guard = lock_stream(&self.stream).map_err(|e| e.to_string())?;
            let Some(stream) = guard.as_mut() else {
                return Err("channel is closed".to_string());
            };
            self.rt.block_on(async {
                let mut len_buf = [0u8; 4];
                stream
                    .read_exact(&mut len_buf)
                    .await
                    .map_err(|e| e.to_string())?;
                let len = u32::from_le_bytes(len_buf) as usize;
                if len > MAX_FRAME_SIZE {
                    return Err(format!(
                        "inbound frame size ({len} bytes) exceeds {MAX_FRAME_SIZE} byte limit"
                    ));
                }
                let mut buf = vec![0u8; len];
                stream
                    .read_exact(&mut buf)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(buf)
            })
        });

        let json_bytes = read_result.map_err(PyOSError::new_err)?;

        // Parse JSON while holding the GIL.
        let json_str = std::str::from_utf8(&json_bytes)
            .map_err(|e| PyUnicodeDecodeError::new_err(e.to_string()))?;
        let json_mod = PyModule::import(py, "json")?;
        json_mod.call_method1("loads", (json_str,))
    }

    /// Close the channel and release the underlying socket.
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let stream_opt = {
            let mut guard = lock_stream(&self.stream)?;
            guard.take()
        };
        if let Some(mut stream) = stream_opt {
            // Graceful shutdown: release GIL while the OS closes the handle.
            py.detach(|| {
                let _ = self.rt.block_on(stream.shutdown());
            });
        }
        Ok(())
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(
        &self,
        py: Python<'_>,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        self.close(py)?;
        Ok(false)
    }
}

#[pymodule]
fn neurohid_bindings(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<IpcChannel>()?;
    Ok(())
}
