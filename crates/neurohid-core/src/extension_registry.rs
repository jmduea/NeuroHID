//! # Extension registry and discovery
//!
//! Scans configured directory paths for extension manifests, enforces unique
//! names (fail on duplicate), and exposes lists per pipeline slot for Hub/CLI.
//! Loading of extensions (dylib/subprocess) is handled in a later plan.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use neurohid_types::{
    error::{ExtensionError, Result},
    outlet::{ExtensionKind, ExtensionManifest},
};

/// Default manifest filename(s) we look for in each extension directory.
const MANIFEST_FILENAMES: &[&str] = &["manifest.json", "neurohid.manifest.json"];

/// A discovered extension (name + path to its manifest directory).
#[derive(Debug, Clone)]
pub struct ExtensionEntry {
    pub name: String,
    pub path: PathBuf,
}

/// Registry of discovered extensions. Build with path list, then call `scan()`.
#[derive(Debug, Default)]
pub struct ExtensionRegistry {
    paths: Vec<PathBuf>,
    /// name -> (kind, path); after scan, names are unique.
    by_name: HashMap<String, (ExtensionKind, PathBuf)>,
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
                        .insert(manifest.name.clone(), (manifest.kind, path));
                }
            }
        }

        Ok(())
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
            .filter(|(_, (k, _))| *k == kind)
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
