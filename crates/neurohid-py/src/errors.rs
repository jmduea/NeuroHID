//! Python exception hierarchy for NeuroHID errors.
//!
//! Maps `neurohid_types::error::Error` variants to a Python exception tree
//! rooted at `NeurohidError`.

use pyo3::exceptions::PyException;
use pyo3::prelude::*;

pyo3::create_exception!(neurohid, NeurohidError, PyException);
pyo3::create_exception!(neurohid, ConfigError, NeurohidError);
pyo3::create_exception!(neurohid, DeviceError, NeurohidError);
pyo3::create_exception!(neurohid, SignalError, NeurohidError);
pyo3::create_exception!(neurohid, IpcError, NeurohidError);
pyo3::create_exception!(neurohid, InternalError, NeurohidError);

/// Convert a `neurohid_types::error::Error` into the appropriate Python exception.
pub fn to_py_err(err: neurohid_types::error::Error) -> PyErr {
    use neurohid_types::error::Error;
    match &err {
        Error::Config(_) => ConfigError::new_err(err.to_string()),
        Error::Device(_) => DeviceError::new_err(err.to_string()),
        Error::Signal(_) => SignalError::new_err(err.to_string()),
        Error::Ipc(_) => IpcError::new_err(err.to_string()),
        Error::Internal(_) => InternalError::new_err(err.to_string()),
        _ => NeurohidError::new_err(err.to_string()),
    }
}

/// Register all exception classes on the module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("NeurohidError", m.py().get_type::<NeurohidError>())?;
    m.add("ConfigError", m.py().get_type::<ConfigError>())?;
    m.add("DeviceError", m.py().get_type::<DeviceError>())?;
    m.add("SignalError", m.py().get_type::<SignalError>())?;
    m.add("IpcError", m.py().get_type::<IpcError>())?;
    m.add("InternalError", m.py().get_type::<InternalError>())?;
    Ok(())
}
