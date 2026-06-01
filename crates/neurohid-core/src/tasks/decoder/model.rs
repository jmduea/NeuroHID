//! Decoder model loading and ONNX artifact handling.

use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use tract_onnx::prelude::*;

use neurohid_storage::ProfileStore;
use neurohid_types::error::{DecoderError, Error, Result};
use neurohid_types::model::ModelManifest;
use neurohid_types::profile::ProfileId;

type OnnxPlan = SimplePlan<TypedFact, Box<dyn TypedOp>, TypedModel>;

#[derive(Clone)]
pub(super) struct LoadedModel {
    pub(super) manifest: ModelManifest,
    pub(super) model: Arc<dyn InferenceModel>,
}

pub(super) trait InferenceModel: Send + Sync {
    fn infer(&self, normalized: &[f32]) -> Result<Vec<f32>>;
}

struct OnnxInferenceModel {
    model: OnnxPlan,
}

impl InferenceModel for OnnxInferenceModel {
    fn infer(&self, normalized: &[f32]) -> Result<Vec<f32>> {
        let input =
            tract_ndarray::Array2::from_shape_vec((1, normalized.len()), normalized.to_vec())
                .map_err(|e| Error::Decoder(DecoderError::InferenceFailed(e.to_string())))?;
        let input = input.into_tensor();
        let output = self
            .model
            .run(tvec!(input.into()))
            .map_err(|e| Error::Decoder(DecoderError::InferenceFailed(e.to_string())))?;
        let first = output.first().ok_or_else(|| {
            Error::Decoder(DecoderError::InferenceFailed(
                "empty model output".to_string(),
            ))
        })?;
        first
            .to_array_view::<f32>()
            .map_err(|e| Error::Decoder(DecoderError::InferenceFailed(e.to_string())))
            .map(|array| array.iter().copied().collect())
    }
}

#[async_trait]
pub(super) trait ArtifactLoader: Send + Sync {
    async fn load(
        &self,
        profile_store: Option<&ProfileStore>,
        profile_id: &ProfileId,
    ) -> Result<LoadedModel>;
}

pub(super) struct OnnxArtifactLoader;

#[async_trait]
impl ArtifactLoader for OnnxArtifactLoader {
    async fn load(
        &self,
        profile_store: Option<&ProfileStore>,
        profile_id: &ProfileId,
    ) -> Result<LoadedModel> {
        let store = profile_store.ok_or_else(|| {
            Error::Decoder(DecoderError::ModelFileError(
                "profile store unavailable for decoder model load".to_string(),
            ))
        })?;

        let manifest = store.load_decoder_manifest(profile_id).await?;
        manifest.validate_runtime_compatibility().map_err(|msg| {
            Error::Decoder(DecoderError::ModelFileError(format!(
                "manifest compatibility check failed: {msg}"
            )))
        })?;

        let model_bytes = store.load_decoder_model_onnx(profile_id).await?;
        let model = load_onnx_model(&model_bytes)?;

        Ok(LoadedModel { manifest, model })
    }
}

pub(super) fn load_onnx_model(bytes: &[u8]) -> Result<Arc<dyn InferenceModel>> {
    let mut cursor = Cursor::new(bytes);
    let model = tract_onnx::onnx()
        .model_for_read(&mut cursor)
        .map_err(|e| Error::Decoder(DecoderError::ModelFileError(e.to_string())))?
        .into_optimized()
        .map_err(|e| Error::Decoder(DecoderError::ModelFileError(e.to_string())))?
        .into_runnable()
        .map_err(|e| Error::Decoder(DecoderError::ModelFileError(e.to_string())))?;
    Ok(Arc::new(OnnxInferenceModel { model }))
}
