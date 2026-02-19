//! # NeuroHID Service (Headless)
//!
//! Standalone background runtime host for NeuroHID. It can run in foreground
//! mode for development, and on Windows it also exposes service lifecycle
//! commands (`install`, `start`, `stop`, `status`, `uninstall`).

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use neurohid_core::runtime::{RuntimeBuilder, RuntimeCommand, RuntimeHandle, RuntimeIpcHandle};
use neurohid_ipc::{
    BrokerConfig as RuntimeBrokerConfig, BrokerError as RuntimeBrokerError,
    IpcBroker as RuntimeIpcBroker, IpcConfig as RuntimeIpcConfig,
    IpcConnection as RuntimeIpcConnection, IpcServer as RuntimeIpcServer,
    IpcTransport as RuntimeIpcTransport, send_control_request_once,
};
use neurohid_storage::ProfileStore;
use neurohid_types::{
    ControlRpcRequestV3, ControlRpcResponseV3, IPC_PROTOCOL_V3, IpcChannelV3, IpcEnvelopeV3,
    RuntimeComponentCapabilityV3, RuntimeEventV3, RuntimeEventsSubscribeV3, RuntimeTelemetryV2,
    TrainerStatusV2,
    config::{IpcMode, SystemConfig},
    control::{ControlCommand, ControlRequest, ControlResponse, ControlSnapshot},
    observability::{self as obs, EmitGate, EmitPolicyConfig, ObservabilityComponent},
    observation::{CursorState, Observation, ScreenInfo},
    profile::ProfileId,
};

#[cfg(windows)]
use neurohid_core::service::NeuroHidService;
use tokio::sync::{Mutex, broadcast};

const DEFAULT_WINDOWS_SERVICE_NAME: &str = "NeuroHIDService";
#[cfg(windows)]
const WINDOWS_SERVICE_DISPLAY_NAME: &str = "NeuroHID Service";
#[cfg(windows)]
const WINDOWS_SERVICE_DESCRIPTION: &str =
    "NeuroHID runtime service for biosignal acquisition, decoding, and HID output";
const DAEMON_METADATA_FILE: &str = "daemon.json";
const RUNTIME_EVENTS_REPLAY_MAX_EVENTS: usize = 10_000;
const RUNTIME_EVENTS_REPLAY_RETENTION_US: i64 = 120_000_000;

#[derive(Debug, Default)]
struct ConnectionChurnState {
    accepted: u64,
    active: u64,
    disconnected: u64,
}

/// Windows service lifecycle command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ServiceCommand {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
}

/// Cross-platform detached daemon lifecycle command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum DaemonCommand {
    Start,
    Stop,
    Status,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Cross-platform detached daemon lifecycle commands.
    Daemon {
        #[arg(value_enum)]
        command: DaemonCommand,
    },
}

/// Command-line arguments for the NeuroHID service.
#[derive(Parser, Debug)]
#[command(name = "neurohid-service")]
#[command(about = "NeuroHID - Brain-computer interface headless service")]
struct Args {
    #[command(subcommand)]
    command: Option<CliCommand>,

    /// Path to configuration file (uses default location if not specified)
    #[arg(short, long)]
    config: Option<String>,

    /// Profile to use (uses default profile if not specified)
    #[arg(short, long)]
    profile: Option<String>,

    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    foreground: bool,

    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Import candidate artifacts from a trainer output directory and exit.
    #[arg(long)]
    import_candidate_dir: Option<String>,

    /// Export decrypted training session logs to a plaintext directory and exit.
    #[arg(long)]
    export_session_logs_dir: Option<String>,

    /// Bind a localhost TCP control RPC endpoint on this port.
    ///
    /// Clients exchange framed IPC v3 envelopes on `control.rpc`.
    #[arg(long)]
    control_port: Option<u16>,

    /// Windows service lifecycle command.
    #[arg(long, value_enum)]
    service_command: Option<ServiceCommand>,

    /// Detached daemon lifecycle command (cross-platform).
    #[arg(long, value_enum)]
    daemon_command: Option<DaemonCommand>,

    /// Windows service name for lifecycle operations and service-host dispatch.
    #[arg(long, default_value = DEFAULT_WINDOWS_SERVICE_NAME)]
    service_name: String,

    /// Internal flag used by SCM service entrypoint.
    #[arg(long, hide = true)]
    run_as_service: bool,
}

struct RuntimeContext {
    profile_store: ProfileStore,
    config: SystemConfig,
    profile_id: Option<ProfileId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DaemonMetadata {
    pid: u32,
    ipc_mode: IpcMode,
    ipc_endpoint: String,
    started_at_us: i64,
    profile: Option<String>,
    config_path: Option<String>,
    binary_path: String,
}

#[cfg(windows)]
#[derive(Clone, Debug)]
struct ServiceLaunchConfig {
    service_name: String,
    profile: Option<String>,
    config: Option<String>,
}

#[cfg(windows)]
static SERVICE_LAUNCH_CONFIG: std::sync::OnceLock<ServiceLaunchConfig> = std::sync::OnceLock::new();

#[path = "../tracing_init.rs"]
mod tracing_init;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    init_logging(args.verbose)?;

    if let Some(command) = args.service_command {
        return run_service_command(command, &args);
    }
    if let Some(command) = &args.command {
        match command {
            CliCommand::Daemon { command } => return run_daemon_command(*command, &args).await,
        }
    }
    // Legacy flag form kept for a compile-window transition.
    if let Some(command) = args.daemon_command {
        return run_daemon_command(command, &args).await;
    }

    #[cfg(windows)]
    if args.run_as_service {
        let launch = ServiceLaunchConfig {
            service_name: args.service_name.clone(),
            profile: args.profile.clone(),
            config: args.config.clone(),
        };

        SERVICE_LAUNCH_CONFIG
            .set(launch)
            .map_err(|_| anyhow::anyhow!("Service launch config already initialized"))?;
        return windows_service_host::dispatch(&args.service_name);
    }

    if !args.foreground {
        tracing::warn!(
            "Running in foreground; use `neurohid-service daemon start` for detached background mode"
        );
    }

    tracing::info!("Starting NeuroHID service");

    let runtime = load_runtime_context(args.profile.as_deref(), args.config.as_deref()).await?;
    if handle_artifact_commands(&args, &runtime.profile_store, runtime.profile_id.as_ref()).await? {
        return Ok(());
    }

    run_managed_runtime(runtime, args.control_port).await?;

    tracing::info!("NeuroHID service stopped");
    Ok(())
}

fn init_logging(verbose: bool) -> anyhow::Result<()> {
    let log_level = if verbose { "debug" } else { "info" };
    tracing_init::init_tracing(log_level)
}

async fn load_runtime_context(
    profile_name_override: Option<&str>,
    config_path_override: Option<&str>,
) -> anyhow::Result<RuntimeContext> {
    let (profile_store, config_store) = neurohid_storage::initialize()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize storage: {}", e))?;

    let mut config = if let Some(config_path) = config_path_override {
        let config_path = PathBuf::from(config_path);
        let config_raw = tokio::fs::read_to_string(&config_path).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to read configuration override '{}': {}",
                config_path.display(),
                e
            )
        })?;
        toml::from_str::<SystemConfig>(&config_raw).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse configuration override '{}': {}",
                config_path.display(),
                e
            )
        })?
    } else {
        config_store
            .load()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to load configuration: {}", e))?
    };

    for warning in config.service.apply_legacy_ipc_aliases() {
        tracing::warn!("{warning}");
    }

    tracing::info!("Configuration loaded");

    let profile_id = if let Some(profile_name) = profile_name_override {
        Some(ProfileId::new(profile_name))
    } else {
        let profiles = profile_store
            .list_profiles()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list profiles: {}", e))?;

        if profiles.is_empty() {
            tracing::warn!(
                "No profiles found. Service will run without a profile (stream discovery only)."
            );
            None
        } else {
            Some(profiles[0].id.clone())
        }
    };

    if let Some(ref pid) = profile_id {
        tracing::info!("Using profile: {}", pid);
        match profile_store.get_metadata(pid).await {
            Ok(metadata) => {
                if !metadata.calibration_state.is_ready() {
                    tracing::warn!(
                        "Profile '{}' is not fully calibrated. HID actions will not be emitted \
                         until calibration is complete.",
                        pid
                    );
                }
            }
            Err(error) => {
                tracing::warn!("Failed to load profile metadata for {}: {}", pid, error);
            }
        }
    } else {
        tracing::info!("Running without a profile");
    }

    Ok(RuntimeContext {
        profile_store,
        config,
        profile_id,
    })
}

