//! # Extension registry and discovery
//!
//! Scans configured directory paths for extension manifests, enforces unique
//! names (fail on duplicate), and exposes lists per pipeline slot for Hub/CLI.
//! Loads in-process device/outlet/signal/decoder extensions via libloading (dylib).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::sync::broadcast;
use neurohid_device::{Device, DeviceProvider};
use neurohid_types::{
    config::{DecoderConfig, OutletConfig, SignalConfig},
    device::{ConnectionSettings, DeviceId, DeviceInfo},
    error::{ExtensionError, Result},
    outlet::{ExtensionKind, ExtensionManifest, Outlet, OutletChannels},
    DecoderChannels, DecoderRunner, SignalChannels, SignalPreprocessor,
};

/// Default manifest filename(s) we look for in each extension directory.
const MANIFEST_FILENAMES: &[&str] = &["manifest.json", "neurohid.manifest.json"];

/// Symbol name exported by device extension cdylibs (same toolchain required; see docs).
const DEVICE_PROVIDER_SYMBOL: &[u8] = b"neurohid_device_provider_create";

/// Symbol name exported by outlet extension cdylibs.
const OUTLET_SYMBOL: &[u8] = b"neurohid_outlet_create";
/// Symbol name exported by signal preprocessing extension cdylibs.
const SIGNAL_PREPROCESSOR_SYMBOL: &[u8] = b"neurohid_signal_preprocessor_create";
/// Symbol name exported by decoder extension cdylibs.
const DECODER_SYMBOL: &[u8] = b"neurohid_decoder_create";

/// Default library filename for device extensions when manifest does not specify one.
fn default_device_library_name() -> String {
    format!("libneurohid_device{}", std::env::consts::DLL_EXTENSION)
}

/// Default library filename for outlet extensions when manifest does not specify one.
fn default_outlet_library_name() -> String {
    format!("libneurohid_outlet{}", std::env::consts::DLL_EXTENSION)
}

fn default_signal_preprocessor_library_name() -> String {
    format!("libneurohid_signal{}", std::env::consts::DLL_EXTENSION)
}

fn default_decoder_library_name() -> String {
    format!("libneurohid_decoder{}", std::env::consts::DLL_EXTENSION)
}

/// A discovered extension (name + path to its manifest directory).
#[derive(Debug, Clone)]
pub struct ExtensionEntry {
    pub name: String,
    pub path: PathBuf,
}

/// Wrapper that keeps the loaded library alive while the device provider is in use.
/// In-process plugins must be built with the same Rust toolchain as the host (ABI).
pub struct LoadedDeviceProvider {
    _lib: libloading::Library,
    provider: Box<dyn DeviceProvider>,
}

#[async_trait]
impl DeviceProvider for LoadedDeviceProvider {
    fn device_type(&self) -> neurohid_types::device::DeviceType {
        self.provider.device_type()
    }

