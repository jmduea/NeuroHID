//! In-process `PyO3` bindings for `NeuroHID`.
//!
//! Embeds the managed Rust runtime directly in the Python process via
//! `neurohid-core`.  All data flows through shared-memory broadcast channels
//! — no serialization, no sockets.
//!
//! # Quick start (Python)
//! ```python
//! from neurohid import SystemConfig, RuntimeBuilder
//!
//! config = SystemConfig.default()
//! builder = RuntimeBuilder(config)
//! runtime = await builder.start()
//!
//! async for sample in runtime.subscribe_samples():
//!     print(sample)
//!
//! await runtime.shutdown()
//! ```

#![expect(clippy::used_underscore_binding, reason = "pyo3 macro generates these")]

mod errors;
mod ipc;
mod platform;
mod runtime;
mod signal;
mod storage;
mod streams;
mod types;

use std::sync::OnceLock;

use pyo3::prelude::*;

/// Module-level shared tokio runtime.
///
/// All async work (runtime start, stream reads, callbacks) is multiplexed onto
/// this single multi-thread executor.  `pyo3_async_runtimes::tokio` is
/// initialized with a reference to it so that `future_into_py` works from any
/// Python thread.
static TOKIO_RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Get or lazily initialize the shared tokio runtime.
pub(crate) fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    TOKIO_RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("neurohid-py")
            .build()
            .expect("failed to create tokio runtime for neurohid-py")
    })
}

#[pymodule]
fn neurohid(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize pyo3-async-runtimes with our shared tokio runtime.
    // Uses init_with_runtime which takes &'static Runtime (provided by OnceLock).
    let _ = pyo3_async_runtimes::tokio::init_with_runtime(tokio_runtime());

    // Register submodule components.
    errors::register(m)?;
    types::register(m)?;
    streams::register(m)?;
    signal::register(m)?;
    platform::register(m)?;
    ipc::register(m)?;
    storage::register(m)?;
    runtime::register(m)?;
    Ok(())
}
