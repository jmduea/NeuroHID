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

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::sync::{broadcast, mpsc};

use neurohid_ipc::{
    server::IpcConnection, IpcConfig, IpcServer, ObservationContext, PythonToRust, RustToPython,
};
use neurohid_types::{
    action::Action,
    config::ServiceConfig,
    error::{Error, IpcError, Result},
    event::{MarkerPayload, MarkerType, StreamMarker},
    reward::ErrPResult,
    signal::FeatureVector,
};

use crate::service::ServiceState;

const FEATURE_BATCH_SIZE: usize = 4;
const SIMULATED_CONNECT_DELAY_MS: u64 = 100;
const REAL_MESSAGE_POLL_MS: u64 = 25;

/// The IPC task manages communication with the Python ML process.
pub struct IpcTask {
    config: ServiceConfig,
    feature_rx: mpsc::Receiver<FeatureVector>,
    action_tx: mpsc::Sender<Action>,
    errp_tx: mpsc::Sender<ErrPResult>,
    state: Arc<RwLock<ServiceState>>,
    marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,

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
        state: Arc<RwLock<ServiceState>>,
        marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
    ) -> Self {
        Self {
            config,
            feature_rx,
            action_tx,
            errp_tx,
            state,
            marker_broadcast_tx,
            send_sequence: 0,
        }
    }

    /// Runs the IPC task until shutdown is signaled.
    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!("IPC task started");

        let result = if self.config.ipc_simulation_enabled {
            self.run_simulated(&mut shutdown).await
        } else {
            self.run_real_bridge(&mut shutdown).await
        };

        self.set_connection_state(false, false).await;
        tracing::info!("IPC task completed");

        result
    }

    async fn run_real_bridge(&mut self, shutdown: &mut broadcast::Receiver<()>) -> Result<()> {
        let address = format!("127.0.0.1:{}", self.config.ipc_port);
        let server = IpcServer::new(IpcConfig {
            address,
            ..IpcConfig::default()
        })
        .await?;

        loop {
            tracing::info!("Waiting for Python ML process to connect...");

            let connection = tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("IPC task received shutdown signal");
                    return Ok(());
                }
                result = server.accept() => result?,
            };

            self.set_connection_state(true, false).await;
            tracing::info!("Python ML process connected");

            let run_result = self.run_connected_loop(connection, shutdown).await;
            self.set_connection_state(false, false).await;

            match run_result {
                Ok(()) => return Ok(()),
                Err(err) if is_connection_lost_error(&err) => {
                    tracing::warn!("Python ML process disconnected; waiting for reconnect");
                }
                Err(err) => return Err(err),
            }
        }
    }

    async fn run_connected_loop(
        &mut self,
        connection: IpcConnection,
        shutdown: &mut broadcast::Receiver<()>,
    ) -> Result<()> {
        let mut feature_batch: Vec<FeatureVector> = Vec::with_capacity(FEATURE_BATCH_SIZE);
        let mut poll = tokio::time::interval(Duration::from_millis(REAL_MESSAGE_POLL_MS));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("IPC task received shutdown signal");
                    let _ = connection.send(RustToPython::Shutdown).await;
                    break;
                }
                _ = poll.tick() => {
                    self.drain_python_messages(&connection).await?;
                }
                feature = self.feature_rx.recv() => {
                    match feature {
                        Some(f) => {
                            feature_batch.push(f);

                            if feature_batch.len() >= FEATURE_BATCH_SIZE {
                                self.send_features_real(&connection, &feature_batch).await?;
                                feature_batch.clear();
                                self.drain_python_messages(&connection).await?;
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

        Ok(())
    }

    async fn drain_python_messages(&mut self, connection: &IpcConnection) -> Result<()> {
        loop {
            match connection.try_recv()? {
                Some(msg) => self.handle_python_message(msg).await?,
                None => break,
            }
        }

        Ok(())
    }

    async fn handle_python_message(&mut self, message: PythonToRust) -> Result<()> {
        match message {
            PythonToRust::Ready => {
                tracing::debug!("Python bridge reported ready");
            }
            PythonToRust::Action {
                action,
                sequence,
                inference_latency_us,
            } => {
                tracing::trace!(
                    sequence,
                    inference_latency_us,
                    "Received decoded action from Python"
                );

                if self.action_tx.send(action).await.is_err() {
                    tracing::warn!("Action receiver dropped");
                }
            }
            PythonToRust::ErrPResult { result, sequence } => {
                tracing::trace!(sequence, "Received ErrP result from Python");

                if let Some(tx) = &self.marker_broadcast_tx {
                    let marker = StreamMarker::now(MarkerType::ErrpWindowResult).with_payload(
                        MarkerPayload::ErrpResult {
                            sequence,
                            error_probability: result.error_probability,
                        },
                    );
                    let _ = tx.send(marker);
                }

                if self.errp_tx.send(result).await.is_err() {
                    tracing::warn!("ErrP receiver dropped");
                }
            }
            PythonToRust::Error {
                message,
                recoverable,
            } => {
                tracing::warn!(recoverable, %message, "Python bridge reported error");

                if !recoverable {
                    return Err(IpcError::ReceiveFailed(message).into());
                }
            }
            PythonToRust::TrainingComplete { sequence, .. } => {
                tracing::trace!(sequence, "Python training batch completed");
            }
            PythonToRust::ModelLoaded { model_type, .. } => {
                tracing::info!(?model_type, "Python model loaded");
            }
            PythonToRust::Pong { timestamp, .. } => {
                tracing::trace!(timestamp, "Received pong from Python");
            }
        }

        Ok(())
    }

    async fn run_simulated(&mut self, shutdown: &mut broadcast::Receiver<()>) -> Result<()> {
        tracing::info!("IPC simulation mode enabled; starting simulated bridge");
        tokio::time::sleep(Duration::from_millis(SIMULATED_CONNECT_DELAY_MS)).await;
        self.set_connection_state(true, true).await;
        tracing::info!("Python ML process connected (simulated)");

        let mut feature_batch: Vec<FeatureVector> = Vec::with_capacity(FEATURE_BATCH_SIZE);

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("IPC task received shutdown signal");
                    break;
                }
                feature = self.feature_rx.recv() => {
                    match feature {
                        Some(f) => {
                            feature_batch.push(f);

                            if feature_batch.len() >= FEATURE_BATCH_SIZE {
                                self.send_features_simulated(&feature_batch).await?;
                                let mock_action = self.generate_mock_action();

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

        Ok(())
    }

    /// Sends a batch of features to the Python process over the real bridge.
    async fn send_features_real(
        &mut self,
        connection: &IpcConnection,
        features: &[FeatureVector],
    ) -> Result<()> {
        let message = self.build_feature_batch(features);
        connection.send(message).await
    }

    /// Simulated send path used in MVP fallback mode.
    async fn send_features_simulated(&mut self, features: &[FeatureVector]) -> Result<()> {
        let _message = self.build_feature_batch(features);
        Ok(())
    }

    fn build_feature_batch(&mut self, features: &[FeatureVector]) -> RustToPython {
        self.send_sequence += 1;
        if let Some(tx) = &self.marker_broadcast_tx {
            let marker = StreamMarker::now(MarkerType::ErrpWindowStart).with_payload(
                MarkerPayload::ErrpWindow {
                    sequence: self.send_sequence,
                    action_timestamp: neurohid_types::now_micros(),
                },
            );
            let _ = tx.send(marker);
        }

        RustToPython::FeatureBatch {
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
        }
    }

    async fn set_connection_state(&self, connected: bool, simulated: bool) {
        let mut state = self.state.write().await;
        state.ipc_connected = connected;
        state.ipc_simulated = simulated;
    }

    /// Generates a mock action for testing.
    ///
    /// In a real implementation, this would come from the Python decoder.
    fn generate_mock_action(&self) -> Action {
        use neurohid_types::action::MouseAction;

        // Use a deterministic but varying movement based on sequence number
        let angle = (self.send_sequence as f32 * 0.1) % (2.0 * std::f32::consts::PI);
        let dx = angle.cos() * 2.0;
        let dy = angle.sin() * 2.0;

        Action::mouse(MouseAction::move_relative(dx, dy)).with_confidence(0.8)
    }
}

fn is_connection_lost_error(err: &Error) -> bool {
    matches!(err, Error::Ipc(IpcError::ConnectionLost))
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::Arc;

    use neurohid_ipc::{IpcClient, IpcConfig, PythonToRust, RustToPython};
    use tokio::sync::{broadcast, mpsc, RwLock};
    use tokio::time::{sleep, timeout, Duration};

    use neurohid_types::action::{Action, MouseAction};

    use super::IpcTask;
    use crate::service::ServiceState;

    #[tokio::test]
    async fn ipc_simulation_emits_actions_and_cleans_up_state() {
        let config = neurohid_types::config::ServiceConfig::default();

        let (feature_tx, feature_rx) = mpsc::channel(16);
        let (action_tx, mut action_rx) = mpsc::channel(16);
        let (errp_tx, _errp_rx) = mpsc::channel(16);
        let state = Arc::new(RwLock::new(ServiceState::default()));
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        let task = IpcTask::new(
            config,
            feature_rx,
            action_tx,
            errp_tx,
            Arc::clone(&state),
            None,
        );
        let run_handle = tokio::spawn(async move { task.run(shutdown_rx).await });

        wait_for_connection_state(&state, true, true).await;

        for _ in 0..4 {
            feature_tx
                .send(neurohid_types::signal::FeatureVector::new(vec![
                    0.1, 0.2, 0.3, 0.4,
                ]))
                .await
                .expect("feature send should succeed");
        }
        drop(feature_tx);

        let action = timeout(Duration::from_secs(1), action_rx.recv())
            .await
            .expect("action receive should not time out")
            .expect("mock action should be produced");
        assert!(action.mouse.is_some());
        assert_eq!(action.confidence, 0.8);

        let result = timeout(Duration::from_secs(2), run_handle)
            .await
            .expect("ipc task should finish")
            .expect("join should succeed");
        assert!(result.is_ok());

        // Shutdown send is harmless if the task already finished.
        let _ = shutdown_tx.send(());

        let state_guard = state.read().await;
        assert!(!state_guard.ipc_connected);
        assert!(!state_guard.ipc_simulated);
    }

    #[tokio::test]
    async fn ipc_real_bridge_forwards_actions() {
        let mut config = neurohid_types::config::ServiceConfig::default();
        config.ipc_simulation_enabled = false;
        config.ipc_port = allocate_test_port();

        let (feature_tx, feature_rx) = mpsc::channel(16);
        let (action_tx, mut action_rx) = mpsc::channel(16);
        let (errp_tx, _errp_rx) = mpsc::channel(16);
        let state = Arc::new(RwLock::new(ServiceState::default()));
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        let task = IpcTask::new(
            config.clone(),
            feature_rx,
            action_tx,
            errp_tx,
            Arc::clone(&state),
            None,
        );
        let run_handle = tokio::spawn(async move { task.run(shutdown_rx).await });

        let mut client = connect_test_client(config.ipc_port).await;
        client
            .send(PythonToRust::Ready)
            .await
            .expect("ready message should send");

        wait_for_connection_state(&state, true, false).await;

        for _ in 0..4 {
            feature_tx
                .send(neurohid_types::signal::FeatureVector::new(vec![
                    1.0, 2.0, 3.0, 4.0,
                ]))
                .await
                .expect("feature send should succeed");
        }

        let outbound = timeout(Duration::from_secs(1), client.recv())
            .await
            .expect("feature batch receive should not time out")
            .expect("feature batch should decode");

        match outbound {
            RustToPython::FeatureBatch {
                features, sequence, ..
            } => {
                assert_eq!(features.len(), 4);
                assert_eq!(sequence, 1);
            }
            msg => panic!("unexpected outbound message: {msg:?}"),
        }

        let expected = Action::mouse(MouseAction::move_relative(3.0, -1.0)).with_confidence(0.91);
        client
            .send(PythonToRust::Action {
                action: expected,
                sequence: 42,
                inference_latency_us: 1_500,
            })
            .await
            .expect("action message should send");

        let action = timeout(Duration::from_secs(1), action_rx.recv())
            .await
            .expect("action receive should not time out")
            .expect("action should be forwarded");

        let movement = action
            .mouse
            .and_then(|mouse| mouse.movement)
            .expect("forwarded action should include movement");
        assert_eq!(movement.dx, 3.0);
        assert_eq!(movement.dy, -1.0);
        assert_eq!(action.confidence, 0.91);

        feature_tx
            .send(neurohid_types::signal::FeatureVector::new(vec![9.0]))
            .await
            .expect("feature send should still work before shutdown");

        let _ = shutdown_tx.send(());

        let result = timeout(Duration::from_secs(2), run_handle)
            .await
            .expect("ipc task should finish")
            .expect("join should succeed");
        assert!(result.is_ok());

        let state_guard = state.read().await;
        assert!(!state_guard.ipc_connected);
        assert!(!state_guard.ipc_simulated);
    }

    #[tokio::test]
    async fn ipc_real_bridge_tracks_disconnect_and_reconnect() {
        let mut config = neurohid_types::config::ServiceConfig::default();
        config.ipc_simulation_enabled = false;
        config.ipc_port = allocate_test_port();

        let (_feature_tx, feature_rx) = mpsc::channel(16);
        let (action_tx, _action_rx) = mpsc::channel(16);
        let (errp_tx, _errp_rx) = mpsc::channel(16);
        let state = Arc::new(RwLock::new(ServiceState::default()));
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        let task = IpcTask::new(
            config.clone(),
            feature_rx,
            action_tx,
            errp_tx,
            Arc::clone(&state),
            None,
        );
        let run_handle = tokio::spawn(async move { task.run(shutdown_rx).await });

        let mut first = connect_test_client(config.ipc_port).await;
        first
            .send(PythonToRust::Ready)
            .await
            .expect("ready message should send");
        wait_for_connection_state(&state, true, false).await;

        first.disconnect().await.expect("disconnect should succeed");
        wait_for_connection_state(&state, false, false).await;

        let second = connect_test_client(config.ipc_port).await;
        second
            .send(PythonToRust::Ready)
            .await
            .expect("ready message should send");
        wait_for_connection_state(&state, true, false).await;

        let _ = shutdown_tx.send(());
        let result = timeout(Duration::from_secs(2), run_handle)
            .await
            .expect("ipc task should finish")
            .expect("join should succeed");
        assert!(result.is_ok());
    }

    async fn wait_for_connection_state(
        state: &Arc<RwLock<ServiceState>>,
        connected: bool,
        simulated: bool,
    ) {
        timeout(Duration::from_secs(2), async {
            loop {
                let guard = state.read().await;
                if guard.ipc_connected == connected && guard.ipc_simulated == simulated {
                    break;
                }
                drop(guard);
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("state should reach expected connection status");
    }

    async fn connect_test_client(port: u16) -> IpcClient {
        let mut client = IpcClient::new(IpcConfig {
            address: format!("127.0.0.1:{port}"),
            connect_timeout_ms: 250,
            ..IpcConfig::default()
        });

        let start = tokio::time::Instant::now();

        loop {
            match client.connect().await {
                Ok(()) => return client,
                Err(err) if start.elapsed() < Duration::from_secs(2) => {
                    tracing::debug!(%err, "Waiting for IPC server to accept client");
                    sleep(Duration::from_millis(25)).await;
                }
                Err(err) => panic!("test IPC client failed to connect: {err}"),
            }
        }
    }

    fn allocate_test_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral port bind should succeed")
            .local_addr()
            .expect("local addr should resolve")
            .port()
    }
}