async fn handle_artifact_commands(
    args: &Args,
    profile_store: &ProfileStore,
    profile_id: Option<&ProfileId>,
) -> anyhow::Result<bool> {
    if let Some(source_dir) = &args.import_candidate_dir {
        if args.export_session_logs_dir.is_some() {
            return Err(anyhow::anyhow!(
                "--import-candidate-dir and --export-session-logs-dir are mutually exclusive"
            ));
        }
        let Some(pid) = profile_id else {
            return Err(anyhow::anyhow!(
                "--import-candidate-dir requires an active profile (--profile ...)"
            ));
        };
        let source_dir = PathBuf::from(source_dir);
        tracing::info!(
            "Importing candidate artifacts from '{}' into profile '{}'",
            source_dir.display(),
            pid
        );
        profile_store
            .import_decoder_candidate_from_dir(pid, &source_dir)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to import candidate artifacts: {}", e))?;
        tracing::info!("Candidate artifacts imported successfully");
        return Ok(true);
    }

    if let Some(output_dir) = &args.export_session_logs_dir {
        let Some(pid) = profile_id else {
            return Err(anyhow::anyhow!(
                "--export-session-logs-dir requires an active profile (--profile ...)"
            ));
        };
        let output_dir = PathBuf::from(output_dir);
        tracing::info!(
            "Exporting training session logs for profile '{}' to '{}'",
            pid,
            output_dir.display()
        );
        let exported = profile_store
            .export_training_session_logs_to_dir(pid, &output_dir)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to export session logs: {}", e))?;
        tracing::info!("Exported {} training session log(s)", exported);
        return Ok(true);
    }

    Ok(false)
}

#[cfg(windows)]
async fn run_core_service(
    config: SystemConfig,
    profile_store: ProfileStore,
    profile_id: Option<ProfileId>,
    shutdown_rx: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let service =
        NeuroHidService::new(config, Some(profile_store), profile_id, shutdown_rx).await?;
    tracing::info!("Service initialized, starting main loop");
    service.run().await?;
    Ok(())
}

async fn run_managed_runtime(
    runtime: RuntimeContext,
    control_port: Option<u16>,
) -> anyhow::Result<()> {
    let service_config = runtime.config.service.clone();
    let control_observability_policy = service_config
        .observability
        .policy_for(ObservabilityComponent::Control);
    let mut builder = RuntimeBuilder::new(runtime.config).with_profile_store(runtime.profile_store);
    if let Some(profile_id) = runtime.profile_id {
        builder = builder.with_profile_id(profile_id);
    }
    let runtime_handle = builder.start().await?;
    let runtime_ipc_handle = runtime_handle.ipc_handle();
    tracing::info!("Managed runtime started");

    if let Some(server_config) = resolve_runtime_ipc_server_config(&service_config, control_port)? {
        tracing::info!(
            transport = ?server_config.transport,
            endpoint = %server_config.endpoint,
            "Starting unified IPC v3 server (control.rpc + runtime.events)"
        );
        tokio::select! {
            result = run_ipc_control_server(
                server_config,
                runtime_ipc_handle,
                control_observability_policy.clone()
            ) => {
                if let Err(error) = result {
                    tracing::warn!("Unified IPC server exited with error: {}", error);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutdown signal received");
            }
            _ = wait_until_runtime_stopped(&runtime_handle) => {}
        }
    } else {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutdown signal received");
            }
            _ = wait_until_runtime_stopped(&runtime_handle) => {}
        }
    }

    if runtime_handle.snapshot().running {
        runtime_handle.command(RuntimeCommand::Stop)?;
    }
    runtime_handle.wait().await?;
    Ok(())
}

async fn wait_until_runtime_stopped(handle: &RuntimeHandle) {
    while handle.snapshot().running {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

fn resolve_runtime_ipc_server_config(
    service_config: &neurohid_types::config::ServiceConfig,
    control_port: Option<u16>,
) -> anyhow::Result<Option<RuntimeIpcConfig>> {
    if let Some(port) = control_port {
        return Ok(Some(RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        }));
    }

    let (transport, endpoint) = match service_config.ipc_mode {
        IpcMode::LocalSocket => (
            RuntimeIpcTransport::LocalSocket,
            service_config.ipc_endpoint.clone(),
        ),
        IpcMode::TcpLoopback => (
            RuntimeIpcTransport::TcpLoopback,
            service_config.ipc_endpoint.clone(),
        ),
    };

    if endpoint.trim().is_empty() {
        return Ok(None);
    }

    validate_local_only_endpoint(transport, &endpoint)?;

    Ok(Some(RuntimeIpcConfig {
        transport,
        endpoint,
        ..RuntimeIpcConfig::default()
    }))
}

fn validate_local_only_endpoint(
    transport: RuntimeIpcTransport,
    endpoint: &str,
) -> anyhow::Result<()> {
    if transport == RuntimeIpcTransport::LocalSocket {
        if endpoint.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "local_socket endpoint must not be empty for IPC server"
            ));
        }
        return Ok(());
    }

    let mut addrs = endpoint.to_socket_addrs().map_err(|error| {
        anyhow::anyhow!("invalid tcp_loopback endpoint '{}': {}", endpoint, error)
    })?;
    let mut resolved_any = false;
    for addr in addrs.by_ref() {
        resolved_any = true;
        if !addr.ip().is_loopback() {
            return Err(anyhow::anyhow!(
                "non-loopback IPC endpoint '{}' is not allowed (resolved {})",
                endpoint,
                addr
            ));
        }
    }
    if !resolved_any {
        return Err(anyhow::anyhow!(
            "tcp_loopback endpoint '{}' did not resolve to any address",
            endpoint
        ));
    }
    Ok(())
}

