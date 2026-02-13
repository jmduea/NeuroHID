//! # NeuroHID Service (Headless)
//!
//! Standalone background runtime host for NeuroHID. It can run in foreground
//! mode for development, and on Windows it also exposes service lifecycle
//! commands (`install`, `start`, `stop`, `status`, `uninstall`).

use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::time::Instant;

use neurohid_core::runtime::{RuntimeBuilder, RuntimeCommand, RuntimeHandle};
use neurohid_storage::ProfileStore;
use neurohid_types::{
    config::{ControlTransport, SystemConfig},
    control::{ControlCommand, ControlRequest, ControlResponse},
    observability::{
        self as obs, EmitGate, EmitPolicyConfig, ObservabilityComponent,
    },
    profile::ProfileId,
};

#[cfg(windows)]
use neurohid_core::service::NeuroHidService;
#[cfg(windows)]
use tokio::sync::broadcast;

const DEFAULT_WINDOWS_SERVICE_NAME: &str = "NeuroHIDService";
#[cfg(windows)]
const WINDOWS_SERVICE_DISPLAY_NAME: &str = "NeuroHID Service";
#[cfg(windows)]
const WINDOWS_SERVICE_DESCRIPTION: &str =
    "NeuroHID runtime service for biosignal acquisition, decoding, and HID output";

/// Windows service lifecycle command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ServiceCommand {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
}

/// Command-line arguments for the NeuroHID service.
#[derive(Parser, Debug)]
#[command(name = "neurohid-service")]
#[command(about = "NeuroHID - Brain-computer interface headless service")]
struct Args {
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

    /// Bind a localhost TCP control protocol server on this port.
    ///
    /// Clients exchange line-delimited JSON `ControlRequest` / `ControlResponse`
    /// messages defined in `neurohid-types::control`.
    #[arg(long)]
    control_port: Option<u16>,

    /// Windows service lifecycle command.
    #[arg(long, value_enum)]
    service_command: Option<ServiceCommand>,

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
        tracing::warn!("Background daemon mode is not implemented yet; running in foreground");
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

