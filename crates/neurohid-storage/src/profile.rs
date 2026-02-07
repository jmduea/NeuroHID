//! # Profile Storage
//!
//! Manages user profiles including metadata, calibration data, and models.

use tokio::fs;

use neurohid_types::{
    profile::{ProfileId, ProfileMetadata, CalibrationState},
    error::{StorageError, Result},
};
use crate::{DataPaths, SecureStorage};

/// Manages storage and retrieval of user profiles.
#[derive(Clone)]
pub struct ProfileStore {
    paths: DataPaths,
    secure: SecureStorage,
}

impl ProfileStore {
    /// Creates a new ProfileStore.
    pub fn new(paths: DataPaths, secure: SecureStorage) -> Self {
        Self { paths, secure }
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
        fs::create_dir_all(&profile_dir).await
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
        
        let contents = fs::read_to_string(&path).await
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
        
        fs::write(&path, contents).await
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
            }.into());
        }
        
        fs::remove_dir_all(&profile_dir).await
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
        
        Ok(ProfileData {
            metadata,
            calibration,
            errp_model,
            decoder_model,
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
        fs::create_dir_all(&profile_dir).await
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
        
        Ok(new_id)
    }
}