    async fn is_available(&self) -> bool {
        self.provider.is_available().await
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>> {
        self.provider.discover().await
    }

    async fn connect(
        &self,
        device_id: &DeviceId,
        settings: Option<ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        self.provider.connect(device_id, settings).await
    }
}

/// Wrapper that keeps the loaded library alive while the outlet is in use.
pub struct LoadedOutlet {
    _lib: libloading::Library,
    outlet: Box<dyn Outlet>,
}

#[async_trait]
impl Outlet for LoadedOutlet {
    async fn run(
        self: Box<Self>,
        shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        let LoadedOutlet { _lib: _, outlet } = *self;
        outlet.run(shutdown).await
    }
}

/// Wrapper that keeps the loaded library alive while the signal preprocessor is in use.
pub struct LoadedSignalPreprocessor {
    _lib: libloading::Library,
    runner: Box<dyn SignalPreprocessor>,
}

#[async_trait]
impl SignalPreprocessor for LoadedSignalPreprocessor {
    async fn run(
        self: Box<Self>,
        shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        let LoadedSignalPreprocessor { _lib: _, runner } = *self;
        runner.run(shutdown).await
    }
}

/// Wrapper that keeps the loaded library alive while the decoder is in use.
pub struct LoadedDecoderRunner {
    _lib: libloading::Library,
    runner: Box<dyn DecoderRunner>,
}

#[async_trait]
impl DecoderRunner for LoadedDecoderRunner {
    async fn run(
        self: Box<Self>,
        shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        let LoadedDecoderRunner { _lib: _, runner } = *self;
        runner.run(shutdown).await
    }
}

/// Registry of discovered extensions. Build with path list, then call `scan()`.
#[derive(Debug, Default)]
pub struct ExtensionRegistry {
    paths: Vec<PathBuf>,
    /// name -> (manifest, path); after scan, names are unique.
    by_name: HashMap<String, (ExtensionManifest, PathBuf)>,
}

impl ExtensionRegistry {
    /// Create a registry that will scan the given directory paths.
    /// Use [`default_extension_paths()`] for the default platform path.
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            by_name: HashMap::new(),
        }
    }

    /// Scan all configured paths for manifest files, parse manifests, and enforce unique names.
    /// Call this at startup and on explicit refresh (Hub/CLI). Returns an error if any name
    /// appears in more than one manifest.
    pub fn scan(&mut self) -> Result<()> {
        self.by_name.clear();
        let mut seen_names: HashMap<String, PathBuf> = HashMap::new();

        for base in &self.paths {
            let base = match base.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !base.is_dir() {
                continue;
            }
            let entries = match std::fs::read_dir(&base) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if let Some(manifest) = Self::read_manifest_in_dir(&path) {
                    if seen_names.contains_key(&manifest.name) {
                        return Err(ExtensionError::DuplicateName {
                            name: manifest.name.clone(),
                        }
                        .into());
                    }
                    seen_names
                        .insert(manifest.name.clone(), path.clone());
                    self.by_name
                        .insert(manifest.name.clone(), (manifest, path));
                }
            }
        }

        Ok(())
    }

    /// Load a device provider extension by name. Returns a provider that holds the
    /// library guard so the dylib is not unloaded. Fails with a clear error if the
    /// name is unknown, not a device extension, or the library/symbol fails to load.
    ///
    /// In-process plugins must be built with the same Rust toolchain as the host.
    pub fn load_device_provider(&self, name: &str) -> Result<LoadedDeviceProvider> {
        let (manifest, dir) = self
            .by_name
            .get(name)
            .ok_or_else(|| ExtensionError::NotFound {
                name: name.to_string(),
            })?;
        if manifest.kind != ExtensionKind::Device {
            return Err(ExtensionError::LoadError {
                name: name.to_string(),
                reason: format!("extension '{}' is not a device extension (kind: {:?})", name, manifest.kind),
            }.into());
        }
        let lib_name: String = manifest
            .library
            .clone()
            .unwrap_or_else(default_device_library_name);
        let path = dir.join(&lib_name);
        // SAFETY: We load a cdylib that is built with the same Rust toolchain; the plugin
        // contract documents that in-process extensions must match the host toolchain (ABI).
        let lib = unsafe { libloading::Library::new(&path) }.map_err(|e| {
            ExtensionError::LoadError {
                name: name.to_string(),
                reason: format!("failed to load library '{}': {}", path.display(), e),
            }
        })?;
        type CreateFn = unsafe extern "Rust" fn() -> Result<Box<dyn DeviceProvider>>;
        // SAFETY: Symbol is from a library we hold; plugin exports this symbol per contract.
        let create: libloading::Symbol<CreateFn> = unsafe {
            lib.get(DEVICE_PROVIDER_SYMBOL).map_err(|e| {
                ExtensionError::LoadError {
                    name: name.to_string(),
                    reason: format!(
                        "symbol 'neurohid_device_provider_create' not found or incompatible: {}",
                        e
                    ),
                }
            })?
        };
        // SAFETY: Plugin factory is documented to return a valid provider for the same toolchain.
        let provider = unsafe { create() }.map_err(|e| ExtensionError::LoadError {
            name: name.to_string(),
            reason: e.to_string(),
        })?;
        Ok(LoadedDeviceProvider {
            _lib: lib,
            provider,
        })
    }

    /// Load an outlet extension by name. Returns an outlet that holds the library guard.
    /// Fails with a clear error if the name is unknown, not an outlet extension, or load fails.
    pub fn load_outlet(
        &self,
        name: &str,
        config: OutletConfig,
        channels: OutletChannels,
    ) -> Result<LoadedOutlet> {
        let (manifest, dir) = self
            .by_name
            .get(name)
            .ok_or_else(|| ExtensionError::NotFound {
                name: name.to_string(),
            })?;
        if manifest.kind != ExtensionKind::Outlet {
            return Err(ExtensionError::LoadError {
                name: name.to_string(),
                reason: format!(
                    "extension '{}' is not an outlet extension (kind: {:?})",
                    name, manifest.kind
                ),
            }
            .into());
        }
        let lib_name: String = manifest
            .library
            .clone()
            .unwrap_or_else(default_outlet_library_name);
        let path = dir.join(&lib_name);
        // SAFETY: Same-toolchain plugin contract; see device loader.
        let lib = unsafe { libloading::Library::new(&path) }.map_err(|e| {
            ExtensionError::LoadError {
                name: name.to_string(),
                reason: format!("failed to load library '{}': {}", path.display(), e),
            }
        })?;
        type CreateFn =
            unsafe extern "Rust" fn(OutletConfig, OutletChannels) -> Result<Box<dyn Outlet>>;
        // SAFETY: Symbol from loaded library; plugin exports per contract.
        let create: libloading::Symbol<CreateFn> = unsafe {
            lib.get(OUTLET_SYMBOL).map_err(|e| {
                ExtensionError::LoadError {
                    name: name.to_string(),
                    reason: format!(
                        "symbol 'neurohid_outlet_create' not found or incompatible: {}",
                        e
                    ),
                }
            })?
        };
        // SAFETY: Plugin factory returns valid outlet for same toolchain.
        let outlet = unsafe { create(config, channels) }.map_err(|e| {
            ExtensionError::LoadError {
                name: name.to_string(),
                reason: e.to_string(),
            }
        })?;
        Ok(LoadedOutlet {
            _lib: lib,
            outlet,
        })
    }

    /// Load a signal preprocessing extension by name. Returns a runner that holds the library guard.
    pub fn load_signal_preprocessor(
        &self,
        name: &str,
        config: SignalConfig,
        channels: SignalChannels,
    ) -> Result<LoadedSignalPreprocessor> {
        let (manifest, dir) = self
            .by_name
            .get(name)
            .ok_or_else(|| ExtensionError::NotFound {
                name: name.to_string(),
            })?;
        if manifest.kind != ExtensionKind::SignalPreprocessing {
            return Err(ExtensionError::LoadError {
                name: name.to_string(),
                reason: format!(
                    "extension '{}' is not a signal preprocessing extension (kind: {:?})",
                    name, manifest.kind
                ),
            }
            .into());
        }
        let lib_name: String = manifest
            .library
            .clone()
            .unwrap_or_else(default_signal_preprocessor_library_name);
        let path = dir.join(&lib_name);
        let lib = unsafe { libloading::Library::new(&path) }.map_err(|e| {
            ExtensionError::LoadError {
                name: name.to_string(),
                reason: format!("failed to load library '{}': {}", path.display(), e),
            }
        })?;
        type CreateFn =
            unsafe extern "Rust" fn(SignalConfig, SignalChannels) -> Result<Box<dyn SignalPreprocessor>>;
        let create: libloading::Symbol<CreateFn> = unsafe {
            lib.get(SIGNAL_PREPROCESSOR_SYMBOL).map_err(|e| {
                ExtensionError::LoadError {
                    name: name.to_string(),
                    reason: format!(
                        "symbol 'neurohid_signal_preprocessor_create' not found: {}",
                        e
                    ),
                }
            })?
        };
        let runner = unsafe { create(config, channels) }.map_err(|e| {
            ExtensionError::LoadError {
                name: name.to_string(),
                reason: e.to_string(),
            }
        })?;
        Ok(LoadedSignalPreprocessor {
            _lib: lib,
            runner,
        })
    }

    /// Load a decoder extension by name. Returns a runner that holds the library guard.
    pub fn load_decoder(
        &self,
        name: &str,
        config: DecoderConfig,
        channels: DecoderChannels,
    ) -> Result<LoadedDecoderRunner> {
        let (manifest, dir) = self
            .by_name
            .get(name)
            .ok_or_else(|| ExtensionError::NotFound {
                name: name.to_string(),
            })?;
        if manifest.kind != ExtensionKind::Decoder {
            return Err(ExtensionError::LoadError {
                name: name.to_string(),
                reason: format!(
                    "extension '{}' is not a decoder extension (kind: {:?})",
                    name, manifest.kind
                ),
            }
            .into());
        }
        let lib_name: String = manifest
            .library
            .clone()
            .unwrap_or_else(default_decoder_library_name);
        let path = dir.join(&lib_name);
        let lib = unsafe { libloading::Library::new(&path) }.map_err(|e| {
            ExtensionError::LoadError {
                name: name.to_string(),
                reason: format!("failed to load library '{}': {}", path.display(), e),
            }
        })?;
        type CreateFn =
            unsafe extern "Rust" fn(DecoderConfig, DecoderChannels) -> Result<Box<dyn DecoderRunner>>;
        let create: libloading::Symbol<CreateFn> = unsafe {
            lib.get(DECODER_SYMBOL).map_err(|e| {
                ExtensionError::LoadError {
                    name: name.to_string(),
                    reason: format!("symbol 'neurohid_decoder_create' not found: {}", e),
                }
            })?
        };
        let runner = unsafe { create(config, channels) }.map_err(|e| {
            ExtensionError::LoadError {
                name: name.to_string(),
                reason: e.to_string(),
            }
        })?;
        Ok(LoadedDecoderRunner {
            _lib: lib,
            runner,
        })
    }

    fn read_manifest_in_dir(dir: &Path) -> Option<ExtensionManifest> {
        for &filename in MANIFEST_FILENAMES {
            let path = dir.join(filename);
            let contents = std::fs::read_to_string(&path).ok()?;
            let manifest: ExtensionManifest = serde_json::from_str(&contents).ok()?;
            return Some(manifest);
        }
        None
    }

    /// List discovered extensions for the outlet slot.
    pub fn list_outlets(&self) -> Vec<ExtensionEntry> {
        self.list_by_kind(ExtensionKind::Outlet)
    }

    /// List discovered extensions for the device slot.
    pub fn list_devices(&self) -> Vec<ExtensionEntry> {
        self.list_by_kind(ExtensionKind::Device)
    }

    /// List discovered extensions for the signal preprocessing slot.
    pub fn list_signal_preprocessors(&self) -> Vec<ExtensionEntry> {
        self.list_by_kind(ExtensionKind::SignalPreprocessing)
    }

    /// List discovered extensions for the decoder slot.
    pub fn list_decoders(&self) -> Vec<ExtensionEntry> {
        self.list_by_kind(ExtensionKind::Decoder)
    }

    fn list_by_kind(&self, kind: ExtensionKind) -> Vec<ExtensionEntry> {
        self.by_name
            .iter()
            .filter(|(_, (m, _))| m.kind == kind)
            .map(|(name, (_, path))| ExtensionEntry {
                name: name.clone(),
                path: path.clone(),
            })
            .collect()
    }
}

