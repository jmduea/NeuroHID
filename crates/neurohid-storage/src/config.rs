//! # Configuration Storage
//!
//! Manages the system configuration file.

use tokio::fs;

use crate::DataPaths;
use neurohid_types::{
    config::SystemConfig,
    error::{Result, StorageError},
};

/// Manages the system configuration file.
pub struct ConfigStore {
    paths: DataPaths,
}

impl ConfigStore {
    /// Creates a new ConfigStore.
    pub fn new(paths: DataPaths) -> Self {
        Self { paths }
    }

    /// Loads the system configuration.
    ///
    /// If no configuration file exists, returns the default configuration.
    pub async fn load(&self) -> Result<SystemConfig> {
        let path = self.paths.config_file();

        if !path.exists() {
            return Ok(SystemConfig::default());
        }

        let contents = fs::read_to_string(&path)
            .await
            .map_err(|e| StorageError::ReadError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

        let config: SystemConfig = toml::from_str(&contents)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;

        Ok(config)
    }

    /// Saves the system configuration.
    pub async fn save(&self, config: &SystemConfig) -> Result<()> {
        let path = self.paths.config_file();

        let contents = toml::to_string_pretty(config)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;

        fs::write(&path, contents)
            .await
            .map_err(|e| StorageError::WriteError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

        Ok(())
    }

    /// Updates specific fields in the configuration.
    ///
    /// Loads the current config, applies the update function, and saves.
    pub async fn update<F>(&self, f: F) -> Result<SystemConfig>
    where
        F: FnOnce(&mut SystemConfig),
    {
        let mut config = self.load().await?;
        f(&mut config);
        self.save(&config).await?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigStore;
    use crate::DataPaths;
    use neurohid_types::config::SystemConfig;

    fn make_store(root: std::path::PathBuf) -> ConfigStore {
        let paths = DataPaths::new(Some(root)).unwrap();
        ConfigStore::new(paths)
    }

    #[tokio::test]
    async fn load_returns_default_when_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path().to_path_buf());

        let config = store.load().await.unwrap();
        // Verify it matches Default
        assert_eq!(
            config.format_version,
            neurohid_types::config::CURRENT_CONFIG_FORMAT_VERSION
        );
        assert_eq!(
            config.signal.notch_filter_hz,
            SystemConfig::default().signal.notch_filter_hz
        );
        assert_eq!(
            config.service.auto_start,
            SystemConfig::default().service.auto_start
        );
    }

    #[tokio::test]
    async fn save_then_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path().to_path_buf());

        let mut config = SystemConfig::default();
        config.signal.notch_filter_hz = 50.0;
        config.service.log_level = "debug".to_string();

        store.save(&config).await.unwrap();
        let loaded = store.load().await.unwrap();

        assert_eq!(
            loaded.format_version,
            neurohid_types::config::CURRENT_CONFIG_FORMAT_VERSION,
            "roundtrip must persist format_version"
        );
        assert_eq!(loaded.signal.notch_filter_hz, 50.0);
        assert_eq!(loaded.service.log_level, "debug");
    }

    #[tokio::test]
    async fn save_creates_toml_file() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path().to_path_buf());

        store.save(&SystemConfig::default()).await.unwrap();

        let config_path = tmp.path().join("config.toml");
        assert!(config_path.exists());

        let contents = std::fs::read_to_string(&config_path).unwrap();
        assert!(
            contents.contains("[signal]"),
            "TOML should contain signal section"
        );
        assert!(
            contents.contains("[service]"),
            "TOML should contain service section"
        );
    }

    #[tokio::test]
    async fn update_modifies_and_persists() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path().to_path_buf());

        // First save a default config
        store.save(&SystemConfig::default()).await.unwrap();

        // Update a field
        let updated = store
            .update(|cfg| {
                cfg.signal.bandpass_low_hz = 1.0;
            })
            .await
            .unwrap();

        assert_eq!(updated.signal.bandpass_low_hz, 1.0);

        // Re-load and verify persistence
        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.signal.bandpass_low_hz, 1.0);
    }

    #[tokio::test]
    async fn update_on_missing_file_creates_from_default() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path().to_path_buf());

        let result = store
            .update(|cfg| {
                cfg.signal.notch_filter_hz = 50.0;
            })
            .await
            .unwrap();

        assert_eq!(result.signal.notch_filter_hz, 50.0);
        // Other fields should be default
        assert_eq!(
            result.signal.bandpass_high_hz,
            SystemConfig::default().signal.bandpass_high_hz
        );
    }

    #[tokio::test]
    async fn load_legacy_toml_without_format_version_deserializes_as_version_1() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path().to_path_buf());
        let mut config = SystemConfig::default();
        config.signal.notch_filter_hz = 50.0;
        let with_version = toml::to_string_pretty(&config).unwrap();
        // Remove the format_version line to simulate legacy file
        let legacy: String = with_version
            .lines()
            .filter(|line| !line.starts_with("format_version"))
            .collect::<Vec<_>>()
            .join("\n");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, legacy).unwrap();

        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.format_version, 1, "legacy file without format_version should default to 1");
        assert_eq!(loaded.signal.notch_filter_hz, 50.0);
    }

    #[tokio::test]
    async fn multiple_saves_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path().to_path_buf());

        let mut config = SystemConfig::default();
        config.signal.notch_filter_hz = 50.0;
        store.save(&config).await.unwrap();

        config.signal.notch_filter_hz = 60.0;
        store.save(&config).await.unwrap();

        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.signal.notch_filter_hz, 60.0);
    }
}
