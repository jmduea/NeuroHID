//! # NeuroHID Service (Headless)
//!
//! Standalone background runtime host for NeuroHID. It can run in foreground
//! mode for development, and on Windows it also exposes service lifecycle
//! commands (`install`, `start`, `stop`, `status`, `uninstall`).

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use neurohid_core::recording;
use neurohid_core::runtime::{RuntimeBuilder, RuntimeCommand, RuntimeHandle};
use neurohid_ipc::{
    IpcConfig as RuntimeIpcConfig, IpcTransport as RuntimeIpcTransport, send_control_request_once,
};
use neurohid_storage::{ConfigStore, DataPaths, ProfileStore};
use neurohid_types::{
    config::{IpcMode, SystemConfig},
    control::{ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload},
    device::DiscoveredStream,
    observability::ObservabilityComponent,
    profile::ProfileId,
};

#[cfg(windows)]
use neurohid_core::service::NeuroHidService;
use tokio::sync::broadcast;

const DEFAULT_WINDOWS_SERVICE_NAME: &str = "NeuroHIDService";
#[cfg(windows)]
const WINDOWS_SERVICE_DISPLAY_NAME: &str = "NeuroHID Service";
#[cfg(windows)]
const WINDOWS_SERVICE_DESCRIPTION: &str =
    "NeuroHID runtime service for biosignal acquisition, decoding, and HID output";
const DAEMON_METADATA_FILE: &str = "daemon.json";

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

mod ipc_server;
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
            Some(ipc_server::DEFAULT_STANDALONE_CONTROL_PORT)
        } else {
            None
        }
    });
    if let Some(server_config) =
        ipc_server::resolve_runtime_ipc_server_config(&service_config, effective_control_port)?
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
            result = ipc_server::run_ipc_control_server(
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
    ipc_server::resolve_runtime_ipc_server_config(service_config, control_port_override)?
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
    ipc_server::validate_local_only_endpoint(transport, &metadata.ipc_endpoint)?;
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
    if endpoint != ipc_server::DEFAULT_CONTROL_ENDPOINT {
        return RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: endpoint.to_string(),
            ..RuntimeIpcConfig::default()
        };
    }
    let default_ipc = RuntimeIpcConfig {
        transport: RuntimeIpcTransport::TcpLoopback,
        endpoint: ipc_server::DEFAULT_CONTROL_ENDPOINT.to_string(),
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
            if ipc_server::validate_local_only_endpoint(
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
