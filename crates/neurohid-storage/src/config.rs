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
