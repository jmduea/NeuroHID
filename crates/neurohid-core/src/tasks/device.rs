//! # Device Task
//!
//! This task is responsible for connecting to the EEG device and streaming
//! samples to the signal processing task. It's the entry point for all brain
//! signal data in the system.
//!
//! ## Provider Selection
//!
//! The provider is selected based on `DeviceConfig::backend`:
//!
//! | Backend     | Feature flag  | Description                        |
//! |-------------|---------------|------------------------------------|
//! | `Mock`      | *(always)*    | Synthetic sine-wave generator      |
//! | `BrainFlow` | `brainflow`   | OpenBCI, Muse, Unicorn, etc.       |
//! | `Emotiv`    | `emotiv`      | Emotiv Insight/EPOC via Cortex API |
//! | `Auto`      | *(any)*       | Tries available backends in order  |

use futures::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

use neurohid_device::mock::MockProvider;
use neurohid_device::{DeviceProvider, MockDeviceConfig};
use neurohid_types::{
    config::{DeviceBackend, DeviceConfig},
    error::{DeviceError, Result},
    signal::Sample,
};

use crate::service::ServiceState;

/// The device task connects to the EEG device and streams samples.
pub struct DeviceTask {
    config: DeviceConfig,
    sample_tx: mpsc::Sender<Sample>,
    state: Arc<RwLock<ServiceState>>,
    /// Optional channel to forward samples to calibration panel
    calibration_sample_tx: Option<mpsc::Sender<Sample>>,
    /// Atomic flag: when true, samples are also sent to calibration channel
    calibration_mode: Option<Arc<AtomicBool>>,
}

impl DeviceTask {
    /// Creates a new device task.
    pub fn new(
        config: DeviceConfig,
        sample_tx: mpsc::Sender<Sample>,
        state: Arc<RwLock<ServiceState>>,
        calibration_sample_tx: Option<mpsc::Sender<Sample>>,
        calibration_mode: Option<Arc<AtomicBool>>,
    ) -> Self {
        Self {
            config,
            sample_tx,
            state,
            calibration_sample_tx,
            calibration_mode,
        }
    }

