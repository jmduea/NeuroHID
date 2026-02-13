//! Validation harness for NeuroHID runtime matrix checks.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use neurohid_types::config::{
    ControlTransport, DeviceBackend, MlTransport, ServiceRuntimeMode, SystemConfig,
};
use neurohid_types::control::{
    ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload, ControlSnapshot,
    RuntimeModeState,
};

#[derive(Parser, Debug)]
#[command(name = "neurohid-validate")]
#[command(about = "Validation harness for runtime soak/latency/boot matrix checks")]
struct Args {
    /// Path to a prebuilt neurohid-service executable.
    #[arg(long)]
    service_bin: Option<PathBuf>,
    #[command(subcommand)]
    command: ValidationCommand,
}

#[derive(Subcommand, Debug)]
enum ValidationCommand {
    /// Run reconnect soak test and collect mode/latency/resource snapshots.
    Soak(SoakArgs),
    /// Run full/fallback/degraded latency+resource comparison sessions.
    LatencyMatrix(LatencyMatrixArgs),
    /// Run no-Python-bridge boot scenario matrix.
    BootMatrix(BootMatrixArgs),
}

#[derive(Parser, Debug)]
struct SoakArgs {
    /// Soak test duration in seconds (use 86400 for 24h).
    #[arg(long, default_value_t = 900)]
    duration_secs: u64,
    /// Snapshot sampling interval.
    #[arg(long, default_value_t = 1000)]
    snapshot_interval_ms: u64,
    /// Interval for forced `MlBridgeReconnect`.
    #[arg(long, default_value_t = 120)]
    reconnect_interval_secs: u64,
    /// Control port exposed by spawned service.
    #[arg(long, default_value_t = 47395)]
    control_port: u16,
    /// Optional profile id passed to service.
    #[arg(long)]
    profile: Option<String>,
    /// Disable runtime IPC simulation (bridge must connect for full mode).
    #[arg(long, default_value_t = false)]
    disable_ipc_simulation: bool,
    /// Disable lightweight fallback model.
    #[arg(long, default_value_t = false)]
    disable_fallback: bool,
}

#[derive(Parser, Debug)]
struct LatencyMatrixArgs {
    /// Duration for each mode scenario.
    #[arg(long, default_value_t = 40)]
    duration_secs_per_mode: u64,
    /// Snapshot interval while collecting latency/resource stats.
    #[arg(long, default_value_t = 500)]
    snapshot_interval_ms: u64,
    /// Base control port; scenarios use base, base+1, base+2.
    #[arg(long, default_value_t = 47410)]
    base_control_port: u16,
    /// Optional profile id expected to have ONNX artifacts.
    #[arg(long)]
    profile: Option<String>,
    /// Profile id guaranteed to be absent (for degraded scenario).
    #[arg(long, default_value = "__validation_missing_profile__")]
    missing_profile: String,
}

#[derive(Parser, Debug)]
struct BootMatrixArgs {
    /// Seconds to wait for each scenario before evaluating snapshot.
    #[arg(long, default_value_t = 8)]
    settle_secs: u64,
    /// Base control port; scenarios use base..base+3.
    #[arg(long, default_value_t = 47440)]
    base_control_port: u16,
    /// Optional profile id expected to have ONNX artifacts.
    #[arg(long)]
    profile: Option<String>,
    /// Profile id guaranteed to be absent (for degraded/fallback-without-onnx scenarios).
    #[arg(long, default_value = "__validation_missing_profile__")]
    missing_profile: String,
}

#[derive(Clone)]
struct ScenarioLaunch {
    name: &'static str,
    control_port: u16,
    profile: Option<String>,
    ipc_simulation_enabled: bool,
    fallback_enabled: bool,
}

