//! # IPC Task
//!
//! This task is the bridge between Rust and Python. It receives processed
//! features from the signal task, sends them to the Python ML process, and
//! receives decoded actions and ErrP results back.
//!
//! The Python process runs the "heavy" ML workloads: the decoder neural network
//! and the ErrP classifier. By keeping these in a separate process, we get
//! several benefits:
//!
//! 1. If Python crashes, Rust keeps running (graceful degradation)
//! 2. We can use the full PyTorch ecosystem
//! 3. We can restart Python to pick up code changes without restarting Rust

use tokio::sync::{broadcast, mpsc};

use neurohid_types::{
    config::ServiceConfig,
    signal::FeatureVector,
    action::Action,
    reward::ErrPResult,
    error::Result,
};
use neurohid_ipc::{RustToPython, ObservationContext};

/// The IPC task manages communication with the Python ML process.
pub struct IpcTask {
    #[allow(dead_code)] // will configure IPC socket path/port
    config: ServiceConfig,
    feature_rx: mpsc::Receiver<FeatureVector>,
    action_tx: mpsc::Sender<Action>,
    #[allow(dead_code)] // will send ErrP results when Python bridge is wired
    errp_tx: mpsc::Sender<ErrPResult>,
    
    // Sequence numbers for message ordering
    send_sequence: u64,
}

impl IpcTask {
    /// Creates a new IPC task.
    pub fn new(
        config: ServiceConfig,
        feature_rx: mpsc::Receiver<FeatureVector>,
        action_tx: mpsc::Sender<Action>,
        errp_tx: mpsc::Sender<ErrPResult>,
    ) -> Self {
        Self {
            config,
            feature_rx,
            action_tx,
            errp_tx,
            send_sequence: 0,
        }
    }
    
    /// Runs the IPC task until shutdown is signaled.
    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!("IPC task started");
        
        // In a full implementation, we would:
        // 1. Start the IPC server (Unix socket or named pipe)
        // 2. Wait for the Python process to connect
        // 3. Enter the main loop of sending features and receiving actions
        //
        // For the MVP skeleton, we'll simulate the behavior with a simple
        // passthrough that generates mock actions.
        
        tracing::info!("Waiting for Python ML process to connect...");
        
        // Simulate connection (in reality, we'd wait for actual connection)
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        tracing::info!("Python ML process connected (simulated)");
        
        // Batch features to reduce IPC overhead
        let mut feature_batch: Vec<FeatureVector> = Vec::with_capacity(4);
        
        loop {
            tokio::select! {
                // Check for shutdown
                _ = shutdown.recv() => {
                    tracing::info!("IPC task received shutdown signal");
                    break;
                }
                
                // Receive features from signal task
                feature = self.feature_rx.recv() => {
                    match feature {
                        Some(f) => {
                            feature_batch.push(f);
                            
                            // Send batch when we have enough features
                            // (or on a timer, whichever comes first)
                            if feature_batch.len() >= 4 {
                                self.send_features(&feature_batch).await?;
                                
                                // In a real implementation, we'd receive the action
                                // from Python here. For now, generate a mock action.
                                let mock_action = self.generate_mock_action(&feature_batch);
                                
                                if self.action_tx.send(mock_action).await.is_err() {
                                    tracing::warn!("Action receiver dropped");
                                    break;
                                }
                                
                                feature_batch.clear();
                            }
                        }
                        None => {
                            tracing::info!("Feature channel closed");
                            break;
                        }
                    }
                }
            }
        }
        
        tracing::info!("IPC task completed");
        Ok(())
    }
    
    /// Sends a batch of features to the Python process.
    async fn send_features(&mut self, features: &[FeatureVector]) -> Result<()> {
        self.send_sequence += 1;
        
        // In a real implementation, we would:
        // 1. Serialize the features to JSON
        // 2. Frame the message with a length prefix
        // 3. Send over the IPC socket
        
        // Create the message (for reference, even though we're not sending it yet)
        let _message = RustToPython::FeatureBatch {
            features: features.to_vec(),
            context: ObservationContext {
                cursor_x: 0.5,
                cursor_y: 0.5,
                cursor_velocity_x: 0.0,
                cursor_velocity_y: 0.0,
                screen_width: 1920,
                screen_height: 1080,
                signal_quality: neurohid_types::reward::SignalQuality::Good,
                timestamp: neurohid_types::now_micros(),
            },
            sequence: self.send_sequence,
        };
        
        // tracing::debug!("Would send {} features to Python", features.len());
        
        Ok(())
    }
    
    /// Generates a mock action for testing.
    ///
    /// In a real implementation, this would come from the Python decoder.
    fn generate_mock_action(&self, _features: &[FeatureVector]) -> Action {
        // Generate small random mouse movements for testing
        use neurohid_types::action::MouseAction;
        
        // Use a deterministic but varying movement based on sequence number
        let angle = (self.send_sequence as f32 * 0.1) % (2.0 * std::f32::consts::PI);
        let dx = angle.cos() * 2.0;
        let dy = angle.sin() * 2.0;
        
        Action::mouse(MouseAction::move_relative(dx, dy))
            .with_confidence(0.8)
    }
}
