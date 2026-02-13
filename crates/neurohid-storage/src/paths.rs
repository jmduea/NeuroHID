//! # Data Paths
//!
//! Manages the directory structure for NeuroHID data storage.

use std::path::{Path, PathBuf};
use tokio::fs;

use crate::ProfileId;
use neurohid_types::error::{Result, StorageError};

/// Manages paths to all data storage locations.
#[derive(Debug, Clone)]
pub struct DataPaths {
    /// Root data directory
    root: PathBuf,
}

impl DataPaths {
    /// Creates a new DataPaths instance.
    ///
    /// If `root` is None, uses the platform default location.
    pub fn new(root: Option<PathBuf>) -> Result<Self> {
        let root =
            root.or_else(crate::default_data_dir)
                .ok_or_else(|| StorageError::ReadError {
                    path: "config directory".to_string(),
                    reason: "Could not determine config directory".to_string(),
                })?;

        Ok(Self { root })
    }

    /// Returns the root data directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the path to the main configuration file.
    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    /// Returns the profiles directory.
    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    /// Returns the directory for a specific profile.
    pub fn profile_dir(&self, profile_id: &ProfileId) -> PathBuf {
        self.profiles_dir().join(&profile_id.0)
    }

    /// Returns the metadata file for a profile.
    pub fn profile_metadata(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id).join("metadata.json")
    }

    /// Returns the calibration data file for a profile.
    pub fn profile_calibration(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id).join("calibration.enc")
    }

    /// Returns the ErrP model file for a profile.
    pub fn profile_errp_model(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id).join("errp_model.enc")
    }

    /// Returns the decoder model file for a profile.
    pub fn profile_decoder_model(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id).join("decoder_model.enc")
    }

    /// Returns the ONNX model file for runtime inference.
    pub fn profile_decoder_model_onnx(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id).join("decoder_model.onnx.enc")
    }

    /// Returns the model manifest file for runtime inference.
    pub fn profile_decoder_manifest(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id)
            .join("decoder_model_manifest.json")
    }

    /// Returns the candidate ONNX model file for guarded activation.
    pub fn profile_decoder_candidate_model_onnx(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id)
            .join("decoder_candidate_model.onnx.enc")
    }

    /// Returns the candidate manifest file for guarded activation.
    pub fn profile_decoder_candidate_manifest(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id)
            .join("decoder_candidate_manifest.json")
    }

    /// Returns the candidate metrics file for guarded activation.
    pub fn profile_decoder_candidate_metrics(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id)
            .join("decoder_candidate_metrics.json")
    }

    /// Returns the per-profile sessions directory.
    pub fn profile_sessions_dir(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_dir(profile_id).join("sessions")
    }

    /// Returns the encrypted training-session log file for a profile/session id.
    pub fn profile_session_log(&self, profile_id: &ProfileId, session_id: &str) -> PathBuf {
        self.profile_sessions_dir(profile_id)
            .join(format!("session_{session_id}.json.enc"))
    }

    /// Returns the logs directory.
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Creates all necessary directories.
    pub async fn ensure_directories(&self) -> Result<()> {
        // Create root directory
        fs::create_dir_all(&self.root)
            .await
            .map_err(|e| StorageError::WriteError {
                path: self.root.display().to_string(),
                reason: e.to_string(),
            })?;

        // Create subdirectories
        fs::create_dir_all(self.profiles_dir())
            .await
            .map_err(|e| StorageError::WriteError {
                path: self.profiles_dir().display().to_string(),
                reason: e.to_string(),
            })?;

        fs::create_dir_all(self.logs_dir())
            .await
            .map_err(|e| StorageError::WriteError {
                path: self.logs_dir().display().to_string(),
                reason: e.to_string(),
            })?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            std::fs::set_permissions(&self.root, perms).map_err(|e| StorageError::WriteError {
                path: self.root.display().to_string(),
                reason: format!("Failed to set permissions: {}", e),
            })?;
        }

        Ok(())
    }

    /// Lists all profile IDs.
    pub async fn list_profiles(&self) -> Result<Vec<ProfileId>> {
        let profiles_dir = self.profiles_dir();

        if !profiles_dir.exists() {
            return Ok(Vec::new());
        }

        let mut profiles = Vec::new();
        let mut entries =
            fs::read_dir(&profiles_dir)
                .await
                .map_err(|e| StorageError::ReadError {
                    path: profiles_dir.display().to_string(),
                    reason: e.to_string(),
                })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| StorageError::ReadError {
                path: profiles_dir.display().to_string(),
                reason: e.to_string(),
            })?
        {
            if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(name) = entry.file_name().to_str() {
                    profiles.push(ProfileId::new(name));
                }
            }
        }

        Ok(profiles)
    }
}