#[derive(Default)]
struct SnapshotStats {
    samples: u64,
    full_count: u64,
    fallback_count: u64,
    degraded_count: u64,
    mode_transitions: u64,
    last_mode: Option<RuntimeModeState>,
    max_decode_p95_us: u64,
    max_action_p95_us: u64,
    sum_decode_p95_us: u128,
    sum_action_p95_us: u128,
}

impl SnapshotStats {
    fn observe(&mut self, snapshot: &ControlSnapshot) {
        self.samples = self.samples.saturating_add(1);
        self.max_decode_p95_us = self.max_decode_p95_us.max(snapshot.decode_latency_p95_us);
        self.max_action_p95_us = self.max_action_p95_us.max(snapshot.action_latency_p95_us);
        self.sum_decode_p95_us = self
            .sum_decode_p95_us
            .saturating_add(snapshot.decode_latency_p95_us as u128);
        self.sum_action_p95_us = self
            .sum_action_p95_us
            .saturating_add(snapshot.action_latency_p95_us as u128);

        match snapshot.runtime_mode_state {
            RuntimeModeState::Full => self.full_count = self.full_count.saturating_add(1),
            RuntimeModeState::Fallback => {
                self.fallback_count = self.fallback_count.saturating_add(1);
            }
            RuntimeModeState::Degraded => {
                self.degraded_count = self.degraded_count.saturating_add(1);
            }
        }
        if self.last_mode != Some(snapshot.runtime_mode_state) {
            if self.last_mode.is_some() {
                self.mode_transitions = self.mode_transitions.saturating_add(1);
            }
            self.last_mode = Some(snapshot.runtime_mode_state);
        }
    }

    fn avg_decode_p95_us(&self) -> u64 {
        if self.samples == 0 {
            0
        } else {
            (self.sum_decode_p95_us / self.samples as u128) as u64
        }
    }

    fn avg_action_p95_us(&self) -> u64 {
        if self.samples == 0 {
            0
        } else {
            (self.sum_action_p95_us / self.samples as u128) as u64
        }
    }

    fn dominant_mode(&self) -> RuntimeModeState {
        if self.full_count >= self.fallback_count && self.full_count >= self.degraded_count {
            RuntimeModeState::Full
        } else if self.fallback_count >= self.degraded_count {
            RuntimeModeState::Fallback
        } else {
            RuntimeModeState::Degraded
        }
    }
}

#[derive(Default, Clone, Copy)]
struct ResourceSummary {
    #[cfg(target_os = "linux")]
    max_cpu_percent: f64,
    max_rss_mb: f64,
    #[cfg(target_os = "linux")]
    samples: u64,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
struct CpuTick {
    proc_ticks: u64,
    total_ticks: u64,
}

struct ServiceProcess {
    child: Child,
    generated_config: Option<PathBuf>,
    control_port: u16,
}

impl ServiceProcess {
    fn launch(
        service_bin: &Path,
        launch: &ScenarioLaunch,
        explicit_config: Option<&Path>,
    ) -> Result<Self> {
        let config_path = if let Some(path) = explicit_config {
            path.to_path_buf()
        } else {
            let config = build_config(launch.ipc_simulation_enabled, launch.fallback_enabled);
            write_temp_config(&config, launch.name)?
        };
        let generated_config = explicit_config.is_none().then_some(config_path.clone());

        let mut cmd = Command::new(service_bin);
        cmd.arg("--foreground")
            .arg("--control-port")
            .arg(launch.control_port.to_string())
            .arg("--config")
            .arg(&config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(profile) = &launch.profile {
            cmd.arg("--profile").arg(profile);
        }

        let child = cmd.spawn().with_context(|| {
            format!(
                "failed to launch service binary '{}'",
                service_bin.display()
            )
        })?;

        let mut process = Self {
            child,
            generated_config,
            control_port: launch.control_port,
        };
        process.wait_ready(Duration::from_secs(30))?;
        Ok(process)
    }

    #[cfg(target_os = "linux")]
    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn wait_ready(&mut self, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(snapshot) = request_snapshot(self.control_port) {
                if snapshot.running {
                    return Ok(());
                }
            }
            if let Some(status) = self.child.try_wait()? {
                bail!("service exited early with status {}", status);
            }
            thread::sleep(Duration::from_millis(200));
        }
        bail!("service did not become ready within {:?}", timeout);
    }

