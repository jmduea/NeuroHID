//! # NeuroHID Service (Headless)
//!
//! Standalone background runtime host for NeuroHID. It can run in foreground
//! mode for development, and on Windows it also exposes service lifecycle
//! commands (`install`, `start`, `stop`, `status`, `uninstall`).

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use neurohid_core::observability::EmitGate;
use neurohid_core::recording;
use neurohid_core::runtime::{RuntimeBuilder, RuntimeCommand, RuntimeHandle, RuntimeIpcHandle};
use neurohid_ipc::{
    BrokerConfig as RuntimeBrokerConfig, BrokerError as RuntimeBrokerError,
    IpcBroker as RuntimeIpcBroker, IpcConfig as RuntimeIpcConfig,
    IpcConnection as RuntimeIpcConnection, IpcServer as RuntimeIpcServer,
    IpcTransport as RuntimeIpcTransport, send_control_request_once,
};
use neurohid_ipc::{
    ControlRpcRequest, ControlRpcResponse, IPC_PROTOCOL_VERSION, IpcChannel, IpcEnvelope,
    RuntimeEvent, RuntimeEventsSubscribe,
};
use neurohid_storage::{ConfigStore, DataPaths, ProfileStore};
use neurohid_types::{
    config::{IpcMode, SystemConfig},
    control::{ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload},
    device::DiscoveredStream,
    observability::{self as obs, EmitPolicyConfig, ObservabilityComponent},
    profile::ProfileId,
};

#[cfg(windows)]
use neurohid_core::service::NeuroHidService;
use tokio::sync::{Mutex, broadcast};

const DEFAULT_WINDOWS_SERVICE_NAME: &str = "NeuroHIDService";
/// Default TCP port for the control server when running standalone with no config endpoint.
const DEFAULT_STANDALONE_CONTROL_PORT: u16 = 47384;
/// Default control endpoint string used by CLI when --endpoint is not set.
const DEFAULT_CONTROL_ENDPOINT: &str = "127.0.0.1:47384";
#[cfg(windows)]
const WINDOWS_SERVICE_DISPLAY_NAME: &str = "NeuroHID Service";
#[cfg(windows)]
const WINDOWS_SERVICE_DESCRIPTION: &str =
    "NeuroHID runtime service for biosignal acquisition, decoding, and HID output";
const DAEMON_METADATA_FILE: &str = "daemon.json";

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

/// Control subcommand: send a single request to a running service (client-only; no runtime started).
#[derive(Clone, Debug, Subcommand)]
enum ControlCommandCli {
    /// Fetch runtime status (device connected, decoder loaded, output enabled, integrity).
    Snapshot,
    /// Enable or disable action output (e.g. HID) while the runtime is running.
    SetOutputEnabled {
        /// Whether output should be enabled (true) or disabled (false).
        #[arg(value_parser = clap::builder::BoolishValueParser::new())]
        enabled: bool,
    },
}

/// Device subcommand: list discovered streams or connect by id/criteria (client-only; talks to running service).
#[derive(Clone, Debug, Subcommand)]
enum DeviceCommandCli {
    /// List discovered streams (id, name, type, channels). Human-readable table by default; --json for scriptable output.
    List {
        /// Emit compact one-line JSON to stdout.
        #[arg(long)]
        json: bool,
        /// Suppress progress messages on stderr.
        #[arg(short, long)]
        quiet: bool,
        /// Control endpoint address (e.g. 127.0.0.1:47384).
        #[arg(long, default_value = "127.0.0.1:47384")]
        endpoint: String,
    },
    /// Connect to a stream by id or by criteria (e.g. first LSL stream).
    Connect {
        /// Stream id to connect to (from device list).
        #[arg(long)]
        device_id: Option<String>,
        /// Connect to first stream matching type/criteria (e.g. LSL, EEG). Ignored if --device-id is set.
        #[arg(long)]
        criteria: Option<String>,
        /// Control endpoint address (e.g. 127.0.0.1:47384).
        #[arg(long, default_value = "127.0.0.1:47384")]
        endpoint: String,
    },
}

/// Config subcommand: show or validate system configuration.
#[derive(Clone, Debug, Subcommand)]
enum ConfigCommandCli {
    /// Print current config to stdout (human-readable or --json).
    Show {
        /// Emit JSON to stdout.
        #[arg(long)]
        json: bool,
    },
    /// Load config and exit 0 if valid, non-zero if invalid. With --json, write error object to stderr on failure.
    Validate {
        /// Emit machine-readable error JSON to stderr on failure.
        #[arg(long)]
        json: bool,
    },
}

