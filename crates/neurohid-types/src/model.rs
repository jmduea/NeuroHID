//! # Model Artifact Types
//!
//! Types describing ML model artifacts exchanged between Python training and
//! Rust runtime inference.

use serde::{Deserialize, Serialize};

use crate::Timestamp;

/// Runtime feature schema version used by NeuroHID v1.
pub const CURRENT_FEATURE_SCHEMA_VERSION: u32 = 1;
/// Runtime action schema version used by NeuroHID v1.
pub const CURRENT_ACTION_SCHEMA_VERSION: u32 = 1;

/// Feature normalization statistics used before model inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizationStats {
    /// Per-dimension mean values.
    pub mean: Vec<f32>,
    /// Per-dimension standard deviation values.
    pub std: Vec<f32>,
}

impl NormalizationStats {
    /// Returns true when mean/std vectors are present and aligned.
    pub fn is_valid(&self) -> bool {
        !self.mean.is_empty()
            && self.mean.len() == self.std.len()
            && self.std.iter().all(|v| v.is_finite() && *v > 0.0)
            && self.mean.iter().all(|v| v.is_finite())
    }
}

/// Canonical metadata contract for ONNX inference models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelManifest {
    /// Semantic model version (for example, `1.3.0`).
    pub model_version: String,
    /// Flat input vector dimension expected by the model.
    pub input_dim: usize,
    /// Feature schema version used to build model inputs.
    pub feature_schema_version: u32,
    /// Action schema version produced by model outputs.
    pub action_schema_version: u32,
    /// Normalization stats applied before model inference.
    pub normalization_stats: NormalizationStats,
    /// Model training timestamp in microseconds since Unix epoch.
    pub trained_at: Timestamp,
}

impl ModelManifest {
    /// Validate manifest consistency before loading into runtime inference.
    pub fn validate(&self) -> Result<(), String> {
        if self.model_version.trim().is_empty() {
            return Err("model_version must not be empty".to_string());
        }
        if self.input_dim == 0 {
            return Err("input_dim must be greater than zero".to_string());
        }
        if self.feature_schema_version == 0 {
            return Err("feature_schema_version must be greater than zero".to_string());
        }
        if self.action_schema_version == 0 {
            return Err("action_schema_version must be greater than zero".to_string());
        }
        if self.trained_at <= 0 {
            return Err("trained_at must be a positive timestamp".to_string());
        }
        if !self.normalization_stats.is_valid() {
            return Err(
                "normalization_stats must contain aligned finite mean/std values".to_string(),
            );
        }
        if self.normalization_stats.mean.len() != self.input_dim {
            return Err(format!(
                "normalization_stats dim {} does not match input_dim {}",
                self.normalization_stats.mean.len(),
                self.input_dim
            ));
        }
        Ok(())
    }

    /// Validate against the runtime schema contract.
    pub fn validate_runtime_compatibility(&self) -> Result<(), String> {
        self.validate()?;
        if self.feature_schema_version != CURRENT_FEATURE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported feature_schema_version {} (expected {})",
                self.feature_schema_version, CURRENT_FEATURE_SCHEMA_VERSION
            ));
        }
        if self.action_schema_version != CURRENT_ACTION_SCHEMA_VERSION {
            return Err(format!(
                "unsupported action_schema_version {} (expected {})",
                self.action_schema_version, CURRENT_ACTION_SCHEMA_VERSION
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ModelManifest, NormalizationStats, CURRENT_ACTION_SCHEMA_VERSION,
        CURRENT_FEATURE_SCHEMA_VERSION,
    };

    fn base_manifest() -> ModelManifest {
        ModelManifest {
            model_version: "1.0.0".to_string(),
            input_dim: 4,
            feature_schema_version: CURRENT_FEATURE_SCHEMA_VERSION,
            action_schema_version: CURRENT_ACTION_SCHEMA_VERSION,
            normalization_stats: NormalizationStats {
                mean: vec![0.0; 4],
                std: vec![1.0; 4],
            },
            trained_at: 1,
        }
    }

    #[test]
    fn runtime_compatibility_accepts_matching_schema() {
        let manifest = base_manifest();
        assert!(manifest.validate_runtime_compatibility().is_ok());
    }

    #[test]
    fn runtime_compatibility_rejects_mismatched_feature_schema() {
        let mut manifest = base_manifest();
        manifest.feature_schema_version = CURRENT_FEATURE_SCHEMA_VERSION + 1;
        assert!(manifest.validate_runtime_compatibility().is_err());
    }
}