    /// Runs the device task until shutdown is signaled.
    pub async fn run(self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        let provider = create_provider(&self.config).await?;

        // Discover available devices
        tracing::info!("Discovering devices...");
        let devices = provider.discover().await?;

        if devices.is_empty() {
            tracing::error!("No devices found");
            return Err(DeviceError::NoDeviceFound.into());
        }

        tracing::info!("Found {} device(s), connecting to first", devices.len());

        // Connect to the first device
        let mut device = provider.connect(&devices[0].id, None).await?;

        tracing::info!("Connected to device: {}", device.id());

        // Update shared state with device info
        {
            let mut state = self.state.write().await;
            state.device_connected = true;
            state.device_name = Some(device.id().to_string());
        }

        // Start streaming
        let mut stream = device.start_streaming().await?;

        tracing::info!("Streaming started");

        // Main loop: read samples and forward them to the signal task
        loop {
            tokio::select! {
                // Check for shutdown signal
                _ = shutdown.recv() => {
                    tracing::info!("Device task received shutdown signal");
                    break;
                }

                // Read next sample from device
                sample_result = stream.next() => {
                    match sample_result {
                        Some(Ok(sample)) => {
                            // Update signal quality in shared state
                            if let Some(quality) = &sample.quality {
                                let avg_quality = quality.iter().sum::<f32>() / quality.len() as f32;
                                let mut state = self.state.write().await;
                                state.signal_quality = avg_quality;
                            }

                            // If calibration mode is active, fan-out the sample
                            if let (Some(flag), Some(tx)) = (&self.calibration_mode, &self.calibration_sample_tx) {
                                if flag.load(Ordering::Relaxed) {
                                    // try_send to avoid blocking the device loop
                                    let _ = tx.try_send(sample.clone());
                                }
                            }

                            // Send sample to signal task
                            if self.sample_tx.send(sample).await.is_err() {
                                // Receiver dropped, time to shut down
                                tracing::warn!("Sample receiver dropped, stopping device task");
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!("Error reading sample: {}", e);
                            // Continue trying - transient errors are expected
                        }
                        None => {
                            // Stream ended
                            tracing::info!("Device stream ended");
                            break;
                        }
                    }
                }
            }
        }

        // Clean up
        tracing::info!("Stopping device streaming");
        {
            let mut state = self.state.write().await;
            state.device_connected = false;
            state.device_name = None;
        }
        device.stop_streaming().await?;
        device.disconnect().await?;

        Ok(())
    }
}

// ─── Provider Factory ────────────────────────────────────────────────────────

/// Create a device provider based on the backend configuration.
///
/// For `Auto` mode, tries backends in priority order: Emotiv → BrainFlow → Mock.
/// Each real backend is gated behind a Cargo feature flag; selecting a backend
/// that wasn't compiled returns `DeviceError::UnsupportedDevice`.
async fn create_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    match config.backend {
        DeviceBackend::Mock => {
            tracing::info!("Using Mock device backend");
            Ok(Box::new(MockProvider::new(MockDeviceConfig::default())))
        }

        DeviceBackend::BrainFlow => create_brainflow_provider(config),

        DeviceBackend::Emotiv => create_emotiv_provider(config).await,

        DeviceBackend::Auto => {
            tracing::info!("Auto-detecting device backend...");

            // Try Emotiv first (if compiled in)
            #[cfg(feature = "emotiv")]
            {
                match create_emotiv_provider(config).await {
                    Ok(provider) => {
                        if provider.is_available().await {
                            tracing::info!("Auto-detected: Emotiv Cortex service available");
                            return Ok(provider);
                        }
                        tracing::warn!(
                            "Emotiv Cortex service not reachable, trying next backend. \
                             Verify the Emotiv Launcher is running and check Settings > Device > Cortex URL."
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Emotiv provider creation failed: {}, trying next backend",
                            e
                        );
                    }
                }
            }

            // Try BrainFlow next (if compiled in)
            #[cfg(feature = "brainflow")]
            {
                match create_brainflow_provider(config) {
                    Ok(provider) => {
                        if provider.is_available().await {
                            tracing::info!("Auto-detected: BrainFlow library available");
                            return Ok(provider);
                        }
                        tracing::warn!("BrainFlow not available, falling back to Mock");
                    }
                    Err(e) => {
                        tracing::warn!(
                            "BrainFlow provider creation failed: {}, falling back to Mock",
                            e
                        );
                    }
                }
            }

            // Fall back to Mock
            tracing::warn!(
                "Auto-detect: no real device backend available, falling back to Mock. \
                 If you have an Emotiv headset, check that the Emotiv Launcher is running."
            );
            Ok(Box::new(MockProvider::new(MockDeviceConfig::default())))
        }
    }
}

// ─── BrainFlow ───────────────────────────────────────────────────────────────

#[cfg(feature = "brainflow")]
fn create_brainflow_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    use neurohid_device::brainflow::{BoardParams, BrainFlowConfig, BrainFlowDeviceProvider};

    tracing::info!("Using BrainFlow device backend");

    let bf_config = if let Some(ref bf) = config.brainflow {
        let mut bf_cfg = BrainFlowConfig::default();
        bf_cfg.include_synthetic = bf.include_synthetic;

        // If a serial port or other params are specified, set them on the
        // default board set. The user would typically also narrow board_ids
        // in production, but for now we apply params globally.
        if bf.serial_port.is_some()
            || bf.ip_address.is_some()
            || bf.ip_port.is_some()
            || bf.mac_address.is_some()
        {
            let params = BoardParams {
                serial_port: bf.serial_port.clone(),
                ip_address: bf.ip_address.clone(),
                ip_port: bf.ip_port,
                mac_address: bf.mac_address.clone(),
                file: None,
            };
            // Apply to all configured boards
            for &bid in &bf_cfg.board_ids {
                bf_cfg.board_params.insert(bid as i32, params.clone());
            }
        }

        bf_cfg
    } else {
        BrainFlowConfig::default()
    };

    Ok(Box::new(BrainFlowDeviceProvider::new(bf_config)))
}

#[cfg(not(feature = "brainflow"))]
fn create_brainflow_provider(_config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    Err(DeviceError::UnsupportedDevice {
        device_type: "BrainFlow (compile with --features brainflow)".into(),
    }
    .into())
}

// ─── Emotiv ──────────────────────────────────────────────────────────────────

/// Wraps an `EmotivProvider` and keeps an optional WSL2 TCP relay alive
/// for the provider's entire lifetime.
///
/// When running in WSL2, the Emotiv Cortex API rejects JSON-RPC methods
/// from WSL2-originated connections (-32601). This wrapper holds the native
/// Windows TCP relay process that proxies connections so Cortex sees them
/// as originating from a Windows process.
///
/// On non-WSL2 systems, `_relay` is `None` and this wrapper is transparent.
#[cfg(feature = "emotiv")]
struct RelayEmotivProvider {
    inner: neurohid_device::emotiv::EmotivProvider,
    #[cfg(unix)]
    _relay: Option<super::wsl2_relay::Wsl2Relay>,
}

