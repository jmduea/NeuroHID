//! Bridge tokio broadcast receivers to Python async iterators and callbacks.

use std::sync::Arc;

use pyo3::prelude::*;
use tokio::sync::{Mutex, broadcast};
use tracing::warn;

use crate::types::{PyAction, PyFeatureVector, PyRuntimeEvent, PySample, PyStreamMarker};

// NOTE: Free-threaded Python 3.14 — no GIL exists.
// All stream items are returned via `future_into_py` which converts
// the return type through PyO3's managed boundary.  No Python interaction
// happens inside async blocks.

// ---------------------------------------------------------------------------
// Async iterator wrappers
// ---------------------------------------------------------------------------

/// Async iterator over live `Sample` values.
#[pyclass(name = "SampleStream")]
pub struct PySampleStream {
    rx: Arc<Mutex<broadcast::Receiver<neurohid_types::signal::Sample>>>,
}

impl PySampleStream {
    pub fn new(rx: broadcast::Receiver<neurohid_types::signal::Sample>) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
        }
    }
}

#[pymethods]
impl PySampleStream {
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let rx = Arc::clone(&self.rx);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            loop {
                let mut guard = rx.lock().await;
                match guard.recv().await {
                    Ok(sample) => return Ok(PySample { inner: sample }),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        drop(guard);
                        warn!("SampleStream lagged, dropped {n} items");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(pyo3::exceptions::PyStopAsyncIteration::new_err(
                            "stream closed",
                        ));
                    }
                }
            }
        })
    }
}

/// Async iterator over live `FeatureVector` values.
#[pyclass(name = "FeatureStream")]
pub struct PyFeatureStream {
    rx: Arc<Mutex<broadcast::Receiver<neurohid_types::signal::FeatureVector>>>,
}

impl PyFeatureStream {
    pub fn new(rx: broadcast::Receiver<neurohid_types::signal::FeatureVector>) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
        }
    }
}

#[pymethods]
impl PyFeatureStream {
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let rx = Arc::clone(&self.rx);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            loop {
                let mut guard = rx.lock().await;
                match guard.recv().await {
                    Ok(fv) => return Ok(PyFeatureVector { inner: fv }),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        drop(guard);
                        warn!("FeatureStream lagged, dropped {n} items");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(pyo3::exceptions::PyStopAsyncIteration::new_err(
                            "stream closed",
                        ));
                    }
                }
            }
        })
    }
}

/// Async iterator over live `Action` values.
#[pyclass(name = "ActionStream")]
pub struct PyActionStream {
    rx: Arc<Mutex<broadcast::Receiver<neurohid_types::action::Action>>>,
}

impl PyActionStream {
    pub fn new(rx: broadcast::Receiver<neurohid_types::action::Action>) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
        }
    }
}

#[pymethods]
impl PyActionStream {
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let rx = Arc::clone(&self.rx);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            loop {
                let mut guard = rx.lock().await;
                match guard.recv().await {
                    Ok(action) => return Ok(PyAction { inner: action }),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        drop(guard);
                        warn!("ActionStream lagged, dropped {n} items");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(pyo3::exceptions::PyStopAsyncIteration::new_err(
                            "stream closed",
                        ));
                    }
                }
            }
        })
    }
}

/// Async iterator over live `StreamMarker` values.
#[pyclass(name = "MarkerStream")]
pub struct PyMarkerStream {
    rx: Arc<Mutex<broadcast::Receiver<neurohid_types::event::StreamMarker>>>,
}

impl PyMarkerStream {
    pub fn new(rx: broadcast::Receiver<neurohid_types::event::StreamMarker>) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
        }
    }
}

#[pymethods]
impl PyMarkerStream {
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let rx = Arc::clone(&self.rx);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            loop {
                let mut guard = rx.lock().await;
                match guard.recv().await {
                    Ok(marker) => return Ok(PyStreamMarker { inner: marker }),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        drop(guard);
                        warn!("MarkerStream lagged, dropped {n} items");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(pyo3::exceptions::PyStopAsyncIteration::new_err(
                            "stream closed",
                        ));
                    }
                }
            }
        })
    }
}

/// Async iterator over live `RuntimeEvent` values.
#[pyclass(name = "EventStream")]
pub struct PyEventStream {
    rx: Arc<Mutex<broadcast::Receiver<neurohid_ipc::RuntimeEvent>>>,
}

impl PyEventStream {
    pub fn new(rx: broadcast::Receiver<neurohid_ipc::RuntimeEvent>) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
        }
    }
}

#[pymethods]
impl PyEventStream {
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let rx = Arc::clone(&self.rx);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            loop {
                let mut guard = rx.lock().await;
                match guard.recv().await {
                    Ok(event) => return Ok(PyRuntimeEvent { inner: event }),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        drop(guard);
                        warn!("EventStream lagged, dropped {n} items");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(pyo3::exceptions::PyStopAsyncIteration::new_err(
                            "stream closed",
                        ));
                    }
                }
            }
        })
    }
}

/// Register stream classes on the module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySampleStream>()?;
    m.add_class::<PyFeatureStream>()?;
    m.add_class::<PyActionStream>()?;
    m.add_class::<PyMarkerStream>()?;
    m.add_class::<PyEventStream>()?;
    Ok(())
}
