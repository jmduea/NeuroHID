//! # Credential Management
//!
//! Provides get/set access to device API credentials stored in the
//! platform keychain. These are kept separate from the config file
//! because they are sensitive secrets that should never touch the
//! filesystem.

use crate::KEYCHAIN_SERVICE;
use neurohid_types::error::{Result, StorageError};

/// Retrieve the Emotiv Cortex API credentials from the platform keyring.
///
/// Returns `(client_id, client_secret)` if both are stored, or an error
/// if either entry is missing.
pub fn get_emotiv_credentials() -> Result<(String, String)> {
    let client_id = keyring::Entry::new(KEYCHAIN_SERVICE, "emotiv_client_id")
        .and_then(|entry| entry.get_password())
        .map_err(|e| StorageError::KeyringError(format!(
            "Failed to read emotiv_client_id: {}", e
        )))?;

    let client_secret = keyring::Entry::new(KEYCHAIN_SERVICE, "emotiv_client_secret")
        .and_then(|entry| entry.get_password())
        .map_err(|e| StorageError::KeyringError(format!(
            "Failed to read emotiv_client_secret: {}", e
        )))?;

    Ok((client_id, client_secret))
}

/// Store the Emotiv Cortex API credentials in the platform keyring.
///
/// Both values must be non-empty. This overwrites any previously
/// stored credentials.
pub fn set_emotiv_credentials(client_id: &str, client_secret: &str) -> Result<()> {
    if client_id.is_empty() || client_secret.is_empty() {
        return Err(StorageError::KeyringError(
            "Client ID and secret must not be empty".into(),
        ).into());
    }

    keyring::Entry::new(KEYCHAIN_SERVICE, "emotiv_client_id")
        .and_then(|entry| entry.set_password(client_id))
        .map_err(|e| StorageError::KeyringError(format!(
            "Failed to store emotiv_client_id: {}", e
        )))?;

    keyring::Entry::new(KEYCHAIN_SERVICE, "emotiv_client_secret")
        .and_then(|entry| entry.set_password(client_secret))
        .map_err(|e| StorageError::KeyringError(format!(
            "Failed to store emotiv_client_secret: {}", e
        )))?;

    tracing::info!("Emotiv credentials saved to keyring");

    Ok(())
}
