//! Pipeline integration test: device → signal → decoder → action.
//!
//! Exercises the key boundary in one flow using a mock device (in-memory
//! samples), in-memory signal pipeline, and built-in decoder with fallback.
//! No full binary or real HID; validates interface compatibility at the
//! pipeline boundary.

use std::sync::Arc;
use std::time::Duration;

use neurohid_core::service::ServiceState;
use neurohid_core::tasks::create_decoder;
use neurohid_signal::{PipelineConfig, SignalPipeline};
use neurohid_types::config::DecoderConfig;
use neurohid_types::observability::ObservabilityConfig;
use neurohid_types::signal::FeatureVector;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::timeout;

const SAMPLE_RATE_HZ: f32 = 128.0;
const CHANNELS: usize = 5;
/// Enough samples to fill at least one feature window and step (default config: window 64, step 7).
const MOCK_SAMPLES: usize = 200;

fn make_mock_samples() -> Vec<(Vec<f32>, i64)> {
    (0..MOCK_SAMPLES)
        .map(|i| {
            let t = (i as i64) * 1_000_000 / (SAMPLE_RATE_HZ as i64);
            (vec![1.0; CHANNELS], t)
        })
        .collect()
}

#[tokio::test]
async fn pipeline_device_signal_decoder_action_flow() {
    // 1. Signal: build pipeline and produce features from mock samples (device → signal)
    let config = PipelineConfig::default();
    let mut pipeline = SignalPipeline::new(config).expect("pipeline must build");

    let samples = make_mock_samples();
    let mut features = Vec::new();
    for (values, ts) in &samples {
        pipeline.push_sample(values, *ts).expect("push_sample");
        if let Ok(Some(fv)) = pipeline.try_extract() {
            features.push(fv);
        }
    }

    assert!(
        !features.is_empty(),
        "pipeline must produce at least one feature vector from mock samples"
    );

    // Ensure at least one feature yields a non-none action (fallback uses values[0], values[1] for dx, dy)
    let mut synthetic = FeatureVector::new(vec![0.2, 0.2, 0.5, 0.0, 0.0]);
    synthetic.timestamp = 1_000_000;
    features.push(synthetic);

    // 2. Decoder + action: wire decoder to action channel and run (decoder → action)
    let (feature_tx, feature_rx) = mpsc::channel(16);
    let (action_tx, mut action_rx) = mpsc::channel(16);
    let state = Arc::new(RwLock::new(ServiceState::default()));

    let (runner, _name) = create_decoder(
        DecoderConfig::default(),
        feature_rx,
        action_tx,
        Arc::clone(&state),
        None,
        None,
        None,
        None,
        None,
        true, // fallback so no ONNX model required
        ObservabilityConfig::default(),
        None,
    )
    .expect("create_decoder");

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let decoder_handle = tokio::spawn(async move {
        runner.run(shutdown_rx).await
    });

    // Send features and close sender so decoder can exit after draining
    for fv in features {
        feature_tx.send(fv).await.expect("send feature");
    }
    drop(feature_tx);

    // Receive at least one action with a bounded wait (condition-based: we expect actions from fallback)
    let deadline = Duration::from_secs(3);
    let mut received = 0u32;
    loop {
        match timeout(deadline, action_rx.recv()).await {
            Ok(Some(_action)) => {
                received += 1;
                if received >= 1 {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => {
                assert!(received >= 1, "decoder should emit at least one action within {:?}", deadline);
                break;
            }
        }
    }

    shutdown_tx.send(()).ok();
    let _ = timeout(Duration::from_secs(1), decoder_handle).await;

    assert!(received >= 1, "pipeline boundary must deliver at least one action from features");
}