#[cfg(feature = "emotiv")]
#[async_trait::async_trait]
impl DeviceProvider for RelayEmotivProvider {
    fn device_type(&self) -> neurohid_types::device::DeviceType {
        self.inner.device_type()
    }

    async fn is_available(&self) -> bool {
        self.inner.is_available().await
    }

    async fn discover(&self) -> Result<Vec<neurohid_types::device::DeviceInfo>> {
        self.inner.discover().await
    }

    async fn connect(
        &self,
        device_id: &neurohid_types::device::DeviceId,
        settings: Option<neurohid_types::device::ConnectionSettings>,
    ) -> Result<Box<dyn neurohid_device::Device>> {
        self.inner.connect(device_id, settings).await
    }
}

#[cfg(feature = "emotiv")]
async fn create_emotiv_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    use neurohid_device::emotiv::EmotivProvider;

    tracing::info!("Using Emotiv device backend");

    let mut emotiv_config = config.emotiv.clone().unwrap_or_default();

    // WSL2: launch a native Windows TCP relay so the Cortex service sees
    // connections from a Windows process (it rejects WSL2-originated ones).
    #[cfg(unix)]
    let relay = {
        let is_wsl2 = std::env::var("WSL_DISTRO_NAME").is_ok() || std::env::var("WSLENV").is_ok();

        if is_wsl2
            && (emotiv_config.cortex_url.contains("localhost")
                || emotiv_config.cortex_url.contains("127.0.0.1"))
        {
            let target_port = extract_port_from_wss_url(&emotiv_config.cortex_url).unwrap_or(6868);

            match super::wsl2_relay::Wsl2Relay::launch(target_port).await {
                Ok(Some(r)) => {
                    tracing::info!(
                        relay_port = r.port(),
                        target_port,
                        "WSL2 TCP relay active, routing Cortex connections through \
                         native Windows process"
                    );
                    emotiv_config.cortex_url = super::wsl2_relay::rewrite_url_for_relay(
                        &emotiv_config.cortex_url,
                        r.port(),
                    );
                    Some(r)
                }
                Ok(None) => {
                    tracing::warn!(
                        "WSL2 detected but PowerShell relay unavailable (interop \
                         may be disabled); falling back to direct connection. \
                         Cortex API may return -32601 errors."
                    );
                    emotiv_config.cortex_url =
                        resolve_wsl2_cortex_url(&emotiv_config.cortex_url).await;
                    None
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Failed to launch WSL2 relay; falling back to direct connection"
                    );
                    emotiv_config.cortex_url =
                        resolve_wsl2_cortex_url(&emotiv_config.cortex_url).await;
                    None
                }
            }
        } else {
            // Not WSL2, or URL doesn't target localhost — no relay needed
            if is_wsl2 {
                // WSL2 with non-localhost URL (user configured a remote endpoint)
                tracing::debug!("WSL2 detected but Cortex URL is not localhost; skipping relay");
            }
            emotiv_config.cortex_url = resolve_wsl2_cortex_url(&emotiv_config.cortex_url).await;
            None
        }
    };

    // On non-Unix platforms, no relay is needed (not WSL2)
    #[cfg(not(unix))]
    {
        let _ = resolve_wsl2_cortex_url; // suppress unused warning
    }

    tracing::info!(cortex_url = %emotiv_config.cortex_url, "Emotiv Cortex URL");

    let (client_id, client_secret) = neurohid_storage::get_emotiv_credentials().map_err(|e| {
        DeviceError::PermissionDenied(format!(
            "Failed to read Emotiv credentials from keyring: {}. \
             Set them in Settings > Device > Emotiv API Credentials.",
            e
        ))
    })?;

    let provider = EmotivProvider::new(emotiv_config, client_id, client_secret);

    Ok(Box::new(RelayEmotivProvider {
        inner: provider,
        #[cfg(unix)]
        _relay: relay,
    }))
}

#[cfg(not(feature = "emotiv"))]
async fn create_emotiv_provider(_config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    Err(DeviceError::UnsupportedDevice {
        device_type: "Emotiv (compile with --features emotiv)".into(),
    }
    .into())
}

// ─── WSL2 Host Resolution ──────────────────────────────────────────────────