/// Returns the default directory path(s) for extension discovery.
///
/// Uses the same platform config root as storage (`~/.config/neurohid` on Linux,
/// etc.) and appends `extensions`. Override via config or env in application code.
pub fn default_extension_paths() -> Vec<PathBuf> {
    neurohid_storage::default_data_dir()
        .map(|p| vec![p.join("extensions")])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn scan_single_manifest_lists_by_kind() {
        let temp = std::env::temp_dir().join("neurohid_ext_test_1");
        let _ = std::fs::remove_dir_all(&temp);
        let ext_dir = temp.join("my-outlet");
        std::fs::create_dir_all(&ext_dir).unwrap();
        let manifest_path = ext_dir.join("manifest.json");
        let mut f = std::fs::File::create(&manifest_path).unwrap();
        writeln!(
            f,
            r#"{{ "name": "test-outlet", "kind": "outlet" }}"#
        )
        .unwrap();
        drop(f);

        let mut reg = ExtensionRegistry::new(vec![temp.clone()]);
        reg.scan().unwrap();
        let outlets = reg.list_outlets();
        assert_eq!(outlets.len(), 1);
        assert_eq!(outlets[0].name, "test-outlet");
        assert_eq!(outlets[0].path.file_name().unwrap(), "my-outlet");
        assert!(reg.list_decoders().is_empty());
        assert!(reg.list_signal_preprocessors().is_empty());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn duplicate_name_fails_scan() {
        let temp = std::env::temp_dir().join("neurohid_ext_test_2");
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(temp.join("ext1")).unwrap();
        std::fs::create_dir_all(temp.join("ext2")).unwrap();
        for (dir, kind) in [("ext1", "outlet"), ("ext2", "decoder")] {
            let path = temp.join(dir).join("manifest.json");
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, r#"{{ "name": "same-name", "kind": "{}" }}"#, kind).unwrap();
            drop(f);
        }

        let mut reg = ExtensionRegistry::new(vec![temp.clone()]);
        let res = reg.scan();
        assert!(res.is_err());
        let err = res.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Duplicate"));
        assert!(msg.contains("same-name"));

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn list_signal_preprocessors_and_decoders_by_kind() {
        let temp = std::env::temp_dir().join("neurohid_ext_test_3");
        let _ = std::fs::remove_dir_all(&temp);
        for (dir, name, kind) in [
            ("sp1", "sp-a", "signal_preprocessing"),
            ("sp2", "sp-b", "signal_preprocessing"),
            ("dec1", "dec-x", "decoder"),
        ] {
            std::fs::create_dir_all(temp.join(dir)).unwrap();
            let path = temp.join(dir).join("manifest.json");
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(
                f,
                r#"{{ "name": "{}", "kind": "{}" }}"#,
                name, kind
            )
            .unwrap();
            drop(f);
        }

        let mut reg = ExtensionRegistry::new(vec![temp.clone()]);
        reg.scan().unwrap();
        let sp = reg.list_signal_preprocessors();
        let dec = reg.list_decoders();
        assert_eq!(sp.len(), 2);
        assert_eq!(dec.len(), 1);
        let names: Vec<_> = sp.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"sp-a"));
        assert!(names.contains(&"sp-b"));
        assert_eq!(dec[0].name, "dec-x");

        let _ = std::fs::remove_dir_all(&temp);
    }
}
