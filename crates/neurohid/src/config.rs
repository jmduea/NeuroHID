//! # Config load/save API
//!
//! Load and save system configuration from Rust code. The config format is versioned
//! (`format_version` in the file); both **YAML** (`.yaml`/`.yml`) and **TOML** are
//! supported with the same schema. Format is detected from the file path extension.
//!
//! When `config_path` is `None`, the default platform config file is used
//! (e.g. `~/.config/neurohid/config.toml`). When `Some(path)`, that path is used
//! and the format is inferred from the extension.

use std::path::PathBuf;

use neurohid_storage::ConfigStore;
use neurohid_storage::DataPaths;
use neurohid_types::config::SystemConfig;

/// Load system configuration.
///
/// - `config_path: None` — load from the default config file (TOML at platform path).
/// - `config_path: Some(path)` — load from the given path; YAML or TOML by extension.
///
/// Returns the default `SystemConfig` if the file does not exist.
pub async fn load(config_path: Option<PathBuf>) -> neurohid_storage::Result<SystemConfig> {
    let paths = DataPaths::new(neurohid_storage::default_data_dir())?;
    let store = ConfigStore::new(paths);
    match config_path {
        Some(p) => store.load_from_path(&p).await,
        None => store.load().await,
    }
}

/// Save system configuration.
///
/// - `config_path: None` — save to the default config file (TOML).
/// - `config_path: Some(path)` — save to the given path; format by extension (YAML/TOML).
pub async fn save(
    config_path: Option<PathBuf>,
    config: &SystemConfig,
) -> neurohid_storage::Result<()> {
    let paths = DataPaths::new(neurohid_storage::default_data_dir())?;
    let store = ConfigStore::new(paths);
    match config_path {
        Some(p) => store.save_to_path(&p, config).await,
        None => store.save(config).await,
    }
}
