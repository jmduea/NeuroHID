//! Python wrappers for storage helpers.

use pyo3::prelude::*;

use crate::errors::to_py_err;
use crate::types::{PyProfileId, PySystemConfig};

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
        let inner = neurohid_storage::DataPaths::new(root.map(std::path::PathBuf::from))
            .map_err(to_py_err)?;
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

    /// Save a system configuration (async).
    fn save<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        config: &PySystemConfig,
    ) -> PyResult<Bound<'py, PyAny>> {
        let paths = slf.borrow().paths.clone();
        let cfg = config.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let store = neurohid_storage::ConfigStore::new(paths);
            store.save(&cfg).await.map_err(to_py_err)?;
            Ok(())
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
    /// List all profiles (async). Returns a list of dicts.
    fn list_profiles<'py>(slf: &Bound<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let store = slf.borrow().inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let profiles = store.list_profiles().await.map_err(to_py_err)?;
            Python::try_attach(|py| {
                let list = pyo3::types::PyList::empty(py);
                for meta in &profiles {
                    let dict = pythonize::pythonize(py, meta).map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!("serialize: {e}"))
                    })?;
                    list.append(dict)?;
                }
                Ok(list.unbind())
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Python interpreter not available")
            })?
        })
    }

    /// Create a new profile (async). Returns its metadata as a dict.
    fn create_profile<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        name: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let store = slf.borrow().inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let meta = store.create_profile(name).await.map_err(to_py_err)?;
            Python::try_attach(|py| {
                pythonize::pythonize(py, &meta)
                    .map(|v| v.unbind())
                    .map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!("serialize: {e}"))
                    })
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Python interpreter not available")
            })?
        })
    }

    /// Get metadata for a profile (async). Returns a dict.
    fn get_metadata<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        profile_id: &PyProfileId,
    ) -> PyResult<Bound<'py, PyAny>> {
        let store = slf.borrow().inner.clone();
        let id = profile_id.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let meta = store.get_metadata(&id).await.map_err(to_py_err)?;
            Python::try_attach(|py| {
                pythonize::pythonize(py, &meta)
                    .map(|v| v.unbind())
                    .map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!("serialize: {e}"))
                    })
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Python interpreter not available")
            })?
        })
    }

    /// Delete a profile (async).
    fn delete_profile<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        profile_id: &PyProfileId,
    ) -> PyResult<Bound<'py, PyAny>> {
        let store = slf.borrow().inner.clone();
        let id = profile_id.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            store.delete_profile(&id).await.map_err(to_py_err)?;
            Ok(())
        })
    }

    /// Check if a profile exists (async).
    fn profile_exists<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        profile_id: &PyProfileId,
    ) -> PyResult<Bound<'py, PyAny>> {
        let store = slf.borrow().inner.clone();
        let id = profile_id.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            Ok(store.profile_exists(&id).await)
        })
    }

    /// Export a full profile (async). Returns a dict with metadata + model data.
    fn export_profile<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        profile_id: &PyProfileId,
    ) -> PyResult<Bound<'py, PyAny>> {
        let store = slf.borrow().inner.clone();
        let id = profile_id.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let data = store.export_profile(&id).await.map_err(to_py_err)?;
            Python::try_attach(|py| {
                pythonize::pythonize(py, &data)
                    .map(|v| v.unbind())
                    .map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!("serialize: {e}"))
                    })
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Python interpreter not available")
            })?
        })
    }

    fn __repr__(&self) -> String {
        format!("ProfileStore(root='{}')", self.inner.data_root().display())
    }
}

/// Secure storage for encryption/decryption.
#[pyclass(name = "SecureStorage", skip_from_py_object)]
#[derive(Clone)]
pub struct PySecureStorage {
    inner: neurohid_storage::SecureStorage,
}

#[pymethods]
impl PySecureStorage {
    /// Create a new SecureStorage instance.
    #[new]
    fn new() -> PyResult<Self> {
        let inner = neurohid_storage::SecureStorage::new().map_err(to_py_err)?;
        Ok(Self { inner })
    }

    /// Ensure the master encryption key exists (async).
    fn ensure_master_key<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let secure = slf.borrow().inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            secure.ensure_master_key().await.map_err(to_py_err)?;
            Ok(())
        })
    }

    /// Encrypt data (sync). Returns bytes.
    fn encrypt(&self, data: &[u8]) -> PyResult<Vec<u8>> {
        self.inner.encrypt(data).map_err(to_py_err)
    }

    /// Decrypt data (sync). Returns bytes.
    fn decrypt(&self, data: &[u8]) -> PyResult<Vec<u8>> {
        self.inner.decrypt(data).map_err(to_py_err)
    }

    /// Write encrypted data to a file (async).
    fn write_encrypted<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        path: String,
        data: Vec<u8>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let secure = slf.borrow().inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            secure
                .write_encrypted(std::path::Path::new(&path), &data)
                .await
                .map_err(to_py_err)?;
            Ok(())
        })
    }

    /// Read and decrypt data from a file (async).
    fn read_encrypted<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        path: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let secure = slf.borrow().inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let data = secure
                .read_encrypted(std::path::Path::new(&path))
                .await
                .map_err(to_py_err)?;
            Ok(data)
        })
    }

    fn __repr__(&self) -> String {
        "SecureStorage()".to_string()
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
    m.add_class::<PySecureStorage>()?;
    m.add_function(pyo3::wrap_pyfunction!(initialize_storage, m)?)?;
    Ok(())
}