/// If running inside WSL2 and the Cortex URL points at localhost, probe the
/// network to decide whether to rewrite the URL.
///
/// **Note**: This function only handles TCP-level reachability (NAT mode
/// IP rewriting). It does NOT solve the Cortex API `-32601` issue where the
/// service rejects JSON-RPC methods from WSL2-originated connections. The
/// primary fix for that is the WSL2 TCP relay (see [`super::wsl2_relay`]).
/// This function is used as a fallback when the relay cannot be launched.
///
/// 1. **Try localhost first** (TCP connect with 2s timeout). If it succeeds,
///    the URL works as-is — this covers mirrored networking, port forwarding,
///    and any other configuration where localhost already reaches the host.
/// 2. **Fall back to Windows host IP** from `/etc/resolv.conf` or `ip route`,
///    verifying reachability with a TCP probe before rewriting.
///
/// This avoids unreliable filesystem heuristics (e.g. `/sys/class/net/eth0`)
/// that vary across WSL2 kernel versions and networking configurations.
///
/// Detection: `WSL_DISTRO_NAME` or `WSLENV` environment variables.
///
/// Returns the original URL unchanged when not in WSL2, the URL doesn't
/// target localhost, or no reachable alternative is found.
#[cfg(all(unix, feature = "emotiv"))]
async fn resolve_wsl2_cortex_url(url: &str) -> String {
    let is_wsl2 = std::env::var("WSL_DISTRO_NAME").is_ok() || std::env::var("WSLENV").is_ok();
    if !is_wsl2 {
        return url.to_string();
    }

    if !url.contains("localhost") && !url.contains("127.0.0.1") {
        return url.to_string();
    }

    let port = extract_port_from_wss_url(url).unwrap_or(6868);

    // Try localhost first — works in mirrored networking, port-forwarded setups,
    // and any config where localhost already reaches the Windows host.
    if tcp_probe("127.0.0.1", port).await {
        tracing::info!(
            port,
            "WSL2: localhost:{} is reachable, using URL unchanged",
            port,
        );
        return url.to_string();
    }

    tracing::debug!(
        port,
        "WSL2: localhost:{} not reachable, trying Windows host IP",
        port
    );

    // localhost failed — try the Windows host IP from resolv.conf
    if let Some(host_ip) = read_wsl2_host_from_resolv_conf() {
        if tcp_probe(&host_ip, port).await {
            tracing::info!(
                host_ip = %host_ip,
                "WSL2 NAT mode: rewriting Cortex URL to reach Windows host"
            );
            return url
                .replace("localhost", &host_ip)
                .replace("127.0.0.1", &host_ip);
        }
        tracing::debug!(host_ip = %host_ip, "resolv.conf host IP not reachable on port {}", port);
    }

    // Try the default gateway from ip route
    if let Some(host_ip) = read_wsl2_host_from_ip_route() {
        if tcp_probe(&host_ip, port).await {
            tracing::info!(
                host_ip = %host_ip,
                "WSL2 NAT mode (via ip route): rewriting Cortex URL to reach Windows host"
            );
            return url
                .replace("localhost", &host_ip)
                .replace("127.0.0.1", &host_ip);
        }
        tracing::debug!(host_ip = %host_ip, "ip route gateway not reachable on port {}", port);
    }

    tracing::warn!(
        "WSL2 detected but Cortex not reachable on localhost or resolved Windows host IPs; \
         using original URL '{}'. Verify the Emotiv Launcher is running and check \
         Settings > Device > Cortex URL.",
        url
    );
    url.to_string()
}

/// On non-Unix platforms, WSL2 resolution is a no-op.
#[cfg(all(not(unix), feature = "emotiv"))]
async fn resolve_wsl2_cortex_url(url: &str) -> String {
    url.to_string()
}

/// Attempt a TCP connection to `host:port` with a 2-second timeout.
/// Returns `true` if the connection was established.
#[cfg(all(unix, feature = "emotiv"))]
async fn tcp_probe(host: &str, port: u16) -> bool {
    use std::time::Duration;
    let addr = format!("{}:{}", host, port);
    tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

/// Extract the port number from a `wss://host:port` or `ws://host:port` URL.
#[cfg(any(feature = "emotiv", test))]
fn extract_port_from_wss_url(url: &str) -> Option<u16> {
    // Strip schema, then split on ':' to find the port at the end
    let after_schema = url.split("://").nth(1)?;
    after_schema
        .split(':')
        .nth(1)?
        .trim_end_matches('/')
        .parse()
        .ok()
}

/// Parse `/etc/resolv.conf` for the nameserver entry.
/// In WSL2 with default (NAT) networking, this points at the Windows host.
#[cfg(all(unix, feature = "emotiv"))]
fn read_wsl2_host_from_resolv_conf() -> Option<String> {
    let contents = std::fs::read_to_string("/etc/resolv.conf").ok()?;
    parse_nameserver_from_resolv_conf(&contents)
}

/// Parse `ip route show default` output for the gateway IP.
#[cfg(all(unix, feature = "emotiv"))]
fn read_wsl2_host_from_ip_route() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_gateway_from_ip_route(&stdout)
}