    fn shutdown(&mut self) -> Result<()> {
        let _ = send_control_command(self.control_port, ControlCommand::Shutdown);
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        self.child.kill().ok();
        Ok(())
    }
}

impl Drop for ServiceProcess {
    fn drop(&mut self) {
        let _ = self.shutdown();
        if let Some(path) = &self.generated_config {
            let _ = fs::remove_file(path);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CaseStatus {
    Pass,
    Fail,
    Inconclusive,
}

struct BootCaseResult {
    name: &'static str,
    status: CaseStatus,
    detail: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let service_bin = resolve_service_bin(args.service_bin.as_deref())?;

    match args.command {
        ValidationCommand::Soak(cmd) => run_soak(&service_bin, &cmd),
        ValidationCommand::LatencyMatrix(cmd) => run_latency_matrix(&service_bin, &cmd),
        ValidationCommand::BootMatrix(cmd) => run_boot_matrix(&service_bin, &cmd),
    }
}

fn resolve_service_bin(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if !path.exists() {
            bail!("service binary does not exist: {}", path.display());
        }
        return Ok(path.to_path_buf());
    }

    if let Ok(path) = std::env::var("NEUROHID_SERVICE_BIN") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let current = std::env::current_exe().context("failed to resolve current executable path")?;
    if let Some(dir) = current.parent() {
        let mut sibling = dir.join("neurohid-service");
        if cfg!(windows) {
            sibling.set_extension("exe");
        }
        if sibling.exists() {
            return Ok(sibling);
        }
    }

    bail!(
        "unable to locate neurohid-service binary; pass --service-bin or set NEUROHID_SERVICE_BIN"
    );
}

fn run_soak(service_bin: &Path, args: &SoakArgs) -> Result<()> {
    let launch = ScenarioLaunch {
        name: "soak",
        control_port: args.control_port,
        profile: args.profile.clone(),
        ipc_simulation_enabled: !args.disable_ipc_simulation,
        fallback_enabled: !args.disable_fallback,
    };
    let mut process = ServiceProcess::launch(service_bin, &launch, None)?;
    let soak_duration = Duration::from_secs(args.duration_secs.max(1));
    let sample_interval = Duration::from_millis(args.snapshot_interval_ms.max(100));
    let reconnect_interval = Duration::from_secs(args.reconnect_interval_secs.max(1));

    let mut snapshot_stats = SnapshotStats::default();
    #[cfg(target_os = "linux")]
    let mut resource_summary = ResourceSummary::default();
    #[cfg(not(target_os = "linux"))]
    let _resource_summary = ResourceSummary::default();
    let mut reconnect_requests = 0_u64;
    let mut reconnect_errors = 0_u64;
    let mut next_reconnect = Instant::now() + reconnect_interval;
    let start = Instant::now();

    #[cfg(target_os = "linux")]
    let mut cpu_tick: Option<CpuTick> = None;

    while start.elapsed() < soak_duration {
        let snapshot = request_snapshot(args.control_port)?;
        snapshot_stats.observe(&snapshot);

        #[cfg(target_os = "linux")]
        {
            if let Some(sample) = sample_linux_resources(process.pid(), &mut cpu_tick) {
                resource_summary.max_cpu_percent =
                    resource_summary.max_cpu_percent.max(sample.cpu_percent);
                resource_summary.max_rss_mb = resource_summary.max_rss_mb.max(sample.rss_mb);
                resource_summary.samples = resource_summary.samples.saturating_add(1);
            }
        }

        if Instant::now() >= next_reconnect {
            reconnect_requests = reconnect_requests.saturating_add(1);
            if send_control_command(args.control_port, ControlCommand::MlBridgeReconnect).is_err() {
                reconnect_errors = reconnect_errors.saturating_add(1);
            }
            next_reconnect += reconnect_interval;
        }

        thread::sleep(sample_interval);
    }

    process.shutdown()?;

    println!("Validation Soak Summary");
    println!("duration_secs={}", args.duration_secs);
    println!("samples={}", snapshot_stats.samples);
    println!("mode_transitions={}", snapshot_stats.mode_transitions);
    println!(
        "mode_counts: full={} fallback={} degraded={}",
        snapshot_stats.full_count, snapshot_stats.fallback_count, snapshot_stats.degraded_count
    );
    println!(
        "decode_p95_us: avg={} max={}",
        snapshot_stats.avg_decode_p95_us(),
        snapshot_stats.max_decode_p95_us
    );
    println!(
        "action_p95_us: avg={} max={}",
        snapshot_stats.avg_action_p95_us(),
        snapshot_stats.max_action_p95_us
    );
    println!(
        "bridge_reconnect_requests={} bridge_reconnect_errors={}",
        reconnect_requests, reconnect_errors
    );
    #[cfg(target_os = "linux")]
    println!(
        "resource_peaks: cpu_percent={:.2} rss_mb={:.2} samples={}",
        resource_summary.max_cpu_percent, resource_summary.max_rss_mb, resource_summary.samples
    );
    #[cfg(not(target_os = "linux"))]
    println!("resource_peaks: unsupported_on_this_host");

    Ok(())
}

fn run_latency_matrix(service_bin: &Path, args: &LatencyMatrixArgs) -> Result<()> {
    let scenarios = vec![
        ScenarioLaunch {
            name: "full_candidate",
            control_port: args.base_control_port,
            profile: args.profile.clone(),
            ipc_simulation_enabled: true,
            fallback_enabled: true,
        },
        ScenarioLaunch {
            name: "fallback_candidate",
            control_port: args.base_control_port.saturating_add(1),
            profile: args.profile.clone(),
            ipc_simulation_enabled: false,
            fallback_enabled: true,
        },
        ScenarioLaunch {
            name: "degraded_candidate",
            control_port: args.base_control_port.saturating_add(2),
            profile: Some(args.missing_profile.clone()),
            ipc_simulation_enabled: false,
            fallback_enabled: false,
        },
    ];

    println!("Latency Matrix Summary");
    println!("duration_per_mode_secs={}", args.duration_secs_per_mode);
    println!(
        "{:<20} {:<10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "scenario", "mode", "avg_dec", "avg_act", "max_dec", "max_act", "max_rss"
    );

    for scenario in scenarios {
        let mut process = ServiceProcess::launch(service_bin, &scenario, None)?;
        let mut snapshot_stats = SnapshotStats::default();
        let resource_summary = ResourceSummary::default();
        #[cfg(target_os = "linux")]
        let mut resource_summary = resource_summary;
        let start = Instant::now();
        let interval = Duration::from_millis(args.snapshot_interval_ms.max(100));
        #[cfg(target_os = "linux")]
        let mut cpu_tick: Option<CpuTick> = None;

        while start.elapsed() < Duration::from_secs(args.duration_secs_per_mode.max(1)) {
            let snapshot = request_snapshot(scenario.control_port)?;
            snapshot_stats.observe(&snapshot);

            #[cfg(target_os = "linux")]
            {
                if let Some(sample) = sample_linux_resources(process.pid(), &mut cpu_tick) {
                    resource_summary.max_cpu_percent =
                        resource_summary.max_cpu_percent.max(sample.cpu_percent);
                    resource_summary.max_rss_mb = resource_summary.max_rss_mb.max(sample.rss_mb);
                    resource_summary.samples = resource_summary.samples.saturating_add(1);
                }
            }

            thread::sleep(interval);
        }

        process.shutdown()?;

        let dominant_mode = mode_label(snapshot_stats.dominant_mode());
        println!(
            "{:<20} {:<10} {:>10} {:>10} {:>10} {:>10} {:>10.2}",
            scenario.name,
            dominant_mode,
            snapshot_stats.avg_decode_p95_us(),
            snapshot_stats.avg_action_p95_us(),
            snapshot_stats.max_decode_p95_us,
            snapshot_stats.max_action_p95_us,
            resource_summary.max_rss_mb
        );
    }

    Ok(())
}

fn run_boot_matrix(service_bin: &Path, args: &BootMatrixArgs) -> Result<()> {
    let scenarios = vec![
        ScenarioLaunch {
            name: "onnx_bridge_healthy",
            control_port: args.base_control_port,
            profile: args.profile.clone(),
            ipc_simulation_enabled: true,
            fallback_enabled: true,
        },
        ScenarioLaunch {
            name: "onnx_bridge_absent",
            control_port: args.base_control_port.saturating_add(1),
            profile: args.profile.clone(),
            ipc_simulation_enabled: false,
            fallback_enabled: true,
        },
        ScenarioLaunch {
            name: "onnx_absent_lightweight",
            control_port: args.base_control_port.saturating_add(2),
            profile: Some(args.missing_profile.clone()),
            ipc_simulation_enabled: false,
            fallback_enabled: true,
        },
        ScenarioLaunch {
            name: "no_usable_model",
            control_port: args.base_control_port.saturating_add(3),
            profile: Some(args.missing_profile.clone()),
            ipc_simulation_enabled: false,
            fallback_enabled: false,
        },
    ];

    let mut results = Vec::new();
    for scenario in scenarios {
        let mut process = ServiceProcess::launch(service_bin, &scenario, None)?;
        let settle = Duration::from_secs(args.settle_secs.max(1));
        let start = Instant::now();
        let mut last_snapshot = request_snapshot(scenario.control_port)?;

        while start.elapsed() < settle {
            thread::sleep(Duration::from_millis(500));
            if let Ok(snapshot) = request_snapshot(scenario.control_port) {
                last_snapshot = snapshot;
            }
        }

        let result = evaluate_boot_case(scenario.name, &last_snapshot);
        process.shutdown()?;
        results.push(result);
    }

    println!("Boot Matrix Summary");
    for result in results {
        println!(
            "{:<24} {:<12} {}",
            result.name,
            case_status_label(result.status),
            result.detail
        );
    }

    Ok(())
}

fn evaluate_boot_case(name: &'static str, snapshot: &ControlSnapshot) -> BootCaseResult {
    match name {
        "onnx_bridge_healthy" => {
            if snapshot.fallback_model_kind.as_deref() != Some("onnx") || !snapshot.decoder_ready {
                return BootCaseResult {
                    name,
                    status: CaseStatus::Inconclusive,
                    detail: "ONNX decoder artifacts not available in this profile".to_string(),
                };
            }
            let pass = snapshot.runtime_mode_state == RuntimeModeState::Full;
            BootCaseResult {
                name,
                status: if pass {
                    CaseStatus::Pass
                } else {
                    CaseStatus::Fail
                },
                detail: format!(
                    "mode={} bridge_connected={} bridge_stalled={}",
                    mode_label(snapshot.runtime_mode_state),
                    snapshot.ml_bridge_connected,
                    snapshot.ml_bridge_stalled
                ),
            }
        }
        "onnx_bridge_absent" => {
            if snapshot.fallback_model_kind.as_deref() != Some("onnx") || !snapshot.decoder_ready {
                return BootCaseResult {
                    name,
                    status: CaseStatus::Inconclusive,
                    detail: "ONNX decoder artifacts not available in this profile".to_string(),
                };
            }
            let pass = snapshot.runtime_mode_state == RuntimeModeState::Fallback;
            BootCaseResult {
                name,
                status: if pass {
                    CaseStatus::Pass
                } else {
                    CaseStatus::Fail
                },
                detail: format!(
                    "mode={} bridge_connected={} bridge_stalled={}",
                    mode_label(snapshot.runtime_mode_state),
                    snapshot.ml_bridge_connected,
                    snapshot.ml_bridge_stalled
                ),
            }
        }
        "onnx_absent_lightweight" => {
            let pass = !snapshot.decoder_ready
                && snapshot.fallback_model_kind.as_deref() == Some("lightweight_rust")
                && snapshot.runtime_mode_state == RuntimeModeState::Fallback;
            BootCaseResult {
                name,
                status: if pass {
                    CaseStatus::Pass
                } else {
                    CaseStatus::Fail
                },
                detail: format!(
                    "mode={} decoder_ready={} fallback_model_kind={}",
                    mode_label(snapshot.runtime_mode_state),
                    snapshot.decoder_ready,
                    snapshot
                        .fallback_model_kind
                        .clone()
                        .unwrap_or_else(|| "none".to_string())
                ),
            }
        }
        "no_usable_model" => {
            let pass = !snapshot.decoder_ready
                && snapshot.fallback_model_kind.as_deref() == Some("none")
                && snapshot.runtime_mode_state == RuntimeModeState::Degraded;
            BootCaseResult {
                name,
                status: if pass {
                    CaseStatus::Pass
                } else {
                    CaseStatus::Fail
                },
                detail: format!(
                    "mode={} decoder_ready={} fallback_model_kind={}",
                    mode_label(snapshot.runtime_mode_state),
                    snapshot.decoder_ready,
                    snapshot
                        .fallback_model_kind
                        .clone()
                        .unwrap_or_else(|| "none".to_string())
                ),
            }
        }
        _ => BootCaseResult {
            name,
            status: CaseStatus::Fail,
            detail: "unknown scenario".to_string(),
        },
    }
}

fn case_status_label(status: CaseStatus) -> &'static str {
    match status {
        CaseStatus::Pass => "PASS",
        CaseStatus::Fail => "FAIL",
        CaseStatus::Inconclusive => "INCONCLUSIVE",
    }
}

fn build_config(ipc_simulation_enabled: bool, fallback_enabled: bool) -> SystemConfig {
    let mut config = SystemConfig::default();
    config.device.backend = DeviceBackend::Mock;
    config.action.enabled = false;
    config.service.runtime_mode = ServiceRuntimeMode::Embedded;
    config.service.control_transport = ControlTransport::TcpLoopback;
    config.service.ml_transport = MlTransport::TcpLoopback;
    config.service.ipc_simulation_enabled = ipc_simulation_enabled;
    config.service.fallback_policy.enabled = fallback_enabled;
    config
}

fn write_temp_config(config: &SystemConfig, label: &str) -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("failed to read system time")?
        .as_micros();
    let path = std::env::temp_dir().join(format!("neurohid_validate_{label}_{stamp}.toml"));
    let payload = toml::to_string_pretty(config).context("failed to serialize config toml")?;
    fs::write(&path, payload).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn request_snapshot(port: u16) -> Result<ControlSnapshot> {
    let response = send_control_request(port, ControlRequest::new(ControlCommand::Snapshot))?;
    match response.payload {
        ControlResponsePayload::Snapshot { snapshot } => Ok(snapshot),
        ControlResponsePayload::Error { message } => {
            bail!("snapshot request failed: {}", message);
        }
        _ => bail!("snapshot request returned unexpected payload"),
    }
}

fn send_control_command(port: u16, command: ControlCommand) -> Result<()> {
    let response = send_control_request(port, ControlRequest::new(command))?;
    match response.payload {
        ControlResponsePayload::Ack | ControlResponsePayload::Snapshot { .. } => Ok(()),
        ControlResponsePayload::Error { message } => bail!("control command rejected: {}", message),
        ControlResponsePayload::TrainerSnapshot { .. } => {
            bail!("control command returned unexpected trainer snapshot")
        }
    }
}

fn send_control_request(port: u16, request: ControlRequest) -> Result<ControlResponse> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .with_context(|| format!("failed to connect control endpoint 127.0.0.1:{port}"))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(1200)))
        .context("failed to set read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_millis(1200)))
        .context("failed to set write timeout")?;

    let payload = serde_json::to_string(&request).context("failed to serialize control request")?;
    stream
        .write_all(payload.as_bytes())
        .context("failed to write control payload")?;
    stream
        .write_all(b"\n")
        .context("failed to write control newline")?;
    stream.flush().context("failed to flush control stream")?;

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut line)
        .context("failed to read control response")?;
    if line.trim().is_empty() {
        bail!("empty control response");
    }

    serde_json::from_str::<ControlResponse>(line.trim())
        .context("failed to decode control response json")
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
struct ResourceSample {
    cpu_percent: f64,
    rss_mb: f64,
}

