//! # Secure Storage
//!
//! Handles encryption and keychain integration for sensitive data.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine as _, engine::general_purpose};
use rand::Rng;

use crate::{APP_IDENTIFIER, KEYCHAIN_SERVICE};
use neurohid_types::error::{Result, StorageError};

/// Manages encryption keys and secure data operations.
#[derive(Clone)]
pub struct SecureStorage {
    /// The keychain entry for our master key
    keyring_entry: String,
}

impl SecureStorage {
    /// Creates a new SecureStorage instance.
    pub fn new() -> Result<Self> {
        Ok(Self {
            keyring_entry: format!("{}.master_key", APP_IDENTIFIER),
        })
    }

    /// Ensures the master encryption key exists in the keychain.
    ///
    /// If no key exists, generates a new random key and stores it.
    pub async fn ensure_master_key(&self) -> Result<()> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &self.keyring_entry)
            .map_err(|e| StorageError::KeyringError(e.to_string()))?;

        // Try to get existing key
        match entry.get_password() {
            Ok(_) => {
                // Key exists, nothing to do
                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                // No key exists, generate one
                let key = Self::generate_key();
                let key_b64 = general_purpose::STANDARD.encode(key);

                entry
                    .set_password(&key_b64)
                    .map_err(|e| StorageError::KeyringError(e.to_string()))?;

                Ok(())
            }
            Err(e) => Err(StorageError::KeyringError(e.to_string()).into()),
        }
    }

    /// Retrieves the master encryption key from the keychain.
    fn get_master_key(&self) -> Result<[u8; 32]> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &self.keyring_entry)
            .map_err(|e| StorageError::KeyringError(e.to_string()))?;

        let key_b64 = entry
            .get_password()
            .map_err(|e| StorageError::KeyringError(e.to_string()))?;

        let key_bytes = general_purpose::STANDARD
            .decode(&key_b64)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

        key_bytes
            .try_into()
            .map_err(|_| StorageError::EncryptionError("Invalid key length".to_string()).into())
    }

    /// Generates a new random 256-bit key.
    fn generate_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        rand::rng().fill_bytes(&mut key);
        key
    }

    /// Generates a random 96-bit nonce.
    fn generate_nonce() -> [u8; 12] {
        let mut nonce = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce);
        nonce
    }

    /// Encrypts data using AES-256-GCM.
    ///
    /// Returns the nonce prepended to the ciphertext.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let key = self.get_master_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

        let nonce_bytes = Self::generate_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypts data that was encrypted with `encrypt`.
    ///
    /// Expects the nonce to be prepended to the ciphertext.
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(StorageError::EncryptionError("Data too short".to_string()).into());
        }

        let key = self.get_master_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

        Ok(plaintext)
    }

    /// Encrypts and writes data to a file.
    pub async fn write_encrypted(&self, path: &std::path::Path, data: &[u8]) -> Result<()> {
        let encrypted = self.encrypt(data)?;

        tokio::fs::write(path, &encrypted)
            .await
            .map_err(|e| StorageError::WriteError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

        // Set restrictive permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(path, perms).map_err(|e| StorageError::WriteError {
                path: path.display().to_string(),
                reason: format!("Failed to set permissions: {}", e),
            })?;
        }

        Ok(())
    }

    /// Reads and decrypts data from a file.
    pub async fn read_encrypted(&self, path: &std::path::Path) -> Result<Vec<u8>> {
        let encrypted = tokio::fs::read(path)
            .await
            .map_err(|e| StorageError::ReadError {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

        self.decrypt(&encrypted)
    }
}

impl Default for SecureStorage {
    fn default() -> Self {
        Self {
            keyring_entry: format!("{}.master_key", APP_IDENTIFIER),
        }
    }
}

#[cfg(test)]
mod tests {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit},
    };

    use super::SecureStorage;

    #[test]
    fn generate_key_produces_32_bytes() {
        let key = SecureStorage::generate_key();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn generate_key_is_random() {
        let k1 = SecureStorage::generate_key();
        let k2 = SecureStorage::generate_key();
        assert_ne!(k1, k2, "two generated keys should differ");
    }

    #[test]
    fn generate_nonce_produces_12_bytes() {
        let nonce = SecureStorage::generate_nonce();
        assert_eq!(nonce.len(), 12);
    }

    #[test]
    fn generate_nonce_is_unique() {
        let n1 = SecureStorage::generate_nonce();
        let n2 = SecureStorage::generate_nonce();
        assert_ne!(n1, n2, "two generated nonces should differ");
    }

    /// Helper that encrypts using the same algorithm as SecureStorage::encrypt
    /// but with a caller-supplied key, avoiding keyring dependency.
    fn encrypt_with_key(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
        let cipher = Aes256Gcm::new_from_slice(key).unwrap();
        let nonce_bytes = SecureStorage::generate_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, plaintext).unwrap();
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        result
    }

    /// Helper that decrypts using the same algorithm as SecureStorage::decrypt.
    fn decrypt_with_key(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
        if data.len() < 12 {
            return Err("Data too short".to_string());
        }
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = SecureStorage::generate_key();
        let plaintext = b"sensitive brain signal data";
        let encrypted = encrypt_with_key(&key, plaintext);

        assert_ne!(&encrypted[12..], plaintext, "ciphertext should differ from plaintext");
        assert!(encrypted.len() > 12 + plaintext.len(), "encrypted has nonce + tag overhead");

        let decrypted = decrypt_with_key(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_decrypt_roundtrip_empty() {
        let key = SecureStorage::generate_key();
        let plaintext = b"";
        let encrypted = encrypt_with_key(&key, plaintext);
        let decrypted = decrypt_with_key(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_decrypt_roundtrip_large() {
        let key = SecureStorage::generate_key();
        let plaintext: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let encrypted = encrypt_with_key(&key, &plaintext);
        let decrypted = decrypt_with_key(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_rejects_data_too_short() {
        let key = SecureStorage::generate_key();
        let short = vec![0u8; 11];
        let result = decrypt_with_key(&key, &short);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_rejects_tampered_ciphertext() {
        let key = SecureStorage::generate_key();
        let plaintext = b"important calibration data";
        let mut encrypted = encrypt_with_key(&key, plaintext);

        // Flip a bit in the ciphertext portion (after the 12-byte nonce)
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;

        let result = decrypt_with_key(&key, &encrypted);
        assert!(result.is_err(), "tampered ciphertext should fail authentication");
    }

    #[test]
    fn decrypt_rejects_wrong_key() {
        let key1 = SecureStorage::generate_key();
        let key2 = SecureStorage::generate_key();
        let plaintext = b"secret data";
        let encrypted = encrypt_with_key(&key1, plaintext);

        let result = decrypt_with_key(&key2, &encrypted);
        assert!(result.is_err(), "wrong key should fail decryption");
    }

    #[test]
    fn each_encryption_produces_different_ciphertext() {
        let key = SecureStorage::generate_key();
        let plaintext = b"same input";
        let enc1 = encrypt_with_key(&key, plaintext);
        let enc2 = encrypt_with_key(&key, plaintext);

        assert_ne!(enc1, enc2, "random nonce should produce different ciphertext each time");

        // But both should decrypt to the same plaintext
        assert_eq!(decrypt_with_key(&key, &enc1).unwrap(), plaintext);
        assert_eq!(decrypt_with_key(&key, &enc2).unwrap(), plaintext);
    }

    #[test]
    fn secure_storage_new_succeeds() {
        let storage = SecureStorage::new();
        assert!(storage.is_ok());
    }

    #[test]
    fn secure_storage_default_sets_keyring_entry() {
        let storage = SecureStorage::default();
        assert!(
            storage.keyring_entry.contains("master_key"),
            "keyring entry should reference master_key"
        );
    }
}