/// Pipeline subcommand: run (or validate) the signal pipeline.
#[derive(Clone, Debug, Subcommand)]
enum PipelineCommandCli {
    /// Run the pipeline; --dry-run validates config without starting the runtime.
    Run {
        /// Validate config and decoder path only; do not start runtime. Exit 0 if valid.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Record subcommand: start/stop session recording on a running service, or export offline.
#[derive(Clone, Debug, Subcommand)]
enum RecordCommandCli {
    /// Start session recording; uses config default path unless --output-path is set.
    Start {
        /// Override default recording output directory for this session.
        #[arg(long)]
        output_path: Option<std::path::PathBuf>,
        /// Control endpoint address (e.g. 127.0.0.1:47384).
        #[arg(long, default_value = "127.0.0.1:47384")]
        endpoint: String,
    },
    /// Stop the current session recording.
    Stop {
        /// Control endpoint address (e.g. 127.0.0.1:47384).
        #[arg(long, default_value = "127.0.0.1:47384")]
        endpoint: String,
    },
    /// Print whether recording is active and current session id.
    Status {
        /// Control endpoint address (e.g. 127.0.0.1:47384).
        #[arg(long, default_value = "127.0.0.1:47384")]
        endpoint: String,
    },
    /// Export a session folder to XDF 1.0 (offline; no running service required).
    Export {
        /// Path to the session folder (e.g. path/to/session_123).
        session_dir: std::path::PathBuf,
        /// Output .xdf file path.
        #[arg(short, long)]
        output: std::path::PathBuf,
    },
    /// Run the pipeline in replay mode on a session folder (offline decoder run).
    ReplayOffline {
        /// Path to the session folder to replay.
        session_dir: std::path::PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Cross-platform detached daemon lifecycle commands.
    Daemon {
        #[arg(value_enum)]
        command: DaemonCommand,
    },
    /// Send control requests to a running service (snapshot, set-output-enabled). No runtime is started.
    Control {
        #[command(subcommand)]
        command: ControlCommandCli,
        /// Control endpoint address (e.g. 127.0.0.1:47384).
        #[arg(long, default_value = "127.0.0.1:47384")]
        endpoint: String,
    },
    /// List or connect to biosignal streams (talks to a running service).
    Device {
        #[command(subcommand)]
        command: DeviceCommandCli,
    },
    /// Show or validate system configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommandCli,
    },
    /// Run or validate the signal pipeline.
    Pipeline {
        #[command(subcommand)]
        command: PipelineCommandCli,
    },
    /// Start, stop, or query session recording (talks to a running service).
    Record {
        #[command(subcommand)]
        command: RecordCommandCli,
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
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Profile to use (uses default profile if not specified)
    #[arg(short, long, global = true)]
    profile: Option<String>,

    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    foreground: bool,

    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Emit JSON for output/errors where supported (e.g. config validate writes error to stderr).
    #[arg(long, global = true)]
    json: bool,

    /// Suppress progress messages on stderr.
    #[arg(short, long, global = true)]
    quiet: bool,

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

    /// Replay mode: use session folder as sample source instead of live device.
    #[arg(long)]
    replay: Option<std::path::PathBuf>,

    /// Windows service lifecycle command.
    #[arg(long, value_enum)]
    service_command: Option<ServiceCommand>,

    /// Detached daemon lifecycle command (cross-platform).
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
    replay_path: Option<std::path::PathBuf>,
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

#[path = "neurohid-service/runtime_events.rs"]
mod runtime_events;
#[path = "../tracing_init.rs"]
mod tracing_init;

use runtime_events::{
    RuntimeEventsFilter, RuntimeEventsReplayItem, RuntimeEventsState, RuntimeObservationState,
    build_runtime_capabilities_event, build_runtime_telemetry, build_runtime_trainer_status,
    runtime_event_family,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    init_logging(args.verbose)?;

    if let Some(command) = args.service_command {
        return run_service_command(command, &args);
    }
    if let Some(command) = &args.command {
        match command {
            CliCommand::Control { command, endpoint } => {
                return run_control_command(command, endpoint, &args).await;
            }
            CliCommand::Device { command } => {
                return run_device_command(command, &args).await;
            }
            CliCommand::Config { command } => {
                return run_config_command(command, &args).await;
            }
            CliCommand::Pipeline { command } => {
                return run_pipeline_command(command, &args).await;
            }
            CliCommand::Record { command } => {
                return run_record_command(command, &args).await;
            }
            CliCommand::Daemon { command } => return run_daemon_command(*command, &args).await,
        }
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

    let runtime = load_runtime_context(
        args.profile.as_deref(),
        args.config.as_deref(),
        args.replay.clone(),
    )
    .await?;
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
    replay_path: Option<std::path::PathBuf>,
) -> anyhow::Result<RuntimeContext> {
    let (profile_store, config_store) = neurohid_storage::initialize()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize storage: {}", e))?;

    let config = if let Some(config_path) = config_path_override {
        let config_path = std::path::Path::new(config_path);
        config_store
            .load_from_path(config_path)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to load configuration from '{}': {}",
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
        replay_path,
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
    if let Some(path) = runtime.replay_path {
        builder = builder.with_replay_path(path);
    }
    let runtime_handle = builder.start().await?;
    let runtime_ipc_handle = runtime_handle.ipc_handle();
    tracing::info!("Managed runtime started");

    // Use TCP 127.0.0.1:47384 when no port given and config has empty or default endpoint
    // (default "neurohid.control.v3" is a named socket; standalone service and CLI expect TCP).
    let effective_control_port = control_port.or_else(|| {
        let ep = service_config.ipc_endpoint.trim();
        if ep.is_empty() || ep == "neurohid.control.v3" {
            Some(DEFAULT_STANDALONE_CONTROL_PORT)
        } else {
            None
        }
    });
    if let Some(server_config) =
        resolve_runtime_ipc_server_config(&service_config, effective_control_port)?
    {
        tracing::info!(
            transport = ?server_config.transport,
            endpoint = %server_config.endpoint,
            "Starting unified IPC v3 server (control.rpc + runtime.events)"
        );
        if server_config.endpoint.contains("47384") {
            tracing::info!(
                "Control CLI: neurohid device list / neurohid control snapshot (default --endpoint {})",
                server_config.endpoint
            );
        }
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

        if request_envelope.channel == IpcChannel::RuntimeEvents
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

        if request_envelope.channel == IpcChannel::TrainerStream {
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
        )
        .await;
        match broker
            .send_control(connection.send(response_envelope))
            .await
        {
            Ok(()) => {}
            Err(RuntimeBrokerError::QueueFull { .. }) => {
                control_response_seq = control_response_seq.saturating_add(1);
                let queue_full_envelope = IpcEnvelope {
                    v: IPC_PROTOCOL_VERSION,
                    channel: IpcChannel::ControlRpc,
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
    first_envelope: IpcEnvelope,
) -> anyhow::Result<()> {
    if first_envelope.channel != IpcChannel::TrainerStream {
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
            let busy = IpcEnvelope {
                v: IPC_PROTOCOL_VERSION,
                channel: IpcChannel::TrainerStream,
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
                if envelope.channel != IpcChannel::TrainerStream {
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

async fn handle_control_request_envelope(
    envelope: IpcEnvelope,
    runtime: &RuntimeIpcHandle,
    control_gate: &StdMutex<EmitGate>,
    response_seq: &mut u64,
) -> (IpcEnvelope, bool) {
    let request_id = envelope.request_id.clone();
    let started = Instant::now();
    *response_seq = response_seq.saturating_add(1);

    if envelope.channel == IpcChannel::ControlRpc {
        let request_payload = if envelope.msg_type == "request" {
            envelope
                .decode_payload::<ControlRpcRequest>()
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
                drop(_request_span);
                let response = runtime.dispatch_control_request(request).await;
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

        let response_v3 = ControlRpcResponse::from(response);
        let envelope = IpcEnvelope::new(
            IpcChannel::ControlRpc,
            "response",
            *response_seq,
            request_id,
            Some("runtime-control".to_string()),
            &response_v3,
        )
        .unwrap_or_else(|error| IpcEnvelope {
            v: IPC_PROTOCOL_VERSION,
            channel: IpcChannel::ControlRpc,
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

    if envelope.channel == IpcChannel::RuntimeEvents
        && matches!(envelope.msg_type.as_str(), "poll" | "request")
    {
        let family = envelope
            .payload
            .get("family")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("snapshot");
        let snapshot = runtime.snapshot();
        let event = match family {
            "snapshot" => RuntimeEvent::Snapshot {
                snapshot: snapshot.clone(),
            },
            "trainer_snapshot" => RuntimeEvent::TrainerSnapshot {
                snapshot: runtime.trainer_snapshot(),
            },
            "trainer_status" => RuntimeEvent::TrainerStatus {
                status: build_runtime_trainer_status(&snapshot),
            },
            "runtime_telemetry" => RuntimeEvent::RuntimeTelemetry {
                telemetry: build_runtime_telemetry(&snapshot),
            },
            "capabilities" => build_runtime_capabilities_event(&snapshot),
            other => RuntimeEvent::Lifecycle {
                state: "error".to_string(),
                detail: format!("unsupported runtime.events family '{}'", other),
                requested_seq: None,
                replay_window_start_seq: None,
                replay_window_end_seq: None,
            },
        };
        let response = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "event",
            *response_seq,
            request_id,
            Some("runtime-events".to_string()),
            &event,
        )
        .unwrap_or_else(|error| IpcEnvelope {
            v: IPC_PROTOCOL_VERSION,
            channel: IpcChannel::RuntimeEvents,
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

    let error = IpcEnvelope {
        v: IPC_PROTOCOL_VERSION,
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

async fn handle_runtime_events_subscription(
    connection: &RuntimeIpcConnection,
    runtime: &RuntimeIpcHandle,
    broker: &RuntimeIpcBroker,
    envelope: IpcEnvelope,
    runtime_events_state: Arc<Mutex<RuntimeEventsState>>,
) -> anyhow::Result<()> {
    let request = serde_json::from_value::<RuntimeEventsSubscribe>(envelope.payload.clone())
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
                RuntimeEvent::Lifecycle {
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
                RuntimeEvent::Lifecycle {
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
            build_runtime_capabilities_event(&runtime.snapshot()),
            &filter,
            &mut emitted,
        )
        .await?;
    }

    if request.include_snapshot {
        let snapshot = runtime.snapshot();
        emit_runtime_event(
            connection,
            &runtime_events_state,
            &request_id,
            &session_id,
            RuntimeEvent::Snapshot {
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
            RuntimeEvent::TrainerSnapshot {
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
                                RuntimeEvent::Sample { sample },
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
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
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
                            RuntimeEvent::FeatureFrame {
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
                            RuntimeEvent::ObservationFrame { observation },
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
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
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
                            RuntimeEvent::ActionEmitted { action },
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
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
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
                            RuntimeEvent::Marker { marker },
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
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
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
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
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
                let snapshot = runtime.snapshot();
                emit_runtime_event(
                    connection,
                    &runtime_events_state,
                    &request_id,
                    &session_id,
                    RuntimeEvent::Snapshot {
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
                    RuntimeEvent::TrainerStatus {
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
                    RuntimeEvent::RuntimeTelemetry {
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
        RuntimeEvent::Lifecycle {
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

async fn emit_runtime_event(
    connection: &RuntimeIpcConnection,
    runtime_events_state: &Arc<Mutex<RuntimeEventsState>>,
    request_id: &Option<String>,
    session_id: &str,
    event: RuntimeEvent,
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

    let envelope = IpcEnvelope {
        v: IPC_PROTOCOL_VERSION,
        channel: IpcChannel::RuntimeEvents,
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
    let envelope = IpcEnvelope {
        v: IPC_PROTOCOL_VERSION,
        channel: IpcChannel::RuntimeEvents,
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

async fn run_daemon_command(command: DaemonCommand, args: &Args) -> anyhow::Result<()> {
    let runtime = load_runtime_context(
        args.profile.as_deref(),
        args.config.as_deref(),
        args.replay.clone(),
    )
    .await?;
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

/// Resolve control endpoint for client commands (record, control, device). When the default
/// endpoint is used, try daemon metadata first, then config, so that we connect to the
/// same service the user started (daemon or Hub with config).
async fn resolve_control_endpoint_for_client(
    endpoint: &str,
    config_path_override: Option<&String>,
) -> RuntimeIpcConfig {
    if endpoint != DEFAULT_CONTROL_ENDPOINT {
        return RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: endpoint.to_string(),
            ..RuntimeIpcConfig::default()
        };
    }
    let default_ipc = RuntimeIpcConfig {
        transport: RuntimeIpcTransport::TcpLoopback,
        endpoint: DEFAULT_CONTROL_ENDPOINT.to_string(),
        ..RuntimeIpcConfig::default()
    };
    // Prefer daemon metadata (service started via daemon start).
    let resolved = resolve_daemon_target_ipc(&default_ipc);
    if resolved.endpoint != default_ipc.endpoint {
        return resolved;
    }
    // Else try config (e.g. service started by Hub or with --config that sets ipc_endpoint).
    let paths = match DataPaths::new(neurohid_storage::default_data_dir()) {
        Ok(p) => p,
        Err(_) => return default_ipc,
    };
    let store = ConfigStore::new(paths);
    let config = match config_path_override {
        Some(p) => store.load_from_path(std::path::Path::new(p)).await,
        None => store.load().await,
    };
    match config {
        Ok(cfg)
            if cfg.service.ipc_mode == IpcMode::TcpLoopback
                && !cfg.service.ipc_endpoint.trim().is_empty() =>
        {
            if validate_local_only_endpoint(
                RuntimeIpcTransport::TcpLoopback,
                cfg.service.ipc_endpoint.trim(),
            )
            .is_ok()
            {
                return RuntimeIpcConfig {
                    transport: RuntimeIpcTransport::TcpLoopback,
                    endpoint: cfg.service.ipc_endpoint.trim().to_string(),
                    ..RuntimeIpcConfig::default()
                };
            }
        }
        _ => {}
    }
    default_ipc
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
        neurohid_types::control::ControlResponsePayload::TrainerSnapshot { .. }
        | neurohid_types::control::ControlResponsePayload::RecordingStarted { .. }
        | neurohid_types::control::ControlResponsePayload::RecordingStopped { .. } => Err(
            anyhow::anyhow!("daemon stop request returned unexpected payload"),
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
        ControlCommand::StartRecording { .. } => "start_recording",
        ControlCommand::StopRecording => "stop_recording",
    }
}

/// Run record CLI: start/stop/status session recording via control requests to a running service.
async fn run_record_command(command: &RecordCommandCli, args: &Args) -> anyhow::Result<()> {
    match command {
        RecordCommandCli::Start {
            output_path,
            endpoint,
        } => {
            let config = resolve_control_endpoint_for_client(endpoint, args.config.as_ref()).await;
            let response = send_control_request_once(
                config,
                ControlRequest::new(ControlCommand::StartRecording {
                    output_path: output_path.clone(),
                }),
                "record-start",
                1,
            )
            .await
            .map_err(|e| anyhow::anyhow!("control request failed: {}", e))?;
            match response.payload {
                ControlResponsePayload::RecordingStarted {
                    session_id,
                    output_path: path,
                } => {
                    println!("session_id={}", session_id);
                    println!("output_path={}", path);
                    Ok(())
                }
                ControlResponsePayload::Error { message } => {
                    Err(anyhow::anyhow!("start recording failed: {}", message))
                }
                _ => Err(anyhow::anyhow!(
                    "start recording returned unexpected payload"
                )),
            }
        }
        RecordCommandCli::Stop { endpoint } => {
            let config = resolve_control_endpoint_for_client(endpoint, args.config.as_ref()).await;
            let response = send_control_request_once(
                config,
                ControlRequest::new(ControlCommand::StopRecording),
                "record-stop",
                1,
            )
            .await
            .map_err(|e| anyhow::anyhow!("control request failed: {}", e))?;
            match response.payload {
                ControlResponsePayload::RecordingStopped { session_id } => {
                    println!("session_id={}", session_id);
                    Ok(())
                }
                ControlResponsePayload::Error { message } => {
                    Err(anyhow::anyhow!("stop recording failed: {}", message))
                }
                _ => Err(anyhow::anyhow!(
                    "stop recording returned unexpected payload"
                )),
            }
        }
        RecordCommandCli::Status { endpoint } => {
            let config = resolve_control_endpoint_for_client(endpoint, args.config.as_ref()).await;
            let response = send_control_request_once(
                config,
                ControlRequest::new(ControlCommand::Snapshot),
                "record-status",
                1,
            )
            .await
            .map_err(|e| anyhow::anyhow!("control request failed: {}", e))?;
            match response.payload {
                ControlResponsePayload::Snapshot { snapshot } => {
                    println!(
                        "recording_active={} current_session_id={}",
                        snapshot.recording_active,
                        snapshot.current_session_id.as_deref().unwrap_or("")
                    );
                    Ok(())
                }
                ControlResponsePayload::Error { message } => {
                    Err(anyhow::anyhow!("status failed: {}", message))
                }
                _ => Err(anyhow::anyhow!("status returned unexpected payload")),
            }
        }
        RecordCommandCli::Export {
            session_dir,
            output,
        } => {
            recording::export_session_to_xdf(&session_dir, &output)
                .map_err(|e| anyhow::anyhow!("export failed: {}", e))?;
            println!("Exported to {}", output.display());
            Ok(())
        }
        RecordCommandCli::ReplayOffline { session_dir } => {
            run_replay_offline(&session_dir, args).await
        }
    }
}

/// Run the pipeline in replay mode on a session folder (offline); no live device.
async fn run_replay_offline(session_dir: &std::path::Path, args: &Args) -> anyhow::Result<()> {
    let runtime = load_runtime_context(
        args.profile.as_deref(),
        args.config.as_deref(),
        Some(session_dir.to_path_buf()),
    )
    .await?;
    run_managed_runtime(runtime, args.control_port).await
}

/// Run control CLI: send one request to a running service and print result.
async fn run_control_command(
    command: &ControlCommandCli,
    endpoint: &str,
    args: &Args,
) -> anyhow::Result<()> {
    let config = resolve_control_endpoint_for_client(endpoint, args.config.as_ref()).await;

    match command {
        ControlCommandCli::Snapshot => {
            let response = send_control_request_once(
                config,
                ControlRequest::new(ControlCommand::Snapshot),
                "cli",
                1,
            )
            .await
            .map_err(|e| anyhow::anyhow!("control request failed: {}", e))?;
            match response.payload {
                ControlResponsePayload::Snapshot { snapshot } => {
                    println!(
                        "device_connected={} decoder_ready={} output_enabled={} pipeline_integrity_degraded={} integrity_issue_count={}",
                        snapshot.device_connected,
                        snapshot.decoder_ready,
                        snapshot.output_enabled,
                        snapshot.pipeline_integrity_degraded,
                        snapshot.integrity_issue_count
                    );
                    Ok(())
                }
                ControlResponsePayload::Error { message } => {
                    Err(anyhow::anyhow!("snapshot failed: {}", message))
                }
                _ => Err(anyhow::anyhow!(
                    "snapshot request returned unexpected payload"
                )),
            }
        }
        ControlCommandCli::SetOutputEnabled { enabled } => {
            let response = send_control_request_once(
                config,
                ControlRequest::new(ControlCommand::SetOutputEnabled { enabled: *enabled }),
                "cli",
                1,
            )
            .await
            .map_err(|e| anyhow::anyhow!("control request failed: {}", e))?;
            match response.payload {
                ControlResponsePayload::Ack => {
                    println!("output_enabled={}", enabled);
                    Ok(())
                }
                ControlResponsePayload::Error { message } => {
                    Err(anyhow::anyhow!("set_output_enabled failed: {}", message))
                }
                _ => Err(anyhow::anyhow!(
                    "set_output_enabled request returned unexpected payload"
                )),
            }
        }
    }
}

/// Exit codes: 0 success, 1 generic error, 2 not found, 3 config invalid (documented in code).
async fn run_device_command(command: &DeviceCommandCli, args: &Args) -> anyhow::Result<()> {
    match command {
        DeviceCommandCli::List {
            json,
            quiet,
            endpoint,
        } => {
            let config = resolve_control_endpoint_for_client(endpoint, args.config.as_ref()).await;
            run_device_list(&config.endpoint, *json, *quiet).await
        }
        DeviceCommandCli::Connect {
            device_id,
            criteria,
            endpoint,
        } => {
            let config = resolve_control_endpoint_for_client(endpoint, args.config.as_ref()).await;
            run_device_connect(&config.endpoint, device_id.as_deref(), criteria.as_deref()).await
        }
    }
}

/// Machine-readable error for config validate when --json (written to stderr).
#[derive(Serialize)]
struct ConfigErrorJson {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

async fn run_config_command(command: &ConfigCommandCli, args: &Args) -> anyhow::Result<()> {
    let paths = DataPaths::new(neurohid_storage::default_data_dir())
        .map_err(|e| anyhow::anyhow!("config directory: {}", e))?;
    let store = ConfigStore::new(paths);
    let config_path = args.config.as_ref().map(|s| PathBuf::from(s.as_str()));

    match command {
        ConfigCommandCli::Show { json } => {
            let config = match config_path.as_ref() {
                Some(p) => store.load_from_path(p).await,
                None => store.load().await,
            }
            .map_err(|e| anyhow::anyhow!("load config: {}", e))?;
            if *json {
                let out = serde_json::to_string(&config).map_err(|e| anyhow::anyhow!("{}", e))?;
                println!("{}", out);
            } else {
                let out = toml::to_string_pretty(&config).map_err(|e| anyhow::anyhow!("{}", e))?;
                println!("{}", out);
            }
            Ok(())
        }
        ConfigCommandCli::Validate { json } => {
            if let Some(ref p) = config_path
                && !p.exists()
            {
                let msg = format!("config file not found: {}", p.display());
                if *json {
                    let err = ConfigErrorJson {
                        code: 3,
                        message: msg.clone(),
                        details: None,
                    };
                    eprintln!("{}", serde_json::to_string(&err).unwrap_or(msg));
                } else {
                    eprintln!("{}", msg);
                }
                std::process::exit(3);
            }
            let result = match config_path.as_ref() {
                Some(p) => store.load_from_path(p).await,
                None => store.load().await,
            };
            match result {
                Ok(_) => Ok(()),
                Err(e) => {
                    let msg = e.to_string();
                    if *json {
                        let err = ConfigErrorJson {
                            code: 3,
                            message: msg.clone(),
                            details: None,
                        };
                        eprintln!("{}", serde_json::to_string(&err).unwrap_or(msg));
                    } else {
                        eprintln!("{}", msg);
                    }
                    std::process::exit(3);
                }
            }
        }
    }
}

async fn run_pipeline_command(command: &PipelineCommandCli, args: &Args) -> anyhow::Result<()> {
    match command {
        PipelineCommandCli::Run { dry_run } => {
            if !dry_run {
                return Err(anyhow::anyhow!(
                    "pipeline run without --dry-run starts the full runtime; use the default \
                     (no subcommand) to run. For validation only use: pipeline run --dry-run"
                ));
            }
            let paths = DataPaths::new(neurohid_storage::default_data_dir())
                .map_err(|e| anyhow::anyhow!("config directory: {}", e))?;
            let store = ConfigStore::new(paths);
            let config_path = args.config.as_ref().map(|s| PathBuf::from(s.as_str()));
            let _config = match config_path.as_ref() {
                Some(p) => store.load_from_path(p).await,
                None => store.load().await,
            }
            .map_err(|e| anyhow::anyhow!("config invalid: {}", e))?;
            // Exit 0: config is valid. Optional decoder path check can be added later.
            Ok(())
        }
    }
}

fn device_ipc_config(endpoint: &str) -> RuntimeIpcConfig {
    RuntimeIpcConfig {
        transport: RuntimeIpcTransport::TcpLoopback,
        endpoint: endpoint.to_string(),
        ..RuntimeIpcConfig::default()
    }
}

async fn fetch_discovered_streams(
    config: &RuntimeIpcConfig,
) -> anyhow::Result<Vec<DiscoveredStream>> {
    send_control_request_once(
        config.clone(),
        ControlRequest::new(ControlCommand::RescanStreams),
        "cli",
        1,
    )
    .await
    .map_err(|e| anyhow::anyhow!("control request failed: {}", e))?;
    let response = send_control_request_once(
        config.clone(),
        ControlRequest::new(ControlCommand::Snapshot),
        "cli",
        2,
    )
    .await
    .map_err(|e| anyhow::anyhow!("control request failed: {}", e))?;
    match response.payload {
        ControlResponsePayload::Snapshot { snapshot } => Ok(snapshot.discovered_streams),
        ControlResponsePayload::Error { message } => {
            Err(anyhow::anyhow!("snapshot failed: {}", message))
        }
        _ => Err(anyhow::anyhow!("unexpected response payload")),
    }
}

async fn run_device_list(endpoint: &str, json: bool, quiet: bool) -> anyhow::Result<()> {
    let config = device_ipc_config(endpoint);
    if !quiet {
        eprintln!("Listing streams...");
    }
    let streams = fetch_discovered_streams(&config).await?;
    if json {
        let line = serde_json::to_string(&streams).map_err(|e| anyhow::anyhow!("{}", e))?;
        println!("{}", line);
    } else {
        // Human-readable table: id, name, type, channels
        println!("{:<36} {:<24} {:<12} CHANNELS", "ID", "NAME", "TYPE");
        println!("{}", "-".repeat(80));
        for s in &streams {
            println!(
                "{:<36} {:<24} {:<12} {}",
                s.id,
                if s.name.len() > 23 {
                    format!("{}..", &s.name[..21])
                } else {
                    s.name.clone()
                },
                s.stream_type,
                s.channel_count
            );
        }
    }
    Ok(())
}

async fn run_device_connect(
    endpoint: &str,
    device_id: Option<&str>,
    criteria: Option<&str>,
) -> anyhow::Result<()> {
    let config = device_ipc_config(endpoint);
    let streams = fetch_discovered_streams(&config).await?;
    let stream_id = match (device_id, criteria) {
        (Some(id), _) => {
            let id = id.to_string();
            if !streams.iter().any(|s| s.id == id) {
                eprintln!("stream not found: '{}'", id);
                std::process::exit(2);
            }
            id
        }
        (None, Some(crit)) => {
            match streams
                .iter()
                .find(|s| s.stream_type.eq_ignore_ascii_case(crit) || s.id.contains(crit))
            {
                Some(s) => s.id.clone(),
                None => {
                    eprintln!("no stream matched criteria '{}'", crit);
                    std::process::exit(2);
                }
            }
        }
        (None, None) => {
            return Err(anyhow::anyhow!(
                "either --device-id <id> or --criteria <type> is required"
            ));
        }
    };
    let response = send_control_request_once(
        config,
        ControlRequest::new(ControlCommand::ConnectStream {
            stream_id: stream_id.clone(),
        }),
        "cli",
        3,
    )
    .await
    .map_err(|e| anyhow::anyhow!("control request failed: {}", e))?;
    match response.payload {
        ControlResponsePayload::Ack => {
            println!("connected {}", stream_id);
            Ok(())
        }
        ControlResponsePayload::Error { message } => {
            if message.to_lowercase().contains("not found")
                || message.to_lowercase().contains("no stream")
            {
                eprintln!("{}", message);
                std::process::exit(2);
            }
            Err(anyhow::anyhow!("connect failed: {}", message))
        }
        _ => Err(anyhow::anyhow!("unexpected response payload")),
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
    use crate::runtime_events::{
        RUNTIME_EVENTS_REPLAY_MAX_EVENTS, RUNTIME_EVENTS_REPLAY_RETENTION_US,
        RuntimeEventsReplayBuffer,
    };
    use neurohid_core::runtime::RuntimeBuilder;
    use neurohid_ipc::IpcClient as RuntimeIpcClient;
    use neurohid_types::{
        config::{BrainFlowConfig, DeviceBackend, SystemConfig},
        control::{
            ControlCommand, ControlRequest, ControlResponsePayload, ControlSnapshot,
            RuntimeModeState,
        },
    };

    fn replay_item(seq: u64, sent_at_us: i64) -> RuntimeEventsReplayItem {
        RuntimeEventsReplayItem {
            seq,
            sent_at_us,
            family: "lifecycle",
            event: RuntimeEvent::Lifecycle {
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

        assert_eq!(replay.len(), RUNTIME_EVENTS_REPLAY_MAX_EVENTS);
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

    fn test_control_snapshot() -> ControlSnapshot {
        ControlSnapshot {
            running: false,
            uptime_secs: 0,
            calibration_mode: false,
            output_enabled: true,
            profile_ready: false,
            decoder_ready: false,
            decoder_model_version: None,
            active_profile_name: None,
            device_name: None,
            outlet_name: None,
            signal_name: None,
            decoder_name: None,
            device_battery: None,
            signal_quality: 0.0,
            signal_latency_last_us: 0,
            signal_latency_p95_us: 0,
            decode_latency_last_us: 0,
            decode_latency_p95_us: 0,
            action_latency_last_us: 0,
            action_latency_p95_us: 0,
            latency_degraded: false,
            latency_alert_message: None,
            actions_emitted: 0,
            errors_detected: 0,
            ipc_connected: false,
            ipc_simulated: false,
            learning_enabled: true,
            ml_bridge_connected: false,
            ml_bridge_stalled: false,
            runtime_mode_state: RuntimeModeState::Degraded,
            enabled_capabilities: Vec::new(),
            limited_capabilities_message: None,
            fallback_model_kind: None,
            trainer_replay_size: None,
            trainer_step: None,
            trainer_policy_loss: None,
            trainer_value_loss: None,
            trainer_entropy: None,
            trainer_last_error: None,
            candidate_promotions_succeeded: 0,
            candidate_promotions_rejected: 0,
            candidate_last_outcome: None,
            ml_protocol_version: None,
            device_connected: false,
            task_error: None,
            discovered_streams: vec![],
            routed_eeg_streams: 0,
            routed_motion_streams: 0,
            routed_auxiliary_streams: 0,
            routed_unknown_streams: 0,
            pipeline_integrity_degraded: false,
            integrity_issue_count: 0,
            stage_health_summary: None,
            recording_active: false,
            current_session_id: None,
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
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
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
        let subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
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
        assert_eq!(first_event.channel, IpcChannel::RuntimeEvents);

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
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
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
        let hello = IpcEnvelope::new(
            IpcChannel::TrainerStream,
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
        assert_eq!(first_response.channel, IpcChannel::TrainerStream);

        let mut second_client = RuntimeIpcClient::new(server_config);
        second_client
            .connect()
            .await
            .expect("second trainer client should connect");
        let second_hello = IpcEnvelope::new(
            IpcChannel::TrainerStream,
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
        assert_eq!(busy.channel, IpcChannel::TrainerStream);
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

    #[tokio::test]
    async fn runtime_events_resume_replay_hit_emits_lifecycle_metadata() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
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

        let mut seed_client = RuntimeIpcClient::new(server_config.clone());
        seed_client
            .connect()
            .await
            .expect("seed client should connect");
        let seed_subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("seed-subscriber".to_string()),
            &serde_json::json!({
                "families": ["snapshot"],
                "include_snapshot": true,
                "include_capabilities": false,
                "max_events": 1,
                "max_duration_ms": 500,
            }),
        )
        .expect("seed subscribe envelope should encode");
        seed_client
            .send(seed_subscribe)
            .await
            .expect("seed subscribe should send");
        let first_event = seed_client
            .recv()
            .await
            .expect("seed subscriber should receive one event");
        let resume_from_seq = first_event.seq;
        let _ = seed_client.disconnect().await;

        let mut resume_client = RuntimeIpcClient::new(server_config.clone());
        resume_client
            .connect()
            .await
            .expect("resume client should connect");
        let resume_subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("resume-subscriber".to_string()),
            &serde_json::json!({
                "include_snapshot": false,
                "include_capabilities": false,
                "resume_from_seq": resume_from_seq,
                "max_events": 6,
                "max_duration_ms": 1_500,
            }),
        )
        .expect("resume subscribe envelope should encode");
        resume_client
            .send(resume_subscribe)
            .await
            .expect("resume subscribe should send");

        let mut saw_replay_resumed = false;
        for _ in 0..6 {
            let envelope = tokio::time::timeout(Duration::from_millis(500), resume_client.recv())
                .await
                .expect("resume subscriber should receive events")
                .expect("resume subscriber recv should succeed");
            if envelope.channel != IpcChannel::RuntimeEvents || envelope.msg_type != "event" {
                continue;
            }
            if envelope
                .payload
                .get("type")
                .and_then(serde_json::Value::as_str)
                == Some("lifecycle")
                && envelope
                    .payload
                    .get("state")
                    .and_then(serde_json::Value::as_str)
                    == Some("replay_resumed")
            {
                saw_replay_resumed = true;
                assert_eq!(
                    envelope
                        .payload
                        .get("requested_seq")
                        .and_then(serde_json::Value::as_u64),
                    Some(resume_from_seq)
                );
                assert!(
                    envelope
                        .payload
                        .get("replay_window_start_seq")
                        .and_then(serde_json::Value::as_u64)
                        .is_some()
                );
                assert!(
                    envelope
                        .payload
                        .get("replay_window_end_seq")
                        .and_then(serde_json::Value::as_u64)
                        .is_some()
                );
                break;
            }
        }

        assert!(
            saw_replay_resumed,
            "expected replay_resumed lifecycle event"
        );

        let _ = resume_client.disconnect().await;
        server_task.abort();
        let _ = server_task.await;

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[tokio::test]
    async fn runtime_events_resume_replay_miss_emits_lifecycle_metadata() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
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

        let mut client = RuntimeIpcClient::new(server_config);
        client.connect().await.expect("client should connect");
        let subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("replay-miss-subscriber".to_string()),
            &serde_json::json!({
                "include_snapshot": false,
                "include_capabilities": false,
                "resume_from_seq": 0,
                "max_events": 3,
                "max_duration_ms": 1_000,
            }),
        )
        .expect("replay-miss subscribe envelope should encode");
        client
            .send(subscribe)
            .await
            .expect("replay-miss subscribe should send");

        let envelope = tokio::time::timeout(Duration::from_secs(1), client.recv())
            .await
            .expect("replay-miss subscriber should receive lifecycle")
            .expect("replay-miss recv should succeed");
        assert_eq!(envelope.channel, IpcChannel::RuntimeEvents);
        assert_eq!(envelope.msg_type, "event");
        assert_eq!(
            envelope
                .payload
                .get("type")
                .and_then(serde_json::Value::as_str),
            Some("lifecycle")
        );
        assert_eq!(
            envelope
                .payload
                .get("state")
                .and_then(serde_json::Value::as_str),
            Some("replay_miss")
        );
        assert_eq!(
            envelope
                .payload
                .get("requested_seq")
                .and_then(serde_json::Value::as_u64),
            Some(0)
        );

        let _ = client.disconnect().await;
        server_task.abort();
        let _ = server_task.await;

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[tokio::test]
    async fn runtime_events_emit_backpressure_drop_when_broker_subscriber_lags() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server = RuntimeIpcServer::new(server_config.clone())
            .await
            .expect("ipc server should start");
        let mut client = RuntimeIpcClient::new(server_config);
        client.connect().await.expect("client should connect");
        let connection = server.accept().await.expect("server should accept client");

        let mut broker_config = RuntimeBrokerConfig::default();
        broker_config.runtime_events.capacity = 1;
        let broker = RuntimeIpcBroker::new(broker_config);
        let runtime_events_state = Arc::new(Mutex::new(RuntimeEventsState::default()));

        let subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("lagged-broker-subscriber".to_string()),
            &serde_json::json!({
                "families": ["backpressure_drop"],
                "include_snapshot": false,
                "include_capabilities": false,
                "max_events": 128,
                "max_duration_ms": 1_000,
            }),
        )
        .expect("subscribe envelope should encode");

        let reader_task = tokio::spawn(async move {
            let mut saw_drop = false;
            for _ in 0..128 {
                let envelope =
                    match tokio::time::timeout(Duration::from_millis(500), client.recv()).await {
                        Ok(Ok(envelope)) => envelope,
                        _ => break,
                    };
                if envelope.channel != IpcChannel::RuntimeEvents || envelope.msg_type != "event" {
                    continue;
                }

                if envelope
                    .payload
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    == Some("backpressure_drop")
                {
                    let dropped = envelope
                        .payload
                        .get("dropped")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or_default();
                    let reason = envelope
                        .payload
                        .get("reason")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    if dropped > 0 && reason.contains("broker subscriber lagged") {
                        saw_drop = true;
                        break;
                    }
                }

                if envelope
                    .payload
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    == Some("lifecycle")
                    && envelope
                        .payload
                        .get("state")
                        .and_then(serde_json::Value::as_str)
                        == Some("subscription_closed")
                {
                    break;
                }
            }

            let _ = client.disconnect().await;
            saw_drop
        });

        let publish_task = {
            let broker = broker.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(25)).await;
                for seq in 0..5_000_u64 {
                    let _ = broker.publish_runtime_event(RuntimeEvent::Lifecycle {
                        state: "burst".to_string(),
                        detail: seq.to_string(),
                        requested_seq: None,
                        replay_window_start_seq: None,
                        replay_window_end_seq: None,
                    });
                }
            })
        };

        let runtime_ipc = runtime.ipc_handle();
        handle_runtime_events_subscription(
            &connection,
            &runtime_ipc,
            &broker,
            subscribe,
            runtime_events_state,
        )
        .await
        .expect("runtime.events subscription handler should complete");

        publish_task.await.expect("publisher task should complete");
        let saw_drop = reader_task.await.expect("reader task should complete");
        assert!(
            saw_drop,
            "expected runtime.events backpressure_drop event from broker lag"
        );
        let counters = broker.counters();
        assert!(
            counters.runtime_backpressure_drops > 0,
            "expected runtime backpressure drop counter to increase"
        );
        assert!(
            counters.subscriber_lag_events > 0,
            "expected subscriber lag events counter to increase"
        );

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[tokio::test]
    async fn runtime_events_replay_updates_broker_replay_counters() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server = RuntimeIpcServer::new(server_config.clone())
            .await
            .expect("ipc server should start");
        let broker = RuntimeIpcBroker::new(RuntimeBrokerConfig::default());
        let runtime_events_state = Arc::new(Mutex::new(RuntimeEventsState::default()));

        {
            let mut state = runtime_events_state.lock().await;
            let now = neurohid_types::now_micros();
            for offset in 0..3_u64 {
                let seq = state.allocate_seq();
                state.replay.push(RuntimeEventsReplayItem {
                    seq,
                    sent_at_us: now.saturating_add(offset as i64),
                    family: "lifecycle",
                    event: RuntimeEvent::Lifecycle {
                        state: "seed".to_string(),
                        detail: format!("seed-{seq}"),
                        requested_seq: None,
                        replay_window_start_seq: None,
                        replay_window_end_seq: None,
                    },
                });
            }
        }

        let mut hit_client = RuntimeIpcClient::new(server_config.clone());
        hit_client
            .connect()
            .await
            .expect("hit client should connect");
        let hit_connection = server
            .accept()
            .await
            .expect("server should accept hit client");
        let hit_subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("replay-hit-counters".to_string()),
            &serde_json::json!({
                "include_snapshot": false,
                "include_capabilities": false,
                "resume_from_seq": 2,
                "max_events": 4,
                "max_duration_ms": 500,
            }),
        )
        .expect("hit subscribe envelope should encode");
        let hit_reader = tokio::spawn(async move {
            for _ in 0..8 {
                let envelope =
                    match tokio::time::timeout(Duration::from_millis(400), hit_client.recv()).await
                    {
                        Ok(Ok(envelope)) => envelope,
                        _ => break,
                    };
                if envelope
                    .payload
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    == Some("lifecycle")
                    && envelope
                        .payload
                        .get("state")
                        .and_then(serde_json::Value::as_str)
                        == Some("subscription_closed")
                {
                    break;
                }
            }
            let _ = hit_client.disconnect().await;
        });

        let runtime_ipc = runtime.ipc_handle();
        handle_runtime_events_subscription(
            &hit_connection,
            &runtime_ipc,
            &broker,
            hit_subscribe,
            Arc::clone(&runtime_events_state),
        )
        .await
        .expect("replay-hit subscription should complete");
        hit_reader.await.expect("hit reader task should complete");

        let counters_after_hit = broker.counters();
        assert_eq!(counters_after_hit.replay_hits, 1);
        assert_eq!(counters_after_hit.replay_misses, 0);

        let mut miss_client = RuntimeIpcClient::new(server_config);
        miss_client
            .connect()
            .await
            .expect("miss client should connect");
        let miss_connection = server
            .accept()
            .await
            .expect("server should accept miss client");
        let miss_subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("replay-miss-counters".to_string()),
            &serde_json::json!({
                "include_snapshot": false,
                "include_capabilities": false,
                "resume_from_seq": 0,
                "max_events": 2,
                "max_duration_ms": 500,
            }),
        )
        .expect("miss subscribe envelope should encode");
        let miss_reader =
            tokio::spawn(async move {
                for _ in 0..6 {
                    let envelope =
                        match tokio::time::timeout(Duration::from_millis(400), miss_client.recv())
                            .await
                        {
                            Ok(Ok(envelope)) => envelope,
                            _ => break,
                        };
                    if envelope
                        .payload
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        == Some("lifecycle")
                        && envelope
                            .payload
                            .get("state")
                            .and_then(serde_json::Value::as_str)
                            == Some("subscription_closed")
                    {
                        break;
                    }
                }
                let _ = miss_client.disconnect().await;
            });

        handle_runtime_events_subscription(
            &miss_connection,
            &runtime_ipc,
            &broker,
            miss_subscribe,
            runtime_events_state,
        )
        .await
        .expect("replay-miss subscription should complete");
        miss_reader.await.expect("miss reader task should complete");

        let counters_after_miss = broker.counters();
        assert_eq!(counters_after_miss.replay_hits, 1);
        assert_eq!(counters_after_miss.replay_misses, 1);

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[test]
    fn capabilities_mark_live_components_unavailable_when_runtime_not_ready() {
        let mut snapshot = test_control_snapshot();
        snapshot.running = false;
        snapshot.device_connected = false;
        snapshot.profile_ready = false;
        snapshot.decoder_ready = false;
        snapshot.ml_bridge_connected = false;
        snapshot.ml_bridge_stalled = false;

        let RuntimeEvent::Capabilities { components, .. } =
            build_runtime_capabilities_event(&snapshot)
        else {
            panic!("expected capabilities event");
        };

        let sample = components
            .iter()
            .find(|component| component.name == "sample")
            .expect("sample capability should exist");
        assert!(!sample.available);
        assert_eq!(
            sample.unavailable_reason.as_deref(),
            Some("runtime_not_running")
        );

        let decision = components
            .iter()
            .find(|component| component.name == "decision_event")
            .expect("decision_event capability should exist");
        assert!(!decision.available);
    }

    #[test]
    fn capabilities_mark_trainer_components_available_when_runtime_ready() {
        let mut snapshot = test_control_snapshot();
        snapshot.running = true;
        snapshot.device_connected = true;
        snapshot.profile_ready = true;
        snapshot.decoder_ready = true;
        snapshot.ml_bridge_connected = true;
        snapshot.ml_bridge_stalled = false;

        let RuntimeEvent::Capabilities { components, .. } =
            build_runtime_capabilities_event(&snapshot)
        else {
            panic!("expected capabilities event");
        };

        for capability in ["sample", "feature_frame", "decision_event", "errp_result"] {
            let component = components
                .iter()
                .find(|component| component.name == capability)
                .unwrap_or_else(|| panic!("{capability} capability should exist"));
            assert!(component.available, "{capability} should be available");
            assert!(component.unavailable_reason.is_none());
        }
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
                None,
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