    let config = if let Some(config_path) = config_path_override {
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
    tracing::info!("Managed runtime started");

    if let Some(port) = control_port {
        tracing::info!("Starting control protocol server on 127.0.0.1:{port} (tcp)");
        tokio::select! {
            result = run_tcp_control_server(port, &runtime_handle, control_observability_policy.clone()) => {
                if let Err(error) = result {
                    tracing::warn!("Control protocol server exited with error: {}", error);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutdown signal received");
            }
            _ = wait_until_runtime_stopped(&runtime_handle) => {}
        }
    } else if service_config.control_transport == ControlTransport::NamedPipe {
        #[cfg(windows)]
        {
            tracing::info!(
                "Starting control protocol server on named pipe {}",
                service_config.control_pipe_name
            );
            tokio::select! {
                result = run_named_pipe_control_server(
                    &service_config.control_pipe_name,
                    &runtime_handle,
                    control_observability_policy.clone(),
                ) => {
                    if let Err(error) = result {
                        tracing::warn!("Control protocol named-pipe server exited with error: {}", error);
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Shutdown signal received");
                }
                _ = wait_until_runtime_stopped(&runtime_handle) => {}
            }
        }
        #[cfg(not(windows))]
        {
            tracing::warn!("Named pipe control transport requested on non-Windows host; falling back to runtime-only mode.");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Shutdown signal received");
                }
                _ = wait_until_runtime_stopped(&runtime_handle) => {}
            }
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

async fn run_tcp_control_server(
    port: u16,
    runtime: &RuntimeHandle,
    control_policy: EmitPolicyConfig,
) -> anyhow::Result<()> {
    use tokio::net::TcpListener;

    let mut control_gate = EmitGate::new(control_policy);
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind control port {}: {}", port, e))?;

    loop {
        let (stream, peer) = listener
            .accept()
            .await
            .map_err(|e| anyhow::anyhow!("Control accept failed: {}", e))?;

        if let Err(error) = handle_control_client(stream, runtime, &mut control_gate).await {
            tracing::warn!("Control client {} disconnected with error: {}", peer, error);
        }
    }
}

#[cfg(windows)]
async fn run_named_pipe_control_server(
    pipe_name: &str,
    runtime: &RuntimeHandle,
    control_policy: EmitPolicyConfig,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::windows::named_pipe::ServerOptions;

    let mut control_gate = EmitGate::new(control_policy);
    loop {
        let server = ServerOptions::new().create(pipe_name).map_err(|e| {
            anyhow::anyhow!("Failed to create control named pipe {}: {}", pipe_name, e)
        })?;
        server
            .connect()
            .await
            .map_err(|e| anyhow::anyhow!("Control named pipe connect failed: {}", e))?;
        let (read_half, mut write_half) = tokio::io::split(server);
        let mut reader = BufReader::new(read_half);
        let mut line = String::new();

        loop {
            line.clear();
            let read = reader.read_line(&mut line).await?;
            if read == 0 {
                break;
            }

            let payload = line.trim();
            if payload.is_empty() {
                continue;
            }

            let parsed: Result<ControlRequest, _> = serde_json::from_str(payload);
            let (response, should_shutdown) = match parsed {
                Ok(request) => {
                    let request_id = request.request_id.clone();
                    let command = control_command_name(&request.command);
                    let started = Instant::now();
                    if tracing::enabled!(tracing::Level::DEBUG) && control_gate.allow_debug() {
                        tracing::debug!(
                            event = obs::event::CONTROL_REQUEST_RECEIVED,
                            request_id = request_id.as_deref().unwrap_or("none"),
                            decision_id = obs::field::UNKNOWN,
                            stream_id = obs::field::UNKNOWN,
                            command,
                            transport = "named_pipe",
                            "Control request received"
                        );
                    }
                    let should_shutdown = matches!(request.command, ControlCommand::Shutdown);
                    let response = runtime.dispatch_control_request(request);
                    if tracing::enabled!(tracing::Level::DEBUG) && control_gate.allow_debug() {
                        tracing::debug!(
                            event = obs::event::CONTROL_RESPONSE_SENT,
                            request_id = request_id.as_deref().unwrap_or("none"),
                            decision_id = obs::field::UNKNOWN,
                            stream_id = obs::field::UNKNOWN,
                            command,
                            duration_ms = started.elapsed().as_millis() as u64,
                            transport = "named_pipe",
                            "Control request handled"
                        );
                    }
                    (response, should_shutdown)
                }
                Err(error) => (
                    ControlResponse::error(None, format!("invalid control request: {}", error)),
                    false,
                ),
            };

            let response_json = serde_json::to_string(&response)
                .map_err(|e| anyhow::anyhow!("Failed to serialize control response: {}", e))?;
            write_half.write_all(response_json.as_bytes()).await?;
            write_half.write_all(b"\n").await?;
            write_half.flush().await?;

            if should_shutdown {
                break;
            }
        }
    }
}

async fn handle_control_client(
    stream: tokio::net::TcpStream,
    runtime: &RuntimeHandle,
    control_gate: &mut EmitGate,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            break;
        }

        let payload = line.trim();
        if payload.is_empty() {
            continue;
        }

        let parsed: Result<ControlRequest, _> = serde_json::from_str(payload);
        let (response, should_shutdown) = match parsed {
            Ok(request) => {
                let request_id = request.request_id.clone();
                let command = control_command_name(&request.command);
                let started = Instant::now();
                let _request_span = tracing::debug_span!(
                    obs::span::CONTROL_REQUEST,
                    stage = obs::stage::CONTROL,
                    request_id = request_id.as_deref().unwrap_or("none"),
                    command,
                    decision_id = obs::field::UNKNOWN,
                    stream_id = obs::field::UNKNOWN
                )
                .entered();
                if tracing::enabled!(tracing::Level::DEBUG) && control_gate.allow_debug() {
                    tracing::debug!(
                        event = obs::event::CONTROL_REQUEST_RECEIVED,
                        request_id = request_id.as_deref().unwrap_or("none"),
                        decision_id = obs::field::UNKNOWN,
                        stream_id = obs::field::UNKNOWN,
                        command,
                        transport = "tcp",
                        "Control request received"
                    );
                }
                let should_shutdown = matches!(request.command, ControlCommand::Shutdown);
                let response = runtime.dispatch_control_request(request);
                if tracing::enabled!(tracing::Level::DEBUG) && control_gate.allow_debug() {
                    tracing::debug!(
                        event = obs::event::CONTROL_RESPONSE_SENT,
                        request_id = request_id.as_deref().unwrap_or("none"),
                        decision_id = obs::field::UNKNOWN,
                        stream_id = obs::field::UNKNOWN,
                        command,
                        duration_ms = started.elapsed().as_millis() as u64,
                        transport = "tcp",
                        "Control request handled"
                    );
                }
                (response, should_shutdown)
            }
            Err(error) => (
                ControlResponse::error(None, format!("invalid control request: {}", error)),
                false,
            ),
        };

        let response_json = serde_json::to_string(&response)
            .map_err(|e| anyhow::anyhow!("Failed to serialize control response: {}", e))?;
        write_half.write_all(response_json.as_bytes()).await?;
        write_half.write_all(b"\n").await?;
        write_half.flush().await?;

        if should_shutdown {
            break;
        }
    }

    Ok(())
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
    }
}

fn run_service_command(command: ServiceCommand, args: &Args) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        return windows_service_manager::run(command, args);
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

    use super::{load_runtime_context, run_core_service, SERVICE_LAUNCH_CONFIG};

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
