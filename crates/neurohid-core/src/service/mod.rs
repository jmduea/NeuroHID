//! # NeuroHID Service
//!
//! This module contains the main service struct that orchestrates all the
//! concurrent tasks. Think of it as the "conductor" of an orchestra - it doesn't
//! play any instruments itself, but it makes sure everyone starts at the right
//! time, stays in sync, and stops together gracefully.

pub mod handle;
pub mod state;

pub use handle::ServiceHandle;
pub use state::ServiceState;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

use neurohid_ipc::{IpcEnvelope, RuntimeEvent};
use neurohid_storage::ProfileStore;
use neurohid_types::{
    action::Action,
    config::{DecoderConfig, LatencyAlertConfig, SignalConfig, SystemConfig},
    control::RuntimeModeState,
    error::Result,
    event::StreamMarker,
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

/// Commands sent from the hub to the SignalTask for live reconfiguration.
#[derive(Debug, Clone)]
pub enum SignalCommand {
    /// Replace the active signal-processing configuration at runtime.
    UpdateConfig(SignalConfig),
}

/// Runtime integrity stages tracked in the shared health rollup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityStage {
    Device,
    Signal,
    Decoder,
    Action,
    Ipc,
}


/// Commands sent from the hub/runtime to the DecoderTask.
#[derive(Debug, Clone)]
pub enum DecoderCommand {
    /// Reload model artifacts for the currently active profile.
    ReloadModel,
    /// Promote validated candidate artifacts and hot-swap decoder model.
    PromoteCandidateModel,
    /// Switch active profile context and load its decoder model.
    SetActiveProfile { profile_id: Option<ProfileId> },
}

use crate::extension_registry::{ExtensionRegistry, default_extension_paths};
use crate::tasks::{
    ActionTask, DecisionEventRecord, DeviceTask, EpisodeLogRecord, IpcTask,
    LatencyAlertMonitorTask, RecordingRequest, RecordingTask, SessionLoggerTask,
    TrainerIngressEvent, create_decoder, create_outlet, create_signal_preprocessor,
    run_replay_task,
};

/// The main service that coordinates all NeuroHID operations.
///
/// The service follows a "task supervisor" pattern: it spawns several
/// independent tasks that communicate via channels, and monitors them
/// for failures. If any critical task fails, the service initiates
/// a graceful shutdown.
pub struct NeuroHidService {
    /// System configuration
    config: SystemConfig,

    /// Profile storage handle retained for runtime profile operations.
    profile_store: Option<ProfileStore>,

    /// Active profile identifier retained for profile-aware task wiring.
    profile_id: Option<ProfileId>,

    /// Shutdown signal receiver
    shutdown_rx: broadcast::Receiver<()>,

    /// Shared state that tasks can read
    shared_state: Arc<RwLock<ServiceState>>,

    /// When set, run in replay mode: spawn replay source instead of live device.
    replay_session_path: Option<std::path::PathBuf>,
}