#[cfg(target_os = "linux")]
fn sample_linux_resources(pid: u32, previous_tick: &mut Option<CpuTick>) -> Option<ResourceSample> {
    let proc_ticks = read_proc_ticks(pid).ok()?;
    let total_ticks = read_total_cpu_ticks().ok()?;
    let rss_mb = read_rss_mb(pid).ok()?;

    let cpu_percent = if let Some(previous) = previous_tick {
        let proc_delta = proc_ticks.saturating_sub(previous.proc_ticks) as f64;
        let total_delta = total_ticks.saturating_sub(previous.total_ticks) as f64;
        if total_delta <= f64::EPSILON {
            0.0
        } else {
            let cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1) as f64;
            (proc_delta / total_delta) * cores * 100.0
        }
    } else {
        0.0
    };

    *previous_tick = Some(CpuTick {
        proc_ticks,
        total_ticks,
    });

    Some(ResourceSample {
        cpu_percent,
        rss_mb,
    })
}

#[cfg(target_os = "linux")]
fn read_proc_ticks(pid: u32) -> Result<u64> {
    let content = fs::read_to_string(format!("/proc/{pid}/stat"))
        .with_context(|| format!("failed to read /proc/{pid}/stat"))?;
    let end = content
        .rfind(')')
        .context("failed to parse process stat format")?;
    let rest = content
        .get(end + 2..)
        .context("failed to split process stat fields")?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    if fields.len() < 13 {
        bail!("unexpected /proc stat field count");
    }
    let utime: u64 = fields[11]
        .parse()
        .context("failed to parse utime from /proc stat")?;
    let stime: u64 = fields[12]
        .parse()
        .context("failed to parse stime from /proc stat")?;
    Ok(utime.saturating_add(stime))
}

#[cfg(target_os = "linux")]
fn read_total_cpu_ticks() -> Result<u64> {
    let content = fs::read_to_string("/proc/stat").context("failed to read /proc/stat")?;
    let first = content
        .lines()
        .next()
        .context("missing /proc/stat cpu line")?;
    let total = first
        .split_whitespace()
        .skip(1)
        .filter_map(|field| field.parse::<u64>().ok())
        .fold(0_u64, |acc, value| acc.saturating_add(value));
    Ok(total)
}

#[cfg(target_os = "linux")]
fn read_rss_mb(pid: u32) -> Result<f64> {
    let content = fs::read_to_string(format!("/proc/{pid}/status"))
        .with_context(|| format!("failed to read /proc/{pid}/status"))?;
    let value_kb = content
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .context("VmRSS not found in /proc status")?
        .parse::<f64>()
        .context("failed to parse VmRSS value")?;
    Ok(value_kb / 1024.0)
}

fn mode_label(mode: RuntimeModeState) -> &'static str {
    match mode {
        RuntimeModeState::Full => "full",
        RuntimeModeState::Fallback => "fallback",
        RuntimeModeState::Degraded => "degraded",
    }
}
