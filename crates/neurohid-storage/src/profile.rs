//! # Profile Storage
//!
//! Manages user profiles including metadata, calibration data, and models.

use std::path::Path;

use tokio::fs;

use crate::{DataPaths, SecureStorage};
use neurohid_types::{
    error::{Result, StorageError},
    learning::{CandidateModelMetrics, TrainingEpisode, TrainingSessionLog},
    model::ModelManifest,
    profile::{CalibrationState, ProfileId, ProfileMetadata},
};

/// Manages storage and retrieval of user profiles.
#[derive(Clone)]
pub struct ProfileStore {
    paths: DataPaths,
    secure: SecureStorage,
}

const MIN_CANDIDATE_MODEL_BYTES: usize = 1_024;
const MAX_CANDIDATE_MODEL_BYTES: usize = 128 * 1024 * 1024;
const MAX_CANDIDATE_FUTURE_SKEW_US: i64 = 5 * 60 * 1_000_000;
const MAX_CANDIDATE_GENERATION_DELTA_US: i64 = 24 * 60 * 60 * 1_000_000;

impl ProfileStore {
    /// Creates a new ProfileStore.
    pub fn new(paths: DataPaths, secure: SecureStorage) -> Self {
        Self { paths, secure }
    }

    /// Root directory used by this profile store.
    pub fn data_root(&self) -> &Path {
        self.paths.root()
    }

    /// Lists all available profiles.
    pub async fn list_profiles(&self) -> Result<Vec<ProfileMetadata>> {
        let profile_ids = self.paths.list_profiles().await?;
        let mut profiles = Vec::new();

        for id in profile_ids {
            match self.get_metadata(&id).await {
                Ok(metadata) => profiles.push(metadata),
                Err(_) => {
                    // Skip profiles with corrupted metadata
                    tracing::warn!("Skipping profile with invalid metadata: {}", id);
                }
            }
        }

        Ok(profiles)
    }

    /// Creates a new profile.
    pub async fn create_profile(&self, name: String) -> Result<ProfileMetadata> {
        let id = ProfileId::generate();
        let metadata = ProfileMetadata::new(id.clone(), name);

        // Create profile directory
        let profile_dir = self.paths.profile_dir(&id);
        fs::create_dir_all(&profile_dir)
            .await
            .map_err(|e| StorageError::WriteError {
                path: profile_dir.display().to_string(),
                reason: e.to_string(),
            })?;

        // Write metadata
        self.save_metadata(&metadata).await?;

        Ok(metadata)
    }