async fn run_ipc_control_server(
    server_config: RuntimeIpcConfig,
    runtime: RuntimeIpcHandle,
    control_policy: EmitPolicyConfig,
) -> anyhow::Result<()> {
    let server = RuntimeIpcServer::new(server_config)
        .await
        .map_err(|error| anyhow::anyhow!("Failed to start IPC server: {}", error))?;
    let broker = Arc::new(RuntimeIpcBroker::new(RuntimeBrokerConfig::default()));
    {
        let mut runtime_bridge_rx = runtime.subscribe_runtime_bridge_events();
        let bridge_broker = Arc::clone(&broker);
        tokio::spawn(async move {
            loop {
                match runtime_bridge_rx.recv().await {
                    Ok(event) => {
                        let _ = bridge_broker.publish_runtime_event(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        bridge_broker.record_runtime_backpressure_drop(skipped);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
    let control_gate = Arc::new(StdMutex::new(EmitGate::new(control_policy)));
    let runtime_events_state = Arc::new(Mutex::new(RuntimeEventsState::default()));
    let churn_state = Arc::new(Mutex::new(ConnectionChurnState::default()));
    let next_connection_id = AtomicU64::new(1);

    loop {
        let connection = server
            .accept()
            .await
            .map_err(|error| anyhow::anyhow!("IPC accept failed: {}", error))?;
        let connection_id = next_connection_id.fetch_add(1, Ordering::Relaxed);
        broker.record_connection_accepted();
        {
            let mut churn = churn_state.lock().await;
            churn.accepted = churn.accepted.saturating_add(1);
            churn.active = churn.active.saturating_add(1);
            tracing::info!(
                connection_id,
                accepted = churn.accepted,
                active = churn.active,
                disconnected = churn.disconnected,
                "IPC client connected"
            );
        }

        let runtime_for_task = runtime.clone();
        let broker_for_task = Arc::clone(&broker);
        let control_gate_for_task = Arc::clone(&control_gate);
        let runtime_events_state_for_task = Arc::clone(&runtime_events_state);
        let churn_for_task = Arc::clone(&churn_state);
        tokio::spawn(async move {
            let result = handle_ipc_client_connection(
                connection_id,
                connection,
                runtime_for_task,
                Arc::clone(&broker_for_task),
                control_gate_for_task,
                runtime_events_state_for_task,
            )
            .await;
            broker_for_task.record_connection_disconnected();
            let counters = broker_for_task.counters();

            let mut churn = churn_for_task.lock().await;
            churn.active = churn.active.saturating_sub(1);
            churn.disconnected = churn.disconnected.saturating_add(1);
            match result {
                Ok(()) => tracing::info!(
                    connection_id,
                    accepted = churn.accepted,
                    active = churn.active,
                    disconnected = churn.disconnected,
                    replay_hits = counters.replay_hits,
                    replay_misses = counters.replay_misses,
                    control_rejects = counters.control_rejects,
                    trainer_queue_stalls = counters.trainer_queue_stalls,
                    runtime_backpressure_drops = counters.runtime_backpressure_drops,
                    subscriber_lag_events = counters.subscriber_lag_events,
                    "IPC client disconnected"
                ),
                Err(error) => tracing::warn!(
                    connection_id,
                    accepted = churn.accepted,
                    active = churn.active,
                    disconnected = churn.disconnected,
                    replay_hits = counters.replay_hits,
                    replay_misses = counters.replay_misses,
                    control_rejects = counters.control_rejects,
                    trainer_queue_stalls = counters.trainer_queue_stalls,
                    runtime_backpressure_drops = counters.runtime_backpressure_drops,
                    subscriber_lag_events = counters.subscriber_lag_events,
                    "IPC client disconnected with error: {}",
                    error
                ),
            }
        });
    }
}

async fn handle_ipc_client_connection(
    connection_id: u64,
    connection: RuntimeIpcConnection,
    runtime: RuntimeIpcHandle,
    broker: Arc<RuntimeIpcBroker>,
    control_gate: Arc<StdMutex<EmitGate>>,
    runtime_events_state: Arc<Mutex<RuntimeEventsState>>,
) -> anyhow::Result<()> {
    let mut control_response_seq = 0_u64;

    loop {
        let request_envelope = match connection.recv().await {
            Ok(envelope) => envelope,
            Err(error) => {
                tracing::debug!("IPC receive loop terminated: {}", error);
                break;
            }
        };

        if request_envelope.channel == IpcChannelV3::RuntimeEvents
            && request_envelope.msg_type == "subscribe"
        {
            handle_runtime_events_subscription(
                &connection,
                &runtime,
                &broker,
                request_envelope,
                runtime_events_state,
            )
            .await?;
            break;
        }

        if request_envelope.channel == IpcChannelV3::TrainerStream {
            handle_trainer_stream_connection(
                connection_id,
                &connection,
                &runtime,
                &broker,
                request_envelope,
            )
            .await?;
            break;
        }

        let (response_envelope, should_shutdown) = handle_control_request_envelope(
            request_envelope,
            &runtime,
            &control_gate,
            &mut control_response_seq,
        );
        match broker
            .send_control(connection.send(response_envelope))
            .await
        {
            Ok(()) => {}
            Err(RuntimeBrokerError::QueueFull { .. }) => {
                control_response_seq = control_response_seq.saturating_add(1);
                let queue_full_envelope = IpcEnvelopeV3 {
                    v: IPC_PROTOCOL_V3,
                    channel: IpcChannelV3::ControlRpc,
                    msg_type: "error".to_string(),
                    seq: control_response_seq,
                    request_id: None,
                    session_id: Some("runtime-control".to_string()),
                    sent_at_us: neurohid_types::now_micros(),
                    payload: serde_json::json!({
                        "code": "control_queue_full",
                        "message": "control.rpc queue is full; request rejected",
                    }),
                };
                connection
                    .send(queue_full_envelope)
                    .await
                    .map_err(|error| {
                        anyhow::anyhow!(
                            "failed to send control queue-full response envelope: {}",
                            error
                        )
                    })?;
            }
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "failed to send control response through broker: {}",
                    error
                ));
            }
        }
        if should_shutdown {
            tracing::info!(connection_id, "shutdown request processed via control.rpc");
            break;
        }
    }

    Ok(())
}

async fn handle_trainer_stream_connection(
    connection_id: u64,
    connection: &RuntimeIpcConnection,
    runtime: &RuntimeIpcHandle,
    broker: &RuntimeIpcBroker,
    first_envelope: IpcEnvelopeV3,
) -> anyhow::Result<()> {
    if first_envelope.channel != IpcChannelV3::TrainerStream {
        return Err(anyhow::anyhow!(
            "trainer bridge received non-trainer first envelope"
        ));
    }

    let requested_session_id = first_envelope
        .session_id
        .clone()
        .unwrap_or_else(|| format!("trainer-{}", neurohid_types::now_micros()));
    let trainer_guard = match broker.open_trainer_stream(requested_session_id.clone()) {
        Ok(guard) => guard,
        Err(RuntimeBrokerError::TrainerBusy { active_session_id }) => {
            let busy = IpcEnvelopeV3 {
                v: IPC_PROTOCOL_V3,
                channel: IpcChannelV3::TrainerStream,
                msg_type: "error".to_string(),
                seq: first_envelope.seq.saturating_add(1),
                request_id: first_envelope.request_id.clone(),
                session_id: Some(requested_session_id),
                sent_at_us: neurohid_types::now_micros(),
                payload: serde_json::json!({
                    "code": "trainer_busy",
                    "message": "trainer.stream already has an active session",
                    "active_session_id": active_session_id,
                }),
            };
            connection.send(busy).await.map_err(|error| {
                anyhow::anyhow!("failed to send trainer_busy envelope to client: {}", error)
            })?;
            return Ok(());
        }
        Err(error) => {
            return Err(anyhow::anyhow!(
                "failed to open trainer stream in broker: {}",
                error
            ));
        }
    };

    runtime
        .trainer_connected(trainer_guard.session_id().to_string())
        .await
        .map_err(|error| anyhow::anyhow!("failed to notify runtime trainer connect: {}", error))?;
    runtime
        .trainer_send_envelope(first_envelope)
        .await
        .map_err(|error| {
            anyhow::anyhow!("failed to forward initial trainer envelope: {}", error)
        })?;
    tracing::info!(
        connection_id,
        session_id = trainer_guard.session_id(),
        "trainer.stream session started"
    );

    let relay_result: anyhow::Result<()> = loop {
        tokio::select! {
            incoming = connection.recv() => {
                let envelope = match incoming {
                    Ok(envelope) => envelope,
                    Err(_) => break Ok(()),
                };
                if envelope.channel != IpcChannelV3::TrainerStream {
                    break Err(anyhow::anyhow!(
                        "trainer stream received mixed channel {:?}; only trainer.stream is allowed",
                        envelope.channel
                    ));
                }
                runtime.trainer_send_envelope(envelope).await.map_err(|error| {
                    anyhow::anyhow!("failed to forward trainer.stream envelope to runtime: {}", error)
                })?;
            }
            outbound = runtime.recv_trainer_envelope() => {
                let envelope = match outbound {
                    Some(envelope) => envelope,
                    None => break Ok(()),
                };
                broker.send_trainer(connection.send(envelope)).await.map_err(|error| {
                    anyhow::anyhow!("failed to forward trainer.stream envelope to client: {}", error)
                })?;
            }
        }
    };

    let disconnect_result = runtime
        .trainer_disconnected()
        .await
        .map_err(|error| anyhow::anyhow!("failed to notify runtime trainer disconnect: {}", error));
    drop(trainer_guard);
    tracing::info!(connection_id, "trainer.stream session closed");
    relay_result?;
    disconnect_result?;
    Ok(())
}

fn handle_control_request_envelope(
    envelope: IpcEnvelopeV3,
    runtime: &RuntimeIpcHandle,
    control_gate: &StdMutex<EmitGate>,
    response_seq: &mut u64,
) -> (IpcEnvelopeV3, bool) {
    let request_id = envelope.request_id.clone();
    let started = Instant::now();
    *response_seq = response_seq.saturating_add(1);

    if envelope.channel == IpcChannelV3::ControlRpc {
        let request_payload = if envelope.msg_type == "request" {
            envelope
                .decode_payload::<ControlRpcRequestV3>()
                .map_err(|e| format!("invalid control request payload: {}", e))
        } else {
            Err("invalid control envelope channel/msg_type".to_string())
        };

        let (response, should_shutdown) = match request_payload {
            Ok(request_v3) => {
                let request: ControlRequest = request_v3.into();
                let command = control_command_name(&request.command);
                let _request_span = tracing::debug_span!(
                    obs::span::CONTROL_REQUEST,
                    stage = obs::stage::CONTROL,
                    request_id = request_id.as_deref().unwrap_or("none"),
                    command,
                    decision_id = obs::field::UNKNOWN,
                    stream_id = obs::field::UNKNOWN
                )
                .entered();
                if tracing::enabled!(tracing::Level::DEBUG) && gate_allows_debug(control_gate) {
                    tracing::debug!(
                        event = obs::event::CONTROL_REQUEST_RECEIVED,
                        request_id = request_id.as_deref().unwrap_or("none"),
                        decision_id = obs::field::UNKNOWN,
                        stream_id = obs::field::UNKNOWN,
                        command,
                        "Control request received"
                    );
                }
                let should_shutdown = matches!(request.command, ControlCommand::Shutdown);
                let response = runtime.dispatch_control_request(request);
                if tracing::enabled!(tracing::Level::DEBUG) && gate_allows_debug(control_gate) {
                    tracing::debug!(
                        event = obs::event::CONTROL_RESPONSE_SENT,
                        request_id = request_id.as_deref().unwrap_or("none"),
                        decision_id = obs::field::UNKNOWN,
                        stream_id = obs::field::UNKNOWN,
                        command,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "Control request handled"
                    );
                }
                (response, should_shutdown)
            }
            Err(error) => (ControlResponse::error(request_id.clone(), error), false),
        };

        let response_v3 = ControlRpcResponseV3::from(response);
        let envelope = IpcEnvelopeV3::new(
            IpcChannelV3::ControlRpc,
            "response",
            *response_seq,
            request_id,
            Some("runtime-control".to_string()),
            &response_v3,
        )
        .unwrap_or_else(|error| IpcEnvelopeV3 {
            v: IPC_PROTOCOL_V3,
            channel: IpcChannelV3::ControlRpc,
            msg_type: "response".to_string(),
            seq: *response_seq,
            request_id: None,
            session_id: Some("runtime-control".to_string()),
            sent_at_us: neurohid_types::now_micros(),
            payload: serde_json::json!({
                "request_id": null,
                "type": "error",
                "message": format!("failed to encode control response envelope: {}", error),
            }),
        });

        return (envelope, should_shutdown);
    }

    if envelope.channel == IpcChannelV3::RuntimeEvents
        && matches!(envelope.msg_type.as_str(), "poll" | "request")
    {
        let family = envelope
            .payload
            .get("family")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("snapshot");
        let snapshot = ControlSnapshot::from(runtime.snapshot());
        let event = match family {
            "snapshot" => RuntimeEventV3::Snapshot {
                snapshot: snapshot.clone(),
            },
            "trainer_snapshot" => RuntimeEventV3::TrainerSnapshot {
                snapshot: runtime.trainer_snapshot(),
            },
            "trainer_status" => RuntimeEventV3::TrainerStatus {
                status: build_runtime_trainer_status(&snapshot),
            },
            "runtime_telemetry" => RuntimeEventV3::RuntimeTelemetry {
                telemetry: build_runtime_telemetry(&snapshot),
            },
            "capabilities" => build_runtime_capabilities_event(),
            other => RuntimeEventV3::Lifecycle {
                state: "error".to_string(),
                detail: format!("unsupported runtime.events family '{}'", other),
                requested_seq: None,
                replay_window_start_seq: None,
                replay_window_end_seq: None,
            },
        };
        let response = IpcEnvelopeV3::new(
            IpcChannelV3::RuntimeEvents,
            "event",
            *response_seq,
            request_id,
            Some("runtime-events".to_string()),
            &event,
        )
        .unwrap_or_else(|error| IpcEnvelopeV3 {
            v: IPC_PROTOCOL_V3,
            channel: IpcChannelV3::RuntimeEvents,
            msg_type: "error".to_string(),
            seq: *response_seq,
            request_id: None,
            session_id: Some("runtime-events".to_string()),
            sent_at_us: neurohid_types::now_micros(),
            payload: serde_json::json!({
                "message": format!("failed to encode runtime event envelope: {}", error),
            }),
        });
        return (response, false);
    }

    let error = IpcEnvelopeV3 {
        v: IPC_PROTOCOL_V3,
        channel: envelope.channel,
        msg_type: "error".to_string(),
        seq: *response_seq,
        request_id,
        session_id: Some("runtime-control".to_string()),
        sent_at_us: neurohid_types::now_micros(),
        payload: serde_json::json!({
            "message": "unsupported channel/msg_type",
        }),
    };
    (error, false)
}

fn gate_allows_debug(control_gate: &StdMutex<EmitGate>) -> bool {
    match control_gate.lock() {
        Ok(mut gate) => gate.allow_debug(),
        Err(poisoned) => poisoned.into_inner().allow_debug(),
    }
}

#[derive(Debug, Clone)]
struct RuntimeEventsReplayItem {
    seq: u64,
    sent_at_us: i64,
    family: &'static str,
    event: RuntimeEventV3,
}

#[derive(Debug, Default, Clone)]
struct RuntimeEventsReplayBuffer {
    entries: VecDeque<RuntimeEventsReplayItem>,
}

impl RuntimeEventsReplayBuffer {
    fn push(&mut self, item: RuntimeEventsReplayItem) {
        self.entries.push_back(item);
        self.prune();
    }

    fn oldest_seq(&self) -> Option<u64> {
        self.entries.front().map(|item| item.seq)
    }

    fn newest_seq(&self) -> Option<u64> {
        self.entries.back().map(|item| item.seq)
    }

    fn iter_from(&self, from_seq: u64) -> impl Iterator<Item = &RuntimeEventsReplayItem> {
        self.entries.iter().filter(move |item| item.seq >= from_seq)
    }

    fn prune(&mut self) {
        while self.entries.len() > RUNTIME_EVENTS_REPLAY_MAX_EVENTS {
            let _ = self.entries.pop_front();
        }

        let now_us = neurohid_types::now_micros();
        while self.entries.front().is_some_and(|item| {
            now_us.saturating_sub(item.sent_at_us) > RUNTIME_EVENTS_REPLAY_RETENTION_US
        }) {
            let _ = self.entries.pop_front();
        }
    }
}

#[derive(Debug, Default, Clone)]
struct RuntimeEventsState {
    next_seq: u64,
    replay: RuntimeEventsReplayBuffer,
}

impl RuntimeEventsState {
    fn allocate_seq(&mut self) -> u64 {
        self.next_seq = self.next_seq.saturating_add(1);
        self.next_seq
    }
}

#[derive(Debug, Clone)]
struct RuntimeEventsFilter {
    families: Option<HashSet<String>>,
}

impl RuntimeEventsFilter {
    fn from_request(request: &RuntimeEventsSubscribeV3) -> Self {
        if request.families.is_empty() {
            return Self { families: None };
        }
        let families = request
            .families
            .iter()
            .map(|family| family.trim().to_ascii_lowercase())
            .filter(|family| !family.is_empty())
            .collect::<HashSet<_>>();
        if families.is_empty() {
            Self { families: None }
        } else {
            Self {
                families: Some(families),
            }
        }
    }

    fn allows(&self, family: &str) -> bool {
        self.families
            .as_ref()
            .is_none_or(|families| families.contains(family))
    }
}

#[derive(Debug, Clone)]
struct RuntimeObservationState {
    cursor: CursorState,
    screen: ScreenInfo,
}

impl Default for RuntimeObservationState {
    fn default() -> Self {
        Self {
            cursor: CursorState::centered(),
            screen: ScreenInfo {
                width: 1_920,
                height: 1_080,
                active_monitor: 0,
                monitor_count: 1,
            },
        }
    }
}

impl RuntimeObservationState {
    fn update_from_action(&mut self, action: &neurohid_types::Action) {
        if let Some(mouse) = &action.mouse {
            if let Some(movement) = &mouse.movement {
                self.cursor.velocity_x = movement.dx;
                self.cursor.velocity_y = movement.dy;
                self.cursor.x = (self.cursor.x + movement.dx).clamp(0.0, 1.0);
                self.cursor.y = (self.cursor.y + movement.dy).clamp(0.0, 1.0);
            }
            for button in &mouse.buttons {
                self.cursor.button_held = button.pressed;
            }
        } else {
            self.cursor.velocity_x = 0.0;
            self.cursor.velocity_y = 0.0;
        }
    }

    fn observation_from_feature(&self, feature: &neurohid_types::FeatureVector) -> Observation {
        Observation {
            timestamp: feature.timestamp,
            signal_features: feature.clone(),
            cursor: self.cursor,
            screen: self.screen.clone(),
            enhanced: None,
        }
    }
}

async fn handle_runtime_events_subscription(
    connection: &RuntimeIpcConnection,
    runtime: &RuntimeIpcHandle,
    broker: &RuntimeIpcBroker,
    envelope: IpcEnvelopeV3,
    runtime_events_state: Arc<Mutex<RuntimeEventsState>>,
) -> anyhow::Result<()> {
    let request = serde_json::from_value::<RuntimeEventsSubscribeV3>(envelope.payload.clone())
        .unwrap_or_default();
    let filter = RuntimeEventsFilter::from_request(&request);
    let request_id = envelope.request_id.clone();
    let session_id = envelope
        .session_id
        .clone()
        .unwrap_or_else(|| "runtime-events".to_string());
    {
        let mut state = runtime_events_state.lock().await;
        state.replay.prune();
    }

    let mut emitted = 0_u64;
    let max_events = request.max_events.unwrap_or(u64::MAX);
    let max_duration = request.max_duration_ms.map(Duration::from_millis);
    let sample_every = request.sample_every.max(1);
    let snapshot_interval_ms = request.snapshot_interval_ms.max(100);
    let started = Instant::now();

    let mut sample_rx = runtime.subscribe_samples();
    let mut feature_rx = runtime.subscribe_features();
    let mut action_rx = runtime.subscribe_actions();
    let mut marker_rx = runtime.subscribe_markers();
    let mut broker_event_rx = broker.subscribe_runtime_events();
    let mut snapshot_tick = tokio::time::interval(Duration::from_millis(snapshot_interval_ms));
    snapshot_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut observation_state = RuntimeObservationState::default();
    let mut sampled_frames = 0_u64;

    if let Some(resume_from_seq) = request.resume_from_seq {
        let (oldest_seq, newest_seq, replay_items) = {
            let state = runtime_events_state.lock().await;
            let oldest_seq = state.replay.oldest_seq();
            let newest_seq = state.replay.newest_seq();
            let replay_items = oldest_seq
                .zip(newest_seq)
                .is_some_and(|(oldest, newest)| {
                    resume_from_seq >= oldest && resume_from_seq <= newest.saturating_add(1)
                })
                .then(|| {
                    state
                        .replay
                        .iter_from(resume_from_seq)
                        .cloned()
                        .collect::<Vec<_>>()
                });
            (oldest_seq, newest_seq, replay_items)
        };
        let replay_hit = oldest_seq.zip(newest_seq).is_some_and(|(oldest, newest)| {
            resume_from_seq >= oldest && resume_from_seq <= newest.saturating_add(1)
        });

        if replay_hit {
            broker.record_replay_hit();
            for item in replay_items.unwrap_or_default() {
                if emitted >= max_events {
                    break;
                }
                emit_runtime_event_replay(
                    connection,
                    &request_id,
                    &session_id,
                    &item,
                    &filter,
                    &mut emitted,
                )
                .await?;
            }
            emit_runtime_event(
                connection,
                &runtime_events_state,
                &request_id,
                &session_id,
                RuntimeEventV3::Lifecycle {
                    state: "replay_resumed".to_string(),
                    detail: format!("resumed from seq {}", resume_from_seq),
                    requested_seq: Some(resume_from_seq),
                    replay_window_start_seq: oldest_seq,
                    replay_window_end_seq: newest_seq,
                },
                &filter,
                &mut emitted,
            )
            .await?;
        } else {
            broker.record_replay_miss();
            let detail = match (oldest_seq, newest_seq) {
                (Some(oldest), Some(newest)) => {
                    format!(
                        "requested seq {} outside replay window {}..={}",
                        resume_from_seq, oldest, newest
                    )
                }
                _ => format!(
                    "requested seq {} but replay buffer is empty",
                    resume_from_seq
                ),
            };
            emit_runtime_event(
                connection,
                &runtime_events_state,
                &request_id,
                &session_id,
                RuntimeEventV3::Lifecycle {
                    state: "replay_miss".to_string(),
                    detail,
                    requested_seq: Some(resume_from_seq),
                    replay_window_start_seq: oldest_seq,
                    replay_window_end_seq: newest_seq,
                },
                &filter,
                &mut emitted,
            )
            .await?;
        }
    }

    if request.include_capabilities {
        emit_runtime_event(
            connection,
            &runtime_events_state,
            &request_id,
            &session_id,
            build_runtime_capabilities_event(),
            &filter,
            &mut emitted,
        )
        .await?;
    }

    if request.include_snapshot {
        let snapshot = ControlSnapshot::from(runtime.snapshot());
        emit_runtime_event(
            connection,
            &runtime_events_state,
            &request_id,
            &session_id,
            RuntimeEventV3::Snapshot {
                snapshot: snapshot.clone(),
            },
            &filter,
            &mut emitted,
        )
        .await?;
        emit_runtime_event(
            connection,
            &runtime_events_state,
            &request_id,
            &session_id,
            RuntimeEventV3::TrainerSnapshot {
                snapshot: runtime.trainer_snapshot(),
            },
            &filter,
            &mut emitted,
        )
        .await?;
    }

    while emitted < max_events {
        if max_duration.is_some_and(|duration| started.elapsed() >= duration) {
            break;
        }

        tokio::select! {
            sample = sample_rx.recv() => {
                match sample {
                    Ok(sample) => {
                        sampled_frames = sampled_frames.saturating_add(1);
                        if sampled_frames.is_multiple_of(sample_every) {
                            emit_runtime_event(
                                connection,
                                &runtime_events_state,
                                &request_id,
                                &session_id,
                                RuntimeEventV3::Sample { sample },
                                &filter,
                                &mut emitted,
                            )
                            .await?;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEventV3::BackpressureDrop {
                                channel: IpcChannelV3::RuntimeEvents,
                                dropped: skipped,
                                reason: "sample stream lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            feature = feature_rx.recv() => {
                match feature {
                    Ok(feature) => {
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEventV3::FeatureFrame {
                                feature: feature.clone(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                        let observation = observation_state.observation_from_feature(&feature);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEventV3::ObservationFrame { observation },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEventV3::BackpressureDrop {
                                channel: IpcChannelV3::RuntimeEvents,
                                dropped: skipped,
                                reason: "feature stream lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            action = action_rx.recv() => {
                match action {
                    Ok(action) => {
                        observation_state.update_from_action(&action);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEventV3::ActionEmitted { action },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEventV3::BackpressureDrop {
                                channel: IpcChannelV3::RuntimeEvents,
                                dropped: skipped,
                                reason: "action stream lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            marker = marker_rx.recv() => {
                match marker {
                    Ok(marker) => {
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEventV3::Marker { marker },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEventV3::BackpressureDrop {
                                channel: IpcChannelV3::RuntimeEvents,
                                dropped: skipped,
                                reason: "marker stream lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            broker_event = broker_event_rx.recv() => {
                match broker_event {
                    Ok(event) => {
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            event,
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEventV3::BackpressureDrop {
                                channel: IpcChannelV3::RuntimeEvents,
                                dropped: skipped,
                                reason: "runtime.events broker subscriber lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = snapshot_tick.tick() => {
                let snapshot = ControlSnapshot::from(runtime.snapshot());
                emit_runtime_event(
                    connection,
                    &runtime_events_state,
                    &request_id,
                    &session_id,
                    RuntimeEventV3::Snapshot {
                        snapshot: snapshot.clone(),
                    },
                    &filter,
                    &mut emitted,
                )
                .await?;
                emit_runtime_event(
                    connection,
                    &runtime_events_state,
                    &request_id,
                    &session_id,
                    RuntimeEventV3::TrainerStatus {
                        status: build_runtime_trainer_status(&snapshot),
                    },
                    &filter,
                    &mut emitted,
                )
                .await?;
                emit_runtime_event(
                    connection,
                    &runtime_events_state,
                    &request_id,
                    &session_id,
                    RuntimeEventV3::RuntimeTelemetry {
                        telemetry: build_runtime_telemetry(&snapshot),
                    },
                    &filter,
                    &mut emitted,
                )
                .await?;
            }
        }
    }

    let close_detail = if emitted >= max_events {
        "max_events_reached"
    } else if max_duration.is_some_and(|duration| started.elapsed() >= duration) {
        "max_duration_reached"
    } else {
        "stream_closed"
    };
    emit_runtime_event(
        connection,
        &runtime_events_state,
        &request_id,
        &session_id,
        RuntimeEventV3::Lifecycle {
            state: "subscription_closed".to_string(),
            detail: close_detail.to_string(),
            requested_seq: None,
            replay_window_start_seq: None,
            replay_window_end_seq: None,
        },
        &filter,
        &mut emitted,
    )
    .await?;

    Ok(())
}

fn runtime_event_family(event: &RuntimeEventV3) -> &'static str {
    match event {
        RuntimeEventV3::Snapshot { .. } => "snapshot",
        RuntimeEventV3::TrainerSnapshot { .. } => "trainer_snapshot",
        RuntimeEventV3::TrainerStatus { .. } => "trainer_status",
        RuntimeEventV3::RuntimeTelemetry { .. } => "runtime_telemetry",
        RuntimeEventV3::Sample { .. } => "sample",
        RuntimeEventV3::FeatureFrame { .. } => "feature_frame",
        RuntimeEventV3::ActionEmitted { .. } => "action_emitted",
        RuntimeEventV3::Marker { .. } => "marker",
        RuntimeEventV3::ObservationFrame { .. } => "observation_frame",
        RuntimeEventV3::DecisionEvent { .. } => "decision_event",
        RuntimeEventV3::ErrpWindow { .. } => "errp_window",
        RuntimeEventV3::ErrpResult { .. } => "errp_result",
        RuntimeEventV3::IntegrityIssue { .. } => "integrity_issue",
        RuntimeEventV3::Lifecycle { .. } => "lifecycle",
        RuntimeEventV3::BackpressureDrop { .. } => "backpressure_drop",
        RuntimeEventV3::Capabilities { .. } => "capabilities",
    }
}

async fn emit_runtime_event(
    connection: &RuntimeIpcConnection,
    runtime_events_state: &Arc<Mutex<RuntimeEventsState>>,
    request_id: &Option<String>,
    session_id: &str,
    event: RuntimeEventV3,
    filter: &RuntimeEventsFilter,
    emitted: &mut u64,
) -> anyhow::Result<()> {
    let family = runtime_event_family(&event);
    let sent_at_us = neurohid_types::now_micros();
    let payload = serde_json::to_value(&event)
        .map_err(|error| anyhow::anyhow!("failed to encode runtime event payload: {}", error))?;
    let seq = {
        let mut state = runtime_events_state.lock().await;
        let seq = state.allocate_seq();
        state.replay.push(RuntimeEventsReplayItem {
            seq,
            sent_at_us,
            family,
            event,
        });
        seq
    };

    if family != "lifecycle" && !filter.allows(family) {
        return Ok(());
    }

    let envelope = IpcEnvelopeV3 {
        v: IPC_PROTOCOL_V3,
        channel: IpcChannelV3::RuntimeEvents,
        msg_type: "event".to_string(),
        seq,
        request_id: request_id.clone(),
        session_id: Some(session_id.to_string()),
        sent_at_us,
        payload,
    };
    connection
        .send(envelope)
        .await
        .map_err(|error| anyhow::anyhow!("failed to send runtime.events envelope: {}", error))?;
    *emitted = emitted.saturating_add(1);
    Ok(())
}

async fn emit_runtime_event_replay(
    connection: &RuntimeIpcConnection,
    request_id: &Option<String>,
    session_id: &str,
    item: &RuntimeEventsReplayItem,
    filter: &RuntimeEventsFilter,
    emitted: &mut u64,
) -> anyhow::Result<()> {
    if item.family != "lifecycle" && !filter.allows(item.family) {
        return Ok(());
    }

    let payload = serde_json::to_value(&item.event).map_err(|error| {
        anyhow::anyhow!("failed to encode replay runtime event payload: {}", error)
    })?;
    let envelope = IpcEnvelopeV3 {
        v: IPC_PROTOCOL_V3,
        channel: IpcChannelV3::RuntimeEvents,
        msg_type: "event".to_string(),
        seq: item.seq,
        request_id: request_id.clone(),
        session_id: Some(session_id.to_string()),
        sent_at_us: item.sent_at_us,
        payload,
    };
    connection.send(envelope).await.map_err(|error| {
        anyhow::anyhow!(
            "failed to send replay runtime.events envelope for seq {}: {}",
            item.seq,
            error
        )
    })?;
    *emitted = emitted.saturating_add(1);
    Ok(())
}

fn build_runtime_telemetry(snapshot: &ControlSnapshot) -> RuntimeTelemetryV2 {
    RuntimeTelemetryV2 {
        signal_latency_p95_us: snapshot.signal_latency_p95_us,
        decode_latency_p95_us: snapshot.decode_latency_p95_us,
        action_latency_p95_us: snapshot.action_latency_p95_us,
        decision_queue_depth: 0,
        errp_queue_depth: 0,
        dropped_ml_messages: snapshot.integrity_issue_count,
    }
}

fn build_runtime_trainer_status(snapshot: &ControlSnapshot) -> TrainerStatusV2 {
    TrainerStatusV2 {
        state: if snapshot.ml_bridge_stalled {
            "stalled".to_string()
        } else if snapshot.ml_bridge_connected {
            "training".to_string()
        } else {
            "disconnected".to_string()
        },
        replay_size: snapshot.trainer_replay_size.unwrap_or(0),
        training_step: snapshot.trainer_step.unwrap_or(0),
        policy_loss: snapshot.trainer_policy_loss,
        value_loss: snapshot.trainer_value_loss,
        entropy: snapshot.trainer_entropy,
        last_error: snapshot.trainer_last_error.clone(),
    }
}

fn build_runtime_capabilities_event() -> RuntimeEventV3 {
    let available = |name: &str| RuntimeComponentCapabilityV3 {
        name: name.to_string(),
        available: true,
        unavailable_reason: None,
    };
    RuntimeEventV3::Capabilities {
        observation_schema_version: 1,
        channels: vec![
            IpcChannelV3::ControlRpc,
            IpcChannelV3::TrainerStream,
            IpcChannelV3::RuntimeEvents,
        ],
        components: vec![
            available("sample"),
            available("feature_frame"),
            available("action_emitted"),
            available("marker"),
            available("observation_frame"),
            available("snapshot"),
            available("trainer_status"),
            available("runtime_telemetry"),
            available("decision_event"),
            available("errp_window"),
            available("errp_result"),
            available("integrity_issue"),
            available("resume_from_seq"),
            available("replay_miss"),
            available("sample_every"),
            available("backpressure_drop"),
        ],
    }
}

async fn run_daemon_command(command: DaemonCommand, args: &Args) -> anyhow::Result<()> {
    let runtime = load_runtime_context(args.profile.as_deref(), args.config.as_deref()).await?;
    let daemon_ipc = daemon_ipc_config_from_service(&runtime.config.service, args.control_port)?;
    match command {
        DaemonCommand::Start => daemon_start(args, daemon_ipc).await,
        DaemonCommand::Stop => daemon_stop(daemon_ipc).await,
        DaemonCommand::Status => daemon_status(daemon_ipc).await,
    }
}

fn daemon_ipc_config_from_service(
    service_config: &neurohid_types::config::ServiceConfig,
    control_port_override: Option<u16>,
) -> anyhow::Result<RuntimeIpcConfig> {
    resolve_runtime_ipc_server_config(service_config, control_port_override)?
        .ok_or_else(|| anyhow::anyhow!("service IPC endpoint is empty; cannot run daemon command"))
}

fn daemon_metadata_dir() -> anyhow::Result<PathBuf> {
    let root = neurohid_storage::default_data_dir()
        .ok_or_else(|| anyhow::anyhow!("unable to resolve platform config directory"))?;
    Ok(root)
}

fn daemon_metadata_path() -> anyhow::Result<PathBuf> {
    Ok(daemon_metadata_dir()?.join(DAEMON_METADATA_FILE))
}

fn daemon_lock_path() -> anyhow::Result<PathBuf> {
    Ok(daemon_metadata_dir()?.join("daemon.lock"))
}

fn save_daemon_metadata(metadata: &DaemonMetadata) -> anyhow::Result<()> {
    let metadata_path = daemon_metadata_path()?;
    if let Some(parent) = metadata_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            anyhow::anyhow!(
                "failed to create daemon metadata directory '{}': {}",
                parent.display(),
                error
            )
        })?;
    }

    let payload = serde_json::to_vec_pretty(metadata)
        .map_err(|error| anyhow::anyhow!("failed to encode daemon metadata: {}", error))?;
    let temp_path = metadata_path.with_extension("tmp");
    std::fs::write(&temp_path, payload).map_err(|error| {
        anyhow::anyhow!(
            "failed to write daemon metadata temp file '{}': {}",
            temp_path.display(),
            error
        )
    })?;
    std::fs::rename(&temp_path, &metadata_path).map_err(|error| {
        anyhow::anyhow!(
            "failed to atomically replace daemon metadata '{}': {}",
            metadata_path.display(),
            error
        )
    })?;
    Ok(())
}

fn load_daemon_metadata() -> anyhow::Result<Option<DaemonMetadata>> {
    let metadata_path = daemon_metadata_path()?;
    if !metadata_path.exists() {
        return Ok(None);
    }

    let payload = std::fs::read_to_string(&metadata_path).map_err(|error| {
        anyhow::anyhow!(
            "failed to read daemon metadata '{}': {}",
            metadata_path.display(),
            error
        )
    })?;
    let metadata = serde_json::from_str::<DaemonMetadata>(&payload).map_err(|error| {
        anyhow::anyhow!(
            "failed to parse daemon metadata '{}': {}",
            metadata_path.display(),
            error
        )
    })?;
    Ok(Some(metadata))
}

fn save_daemon_lock(pid: u32) -> anyhow::Result<()> {
    let lock_path = daemon_lock_path()?;
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            anyhow::anyhow!(
                "failed to create daemon lock directory '{}': {}",
                parent.display(),
                error
            )
        })?;
    }
    std::fs::write(&lock_path, format!("{pid}\n")).map_err(|error| {
        anyhow::anyhow!(
            "failed to write daemon lock '{}': {}",
            lock_path.display(),
            error
        )
    })?;
    Ok(())
}

fn remove_daemon_state_files() {
    if let Ok(path) = daemon_metadata_path() {
        let _ = std::fs::remove_file(path);
    }
    if let Ok(path) = daemon_lock_path() {
        let _ = std::fs::remove_file(path);
    }
}

fn daemon_metadata_to_ipc_config(metadata: &DaemonMetadata) -> anyhow::Result<RuntimeIpcConfig> {
    let transport = match metadata.ipc_mode {
        IpcMode::LocalSocket => RuntimeIpcTransport::LocalSocket,
        IpcMode::TcpLoopback => RuntimeIpcTransport::TcpLoopback,
    };
    validate_local_only_endpoint(transport, &metadata.ipc_endpoint)?;
    Ok(RuntimeIpcConfig {
        transport,
        endpoint: metadata.ipc_endpoint.clone(),
        ..RuntimeIpcConfig::default()
    })
}

fn daemon_endpoint_label(config: &RuntimeIpcConfig) -> &str {
    &config.endpoint
}

fn resolve_daemon_target_ipc(default_ipc: &RuntimeIpcConfig) -> RuntimeIpcConfig {
    if let Ok(Some(metadata)) = load_daemon_metadata()
        && let Ok(config) = daemon_metadata_to_ipc_config(&metadata)
    {
        return config;
    }
    default_ipc.clone()
}

async fn daemon_start(args: &Args, default_ipc: RuntimeIpcConfig) -> anyhow::Result<()> {
    let target_ipc = resolve_daemon_target_ipc(&default_ipc);
    if let Ok(snapshot) = request_control_snapshot(&target_ipc).await
        && snapshot.running
    {
        println!(
            "neurohid-service daemon already running at {} (uptime={}s)",
            daemon_endpoint_label(&target_ipc),
            snapshot.uptime_secs
        );
        return Ok(());
    }

    remove_daemon_state_files();

    let exe = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("failed to resolve current executable: {}", e))?;
    let mut child_cmd = Command::new(exe);
    child_cmd.arg("--foreground");
    if let Some(port) = args.control_port {
        child_cmd.arg("--control-port").arg(port.to_string());
    }
    child_cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(config) = &args.config {
        child_cmd.arg("--config").arg(config);
    }
    if let Some(profile) = &args.profile {
        child_cmd.arg("--profile").arg(profile);
    }
    if args.verbose {
        child_cmd.arg("--verbose");
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        child_cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    let child = child_cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn daemon child process: {}", e))?;

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if let Ok(snapshot) = request_control_snapshot(&default_ipc).await
            && snapshot.running
        {
            let metadata = DaemonMetadata {
                pid: child.id(),
                ipc_mode: if default_ipc.transport == RuntimeIpcTransport::LocalSocket {
                    IpcMode::LocalSocket
                } else {
                    IpcMode::TcpLoopback
                },
                ipc_endpoint: default_ipc.endpoint.clone(),
                started_at_us: neurohid_types::now_micros(),
                profile: args.profile.clone(),
                config_path: args.config.clone(),
                binary_path: std::env::current_exe()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| "neurohid-service".to_string()),
            };
            save_daemon_metadata(&metadata)?;
            save_daemon_lock(child.id())?;
            println!(
                "neurohid-service daemon started (pid={}, endpoint={})",
                child.id(),
                daemon_endpoint_label(&default_ipc)
            );
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    remove_daemon_state_files();
    Err(anyhow::anyhow!(
        "daemon process spawned (pid={}) but control endpoint did not become ready at {}",
        child.id(),
        daemon_endpoint_label(&default_ipc)
    ))
}

async fn daemon_stop(default_ipc: RuntimeIpcConfig) -> anyhow::Result<()> {
    let target_ipc = resolve_daemon_target_ipc(&default_ipc);
    let response = match send_control_request(
        &target_ipc,
        ControlRequest::new(ControlCommand::Shutdown),
    )
    .await
    {
        Ok(response) => response,
        Err(error) => {
            remove_daemon_state_files();
            return Err(error);
        }
    };
    match response.payload {
        neurohid_types::control::ControlResponsePayload::Ack
        | neurohid_types::control::ControlResponsePayload::Snapshot { .. } => {
            remove_daemon_state_files();
            println!(
                "neurohid-service daemon stop requested at {}",
                daemon_endpoint_label(&target_ipc)
            );
            Ok(())
        }
        neurohid_types::control::ControlResponsePayload::Error { message } => {
            Err(anyhow::anyhow!("daemon stop request rejected: {}", message))
        }
        neurohid_types::control::ControlResponsePayload::TrainerSnapshot { .. } => Err(
            anyhow::anyhow!("daemon stop request returned unexpected trainer_snapshot payload"),
        ),
    }
}

async fn daemon_status(default_ipc: RuntimeIpcConfig) -> anyhow::Result<()> {
    let target_ipc = resolve_daemon_target_ipc(&default_ipc);
    match request_control_snapshot(&target_ipc).await {
        Ok(snapshot) => {
            println!(
                "status=running endpoint={} uptime_secs={} profile_ready={} ipc_connected={} bridge_connected={} bridge_stalled={}",
                daemon_endpoint_label(&target_ipc),
                snapshot.uptime_secs,
                snapshot.profile_ready,
                snapshot.ipc_connected,
                snapshot.ml_bridge_connected,
                snapshot.ml_bridge_stalled
            );
            Ok(())
        }
        Err(error) => {
            remove_daemon_state_files();
            println!(
                "status=stopped endpoint={} reason={}",
                daemon_endpoint_label(&target_ipc),
                error
            );
            Ok(())
        }
    }
}

async fn request_control_snapshot(
    config: &RuntimeIpcConfig,
) -> anyhow::Result<neurohid_types::control::ControlSnapshot> {
    let response =
        send_control_request(config, ControlRequest::new(ControlCommand::Snapshot)).await?;
    match response.payload {
        neurohid_types::control::ControlResponsePayload::Snapshot { snapshot } => Ok(snapshot),
        neurohid_types::control::ControlResponsePayload::Error { message } => {
            Err(anyhow::anyhow!("snapshot request failed: {}", message))
        }
        _ => Err(anyhow::anyhow!(
            "snapshot request returned unexpected payload variant"
        )),
    }
}

async fn send_control_request(
    config: &RuntimeIpcConfig,
    request: ControlRequest,
) -> anyhow::Result<ControlResponse> {
    send_control_request_once(config.clone(), request, "daemon-cli", 1)
        .await
        .map_err(|error| anyhow::anyhow!("control request failed: {}", error))
}

fn control_command_name(command: &ControlCommand) -> &'static str {
    match command {
        ControlCommand::Snapshot => "snapshot",
        ControlCommand::Shutdown => "shutdown",
        ControlCommand::SetCalibrationMode { .. } => "set_calibration_mode",
        ControlCommand::SetOutputEnabled { .. } => "set_output_enabled",
        ControlCommand::ReloadModel => "reload_model",
        ControlCommand::PromoteCandidateModel => "promote_candidate_model",
        ControlCommand::RescanStreams => "rescan_streams",
        ControlCommand::ConnectStream { .. } => "connect_stream",
        ControlCommand::DisconnectStream { .. } => "disconnect_stream",
        ControlCommand::SetLearningEnabled { .. } => "set_learning_enabled",
        ControlCommand::MlBridgeReconnect => "ml_bridge_reconnect",
        ControlCommand::TrainerSnapshot => "trainer_snapshot",
        ControlCommand::SetFallbackPolicy { .. } => "set_fallback_policy",
        ControlCommand::SetSignalConfig { .. } => "set_signal_config",
    }
}

fn run_service_command(command: ServiceCommand, args: &Args) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows_service_manager::run(command, args)
    }

    #[cfg(not(windows))]
    {
        let _ = command;
        let _ = args;
        Err(anyhow::anyhow!(
            "--service-command is only available on Windows hosts"
        ))
    }
}

#[cfg(windows)]
mod windows_service_manager {
    use std::process::Command;

    use super::{Args, ServiceCommand, WINDOWS_SERVICE_DESCRIPTION, WINDOWS_SERVICE_DISPLAY_NAME};

    pub fn run(command: ServiceCommand, args: &Args) -> anyhow::Result<()> {
        match command {
            ServiceCommand::Install => install(args),
            ServiceCommand::Uninstall => uninstall(args),
            ServiceCommand::Start => execute_and_require_success(&["start", &args.service_name]),
            ServiceCommand::Stop => execute_and_require_success(&["stop", &args.service_name]),
            ServiceCommand::Status => {
                execute_and_require_success(&["query", &args.service_name])?;
                execute_and_require_success(&["qc", &args.service_name])
            }
        }
    }

    fn install(args: &Args) -> anyhow::Result<()> {
        let exe_path = std::env::current_exe()
            .map_err(|e| anyhow::anyhow!("Failed to resolve current executable: {}", e))?;
        let mut bin_path = format!("\"{}\" --run-as-service --foreground", exe_path.display());
        if let Some(profile) = &args.profile {
            bin_path.push_str(&format!(" --profile \"{}\"", profile));
        }
        if let Some(config) = &args.config {
            bin_path.push_str(&format!(" --config \"{}\"", config));
        }

        execute_and_require_success(&[
            "create",
            &args.service_name,
            "binPath=",
            &bin_path,
            "start=",
            "auto",
            "DisplayName=",
            WINDOWS_SERVICE_DISPLAY_NAME,
        ])?;

        execute_and_require_success(&[
            "description",
            &args.service_name,
            WINDOWS_SERVICE_DESCRIPTION,
        ])?;

        println!(
            "Installed Windows service '{}' (binPath: {})",
            args.service_name, bin_path
        );
        Ok(())
    }

    fn uninstall(args: &Args) -> anyhow::Result<()> {
        if let Err(error) = execute_and_require_success(&["stop", &args.service_name]) {
            tracing::warn!(
                "Service '{}' stop before uninstall returned error: {}",
                args.service_name,
                error
            );
        }
        execute_and_require_success(&["delete", &args.service_name])?;
        println!("Uninstalled Windows service '{}'", args.service_name);
        Ok(())
    }

    fn execute_and_require_success(args: &[&str]) -> anyhow::Result<()> {
        let output = Command::new("sc")
            .args(args)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to execute 'sc {}': {}", args.join(" "), e))?;

        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "'sc {}' exited with status {}",
                args.join(" "),
                output.status
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurohid_core::runtime::RuntimeBuilder;
    use neurohid_ipc::IpcClient as RuntimeIpcClient;
    use neurohid_types::{
        config::{DeviceBackend, SystemConfig},
        control::{ControlCommand, ControlRequest, ControlResponsePayload},
    };

    fn replay_item(seq: u64, sent_at_us: i64) -> RuntimeEventsReplayItem {
        RuntimeEventsReplayItem {
            seq,
            sent_at_us,
            family: "lifecycle",
            event: RuntimeEventV3::Lifecycle {
                state: "test".to_string(),
                detail: "test".to_string(),
                requested_seq: None,
                replay_window_start_seq: None,
                replay_window_end_seq: None,
            },
        }
    }

    #[test]
    fn replay_buffer_prunes_oldest_when_over_capacity() {
        let now = neurohid_types::now_micros();
        let mut replay = RuntimeEventsReplayBuffer::default();
        let total = RUNTIME_EVENTS_REPLAY_MAX_EVENTS as u64 + 5;
        for seq in 1..=total {
            replay.push(replay_item(seq, now));
        }

        assert_eq!(replay.entries.len(), RUNTIME_EVENTS_REPLAY_MAX_EVENTS);
        assert_eq!(replay.oldest_seq(), Some(6));
        assert_eq!(replay.newest_seq(), Some(total));
    }

    #[test]
    fn replay_buffer_prunes_entries_outside_retention_window() {
        let now = neurohid_types::now_micros();
        let mut replay = RuntimeEventsReplayBuffer::default();
        replay.push(replay_item(1, now - RUNTIME_EVENTS_REPLAY_RETENTION_US - 1));
        replay.push(replay_item(2, now));

        assert_eq!(replay.oldest_seq(), Some(2));
        assert_eq!(replay.newest_seq(), Some(2));
    }

    fn allocate_test_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral bind should succeed")
            .local_addr()
            .expect("socket address should resolve")
            .port()
    }

    async fn wait_for_runtime_start(runtime: &RuntimeHandle) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if runtime.snapshot().running {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "runtime did not become active in time"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test]
    async fn runtime_events_subscription_does_not_block_control_rpc() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::Mock;
        config.service.ipc_simulation_enabled = true;
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server_task = tokio::spawn(run_ipc_control_server(
            server_config.clone(),
            runtime.ipc_handle(),
            EmitPolicyConfig::default(),
        ));

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut events_client = RuntimeIpcClient::new(server_config.clone());
        events_client
            .connect()
            .await
            .expect("events client should connect");
        let subscribe = IpcEnvelopeV3::new(
            IpcChannelV3::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("test-events".to_string()),
            &serde_json::json!({
                "families": ["snapshot"],
                "include_snapshot": true,
                "include_capabilities": false,
                "snapshot_interval_ms": 250,
                "max_duration_ms": 2_000
            }),
        )
        .expect("subscribe envelope should encode");
        events_client
            .send(subscribe)
            .await
            .expect("events subscribe should send");

        let first_event = tokio::time::timeout(Duration::from_secs(1), events_client.recv())
            .await
            .expect("events stream should produce a message")
            .expect("events receive should succeed");
        assert_eq!(first_event.channel, IpcChannelV3::RuntimeEvents);

        let mut control_client = RuntimeIpcClient::new(server_config);
        control_client
            .connect()
            .await
            .expect("control client should connect");
        let started = Instant::now();
        let response = tokio::time::timeout(
            Duration::from_millis(700),
            control_client.send_control_request(
                ControlRequest::new(ControlCommand::Snapshot),
                "test-control",
                1,
            ),
        )
        .await
        .expect("control request timed out")
        .expect("control request should succeed");
        assert!(
            started.elapsed() < Duration::from_millis(700),
            "control request was blocked by runtime.events stream"
        );
        assert!(matches!(
            response.payload,
            ControlResponsePayload::Snapshot { .. }
        ));

        let _ = control_client.disconnect().await;
        let _ = events_client.disconnect().await;
        server_task.abort();
        let _ = server_task.await;

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[tokio::test]
    async fn trainer_stream_rejects_second_active_session() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::Mock;
        config.service.ipc_simulation_enabled = false;
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server_task = tokio::spawn(run_ipc_control_server(
            server_config.clone(),
            runtime.ipc_handle(),
            EmitPolicyConfig::default(),
        ));
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut first_client = RuntimeIpcClient::new(server_config.clone());
        first_client
            .connect()
            .await
            .expect("first trainer client should connect");
        let hello = IpcEnvelopeV3::new(
            IpcChannelV3::TrainerStream,
            "hello",
            1,
            None,
            Some("trainer-a".to_string()),
            &serde_json::json!({
                "protocol": "neurohid_runtime_ml_v3",
                "role": "trainer",
                "capabilities": [],
                "profile_id": null
            }),
        )
        .expect("trainer hello envelope should encode");
        first_client
            .send(hello)
            .await
            .expect("first trainer hello should send");
        let first_response = tokio::time::timeout(Duration::from_secs(1), first_client.recv())
            .await
            .expect("first trainer should receive bootstrap response")
            .expect("first trainer receive should succeed");
        assert_eq!(first_response.channel, IpcChannelV3::TrainerStream);

        let mut second_client = RuntimeIpcClient::new(server_config);
        second_client
            .connect()
            .await
            .expect("second trainer client should connect");
        let second_hello = IpcEnvelopeV3::new(
            IpcChannelV3::TrainerStream,
            "hello",
            1,
            None,
            Some("trainer-b".to_string()),
            &serde_json::json!({
                "protocol": "neurohid_runtime_ml_v3",
                "role": "trainer",
                "capabilities": [],
                "profile_id": null
            }),
        )
        .expect("second trainer hello envelope should encode");
        second_client
            .send(second_hello)
            .await
            .expect("second trainer hello should send");
        let busy = second_client
            .recv()
            .await
            .expect("second trainer should receive busy error");
        assert_eq!(busy.channel, IpcChannelV3::TrainerStream);
        assert_eq!(busy.msg_type, "error");
        assert_eq!(
            busy.payload.get("code").and_then(serde_json::Value::as_str),
            Some("trainer_busy")
        );

        let _ = first_client.disconnect().await;
        let _ = second_client.disconnect().await;
        server_task.abort();
        let _ = server_task.await;

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }
}

#[cfg(windows)]
mod windows_service_host {
    use std::ffi::OsString;
    use std::sync::mpsc;
    use std::time::Duration;

    use windows_service::define_windows_service;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::service_dispatcher;

    use super::{SERVICE_LAUNCH_CONFIG, load_runtime_context, run_core_service};

    define_windows_service!(ffi_service_main, service_main);

    pub fn dispatch(service_name: &str) -> anyhow::Result<()> {
        service_dispatcher::start(service_name, ffi_service_main).map_err(|e| {
            anyhow::anyhow!(
                "Failed to start Windows service dispatcher '{}': {}",
                service_name,
                e
            )
        })
    }

    fn service_main(_args: Vec<OsString>) {
        if let Err(error) = run_service() {
            tracing::error!("Windows service execution failed: {}", error);
        }
    }

    fn run_service() -> anyhow::Result<()> {
        let launch = SERVICE_LAUNCH_CONFIG
            .get()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Service launch config is missing"))?;

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let status_handle = service_control_handler::register(
            launch.service_name.clone(),
            move |event| match event {
                ServiceControl::Stop => {
                    let _ = stop_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            },
        )
        .map_err(|e| anyhow::anyhow!("Failed to register service control handler: {}", e))?;

        set_status(
            &status_handle,
            ServiceState::StartPending,
            ServiceControlAccept::empty(),
            ServiceExitCode::Win32(0),
            Duration::from_secs(15),
        )?;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to initialize tokio runtime: {}", e))?;

        let context = runtime
            .block_on(load_runtime_context(
                launch.profile.as_deref(),
                launch.config.as_deref(),
            ))
            .map_err(|e| anyhow::anyhow!("Failed to initialize runtime context: {}", e))?;

        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
        let shutdown_rx = shutdown_tx.subscribe();
        let service_task = runtime.spawn(async move {
            run_core_service(
                context.config,
                context.profile_store,
                context.profile_id,
                shutdown_rx,
            )
            .await
        });

        set_status(
            &status_handle,
            ServiceState::Running,
            ServiceControlAccept::STOP,
            ServiceExitCode::Win32(0),
            Duration::from_secs(0),
        )?;

        let _ = stop_rx.recv();
        let _ = shutdown_tx.send(());

        set_status(
            &status_handle,
            ServiceState::StopPending,
            ServiceControlAccept::empty(),
            ServiceExitCode::Win32(0),
            Duration::from_secs(15),
        )?;

        let result = runtime.block_on(async {
            service_task
                .await
                .map_err(|e| anyhow::anyhow!("Service task join failure: {}", e))?
        });

        let exit_code = if result.is_ok() {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::Win32(1)
        };

        set_status(
            &status_handle,
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
            exit_code,
            Duration::from_secs(0),
        )?;

        result
    }

    fn set_status(
        handle: &service_control_handler::ServiceStatusHandle,
        current_state: ServiceState,
        controls_accepted: ServiceControlAccept,
        exit_code: ServiceExitCode,
        wait_hint: Duration,
    ) -> anyhow::Result<()> {
        handle
            .set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state,
                controls_accepted,
                exit_code,
                checkpoint: 0,
                wait_hint,
                process_id: None,
            })
            .map_err(|e| anyhow::anyhow!("Failed to update Windows service status: {}", e))
    }
}
