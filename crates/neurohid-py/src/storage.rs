//! Python wrappers for storage helpers.

use pyo3::prelude::*;

use crate::errors::to_py_err;
use crate::types::PySystemConfig;

/// Platform-specific data directory manager.
#[pyclass(name = "DataPaths", skip_from_py_object)]
#[derive(Clone)]
pub struct PyDataPaths {
    pub inner: neurohid_storage::DataPaths,
}

#[pymethods]
impl PyDataPaths {
    /// Create a `DataPaths` instance. If `root` is `None`, uses the platform default.
    #[new]
    #[pyo3(signature = (root = None))]
    fn new(root: Option<String>) -> PyResult<Self> {
        let inner =
            neurohid_storage::DataPaths::new(root.map(std::path::PathBuf::from)).map_err(to_py_err)?;
        Ok(Self { inner })
    }

    /// Root data directory path.
    #[getter]
    fn root(&self) -> String {
        self.inner.root().display().to_string()
    }

    fn __repr__(&self) -> String {
        format!("DataPaths('{}')", self.inner.root().display())
    }
}

/// Configuration file manager.
#[pyclass(name = "ConfigStore")]
pub struct PyConfigStore {
    /// `ConfigStore.paths` is private, so we store a copy and
    /// reconstruct the store in async methods.
    paths: neurohid_storage::DataPaths,
}

#[pymethods]
impl PyConfigStore {
    /// Create a `ConfigStore` from `DataPaths`.
    #[new]
    fn new(paths: &PyDataPaths) -> Self {
        Self {
            paths: paths.inner.clone(),
        }
    }

    /// Load the system configuration (async).
    fn load<'py>(slf: &Bound<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let paths = slf.borrow().paths.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let store = neurohid_storage::ConfigStore::new(paths);
            let config = store.load().await.map_err(to_py_err)?;
            Ok(PySystemConfig { inner: config })
        })
    }
}

/// Profile manager.
#[pyclass(name = "ProfileStore", skip_from_py_object)]
#[derive(Clone)]
pub struct PyProfileStore {
    pub inner: neurohid_storage::ProfileStore,
}

#[pymethods]
impl PyProfileStore {
    fn __repr__(&self) -> String {
        format!(
            "ProfileStore(root='{}')",
            self.inner.data_root().display()
        )
    }
}

/// One-shot storage initialization (async). Returns `(ProfileStore, ConfigStore)`.
#[pyfunction]
fn initialize_storage(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let (profile_store, _config_store) =
            neurohid_storage::initialize().await.map_err(to_py_err)?;
        Ok(PyProfileStore {
            inner: profile_store,
        })
    })
}

/// Register storage classes on the module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDataPaths>()?;
    m.add_class::<PyConfigStore>()?;
    m.add_class::<PyProfileStore>()?;
    m.add_function(pyo3::wrap_pyfunction!(initialize_storage, m)?)?;
    Ok(())
}
