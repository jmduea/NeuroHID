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
            if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false)
                && let Some(name) = entry.file_name().to_str()
            {
                profiles.push(ProfileId::new(name));
            }
        }

        Ok(profiles)
    }
}

#[cfg(test)]
mod tests {
    use super::DataPaths;
    use crate::ProfileId;

    #[test]
    fn new_with_explicit_root() {
        let root = std::env::temp_dir().join("neurohid_paths_test_explicit");
        let paths = DataPaths::new(Some(root.clone())).unwrap();
        assert_eq!(paths.root(), root);
    }

    #[test]
    fn config_file_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root.clone())).unwrap();
        assert_eq!(paths.config_file(), root.join("config.toml"));
    }

    #[test]
    fn profiles_dir_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root.clone())).unwrap();
        assert_eq!(paths.profiles_dir(), root.join("profiles"));
    }

    #[test]
    fn profile_dir_includes_profile_id() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root.clone())).unwrap();
        let id = ProfileId::new("alice");
        assert_eq!(paths.profile_dir(&id), root.join("profiles").join("alice"));
    }

    #[test]
    fn profile_metadata_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root)).unwrap();
        let id = ProfileId::new("test_user");
        let meta = paths.profile_metadata(&id);
        assert!(meta.ends_with("metadata.json"));
        assert!(meta.to_str().unwrap().contains("test_user"));
    }

    #[test]
    fn profile_calibration_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root)).unwrap();
        let id = ProfileId::new("u1");
        assert!(paths.profile_calibration(&id).ends_with("calibration.enc"));
    }

    #[test]
    fn profile_errp_model_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root)).unwrap();
        let id = ProfileId::new("u1");
        assert!(paths.profile_errp_model(&id).ends_with("errp_model.enc"));
    }

    #[test]
    fn profile_decoder_model_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root)).unwrap();
        let id = ProfileId::new("u1");
        assert!(
            paths
                .profile_decoder_model(&id)
                .ends_with("decoder_model.enc")
        );
    }

    #[test]
    fn profile_decoder_model_onnx_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root)).unwrap();
        let id = ProfileId::new("u1");
        assert!(
            paths
                .profile_decoder_model_onnx(&id)
                .ends_with("decoder_model.onnx.enc")
        );
    }

    #[test]
    fn profile_decoder_manifest_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root)).unwrap();
        let id = ProfileId::new("u1");
        assert!(
            paths
                .profile_decoder_manifest(&id)
                .ends_with("decoder_model_manifest.json")
        );
    }

    #[test]
    fn profile_candidate_paths() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root)).unwrap();
        let id = ProfileId::new("u1");
        assert!(
            paths
                .profile_decoder_candidate_model_onnx(&id)
                .ends_with("decoder_candidate_model.onnx.enc")
        );
        assert!(
            paths
                .profile_decoder_candidate_manifest(&id)
                .ends_with("decoder_candidate_manifest.json")
        );
        assert!(
            paths
                .profile_decoder_candidate_metrics(&id)
                .ends_with("decoder_candidate_metrics.json")
        );
    }

    #[test]
    fn profile_session_log_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root)).unwrap();
        let id = ProfileId::new("u1");
        let log_path = paths.profile_session_log(&id, "12345");
        assert!(log_path.ends_with("session_12345.json.enc"));
        assert!(log_path.to_str().unwrap().contains("sessions"));
    }

    #[test]
    fn logs_dir_path() {
        let root = std::path::PathBuf::from("/tmp/neurohid_test");
        let paths = DataPaths::new(Some(root.clone())).unwrap();
        assert_eq!(paths.logs_dir(), root.join("logs"));
    }

    #[tokio::test]
    async fn ensure_directories_creates_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(Some(tmp.path().to_path_buf())).unwrap();

        paths.ensure_directories().await.unwrap();

        assert!(tmp.path().exists());
        assert!(paths.profiles_dir().exists());
        assert!(paths.logs_dir().exists());
    }

    #[tokio::test]
    async fn ensure_directories_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(Some(tmp.path().to_path_buf())).unwrap();

        paths.ensure_directories().await.unwrap();
        paths.ensure_directories().await.unwrap();

        assert!(paths.profiles_dir().exists());
        assert!(paths.logs_dir().exists());
    }

    #[tokio::test]
    async fn list_profiles_empty_when_no_profiles() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(Some(tmp.path().to_path_buf())).unwrap();
        paths.ensure_directories().await.unwrap();

        let profiles = paths.list_profiles().await.unwrap();
        assert!(profiles.is_empty());
    }

    #[tokio::test]
    async fn list_profiles_finds_subdirectories() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(Some(tmp.path().to_path_buf())).unwrap();
        paths.ensure_directories().await.unwrap();

        // Create some profile directories
        std::fs::create_dir(paths.profiles_dir().join("alice")).unwrap();
        std::fs::create_dir(paths.profiles_dir().join("bob")).unwrap();

        // Create a file (should be ignored)
        std::fs::write(paths.profiles_dir().join("not_a_profile.txt"), "hi").unwrap();

        let mut profiles = paths.list_profiles().await.unwrap();
        profiles.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0], ProfileId::new("alice"));
        assert_eq!(profiles[1], ProfileId::new("bob"));
    }

    #[tokio::test]
    async fn list_profiles_returns_empty_when_dir_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // Don't create dirs — profiles dir doesn't exist
        let paths = DataPaths::new(Some(tmp.path().to_path_buf())).unwrap();
        let profiles = paths.list_profiles().await.unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn all_profile_paths_share_same_hierarchy() {
        let root = std::path::PathBuf::from("/data/neurohid");
        let paths = DataPaths::new(Some(root.clone())).unwrap();
        let id = ProfileId::new("work");

        let profile_dir = paths.profile_dir(&id);
        assert!(paths.profile_metadata(&id).starts_with(&profile_dir));
        assert!(paths.profile_calibration(&id).starts_with(&profile_dir));
        assert!(paths.profile_errp_model(&id).starts_with(&profile_dir));
        assert!(paths.profile_decoder_model(&id).starts_with(&profile_dir));
        assert!(
            paths
                .profile_decoder_model_onnx(&id)
                .starts_with(&profile_dir)
        );
        assert!(
            paths
                .profile_decoder_manifest(&id)
                .starts_with(&profile_dir)
        );
        assert!(paths.profile_sessions_dir(&id).starts_with(&profile_dir));
    }
}