    /// Gets profile metadata.
    pub async fn get_metadata(&self, id: &ProfileId) -> Result<ProfileMetadata> {
        let path = self.paths.profile_metadata(id);

        let contents = fs::read_to_string(&path)
            .await
            .map_err(|e| StorageError::ReadError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

        let metadata: ProfileMetadata = serde_json::from_str(&contents)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;

        Ok(metadata)
    }

    /// Saves profile metadata.
    pub async fn save_metadata(&self, metadata: &ProfileMetadata) -> Result<()> {
        let path = self.paths.profile_metadata(&metadata.id);

        let contents = serde_json::to_string_pretty(metadata)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;

        fs::write(&path, contents)
            .await
            .map_err(|e| StorageError::WriteError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

        Ok(())
    }

    /// Deletes a profile and all its data.
    pub async fn delete_profile(&self, id: &ProfileId) -> Result<()> {
        let profile_dir = self.paths.profile_dir(id);

        if !profile_dir.exists() {
            return Err(StorageError::ProfileNotFound {
                profile_id: id.to_string(),
            }
            .into());
        }

        fs::remove_dir_all(&profile_dir)
            .await
            .map_err(|e| StorageError::WriteError {
                path: profile_dir.display().to_string(),
                reason: e.to_string(),
            })?;

        Ok(())
    }

    /// Checks if a profile exists.
    pub async fn profile_exists(&self, id: &ProfileId) -> bool {
        self.paths.profile_dir(id).exists()
    }

    /// Saves calibration data (encrypted).
    pub async fn save_calibration(&self, id: &ProfileId, data: &[u8]) -> Result<()> {
        let path = self.paths.profile_calibration(id);
        self.secure.write_encrypted(&path, data).await
    }

    /// Loads calibration data (decrypted).
    pub async fn load_calibration(&self, id: &ProfileId) -> Result<Vec<u8>> {
        let path = self.paths.profile_calibration(id);
        self.secure.read_encrypted(&path).await
    }

    /// Saves the ErrP model (encrypted).
    pub async fn save_errp_model(&self, id: &ProfileId, data: &[u8]) -> Result<()> {
        let path = self.paths.profile_errp_model(id);
        self.secure.write_encrypted(&path, data).await
    }

    /// Loads the ErrP model (decrypted).
    pub async fn load_errp_model(&self, id: &ProfileId) -> Result<Vec<u8>> {
        let path = self.paths.profile_errp_model(id);
        self.secure.read_encrypted(&path).await
    }

    /// Saves the decoder model (encrypted).
    pub async fn save_decoder_model(&self, id: &ProfileId, data: &[u8]) -> Result<()> {
        let path = self.paths.profile_decoder_model(id);
        self.secure.write_encrypted(&path, data).await
    }

    /// Loads the decoder model (decrypted).
    pub async fn load_decoder_model(&self, id: &ProfileId) -> Result<Vec<u8>> {
        let path = self.paths.profile_decoder_model(id);
        self.secure.read_encrypted(&path).await
    }

    /// Saves the decoder ONNX model (encrypted).
    pub async fn save_decoder_model_onnx(&self, id: &ProfileId, data: &[u8]) -> Result<()> {
        let path = self.paths.profile_decoder_model_onnx(id);
        self.secure.write_encrypted(&path, data).await
    }

    /// Loads the decoder ONNX model (decrypted).
    pub async fn load_decoder_model_onnx(&self, id: &ProfileId) -> Result<Vec<u8>> {
        let path = self.paths.profile_decoder_model_onnx(id);
        self.secure.read_encrypted(&path).await
    }

    /// Saves decoder model manifest metadata.
    pub async fn save_decoder_manifest(
        &self,
        id: &ProfileId,
        manifest: &ModelManifest,
    ) -> Result<()> {
        let path = self.paths.profile_decoder_manifest(id);
        let payload = serde_json::to_string_pretty(manifest)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        fs::write(&path, payload)
            .await
            .map_err(|e| StorageError::WriteError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;
        Ok(())
    }

    /// Loads decoder model manifest metadata.
    pub async fn load_decoder_manifest(&self, id: &ProfileId) -> Result<ModelManifest> {
        let path = self.paths.profile_decoder_manifest(id);
        let payload = fs::read_to_string(&path)
            .await
            .map_err(|e| StorageError::ReadError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;
        let manifest: ModelManifest = serde_json::from_str(&payload)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        Ok(manifest)
    }

    /// Saves candidate decoder ONNX model (encrypted).
    pub async fn save_decoder_candidate_model_onnx(
        &self,
        id: &ProfileId,
        data: &[u8],
    ) -> Result<()> {
        let path = self.paths.profile_decoder_candidate_model_onnx(id);
        self.secure.write_encrypted(&path, data).await
    }

    /// Loads candidate decoder ONNX model (decrypted).
    pub async fn load_decoder_candidate_model_onnx(&self, id: &ProfileId) -> Result<Vec<u8>> {
        let path = self.paths.profile_decoder_candidate_model_onnx(id);
        self.secure.read_encrypted(&path).await
    }

    /// Saves candidate decoder manifest metadata.
    pub async fn save_decoder_candidate_manifest(
        &self,
        id: &ProfileId,
        manifest: &ModelManifest,
    ) -> Result<()> {
        let path = self.paths.profile_decoder_candidate_manifest(id);
        let payload = serde_json::to_string_pretty(manifest)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        fs::write(&path, payload)
            .await
            .map_err(|e| StorageError::WriteError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;
        Ok(())
    }

    /// Loads candidate decoder manifest metadata.
    pub async fn load_decoder_candidate_manifest(&self, id: &ProfileId) -> Result<ModelManifest> {
        let path = self.paths.profile_decoder_candidate_manifest(id);
        let payload = fs::read_to_string(&path)
            .await
            .map_err(|e| StorageError::ReadError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;
        let manifest: ModelManifest = serde_json::from_str(&payload)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        Ok(manifest)
    }

    /// Saves candidate decoder evaluation metrics.
    pub async fn save_decoder_candidate_metrics(
        &self,
        id: &ProfileId,
        metrics: &CandidateModelMetrics,
    ) -> Result<()> {
        let path = self.paths.profile_decoder_candidate_metrics(id);
        let payload = serde_json::to_string_pretty(metrics)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        fs::write(&path, payload)
            .await
            .map_err(|e| StorageError::WriteError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;
        Ok(())
    }

    /// Loads candidate decoder evaluation metrics.
    pub async fn load_decoder_candidate_metrics(
        &self,
        id: &ProfileId,
    ) -> Result<CandidateModelMetrics> {
        let path = self.paths.profile_decoder_candidate_metrics(id);
        let payload = fs::read_to_string(&path)
            .await
            .map_err(|e| StorageError::ReadError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;
        let metrics: CandidateModelMetrics = serde_json::from_str(&payload)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        Ok(metrics)
    }

    /// Import plaintext candidate artifacts from a trainer output directory.
    ///
    /// Expected files:
    /// - `decoder_candidate.onnx`
    /// - `decoder_candidate_manifest.json`
    /// - `decoder_candidate_metrics.json`
    pub async fn import_decoder_candidate_from_dir(
        &self,
        id: &ProfileId,
        source_dir: &Path,
    ) -> Result<()> {
        let model_path = source_dir.join("decoder_candidate.onnx");
        let manifest_path = source_dir.join("decoder_candidate_manifest.json");
        let metrics_path = source_dir.join("decoder_candidate_metrics.json");

        let model_bytes = fs::read(&model_path)
            .await
            .map_err(|e| StorageError::ReadError {
                path: model_path.display().to_string(),
                reason: e.to_string(),
            })?;
        let manifest_payload =
            fs::read_to_string(&manifest_path)
                .await
                .map_err(|e| StorageError::ReadError {
                    path: manifest_path.display().to_string(),
                    reason: e.to_string(),
                })?;
        let metrics_payload =
            fs::read_to_string(&metrics_path)
                .await
                .map_err(|e| StorageError::ReadError {
                    path: metrics_path.display().to_string(),
                    reason: e.to_string(),
                })?;

        let manifest: ModelManifest = serde_json::from_str(&manifest_payload)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        manifest
            .validate_runtime_compatibility()
            .map_err(|details| StorageError::DataCorruption {
                location: manifest_path.display().to_string(),
                details,
            })?;

        let metrics: CandidateModelMetrics = serde_json::from_str(&metrics_payload)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        metrics
            .validate()
            .map_err(|details| StorageError::DataCorruption {
                location: metrics_path.display().to_string(),
                details,
            })?;

        if model_bytes.len() < MIN_CANDIDATE_MODEL_BYTES {
            return Err(StorageError::DataCorruption {
                location: model_path.display().to_string(),
                details: format!(
                    "candidate ONNX artifact is too small ({} bytes, minimum {})",
                    model_bytes.len(),
                    MIN_CANDIDATE_MODEL_BYTES
                ),
            }
            .into());
        }
        if model_bytes.len() > MAX_CANDIDATE_MODEL_BYTES {
            return Err(StorageError::DataCorruption {
                location: model_path.display().to_string(),
                details: format!(
                    "candidate ONNX artifact is too large ({} bytes, maximum {})",
                    model_bytes.len(),
                    MAX_CANDIDATE_MODEL_BYTES
                ),
            }
            .into());
        }

        if metrics.generated_at < manifest.trained_at {
            return Err(StorageError::DataCorruption {
                location: metrics_path.display().to_string(),
                details: format!(
                    "metrics.generated_at {} precedes manifest.trained_at {}",
                    metrics.generated_at, manifest.trained_at
                ),
            }
            .into());
        }

        let generation_delta_us = metrics.generated_at.saturating_sub(manifest.trained_at);
        if generation_delta_us > MAX_CANDIDATE_GENERATION_DELTA_US {
            return Err(StorageError::DataCorruption {
                location: metrics_path.display().to_string(),
                details: format!(
                    "candidate metrics generated {} us after training (maximum {})",
                    generation_delta_us, MAX_CANDIDATE_GENERATION_DELTA_US
                ),
            }
            .into());
        }

        let now_us = neurohid_types::now_micros();
        if manifest.trained_at > now_us.saturating_add(MAX_CANDIDATE_FUTURE_SKEW_US) {
            return Err(StorageError::DataCorruption {
                location: manifest_path.display().to_string(),
                details: format!(
                    "manifest.trained_at {} is too far in the future (now {}, max skew {})",
                    manifest.trained_at, now_us, MAX_CANDIDATE_FUTURE_SKEW_US
                ),
            }
            .into());
        }
        if metrics.generated_at > now_us.saturating_add(MAX_CANDIDATE_FUTURE_SKEW_US) {
            return Err(StorageError::DataCorruption {
                location: metrics_path.display().to_string(),
                details: format!(
                    "metrics.generated_at {} is too far in the future (now {}, max skew {})",
                    metrics.generated_at, now_us, MAX_CANDIDATE_FUTURE_SKEW_US
                ),
            }
            .into());
        }

        self.save_decoder_candidate_model_onnx(id, &model_bytes)
            .await?;
        self.save_decoder_candidate_manifest(id, &manifest).await?;
        self.save_decoder_candidate_metrics(id, &metrics).await?;

        Ok(())
    }

    /// Promote candidate decoder artifacts into active runtime artifacts.
    pub async fn promote_decoder_candidate(&self, id: &ProfileId) -> Result<()> {
        let candidate_model = self.load_decoder_candidate_model_onnx(id).await?;
        let candidate_manifest = self.load_decoder_candidate_manifest(id).await?;
        self.save_decoder_model_onnx(id, &candidate_model).await?;
        self.save_decoder_manifest(id, &candidate_manifest).await?;
        Ok(())
    }

    /// Remove candidate decoder artifacts.
    pub async fn clear_decoder_candidate(&self, id: &ProfileId) -> Result<()> {
        for path in [
            self.paths.profile_decoder_candidate_model_onnx(id),
            self.paths.profile_decoder_candidate_manifest(id),
            self.paths.profile_decoder_candidate_metrics(id),
        ] {
            if path.exists() {
                fs::remove_file(&path)
                    .await
                    .map_err(|e| StorageError::WriteError {
                        path: path.display().to_string(),
                        reason: e.to_string(),
                    })?;
            }
        }
        Ok(())
    }

    /// Saves an encrypted training session log.
    pub async fn save_training_session_log(
        &self,
        id: &ProfileId,
        session: &TrainingSessionLog,
    ) -> Result<()> {
        let sessions_dir = self.paths.profile_sessions_dir(id);
        fs::create_dir_all(&sessions_dir)
            .await
            .map_err(|e| StorageError::WriteError {
                path: sessions_dir.display().to_string(),
                reason: e.to_string(),
            })?;

        let path = self.paths.profile_session_log(id, &session.session_id);
        let payload = serde_json::to_vec(session)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        self.secure.write_encrypted(&path, &payload).await
    }

    /// Loads an encrypted training session log.
    pub async fn load_training_session_log(
        &self,
        id: &ProfileId,
        session_id: &str,
    ) -> Result<TrainingSessionLog> {
        let path = self.paths.profile_session_log(id, session_id);
        let payload = self.secure.read_encrypted(&path).await?;
        let log: TrainingSessionLog = serde_json::from_slice(&payload)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        Ok(log)
    }

    /// Appends one episode to a profile/session training log.
    pub async fn append_training_episode(
        &self,
        id: &ProfileId,
        session_id: &str,
        episode: TrainingEpisode,
    ) -> Result<()> {
        let path = self.paths.profile_session_log(id, session_id);
        let mut log = if path.exists() {
            self.load_training_session_log(id, session_id).await?
        } else {
            TrainingSessionLog::new(
                session_id.to_string(),
                id.to_string(),
                neurohid_types::now_micros(),
            )
        };
        log.append_episode(episode);
        self.save_training_session_log(id, &log).await
    }

    /// Lists session-log ids for a profile.
    pub async fn list_training_session_log_ids(&self, id: &ProfileId) -> Result<Vec<String>> {
        let sessions_dir = self.paths.profile_sessions_dir(id);
        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut ids = Vec::new();
        let mut entries =
            fs::read_dir(&sessions_dir)
                .await
                .map_err(|e| StorageError::ReadError {
                    path: sessions_dir.display().to_string(),
                    reason: e.to_string(),
                })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| StorageError::ReadError {
                path: sessions_dir.display().to_string(),
                reason: e.to_string(),
            })?
        {
            if !entry
                .file_type()
                .await
                .map(|t| t.is_file())
                .unwrap_or(false)
            {
                continue;
            }
            let Some(file_name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                continue;
            };
            if let Some(session_id) = parse_session_log_id(&file_name) {
                ids.push(session_id);
            }
        }

        ids.sort_unstable();
        Ok(ids)
    }

    /// Prunes training session logs older than `cutoff_started_at`.
    pub async fn prune_training_session_logs(
        &self,
        id: &ProfileId,
        cutoff_started_at: i64,
    ) -> Result<usize> {
        let mut removed = 0_usize;
        for session_id in self.list_training_session_log_ids(id).await? {
            let Ok(started_at) = session_id.parse::<i64>() else {
                continue;
            };
            if started_at < cutoff_started_at {
                let path = self.paths.profile_session_log(id, &session_id);
                if path.exists() {
                    fs::remove_file(&path)
                        .await
                        .map_err(|e| StorageError::WriteError {
                            path: path.display().to_string(),
                            reason: e.to_string(),
                        })?;
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    /// Exports decrypted training session logs to plaintext JSON files.
    ///
    /// Output file pattern: `session_<session_id>.json`.
    pub async fn export_training_session_logs_to_dir(
        &self,
        id: &ProfileId,
        output_dir: &Path,
    ) -> Result<usize> {
        fs::create_dir_all(output_dir)
            .await
            .map_err(|e| StorageError::WriteError {
                path: output_dir.display().to_string(),
                reason: e.to_string(),
            })?;

        let mut exported = 0_usize;
        for session_id in self.list_training_session_log_ids(id).await? {
            let session_log = self.load_training_session_log(id, &session_id).await?;
            let file_path = output_dir.join(format!("session_{session_id}.json"));
            let payload = serde_json::to_vec_pretty(&session_log)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;
            fs::write(&file_path, payload)
                .await
                .map_err(|e| StorageError::WriteError {
                    path: file_path.display().to_string(),
                    reason: e.to_string(),
                })?;
            exported += 1;
        }

        Ok(exported)
    }

    /// Updates the calibration state for a profile.
    pub async fn update_calibration_state(
        &self,
        id: &ProfileId,
        state: CalibrationState,
    ) -> Result<()> {
        let mut metadata = self.get_metadata(id).await?;
        metadata.calibration_state = state;
        self.save_metadata(&metadata).await
    }
}

fn parse_session_log_id(file_name: &str) -> Option<String> {
    let stem = file_name.strip_prefix("session_")?;
    let stem = stem.strip_suffix(".json.enc")?;
    if stem.is_empty() {
        return None;
    }
    Some(stem.to_string())
}

/// Complete profile data bundle for import/export.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProfileData {
    /// Profile metadata
    pub metadata: ProfileMetadata,
    /// Calibration data (raw bytes, will be encrypted on save)
    pub calibration: Option<Vec<u8>>,
    /// ErrP model weights
    pub errp_model: Option<Vec<u8>>,
    /// Decoder model weights
    pub decoder_model: Option<Vec<u8>>,
    /// Runtime ONNX decoder model
    pub decoder_model_onnx: Option<Vec<u8>>,
    /// Runtime ONNX decoder manifest
    pub decoder_manifest: Option<ModelManifest>,
}

impl ProfileStore {
    /// Exports a complete profile for backup.
    ///
    /// **Warning**: The exported data is not encrypted. Handle with care.
    pub async fn export_profile(&self, id: &ProfileId) -> Result<ProfileData> {
        let metadata = self.get_metadata(id).await?;

        let calibration = self.load_calibration(id).await.ok();
        let errp_model = self.load_errp_model(id).await.ok();
        let decoder_model = self.load_decoder_model(id).await.ok();
        let decoder_model_onnx = self.load_decoder_model_onnx(id).await.ok();
        let decoder_manifest = self.load_decoder_manifest(id).await.ok();

        Ok(ProfileData {
            metadata,
            calibration,
            errp_model,
            decoder_model,
            decoder_model_onnx,
            decoder_manifest,
        })
    }

    /// Imports a profile from exported data.
    pub async fn import_profile(&self, data: ProfileData) -> Result<ProfileId> {
        // Generate a new ID to avoid conflicts
        let new_id = ProfileId::generate();
        let mut metadata = data.metadata;
        metadata.id = new_id.clone();

        // Create profile directory
        let profile_dir = self.paths.profile_dir(&new_id);
        fs::create_dir_all(&profile_dir)
            .await
            .map_err(|e| StorageError::WriteError {
                path: profile_dir.display().to_string(),
                reason: e.to_string(),
            })?;

        // Save metadata
        self.save_metadata(&metadata).await?;

        // Save encrypted data
        if let Some(calibration) = data.calibration {
            self.save_calibration(&new_id, &calibration).await?;
        }
        if let Some(errp_model) = data.errp_model {
            self.save_errp_model(&new_id, &errp_model).await?;
        }
        if let Some(decoder_model) = data.decoder_model {
            self.save_decoder_model(&new_id, &decoder_model).await?;
        }
        if let Some(decoder_model_onnx) = data.decoder_model_onnx {
            self.save_decoder_model_onnx(&new_id, &decoder_model_onnx)
                .await?;
        }
        if let Some(decoder_manifest) = data.decoder_manifest {
            self.save_decoder_manifest(&new_id, &decoder_manifest)
                .await?;
        }

        Ok(new_id)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ProfileStore, parse_session_log_id};
    use crate::{DataPaths, SecureStorage};

    fn unique_test_root(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "neurohid_storage_{}_{}",
            test_name,
            neurohid_types::now_micros()
        ))
    }

    #[test]
    fn parse_session_log_id_accepts_expected_format() {
        assert_eq!(
            parse_session_log_id("session_1730000000000.json.enc"),
            Some("1730000000000".to_string())
        );
        assert_eq!(parse_session_log_id("session_.json.enc"), None);
        assert_eq!(parse_session_log_id("1730000000000.json.enc"), None);
        assert_eq!(parse_session_log_id("session_1730000000000.json"), None);
    }

    #[test]
    fn export_training_session_logs_to_dir_handles_empty_profile() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        runtime.block_on(async {
            let root = unique_test_root("export_empty");
            let paths = DataPaths::new(Some(root.clone())).expect("paths should build");
            paths
                .ensure_directories()
                .await
                .expect("directories should initialize");

            let store = ProfileStore::new(paths, SecureStorage::default());
            let profile = store
                .create_profile("export-empty".to_string())
                .await
                .expect("profile should be created");

            let output_dir = root.join("exports");
            let exported = store
                .export_training_session_logs_to_dir(&profile.id, &output_dir)
                .await
                .expect("export should succeed");

            assert_eq!(exported, 0);
            assert!(output_dir.exists());

            let _ = std::fs::remove_dir_all(root);
        });
    }
}
