//! # NeuroHID Service
//!
//! This module contains the main service struct that orchestrates all the
//! concurrent tasks. Think of it as the "conductor" of an orchestra - it doesn't
//! play any instruments itself, but it makes sure everyone starts at the right
//! time, stays in sync, and stops together gracefully.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

use neurohid_storage::ProfileStore;
use neurohid_types::{
    action::Action,
    config::SystemConfig,
    device::DiscoveredStream,
    error::Result,
    profile::ProfileId,
    signal::{FeatureVector, Sample},
};

/// Commands sent from the hub to the DeviceTask for stream management.
#[derive(Debug)]
pub enum DeviceCommand {
    /// Re-scan for available LSL streams
    Rescan,
    /// Connect to a specific stream by its id
    Connect(String),
    /// Disconnect from a specific stream by its id
    Disconnect(String),
}

use crate::tasks::{ActionTask, DeviceTask, IpcTask, SignalTask};

/// The main service that coordinates all NeuroHID operations.
///
/// The service follows a "task supervisor" pattern: it spawns several
/// independent tasks that communicate via channels, and monitors them
/// for failures. If any critical task fails, the service initiates
/// a graceful shutdown.
pub struct NeuroHidService {
    /// System configuration
    config: SystemConfig,

    /// Profile storage — will be used for profile reloading
    #[allow(dead_code)]
    profile_store: Option<ProfileStore>,

    /// Active profile ID — will be used when profile data loading is wired
    #[allow(dead_code)]
    profile_id: Option<ProfileId>,

    /// Shutdown signal receiver
    shutdown_rx: broadcast::Receiver<()>,

    /// Shared state that tasks can read
    shared_state: Arc<RwLock<ServiceState>>,
}

/// Shared state accessible by all tasks.
///
/// This struct contains state that multiple tasks need to read. Write access
/// is carefully controlled to avoid contention. Most inter-task communication
/// happens through channels rather than shared state.
pub struct ServiceState {
    /// Whether the service is currently active (processing and emitting actions)
    pub active: bool,

    /// Whether online learning is enabled — will be used for learning control path
    #[allow(dead_code)]
    pub learning_enabled: bool,

    /// Current signal quality (updated by device task)
    pub signal_quality: f32,

    /// Actions emitted since service start
    pub actions_emitted: u64,

    /// Errors detected since service start
    pub errors_detected: u64,

    /// Whether a device is currently connected
    pub device_connected: bool,

    /// Name of the connected device (if any)
    pub device_name: Option<String>,

    /// Battery level of the connected device (0-100)
    pub device_battery: Option<u8>,

    /// When the service was started
    pub started_at: Option<std::time::Instant>,

    /// Name of the active profile
    pub active_profile_name: Option<String>,

    /// Whether the IPC bridge to Python is connected
    pub ipc_connected: bool,

    /// Whether the service is in calibration mode (pauses HID emission)
    pub calibration_mode: bool,

    /// If a task failed at runtime, (task_name, error_message).
    /// Populated by `run_inner()` so the GUI can display what went wrong.
    pub task_error: Option<(String, String)>,

    /// LSL streams discovered on the network.
    /// Updated periodically by the DeviceTask.
    pub discovered_streams: Vec<DiscoveredStream>,
}

impl Default for ServiceState {
    fn default() -> Self {
        Self {
            active: false,
            learning_enabled: true,
            signal_quality: 0.0,
            actions_emitted: 0,
            errors_detected: 0,
            device_connected: false,
            device_name: None,
            device_battery: None,
            started_at: None,
            active_profile_name: None,
            ipc_connected: false,
            calibration_mode: false,
            task_error: None,
            discovered_streams: Vec::new(),
        }
    }
}

/// A handle to a running service, returned by `NeuroHidService::spawn()`.
///
/// The handle lets the owner (e.g., the hub GUI) observe service state,
/// toggle calibration mode, and request shutdown — all without blocking.
pub struct ServiceHandle {
    /// Shared service state — read with `try_read()` from the GUI thread.
    pub state: Arc<RwLock<ServiceState>>,

    /// Send `()` on this channel to request graceful shutdown.
    pub shutdown_tx: broadcast::Sender<()>,

    /// The spawned task's join handle. Await it to detect completion/panics.
    pub join_handle: tokio::task::JoinHandle<Result<()>>,

    /// Receiver for live EEG samples during calibration mode.
    /// Only produces values when `calibration_mode` is `true`.
    pub calibration_sample_rx: mpsc::Receiver<Sample>,

    /// Atomic flag to toggle calibration mode from the GUI thread.
    pub calibration_mode: Arc<AtomicBool>,

    /// Send commands to the DeviceTask (connect/disconnect/rescan).
    pub device_command_tx: mpsc::Sender<DeviceCommand>,

    /// Broadcast receiver for ALL live EEG samples (for visualization widgets).
    /// Unlike `calibration_sample_rx`, this always produces values.
    pub sample_broadcast_rx: broadcast::Receiver<Sample>,