async fn resolve_profile_status(
    profile_store: Option<&ProfileStore>,
    profile_id: Option<&ProfileId>,
) -> (Option<String>, bool) {
    let Some(profile_id) = profile_id else {
        return (None, false);
    };
    let Some(store) = profile_store else {
        return (Some(profile_id.to_string()), false);
    };

    match store.get_metadata(profile_id).await {
        Ok(metadata) => (Some(metadata.name), metadata.calibration_state.is_ready()),
        Err(err) => {
            tracing::warn!(
                "Failed to resolve profile metadata for {}: {}",
                profile_id,
                err
            );
            (Some(profile_id.to_string()), false)
        }
    }
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

        let (active_profile_name, profile_ready) =
            resolve_profile_status(profile_store.as_ref(), profile_id.as_ref()).await;
        {
            let mut state = shared_state.write().await;
            state.active_profile_name = active_profile_name;
            state.profile_ready = profile_ready;
            state.output_enabled = config.action.enabled;
            state.fallback_policy = config.service.fallback_policy.clone();
        }

        Ok(Self {
            config,
            profile_store,
            profile_id,
            shutdown_rx,
            shared_state,
            replay_session_path: None,
        })
    }

    /// Run in replay mode: use a session folder as the sample source instead of a live device.
    pub fn with_replay_path(mut self, path: std::path::PathBuf) -> Self {
        self.replay_session_path = Some(path);
        self
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
        let output_flag = Arc::new(AtomicBool::new(self.config.action.enabled));
        let output_flag_clone = Arc::clone(&output_flag);

        // Channel for forwarding live samples to the calibration panel.
        // Bounded to 256 to avoid unbounded growth if the panel falls behind.
        let (cal_sample_tx, cal_sample_rx) = mpsc::channel::<Sample>(256);

        // Channel for stream management commands from the GUI.
        let (device_cmd_tx, device_cmd_rx) = mpsc::channel::<DeviceCommand>(16);
        // Channel for runtime signal reconfiguration from the GUI.
        let (signal_cmd_tx, signal_cmd_rx) = mpsc::channel::<SignalCommand>(16);
        // Channel for runtime decoder control commands from GUI/runtime API.
        let (decoder_cmd_tx, decoder_cmd_rx) = mpsc::channel::<DecoderCommand>(16);
        let decoder_cmd_tx_for_run_inner = decoder_cmd_tx.clone();
        // Channel for recording start/stop (request + oneshot reply).
        let (recording_cmd_tx, recording_cmd_rx) = mpsc::channel::<RecordingRequest>(16);

        // In-process trainer bridge channels (transport owned by service binary).
        let (trainer_ingress_tx, trainer_ingress_rx) = mpsc::channel::<TrainerIngressEvent>(1024);
        let (trainer_egress_tx, trainer_egress_rx) = mpsc::channel::<IpcEnvelope>(1024);

        // Broadcast channels for live data visualization in the hub.
        // These fan-out to multiple widget subscribers.
        let (sample_broadcast_tx, sample_broadcast_rx) = broadcast::channel::<Sample>(512);
        let (feature_broadcast_tx, feature_broadcast_rx) = broadcast::channel::<FeatureVector>(128);
        let (action_broadcast_tx, action_broadcast_rx) = broadcast::channel::<Action>(128);
        let (marker_broadcast_tx, marker_broadcast_rx) = broadcast::channel::<StreamMarker>(256);
        let (runtime_event_broadcast_tx, runtime_event_broadcast_rx) =
            broadcast::channel::<RuntimeEvent>(512);
        let sample_broadcast_tx_for_handle = sample_broadcast_tx.clone();
        let feature_broadcast_tx_for_handle = feature_broadcast_tx.clone();
        let action_broadcast_tx_for_handle = action_broadcast_tx.clone();
        let marker_broadcast_tx_for_handle = marker_broadcast_tx.clone();
        let runtime_event_broadcast_tx_for_handle = runtime_event_broadcast_tx.clone();

        let join_handle = tokio::spawn(async move {
            self.run_inner(
                Some(calibration_flag_clone),
                Some(output_flag_clone),
                Some(cal_sample_tx),
                Some(device_cmd_rx),
                Some(signal_cmd_rx),
                Some(decoder_cmd_tx_for_run_inner),
                Some(decoder_cmd_rx),
                Some(sample_broadcast_tx),
                Some(feature_broadcast_tx),
                Some(action_broadcast_tx),
                Some(marker_broadcast_tx),
                Some(trainer_ingress_rx),
                Some(trainer_egress_tx),
                Some(runtime_event_broadcast_tx),
                Some(recording_cmd_rx),
            )
            .await
        });

        ServiceHandle {
            state,
            shutdown_tx,
            join_handle,
            calibration_sample_rx: cal_sample_rx,
            calibration_mode: calibration_flag,
            output_enabled: output_flag,
            device_command_tx: device_cmd_tx,
            sample_broadcast_rx,
            sample_broadcast_tx: sample_broadcast_tx_for_handle,
            feature_broadcast_rx,
            feature_broadcast_tx: feature_broadcast_tx_for_handle,
            action_broadcast_rx,
            action_broadcast_tx: action_broadcast_tx_for_handle,
            marker_broadcast_rx,
            marker_broadcast_tx: marker_broadcast_tx_for_handle,
            recording_command_tx: recording_cmd_tx,
            signal_command_tx: signal_cmd_tx,
            decoder_command_tx: decoder_cmd_tx,
            trainer_ingress_tx,
            trainer_egress_rx: Arc::new(Mutex::new(trainer_egress_rx)),
            runtime_event_broadcast_rx,
            runtime_event_broadcast_tx: runtime_event_broadcast_tx_for_handle,
        }
    }

    /// Runs the service until shutdown is requested (blocking).
    ///
    /// This is the entry point for the standalone headless binary.
    pub async fn run(self) -> Result<()> {
        let (decoder_cmd_tx, decoder_cmd_rx) = mpsc::channel::<DecoderCommand>(16);
        self.run_inner(
            None,
            None,
            None,
            None,
            None,
            Some(decoder_cmd_tx),
            Some(decoder_cmd_rx),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
    }

    /// Internal run loop shared by both `run()` and `spawn()`.
    #[expect(
        clippy::too_many_arguments,
        reason = "Service bootstrap needs explicit channel/flag wiring for embedded and headless modes"
    )]
    async fn run_inner(
        mut self,
        calibration_flag: Option<Arc<AtomicBool>>,
        output_enabled_flag: Option<Arc<AtomicBool>>,
        calibration_sample_tx: Option<mpsc::Sender<Sample>>,
        device_command_rx: Option<mpsc::Receiver<DeviceCommand>>,
        signal_command_rx: Option<mpsc::Receiver<SignalCommand>>,
        decoder_command_tx: Option<mpsc::Sender<DecoderCommand>>,
        decoder_command_rx: Option<mpsc::Receiver<DecoderCommand>>,
        sample_broadcast_tx: Option<broadcast::Sender<Sample>>,
        feature_broadcast_tx: Option<broadcast::Sender<FeatureVector>>,
        action_broadcast_tx: Option<broadcast::Sender<Action>>,
        marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
        trainer_ingress_rx: Option<mpsc::Receiver<TrainerIngressEvent>>,
        trainer_egress_tx: Option<mpsc::Sender<IpcEnvelope>>,
        runtime_event_broadcast_tx: Option<broadcast::Sender<RuntimeEvent>>,
        recording_command_rx: Option<mpsc::Receiver<RecordingRequest>>,
    ) -> Result<()> {
        tracing::info!("Starting service tasks");

        // Create channels for inter-task communication.
        // These channels form the "nervous system" of the service.

        // Samples flow from device task to signal task
        let (sample_tx, sample_rx) = mpsc::channel(256);

        // Features flow from signal task to decoder task.
        let (feature_tx, feature_rx) = mpsc::channel(64);
        // Decision events are forwarded to the ML bridge from decoder output.
        let (decision_event_tx, decision_event_rx) = mpsc::channel::<DecisionEventRecord>(64);

        // Actions flow from decoder task to action task.
        let (action_tx, action_rx) = mpsc::channel(64);

        // Runtime episodes flow from decoder to session logger.
        let session_logging_enabled =
            self.config.storage.session_logging_enabled && self.profile_store.is_some();
        let (episode_log_tx, episode_log_rx) = if session_logging_enabled {
            let (tx, rx) = mpsc::channel::<EpisodeLogRecord>(256);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // ErrP results flow from IPC back to signal task (for online learning coordination)
        let (errp_tx, errp_rx) = mpsc::channel(64);
        // Raw samples are mirrored into IPC for runtime-generated ErrP windows.
        let (errp_sample_tx, errp_sample_rx) = mpsc::channel(1024);

        // Clone shared state for each task
        let state_device = Arc::clone(&self.shared_state);
        let state_signal = Arc::clone(&self.shared_state);
        let state_decoder = Arc::clone(&self.shared_state);
        let state_ipc = Arc::clone(&self.shared_state);
        let state_action = Arc::clone(&self.shared_state);
        let state_latency = Arc::clone(&self.shared_state);

        // Clone shutdown receiver for each task (broadcast channels support multiple receivers)
        let shutdown_device = self.shutdown_rx.resubscribe();
        let shutdown_signal = self.shutdown_rx.resubscribe();
        let shutdown_decoder = self.shutdown_rx.resubscribe();
        let shutdown_ipc = self.shutdown_rx.resubscribe();
        let shutdown_action = self.shutdown_rx.resubscribe();
        let shutdown_outlet = self.shutdown_rx.resubscribe();
        let shutdown_latency_monitor = self.shutdown_rx.resubscribe();

        let marker_tx_for_signal = marker_broadcast_tx.clone();
        let marker_tx_for_ipc = marker_broadcast_tx.clone();
        let marker_tx_for_action = marker_broadcast_tx;
        let observability_config = self.config.service.observability.clone();

        let mut registry = ExtensionRegistry::new(default_extension_paths());
        if registry.scan().is_err() {
            tracing::debug!(
                "Extension registry scan skipped or failed (no extensions path or empty)"
            );
        }
        let device_registry = Arc::new(registry);

        // Optional outlet fan-out task: built-in or extension via create_outlet.
        let outlet_config = self.config.outlet.clone();
        let outlet_sample_rx = sample_broadcast_tx.as_ref().map(|tx| tx.subscribe());
        let outlet_feature_rx = feature_broadcast_tx.as_ref().map(|tx| tx.subscribe());
        let outlet_action_rx = action_broadcast_tx.as_ref().map(|tx| tx.subscribe());
        let outlet_marker_rx = marker_tx_for_action.as_ref().map(|tx| tx.subscribe());
        let outlet_registry = device_registry.clone();
        let mut outlet_handle = if outlet_config.enabled {
            match create_outlet(
                outlet_config,
                outlet_sample_rx,
                outlet_feature_rx,
                outlet_action_rx,
                outlet_marker_rx,
                Some(&*outlet_registry),
            ) {
                Ok((outlet, outlet_name)) => {
                    {
                        let mut state = self.shared_state.write().await;
                        state.outlet_name = Some(outlet_name.clone());
                    }
                    let name_for_log = outlet_name.clone();
                    Some(tokio::spawn(async move {
                        tracing::info!(outlet = %name_for_log, "Outlet task starting");
                        outlet.run(shutdown_outlet).await
                    }))
                }
                Err(e) => {
                    tracing::error!("Outlet creation failed: {} — outlet disabled", e);
                    let mut state = self.shared_state.write().await;
                    if state.task_error.is_none() {
                        state.task_error =
                            Some(("outlet".into(), format!("{} — outlet disabled", e)));
                    }
                    state.outlet_name = None;
                    None
                }
            }
        } else {
            None
        };

        // Optional latency monitor task for sustained p95 threshold alerts.
        let latency_alert_config: LatencyAlertConfig = self.config.service.latency_alert.clone();
        let mut latency_monitor_handle = if latency_alert_config.enabled {
            Some(tokio::spawn(async move {
                tracing::info!("Latency monitor task starting");
                let task = LatencyAlertMonitorTask::new(latency_alert_config, state_latency);
                task.run(shutdown_latency_monitor).await
            }))
        } else {
            None
        };

        // Optional session logger for continuous-learning episode capture.
        let storage_config = self.config.storage.clone();
        let profile_store_for_session_logger = self.profile_store.clone();
        let shutdown_session_logger = self.shutdown_rx.resubscribe();
        let mut session_logger_handle = episode_log_rx.map(|episode_log_rx| {
            tokio::spawn(async move {
                tracing::info!("Session logger task starting");
                let task = SessionLoggerTask::new(
                    storage_config,
                    profile_store_for_session_logger,
                    episode_log_rx,
                );
                task.run(shutdown_session_logger).await
            })
        });

        // Optional recording task: subscribes to sample/action broadcast, writes session folder.
        let recording_config = self.config.recording.clone();
        let system_config_for_recording = self.config.clone();
        let profile_id_for_recording = self.profile_id.clone();
        let profile_store_for_recording = self.profile_store.clone();
        let state_for_recording = Arc::clone(&self.shared_state);
        let shutdown_recording = self.shutdown_rx.resubscribe();
        let mut recording_handle = recording_command_rx.and_then(|recording_cmd_rx| {
            let sample_rx = sample_broadcast_tx.as_ref()?.subscribe();
            let action_rx = action_broadcast_tx.as_ref()?.subscribe();
            Some(tokio::spawn(async move {
                tracing::info!("Recording task starting");
                let task = RecordingTask::new(
                    recording_config,
                    system_config_for_recording,
                    profile_id_for_recording,
                    profile_store_for_recording,
                    state_for_recording,
                    sample_rx,
                    action_rx,
                    recording_cmd_rx,
                );
                task.run(shutdown_recording).await
            }))
        });

        // Spawn the device task or replay source. Replay mode uses a session folder
        // as the sample source instead of a live device.
        let replay_path = self.replay_session_path.clone();
        let device_config = self.config.device.clone();
        let cal_tx_for_device = calibration_sample_tx.clone();
        let cal_flag_for_device = calibration_flag.as_ref().map(Arc::clone);
        let device_observability = observability_config.clone();
        let device_registry_for_device = device_registry.clone();
        let mut device_handle = if let Some(path) = replay_path {
            tokio::spawn(async move {
                let _ = run_replay_task(&path, sample_tx, shutdown_device).await;
            })
        } else {
            tokio::spawn(async move {
                tracing::info!("Device task starting");
                let task = DeviceTask::new(
                    device_config,
                    sample_tx,
                    state_device,
                    cal_tx_for_device,
                    cal_flag_for_device,
                    device_command_rx,
                    Some(device_registry_for_device),
                    device_observability,
                );
                let _ = task.run(shutdown_device).await;
            })
        };

        // Spawn the signal processing task (built-in or extension via create_signal_preprocessor).
        let signal_config = self.config.signal.clone();
        let signal_observability = observability_config.clone();
        let signal_registry = device_registry.clone();
        let (signal_runner, signal_name) = create_signal_preprocessor(
            signal_config,
            sample_rx,
            feature_tx,
            errp_rx,
            state_signal,
            signal_command_rx,
            Some(errp_sample_tx),
            sample_broadcast_tx,
            feature_broadcast_tx,
            marker_tx_for_signal,
            signal_observability,
            Some(&*signal_registry),
        )?;
        {
            let mut state = self.shared_state.write().await;
            state.signal_name = Some(signal_name.clone());
        }
        let mut signal_handle = tokio::spawn(async move {
            tracing::info!(signal = %signal_name, "Signal task starting");
            signal_runner.run(shutdown_signal).await
        });

        // Spawn the decoder task (built-in or extension via create_decoder).
        let decoder_config: DecoderConfig = self.config.decoder.clone();
        let profile_store_for_decoder = self.profile_store.clone();
        let profile_id_for_decoder = self.profile_id.clone();
        let fallback_enabled = self.config.service.fallback_policy.enabled;
        let decoder_observability = observability_config.clone();
        let decoder_registry = device_registry.clone();
        let (decoder_runner, decoder_name) = create_decoder(
            decoder_config,
            feature_rx,
            action_tx,
            state_decoder,
            profile_store_for_decoder,
            profile_id_for_decoder,
            decoder_command_rx,
            Some(decision_event_tx),
            episode_log_tx,
            fallback_enabled,
            decoder_observability,
            Some(&*decoder_registry),
        )?;
        {
            let mut state = self.shared_state.write().await;
            state.decoder_name = Some(decoder_name.clone());
        }
        let mut decoder_handle = tokio::spawn(async move {
            tracing::info!(decoder = %decoder_name, "Decoder task starting");
            decoder_runner.run(shutdown_decoder).await
        });

        // Spawn the IPC task. This remains available for Python-side ErrP and
        // training workflows, but action emission is handled by DecoderTask.
        let ipc_config = self.config.service.clone();
        let profile_store_for_ipc = self.profile_store.clone();
        let decoder_command_tx_for_ipc = decoder_command_tx.clone();
        let ipc_observability = observability_config.clone();
        let trainer_ingress_rx_for_ipc = trainer_ingress_rx.unwrap_or_else(|| {
            let (_tx, rx) = mpsc::channel::<TrainerIngressEvent>(1);
            rx
        });
        let trainer_egress_tx_for_ipc = trainer_egress_tx.unwrap_or_else(|| {
            let (tx, _rx) = mpsc::channel::<IpcEnvelope>(1);
            tx
        });
        let mut ipc_handle = tokio::spawn(async move {
            tracing::info!("IPC task starting");
            let task = IpcTask::new(
                ipc_config,
                decision_event_rx,
                errp_tx,
                errp_sample_rx,
                trainer_ingress_rx_for_ipc,
                trainer_egress_tx_for_ipc,
                self.config.errp.clone(),
                state_ipc,
                marker_tx_for_ipc,
                profile_store_for_ipc,
                decoder_command_tx_for_ipc,
                runtime_event_broadcast_tx,
                ipc_observability,
            );
            task.run(shutdown_ipc).await
        });

        // Spawn the action task. This takes decoded actions and emits them
        // as HID events (mouse movements, clicks, keystrokes).
        let action_config = self.config.action.clone();
        let cal_flag_for_action = calibration_flag.as_ref().map(Arc::clone);
        let output_flag_for_action = output_enabled_flag.as_ref().map(Arc::clone);
        let action_observability = observability_config;
        let mut action_handle = tokio::spawn(async move {
            tracing::info!("Action task starting");
            let task = ActionTask::new(
                action_config,
                action_rx,
                state_action,
                cal_flag_for_action,
                output_flag_for_action,
                action_broadcast_tx,
                marker_tx_for_action,
                action_observability,
            );
            task.run(shutdown_action).await
        });

        // Mark the service as active and clear any previous task error
        {
            let mut state = self.shared_state.write().await;
            state.active = true;
            state.started_at = Some(std::time::Instant::now());
            state.calibration_mode = calibration_flag
                .as_ref()
                .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed));
            state.output_enabled = output_enabled_flag
                .as_ref()
                .map_or(self.config.action.enabled, |flag| {
                    flag.load(std::sync::atomic::Ordering::Relaxed)
                });
            state.decoder_ready = false;
            state.decoder_model_version = None;
            state.decode_latency_last_us = 0;
            state.decode_latency_p95_us = 0;
            state.signal_latency_last_us = 0;
            state.signal_latency_p95_us = 0;
            state.action_latency_last_us = 0;
            state.action_latency_p95_us = 0;
            state.latency_degraded = false;
            state.latency_alert_message = None;
            state.ipc_connected = false;
            state.ipc_simulated = false;
            state.ml_bridge_connected = false;
            state.ml_bridge_stalled = false;
            state.ml_bridge_last_heartbeat_us = None;
            state.ml_protocol_version = Some(3);
            state.trainer_replay_size = None;
            state.trainer_step = None;
            state.trainer_policy_loss = None;
            state.trainer_value_loss = None;
            state.trainer_entropy = None;
            state.trainer_last_error = None;
            state.candidate_promotions_succeeded = 0;
            state.candidate_promotions_rejected = 0;
            state.candidate_last_outcome = None;
            state.runtime_mode_state = RuntimeModeState::Degraded;
            state.enabled_capabilities.clear();
            state.limited_capabilities_message = None;
            state.last_runtime_mode_alert_us = None;
            state.fallback_model_kind = None;
            state.rolling_success_score = 1.0;
            state.fallback_policy = self.config.service.fallback_policy.clone();
            state.task_error = None;
            state.reset_integrity_rollup();
        }

        tracing::info!("All tasks started, service is active");

        // Wait for shutdown signal or CRITICAL task failure.
        //
        // Tasks are classified into two tiers:
        //   - Critical (device, signal, decoder): these form the real-time
        //     acquisition->decode pipeline. If any fails, we must shut down.
        //   - Non-critical (ipc, action): failures here degrade functionality
        //     (e.g. no HID output) but the data pipeline keeps flowing so the
        //     console and visualizations continue to work.
        //
        // We use a loop + select with guards so that non-critical task exits
        // are recorded as warnings without breaking out of the loop.
        let mut task_failure: Option<(String, String)> = None;
        let mut ipc_done = false;
        let mut action_done = false;
        let mut latency_monitor_done = latency_monitor_handle.is_none();
        let mut session_logger_done = session_logger_handle.is_none();
        let mut recording_done = recording_handle.is_none();

        loop {
            // Non-critical optional tasks are tracked via `is_finished()` to avoid
            // guarding `select!` branches with `.expect(...)` on `Option<JoinHandle<_>>`.
            if !latency_monitor_done {
                if let Some(handle) = latency_monitor_handle.as_mut() {
                    if handle.is_finished() {
                        latency_monitor_done = true;
                        match handle.await {
                            Ok(Ok(())) => tracing::info!("Latency monitor task completed"),
                            Ok(Err(e)) => {
                                tracing::warn!("Latency monitor task failed (non-critical): {}", e);
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error = Some((
                                        "latency_monitor".into(),
                                        format!("{} — latency alerts disabled", e),
                                    ));
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Latency monitor task panicked (non-critical): {}",
                                    e
                                );
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error = Some((
                                        "latency_monitor".into(),
                                        format!("panicked: {}", e),
                                    ));
                                }
                            }
                        }
                    }
                } else {
                    latency_monitor_done = true;
                }
            }

            if !session_logger_done {
                if let Some(handle) = session_logger_handle.as_mut() {
                    if handle.is_finished() {
                        session_logger_done = true;
                        match handle.await {
                            Ok(Ok(())) => tracing::info!("Session logger task completed"),
                            Ok(Err(e)) => {
                                tracing::warn!("Session logger task failed (non-critical): {}", e);
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error = Some((
                                        "session_logger".into(),
                                        format!("{} — session logging disabled", e),
                                    ));
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Session logger task panicked (non-critical): {}",
                                    e
                                );
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error =
                                        Some(("session_logger".into(), format!("panicked: {}", e)));
                                }
                            }
                        }
                    }
                } else {
                    session_logger_done = true;
                }
            }

            if !recording_done {
                if let Some(handle) = recording_handle.as_mut() {
                    if handle.is_finished() {
                        recording_done = true;
                        match handle.await {
                            Ok(Ok(())) => tracing::info!("Recording task completed"),
                            Ok(Err(e)) => {
                                tracing::warn!("Recording task failed (non-critical): {}", e);
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error = Some((
                                        "recording".into(),
                                        format!("{} — recording disabled", e),
                                    ));
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Recording task panicked (non-critical): {}", e);
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error =
                                        Some(("recording".into(), format!("panicked: {}", e)));
                                }
                            }
                        }
                    }
                } else {
                    recording_done = true;
                }
            }

            tokio::select! {
                // Shutdown signal received (user-initiated, no error)
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("Shutdown signal received, stopping tasks");
                    break;
                }

                // ── Critical tasks ──────────────────────────────────────

                // Device/replay task finished (either error or clean shutdown)
                result = &mut device_handle => {
                    match result {
                        Ok(()) => tracing::info!("Device/replay task completed"),
                        Err(e) => {
                            tracing::error!("Device/replay task panicked: {}", e);
                            task_failure = Some(("device".into(), e.to_string()));
                        }
                    }
                    break;
                }

                // Signal task finished
                result = &mut signal_handle => {
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
                    break;
                }

                // Decoder task finished
                result = &mut decoder_handle => {
                    match result {
                        Ok(Ok(())) => tracing::info!("Decoder task completed"),
                        Ok(Err(e)) => {
                            tracing::error!("Decoder task failed: {}", e);
                            task_failure = Some(("decoder".into(), e.to_string()));
                        }
                        Err(e) => {
                            tracing::error!("Decoder task panicked: {}", e);
                            task_failure = Some(("decoder".into(), e.to_string()));
                        }
                    }
                    break;
                }

                // ── Non-critical tasks ──────────────────────────────────
                // These do NOT break the loop — the data pipeline continues.

                result = &mut ipc_handle, if !ipc_done => {
                    ipc_done = true;
                    match result {
                        Ok(Ok(())) => tracing::info!("IPC task completed"),
                        Ok(Err(e)) => {
                            tracing::warn!("IPC task failed (non-critical): {}", e);
                            let mut state = self.shared_state.write().await;
                            if state.task_error.is_none() {
                                state.task_error = Some((
                                    "ipc".into(),
                                    format!("{} — IPC disabled", e),
                                ));
                            }
                        }
                        Err(e) => {
                            tracing::warn!("IPC task panicked (non-critical): {}", e);
                            let mut state = self.shared_state.write().await;
                            if state.task_error.is_none() {
                                state.task_error = Some(("ipc".into(), format!("panicked: {}", e)));
                            }
                        }
                    }
                    // Continue the loop — data pipeline is unaffected.
                }

                result = &mut action_handle, if !action_done => {
                    action_done = true;
                    match result {
                        Ok(Ok(())) => tracing::info!("Action task completed"),
                        Ok(Err(e)) => {
                            tracing::warn!("Action task failed (non-critical): {}", e);
                            let mut state = self.shared_state.write().await;
                            if state.task_error.is_none() {
                                state.task_error = Some((
                                    "action".into(),
                                    format!("{} — HID output disabled", e),
                                ));
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Action task panicked (non-critical): {}", e);
                            let mut state = self.shared_state.write().await;
                            if state.task_error.is_none() {
                                state.task_error = Some(("action".into(), format!("panicked: {}", e)));
                            }
                        }
                    }
                    // Continue the loop — data pipeline is unaffected.
                }

            }
        }

        // Abort tasks that are still running.
        decoder_handle.abort();
        if !ipc_done {
            ipc_handle.abort();
        }
        if !action_done {
            action_handle.abort();
        }
        if !latency_monitor_done && let Some(handle) = &mut latency_monitor_handle {
            handle.abort();
        }
        if !session_logger_done && let Some(handle) = &mut session_logger_handle {
            handle.abort();
        }
        if !recording_done {
            if let Some(handle) = &mut recording_handle {
                handle.abort();
            }
        }
        if let Some(handle) = &mut outlet_handle {
            handle.abort();
        }

        // Mark service as inactive, store critical failure, and clean up
        // stale connection flags so the GUI doesn't show "Connected" for
        // streams that are no longer active.
        {
            let mut state = self.shared_state.write().await;
            state.active = false;
            // Critical failure overwrites any prior non-critical warning.
            if task_failure.is_some() {
                state.task_error = task_failure;
            }
            state.device_connected = false;
            state.device_name = None;
            state.outlet_name = None;
            state.signal_name = None;
            state.decoder_name = None;
            state.decoder_ready = false;
            state.decoder_model_version = None;
            state.ipc_connected = false;
            state.ipc_simulated = false;
            state.ml_bridge_connected = false;
            state.ml_bridge_stalled = false;
            state.ml_bridge_last_heartbeat_us = None;
            state.trainer_replay_size = None;
            state.trainer_step = None;
            state.trainer_policy_loss = None;
            state.trainer_value_loss = None;
            state.trainer_entropy = None;
            state.trainer_last_error = None;
            state.candidate_promotions_succeeded = 0;
            state.candidate_promotions_rejected = 0;
            state.candidate_last_outcome = None;
            state.runtime_mode_state = RuntimeModeState::Degraded;
            state.enabled_capabilities.clear();
            state.limited_capabilities_message = None;
            state.last_runtime_mode_alert_us = None;
            state.fallback_model_kind = None;
            state.calibration_mode = false;
            state.decode_latency_last_us = 0;
            state.decode_latency_p95_us = 0;
            state.signal_latency_last_us = 0;
            state.signal_latency_p95_us = 0;
            state.action_latency_last_us = 0;
            state.action_latency_p95_us = 0;
            state.latency_degraded = false;
            state.latency_alert_message = None;
            state.recording_active = false;
            state.current_session_id = None;
            for stream in &mut state.discovered_streams {
                stream.connected = false;
            }
        }

        tracing::info!("Service shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{IntegrityStage, ServiceState};

    #[test]
    fn integrity_rollup_stays_warning_until_escalation_threshold() {
        let mut state = ServiceState::default();
        state.set_signal_integrity_snapshot(3, 3, 1);

        assert_eq!(state.integrity_issue_count, 3);
        assert!(!state.pipeline_integrity_degraded);
        assert!(
            state
                .stage_health_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("signal:degraded(3)"))
        );
    }

    #[test]
    fn integrity_rollup_degrades_when_all_eeg_streams_are_impacted() {
        let mut state = ServiceState::default();
        state.set_signal_integrity_snapshot(2, 2, 2);

        assert!(state.pipeline_integrity_degraded);
        assert_eq!(state.integrity_issue_count, 2);
        assert!(
            state
                .stage_health_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("all_eeg_impacted(2/2)"))
        );
    }

    #[test]
    fn integrity_rollup_degrades_after_repeated_critical_violations() {
        let mut state = ServiceState::default();
        for _ in 0..ServiceState::INTEGRITY_CRITICAL_ISSUES_THRESHOLD {
            state.register_integrity_issue(IntegrityStage::Ipc, false);
        }

        assert!(state.pipeline_integrity_degraded);
        assert_eq!(
            state.integrity_issue_count,
            ServiceState::INTEGRITY_CRITICAL_ISSUES_THRESHOLD
        );
        assert!(
            state
                .stage_health_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("critical_threshold"))
        );
    }
}