/// Extract the first `nameserver` IPv4 address from resolv.conf contents.
#[cfg(any(feature = "emotiv", test))]
fn parse_nameserver_from_resolv_conf(contents: &str) -> Option<String> {
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with("nameserver") {
            let ip = line.split_whitespace().nth(1)?;
            if ip.parse::<std::net::Ipv4Addr>().is_ok() {
                return Some(ip.to_string());
            }
        }
    }
    None
}

/// Extract the default gateway IPv4 address from `ip route` output.
#[cfg(any(feature = "emotiv", test))]
fn parse_gateway_from_ip_route(output: &str) -> Option<String> {
    for line in output.lines() {
        if line.starts_with("default via") {
            let ip = line.split_whitespace().nth(2)?;
            if ip.parse::<std::net::Ipv4Addr>().is_ok() {
                return Some(ip.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_resolv_conf_typical_wsl2() {
        let contents = "\
# This file was automatically generated by WSL.
nameserver 172.28.80.1
";
        assert_eq!(
            parse_nameserver_from_resolv_conf(contents),
            Some("172.28.80.1".to_string()),
        );
    }

    #[test]
    fn test_parse_resolv_conf_skips_comments() {
        let contents = "\
# nameserver 8.8.8.8
nameserver 172.16.0.1
";
        assert_eq!(
            parse_nameserver_from_resolv_conf(contents),
            Some("172.16.0.1".to_string()),
        );
    }

    #[test]
    fn test_parse_resolv_conf_no_nameserver() {
        let contents = "search example.com\n";
        assert_eq!(parse_nameserver_from_resolv_conf(contents), None);
    }

    #[test]
    fn test_parse_resolv_conf_skips_ipv6() {
        let contents = "nameserver fd00::1\nnameserver 10.0.0.1\n";
        assert_eq!(
            parse_nameserver_from_resolv_conf(contents),
            Some("10.0.0.1".to_string()),
        );
    }

    #[test]
    fn test_parse_ip_route_typical() {
        let output = "\
default via 172.28.80.1 dev eth0
172.28.80.0/20 dev eth0 proto kernel scope link src 172.28.82.45
";
        assert_eq!(
            parse_gateway_from_ip_route(output),
            Some("172.28.80.1".to_string()),
        );
    }

    #[test]
    fn test_parse_ip_route_no_default() {
        let output = "172.28.80.0/20 dev eth0 proto kernel scope link src 172.28.82.45\n";
        assert_eq!(parse_gateway_from_ip_route(output), None);
    }

    #[test]
    fn test_url_rewrite_localhost() {
        let url = "wss://localhost:6868";
        let rewritten = url.replace("localhost", "172.28.80.1");
        assert_eq!(rewritten, "wss://172.28.80.1:6868");
    }

    #[test]
    fn test_url_rewrite_127_0_0_1() {
        let url = "wss://127.0.0.1:6868";
        let rewritten = url.replace("127.0.0.1", "172.28.80.1");
        assert_eq!(rewritten, "wss://172.28.80.1:6868");
    }

    #[test]
    fn test_url_no_rewrite_for_custom_host() {
        let url = "wss://192.168.1.100:6868";
        assert!(!url.contains("localhost") && !url.contains("127.0.0.1"));
    }

    #[test]
    fn test_extract_port_from_wss_url() {
        assert_eq!(
            extract_port_from_wss_url("wss://localhost:6868"),
            Some(6868)
        );
        assert_eq!(extract_port_from_wss_url("ws://127.0.0.1:8080"), Some(8080));
        assert_eq!(
            extract_port_from_wss_url("wss://example.com:443/"),
            Some(443)
        );
        assert_eq!(extract_port_from_wss_url("wss://no-port"), None);
        assert_eq!(extract_port_from_wss_url("not-a-url"), None);
    }
}