    /// Broadcast receiver for extracted feature vectors (for visualization widgets).
    pub feature_broadcast_rx: broadcast::Receiver<FeatureVector>,

    /// Broadcast receiver for decoded actions (for visualization widgets).
    pub action_broadcast_rx: broadcast::Receiver<Action>,
}

impl NeuroHidService {
    /// Creates a new NeuroHID service.
    ///
    /// This initializes all the components but doesn't start any tasks yet.
    /// Call `run()` to start the service (blocking) or `spawn()` to start
    /// it in the background and get a `ServiceHandle`.
    pub async fn new(
        config: SystemConfig,
        profile_store: Option<ProfileStore>,
        profile_id: Option<ProfileId>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<Self> {
        let shared_state = Arc::new(RwLock::new(ServiceState::default()));

        Ok(Self {
            config,
            profile_store,
            profile_id,
            shutdown_rx,
            shared_state,
        })
    }

    /// Spawns the service on the tokio runtime and returns a non-blocking handle.
    ///
    /// This is the preferred entry point when embedding the service inside a GUI.
    /// The GUI can read state via `handle.state`, toggle calibration via
    /// `handle.calibration_mode`, and shut down via `handle.shutdown_tx`.
    pub fn spawn(self, shutdown_tx: broadcast::Sender<()>) -> ServiceHandle {
        let state = Arc::clone(&self.shared_state);
        let calibration_flag = Arc::new(AtomicBool::new(false));
        let calibration_flag_clone = Arc::clone(&calibration_flag);

        // Channel for forwarding live samples to the calibration panel.
        // Bounded to 256 to avoid unbounded growth if the panel falls behind.
        let (cal_sample_tx, cal_sample_rx) = mpsc::channel::<Sample>(256);

        // Channel for stream management commands from the GUI.
        let (device_cmd_tx, device_cmd_rx) = mpsc::channel::<DeviceCommand>(16);

        // Broadcast channels for live data visualization in the hub.
        // These fan-out to multiple widget subscribers.
        let (sample_broadcast_tx, sample_broadcast_rx) = broadcast::channel::<Sample>(512);
        let (feature_broadcast_tx, feature_broadcast_rx) = broadcast::channel::<FeatureVector>(128);
        let (action_broadcast_tx, action_broadcast_rx) = broadcast::channel::<Action>(128);

        let join_handle = tokio::spawn(async move {
            self.run_inner(
                Some(calibration_flag_clone),
                Some(cal_sample_tx),
                Some(device_cmd_rx),
                Some(sample_broadcast_tx),
                Some(feature_broadcast_tx),
                Some(action_broadcast_tx),
            )
            .await
        });

        ServiceHandle {
            state,
            shutdown_tx,
            join_handle,
            calibration_sample_rx: cal_sample_rx,
            calibration_mode: calibration_flag,
            device_command_tx: device_cmd_tx,
            sample_broadcast_rx,
            feature_broadcast_rx,
            action_broadcast_rx,
        }
    }

    /// Runs the service until shutdown is requested (blocking).
    ///
    /// This is the entry point for the standalone headless binary.
    pub async fn run(self) -> Result<()> {
        self.run_inner(None, None, None, None, None, None).await
    }

    /// Internal run loop shared by both `run()` and `spawn()`.
    async fn run_inner(
        mut self,
        calibration_flag: Option<Arc<AtomicBool>>,
        calibration_sample_tx: Option<mpsc::Sender<Sample>>,
        device_command_rx: Option<mpsc::Receiver<DeviceCommand>>,
        sample_broadcast_tx: Option<broadcast::Sender<Sample>>,
        feature_broadcast_tx: Option<broadcast::Sender<FeatureVector>>,
        action_broadcast_tx: Option<broadcast::Sender<Action>>,
    ) -> Result<()> {
        tracing::info!("Starting service tasks");

        // Create channels for inter-task communication.
        // These channels form the "nervous system" of the service.

        // Samples flow from device task to signal task
        let (sample_tx, sample_rx) = mpsc::channel(256);

        // Features flow from signal task to IPC (and on to Python)
        let (feature_tx, feature_rx) = mpsc::channel(64);

        // Actions flow from IPC (from Python) to action task
        let (action_tx, action_rx) = mpsc::channel(64);

        // ErrP results flow from IPC back to signal task (for online learning coordination)
        let (errp_tx, errp_rx) = mpsc::channel(64);

        // Clone shared state for each task
        let state_device = Arc::clone(&self.shared_state);
        let state_signal = Arc::clone(&self.shared_state);
        let state_action = Arc::clone(&self.shared_state);

        // Clone shutdown receiver for each task (broadcast channels support multiple receivers)
        let shutdown_device = self.shutdown_rx.resubscribe();
        let shutdown_signal = self.shutdown_rx.resubscribe();
        let shutdown_ipc = self.shutdown_rx.resubscribe();
        let shutdown_action = self.shutdown_rx.resubscribe();

        // Spawn the device task. This connects to the EEG device and streams
        // samples into the sample channel.
        let device_config = self.config.device.clone();
        let cal_tx_for_device = calibration_sample_tx.clone();
        let cal_flag_for_device = calibration_flag.as_ref().map(Arc::clone);
        let device_handle = tokio::spawn(async move {
            tracing::info!("Device task starting");
            let task = DeviceTask::new(
                device_config,
                sample_tx,
                state_device,
                cal_tx_for_device,
                cal_flag_for_device,
                device_command_rx,
            );
            task.run(shutdown_device).await
        });

        // Spawn the signal processing task. This reads samples, applies filters,
        // extracts features, and sends them to the IPC channel.
        let signal_config = self.config.signal.clone();
        let signal_handle = tokio::spawn(async move {
            tracing::info!("Signal task starting");
            let task = SignalTask::new(
                signal_config,
                sample_rx,
                feature_tx,
                errp_rx,
                state_signal,
                sample_broadcast_tx,
                feature_broadcast_tx,
            );
            task.run(shutdown_signal).await
        });

        // Spawn the IPC task. This manages communication with the Python ML
        // process, sending features and receiving actions/ErrP results.
        let ipc_config = self.config.service.clone();
        let ipc_handle = tokio::spawn(async move {
            tracing::info!("IPC task starting");
            let task = IpcTask::new(ipc_config, feature_rx, action_tx, errp_tx);
            task.run(shutdown_ipc).await
        });

        // Spawn the action task. This takes decoded actions and emits them
        // as HID events (mouse movements, clicks, keystrokes).
        let action_config = self.config.action.clone();
        let cal_flag_for_action = calibration_flag.as_ref().map(Arc::clone);
        let action_handle = tokio::spawn(async move {
            tracing::info!("Action task starting");
            let task = ActionTask::new(
                action_config,
                action_rx,
                state_action,
                cal_flag_for_action,
                action_broadcast_tx,
            );
            task.run(shutdown_action).await
        });

        // Mark the service as active and clear any previous task error
        {
            let mut state = self.shared_state.write().await;
            state.active = true;
            state.started_at = Some(std::time::Instant::now());
            state.task_error = None;
        }

        tracing::info!("All tasks started, service is active");

        // Wait for shutdown signal or task failure.
        // tokio::select! lets us wait for multiple futures simultaneously.
        // If a task fails, we capture the error so the GUI can display it.
        let mut task_failure: Option<(String, String)> = None;

        tokio::select! {
            // Shutdown signal received (user-initiated, no error)
            _ = self.shutdown_rx.recv() => {
                tracing::info!("Shutdown signal received, stopping tasks");
            }

            // Device task finished (either error or clean shutdown)
            result = device_handle => {
                match result {
                    Ok(Ok(())) => tracing::info!("Device task completed"),
                    Ok(Err(e)) => {
                        tracing::error!("Device task failed: {}", e);
                        task_failure = Some(("device".into(), e.to_string()));
                    }
                    Err(e) => {
                        tracing::error!("Device task panicked: {}", e);
                        task_failure = Some(("device".into(), e.to_string()));
                    }
                }
            }

            // Signal task finished
            result = signal_handle => {
                match result {
                    Ok(Ok(())) => tracing::info!("Signal task completed"),
                    Ok(Err(e)) => {
                        tracing::error!("Signal task failed: {}", e);
                        task_failure = Some(("signal".into(), e.to_string()));
                    }
                    Err(e) => {
                        tracing::error!("Signal task panicked: {}", e);
                        task_failure = Some(("signal".into(), e.to_string()));
                    }
                }
            }

            // IPC task finished
            result = ipc_handle => {
                match result {
                    Ok(Ok(())) => tracing::info!("IPC task completed"),
                    Ok(Err(e)) => {
                        tracing::error!("IPC task failed: {}", e);
                        task_failure = Some(("ipc".into(), e.to_string()));
                    }
                    Err(e) => {
                        tracing::error!("IPC task panicked: {}", e);
                        task_failure = Some(("ipc".into(), e.to_string()));
                    }
                }
            }

            // Action task finished
            result = action_handle => {
                match result {
                    Ok(Ok(())) => tracing::info!("Action task completed"),
                    Ok(Err(e)) => {
                        tracing::error!("Action task failed: {}", e);
                        task_failure = Some(("action".into(), e.to_string()));
                    }
                    Err(e) => {
                        tracing::error!("Action task panicked: {}", e);
                        task_failure = Some(("action".into(), e.to_string()));
                    }
                }
            }
        }

        // Mark service as inactive and store any task error for the GUI
        {
            let mut state = self.shared_state.write().await;
            state.active = false;
            state.task_error = task_failure;
        }

        // In a production implementation, we would wait for all tasks to
        // finish gracefully here. For now, we let them get dropped.

        tracing::info!("Service shutdown complete");
        Ok(())
    }
}
