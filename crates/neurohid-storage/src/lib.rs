//! # NeuroHID Secure Storage
//!
//! This crate provides secure storage for user profiles, calibration data, and
//! trained model weights. It integrates with platform-native secret storage
//! (Keychain on macOS, Credential Manager on Windows, Secret Service on Linux)
//! for encryption keys, while storing the bulk data in encrypted local files.
//!
//! ## Security Model
//!
//! Brain signal data is sensitive biometric information. Our storage approach:
//!
//! 1. **Encryption keys in platform keychain**: The master encryption key never
//!    touches the filesystem. It's stored in the OS-provided secure storage.
//!
//! 2. **Data encrypted at rest**: Profile data, calibration recordings, and
//!    model weights are encrypted with AES-256-GCM before writing to disk.
//!
//! 3. **Minimal permissions**: Data files are created with restrictive permissions
//!    (0600 on Unix) to prevent other users from reading them.
//!
//! 4. **No cloud sync**: All data stays local. Users who want backup must
//!    explicitly export (which warns about sensitivity).
//!
//! ## Directory Structure
//!
//! ```text
//! ~/.config/neurohid/           # Linux (XDG_CONFIG_HOME)
//! ~/Library/Application Support/neurohid/  # macOS
//! %APPDATA%\neurohid\           # Windows
//! │
//! ├── config.toml               # Plain text preferences (non-sensitive)
//! ├── profiles/
//! │   ├── default/
//! │   │   ├── metadata.json     # Plain text profile metadata
//! │   │   ├── calibration.enc   # Encrypted calibration data
//! │   │   ├── errp_model.enc    # Encrypted ErrP classifier
//! │   │   └── decoder_model.enc # Encrypted decoder weights
//! │   └── work/
//! │       └── ...
//! └── logs/
//!     └── session_*.enc         # Encrypted session logs (auto-rotate)
//! ```

pub mod config;
pub mod credentials;
pub mod paths;
pub mod profile;
pub mod secure;

use std::path::PathBuf;

pub use config::ConfigStore;
pub use credentials::{get_emotiv_credentials, set_emotiv_credentials};
pub use paths::DataPaths;
pub use profile::{ProfileData, ProfileStore};
pub use secure::SecureStorage;

// Re-export types from neurohid-types
pub use neurohid_types::config::SystemConfig;
pub use neurohid_types::error::{Result, StorageError};
pub use neurohid_types::profile::{CalibrationState, ProfileId, ProfileMetadata};

/// The application identifier used for keychain access.
pub const APP_IDENTIFIER: &str = "com.neurohid.service";

/// The keychain service name.
pub const KEYCHAIN_SERVICE: &str = "neurohid";

/// Returns the default data directory for the current platform.
///
/// This follows platform conventions:
/// - Linux: `$XDG_CONFIG_HOME/neurohid` or `~/.config/neurohid`
/// - macOS: `~/Library/Application Support/neurohid`
/// - Windows: `%APPDATA%\neurohid`
pub fn default_data_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("neurohid"))
}

/// Initializes the storage system, creating directories if needed.
///
/// This should be called once at application startup. It:
/// 1. Creates the data directory structure if it doesn't exist
/// 2. Verifies or creates the master encryption key in the keychain
/// 3. Returns handles for accessing profiles and configuration
///
/// # Errors
///
/// Returns an error if:
/// - The data directory cannot be created
/// - Keychain access fails
/// - File permissions cannot be set appropriately
pub async fn initialize() -> Result<(ProfileStore, ConfigStore)> {
    let paths = DataPaths::new(default_data_dir())?;
    paths.ensure_directories().await?;

    let secure = SecureStorage::new()?;
    secure.ensure_master_key().await?;

    let profile_store = ProfileStore::new(paths.clone(), secure.clone());
    let config_store = ConfigStore::new(paths);

    Ok((profile_store, config_store))
}
