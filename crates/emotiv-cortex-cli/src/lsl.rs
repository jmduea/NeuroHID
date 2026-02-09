use colored::Colorize;
use emotiv_cortex_v2::headset::HeadsetModel;
use emotiv_cortex_v2::protocol::Streams;
use emotiv_cortex_v2::streams;
use emotiv_cortex_v2::CortexClient;
use futures::StreamExt;
use lsl::Pushable;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

/// Configure liblsl to avoid multicast warnings on Windows.
///
/// liblsl tries to bind multicast responders on 224.0.0.1 ("All Hosts"), which
/// fails on Windows interfaces that don't support it (Hyper-V, VPN, WSL2).
/// This writes a config file restricting liblsl to the standard LSL multicast
/// group and points `LSLAPICFG` at it.
fn configure_lsl() {
    let cfg_content = "\
[multicast]
resolve_scope = link
listen_address = 239.255.172.215
";

    let path = std::env::temp_dir().join("neurohid_lsl_api.cfg");
    if std::fs::write(&path, cfg_content).is_ok() {
        std::env::set_var("LSLAPICFG", &path);
    }
}

/// Wrapper around `lsl::StreamOutlet` that is `Send`.
///
/// liblsl outlets are internally thread-safe (the C library uses its own locking),
/// but the Rust bindings contain a raw pointer which is `!Send` by default.
/// This wrapper allows the outlet to be moved into a `tokio::spawn` task.
struct SendOutlet(lsl::StreamOutlet);

// SAFETY: liblsl outlets are thread-safe. The underlying C library handles
// synchronization for push operations.
unsafe impl Send for SendOutlet {}

impl SendOutlet {
    fn push_sample(&self, data: &Vec<f32>) -> Result<(), String> {
        self.0.push_sample(data).map_err(|e| format!("{e:?}"))
    }
}

/// Which streams to forward to LSL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LslStream {
    Eeg,
    Motion,
    BandPower,
    Metrics,
    MentalCommands,
    FacialExpressions,
    DeviceQuality,
    EegQuality,
}

impl LslStream {
    /// Human-readable label for menu display
    pub fn label(&self) -> &'static str {
        match self {
            LslStream::Eeg => "EEG",
            LslStream::Motion => "Motion",
            LslStream::BandPower => "Band Power",
            LslStream::Metrics => "Performance Metrics",
            LslStream::MentalCommands => "Mental Commands",
            LslStream::FacialExpressions => "Facial Expressions",
            LslStream::DeviceQuality => "Device Quality",
            LslStream::EegQuality => "EEG Quality",
        }
    }

    /// All available stream variants
    pub fn all() -> &'static [LslStream] {
        &[
            LslStream::Eeg,
            LslStream::Motion,
            LslStream::BandPower,
            LslStream::Metrics,
            LslStream::MentalCommands,
            LslStream::FacialExpressions,
            LslStream::DeviceQuality,
            LslStream::EegQuality,
        ]
    }
}

/// Create an LSL outlet for EEG data
fn create_eeg_outlet(
    model: &HeadsetModel,
    source_id: &str,
) -> Result<SendOutlet, Box<dyn std::error::Error>> {
    let info = lsl::StreamInfo::new(
        "EmotivEEG",
        "EEG",
        model.num_channels() as u32,
        model.sampling_rate_hz(),
        lsl::ChannelFormat::Float32,
        source_id,
    )?;
    Ok(SendOutlet(lsl::StreamOutlet::new(&info, 0, 360)?))
}

/// Create an LSL outlet for Motion data
fn create_motion_outlet(
    source_id: &str,
) -> Result<SendOutlet, Box<dyn std::error::Error>> {
    let info = lsl::StreamInfo::new(
        "EmotivMotion",
        "Motion",
        10, // 3 accel + 3 mag + 4 quat
        0.0,
        lsl::ChannelFormat::Float32,
        source_id,
    )?;
    Ok(SendOutlet(lsl::StreamOutlet::new(&info, 0, 360)?))
}

/// Create an LSL outlet for Band Power data
fn create_band_power_outlet(
    model: &HeadsetModel,
    source_id: &str,
) -> Result<SendOutlet, Box<dyn std::error::Error>> {
    let info = lsl::StreamInfo::new(
        "EmotivBandPower",
        "FFT",
        (model.num_channels() * 5) as u32,
        0.0,
        lsl::ChannelFormat::Float32,
        source_id,
    )?;
    Ok(SendOutlet(lsl::StreamOutlet::new(&info, 0, 360)?))
}

/// Create an LSL outlet for Performance Metrics
fn create_metrics_outlet(
    source_id: &str,
) -> Result<SendOutlet, Box<dyn std::error::Error>> {
    let info = lsl::StreamInfo::new(
        "EmotivMetrics",
        "Metrics",
        8,
        0.0,
        lsl::ChannelFormat::Float32,
        source_id,
    )?;
    Ok(SendOutlet(lsl::StreamOutlet::new(&info, 0, 360)?))
}

/// Create an LSL outlet for Mental Commands
fn create_mental_commands_outlet(
    source_id: &str,
) -> Result<SendOutlet, Box<dyn std::error::Error>> {
    let info = lsl::StreamInfo::new(
        "EmotivMentalCommands",
        "Markers",
        1,
        0.0,
        lsl::ChannelFormat::Float32,
        source_id,
    )?;
    Ok(SendOutlet(lsl::StreamOutlet::new(&info, 0, 360)?))
}

/// Create an LSL outlet for Facial Expressions
fn create_facial_expressions_outlet(
    source_id: &str,
) -> Result<SendOutlet, Box<dyn std::error::Error>> {
    let info = lsl::StreamInfo::new(
        "EmotivFacialExpressions",
        "Markers",
        3,
        0.0,
        lsl::ChannelFormat::Float32,
        source_id,
    )?;
    Ok(SendOutlet(lsl::StreamOutlet::new(&info, 0, 360)?))
}

/// Create an LSL outlet for Device Quality
fn create_device_quality_outlet(
    model: &HeadsetModel,
    source_id: &str,
) -> Result<SendOutlet, Box<dyn std::error::Error>> {
    let info = lsl::StreamInfo::new(
        "EmotivDeviceQuality",
        "Quality",
        (model.num_channels() + 3) as u32,
        0.0,
        lsl::ChannelFormat::Float32,
        source_id,
    )?;
    Ok(SendOutlet(lsl::StreamOutlet::new(&info, 0, 360)?))
}

/// Create an LSL outlet for EEG Quality
fn create_eeg_quality_outlet(
    model: &HeadsetModel,
    source_id: &str,
) -> Result<SendOutlet, Box<dyn std::error::Error>> {
    let info = lsl::StreamInfo::new(
        "EmotivEEGQuality",
        "Quality",
        (model.num_channels() + 3) as u32,
        0.0,
        lsl::ChannelFormat::Float32,
        source_id,
    )?;
    Ok(SendOutlet(lsl::StreamOutlet::new(&info, 0, 360)?))
}

/// Run LSL streaming - subscribes to selected Cortex streams and forwards to LSL outlets.
/// Blocks until Ctrl+C.
pub async fn run_lsl_streaming(
    client: &CortexClient,
    token: &str,
    session_id: &str,
    model: &HeadsetModel,
    selected: &[LslStream],
    source_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if selected.is_empty() {
        eprintln!("{}", "No streams selected".yellow());
        return Ok(());
    }

    configure_lsl();

    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
    let mut tasks = Vec::new();
    let mut active_outlets = Vec::new();

    // Sample counters for periodic reporting
    let sample_counts: Arc<Vec<(String, Arc<AtomicU64>)>> = Arc::new(
        selected
            .iter()
            .map(|s| (s.label().to_string(), Arc::new(AtomicU64::new(0))))
            .collect(),
    );

    for (idx, stream_type) in selected.iter().enumerate() {
        let mut shutdown_rx = shutdown_tx.subscribe();
        let counter = sample_counts[idx].1.clone();

        match stream_type {
            LslStream::Eeg => {
                let mut stream =
                    streams::subscribe_eeg(client, token, session_id, model.num_channels()).await?;
                let outlet = create_eeg_outlet(model, source_id)?;
                active_outlets.push("EmotivEEG".cyan().to_string());

                tasks.push(tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            item = stream.next() => {
                                let Some(data) = item else { break };
                                if let Err(e) = outlet.push_sample(&data.channels) {
                                    tracing::warn!("Failed to push EEG sample: {}", e);
                                } else {
                                    counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            _ = shutdown_rx.recv() => break,
                        }
                    }
                }));
            }

            LslStream::Motion => {
                let mut stream =
                    streams::subscribe_motion(client, token, session_id).await?;
                let outlet = create_motion_outlet(source_id)?;
                active_outlets.push("EmotivMotion".cyan().to_string());

                tasks.push(tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            item = stream.next() => {
                                let Some(data) = item else { break };
                                let mut sample = Vec::with_capacity(10);
                                sample.extend_from_slice(&data.accelerometer);
                                sample.extend_from_slice(&data.magnetometer);
                                if let Some(quat) = data.quaternion {
                                    sample.extend_from_slice(&quat);
                                } else {
                                    sample.extend_from_slice(&[0.0, 0.0, 0.0, 1.0]);
                                }
                                if let Err(e) = outlet.push_sample(&sample) {
                                    tracing::warn!("Failed to push Motion sample: {}", e);
                                } else {
                                    counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            _ = shutdown_rx.recv() => break,
                        }
                    }
                }));
            }

            LslStream::BandPower => {
                let mut stream =
                    streams::subscribe_band_power(client, token, session_id, model.num_channels())
                        .await?;
                let outlet = create_band_power_outlet(model, source_id)?;
                active_outlets.push("EmotivBandPower".cyan().to_string());

                tasks.push(tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            item = stream.next() => {
                                let Some(data) = item else { break };
                                let sample: Vec<f32> = data.channel_powers.iter().flatten().copied().collect();
                                if let Err(e) = outlet.push_sample(&sample) {
                                    tracing::warn!("Failed to push Band Power sample: {}", e);
                                } else {
                                    counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            _ = shutdown_rx.recv() => break,
                        }
                    }
                }));
            }

            LslStream::Metrics => {
                let mut stream =
                    streams::subscribe_metrics(client, token, session_id).await?;
                let outlet = create_metrics_outlet(source_id)?;
                active_outlets.push("EmotivMetrics".cyan().to_string());

                tasks.push(tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            item = stream.next() => {
                                let Some(data) = item else { break };
                                let sample = vec![
                                    data.engagement.unwrap_or(0.0),
                                    data.excitement.unwrap_or(0.0),
                                    data.long_excitement.unwrap_or(0.0),
                                    data.stress.unwrap_or(0.0),
                                    data.relaxation.unwrap_or(0.0),
                                    data.interest.unwrap_or(0.0),
                                    data.attention.unwrap_or(0.0),
                                    data.focus.unwrap_or(0.0),
                                ];
                                if let Err(e) = outlet.push_sample(&sample) {
                                    tracing::warn!("Failed to push Metrics sample: {}", e);
                                } else {
                                    counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            _ = shutdown_rx.recv() => break,
                        }
                    }
                }));
            }

            LslStream::MentalCommands => {
                let mut stream =
                    streams::subscribe_mental_commands(client, token, session_id).await?;
                let outlet = create_mental_commands_outlet(source_id)?;
                active_outlets.push("EmotivMentalCommands".cyan().to_string());

                tasks.push(tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            item = stream.next() => {
                                let Some(data) = item else { break };
                                let sample = vec![data.power];
                                if let Err(e) = outlet.push_sample(&sample) {
                                    tracing::warn!("Failed to push Mental Command sample: {}", e);
                                } else {
                                    counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            _ = shutdown_rx.recv() => break,
                        }
                    }
                }));
            }

            LslStream::FacialExpressions => {
                let mut stream =
                    streams::subscribe_facial_expressions(client, token, session_id).await?;
                let outlet = create_facial_expressions_outlet(source_id)?;
                active_outlets.push("EmotivFacialExpressions".cyan().to_string());

                tasks.push(tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            item = stream.next() => {
                                let Some(data) = item else { break };
                                let sample = vec![
                                    data.upper_face_power,
                                    data.lower_face_power,
                                    0.0, // placeholder
                                ];
                                if let Err(e) = outlet.push_sample(&sample) {
                                    tracing::warn!("Failed to push Facial Expression sample: {}", e);
                                } else {
                                    counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            _ = shutdown_rx.recv() => break,
                        }
                    }
                }));
            }

            LslStream::DeviceQuality => {
                let mut stream =
                    streams::subscribe_dev(client, token, session_id, model.num_channels()).await?;
                let outlet = create_device_quality_outlet(model, source_id)?;
                active_outlets.push("EmotivDeviceQuality".cyan().to_string());

                tasks.push(tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            item = stream.next() => {
                                let Some(data) = item else { break };
                                let mut sample = Vec::with_capacity(data.channel_quality.len() + 3);
                                sample.extend_from_slice(&data.channel_quality);
                                sample.push(data.battery_percent as f32);
                                sample.push(data.signal_strength);
                                sample.push(data.overall_quality);
                                if let Err(e) = outlet.push_sample(&sample) {
                                    tracing::warn!("Failed to push Device Quality sample: {}", e);
                                } else {
                                    counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            _ = shutdown_rx.recv() => break,
                        }
                    }
                }));
            }

            LslStream::EegQuality => {
                let mut stream =
                    streams::subscribe_eq(client, token, session_id, model.num_channels()).await?;
                let outlet = create_eeg_quality_outlet(model, source_id)?;
                active_outlets.push("EmotivEEGQuality".cyan().to_string());

                tasks.push(tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            item = stream.next() => {
                                let Some(data) = item else { break };
                                let mut sample = Vec::with_capacity(data.sensor_quality.len() + 3);
                                sample.extend_from_slice(&data.sensor_quality);
                                sample.push(data.battery_percent as f32);
                                sample.push(data.overall);
                                sample.push(data.sample_rate_quality);
                                if let Err(e) = outlet.push_sample(&sample) {
                                    tracing::warn!("Failed to push EEG Quality sample: {}", e);
                                } else {
                                    counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            _ = shutdown_rx.recv() => break,
                        }
                    }
                }));
            }
        }
    }

    println!(
        "{} {} {}",
        "LSL streaming active:".green().bold(),
        active_outlets.join(", "),
        "(Press Ctrl+C to stop)".dimmed()
    );

    // Spawn periodic stats reporter
    let stats_counts = sample_counts.clone();
    let stats_task = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(10));
        loop {
            ticker.tick().await;
            let stats: Vec<String> = stats_counts
                .iter()
                .map(|(name, count)| {
                    format!("{}: {}", name.cyan(), count.load(Ordering::Relaxed))
                })
                .collect();
            println!("{} {}", "Sample counts:".dimmed(), stats.join(", "));
        }
    });

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    println!("\n{}", "Shutting down LSL streaming...".yellow());

    // Signal all tasks to stop
    let _ = shutdown_tx.send(());
    stats_task.abort();

    // Wait for all tasks to complete with timeout
    let shutdown_timeout = tokio::time::timeout(Duration::from_secs(5), async {
        for task in tasks {
            let _ = task.await;
        }
    })
    .await;

    if shutdown_timeout.is_err() {
        eprintln!("{}", "Warning: Some tasks did not shut down cleanly".yellow());
    }

    // Unsubscribe from all streams
    let stream_names: Vec<&str> = selected
        .iter()
        .map(|s| match s {
            LslStream::Eeg => Streams::EEG,
            LslStream::Motion => Streams::MOT,
            LslStream::BandPower => Streams::POW,
            LslStream::Metrics => Streams::MET,
            LslStream::MentalCommands => Streams::COM,
            LslStream::FacialExpressions => Streams::FAC,
            LslStream::DeviceQuality => Streams::DEV,
            LslStream::EegQuality => Streams::EQ,
        })
        .collect();

    if let Err(e) = streams::unsubscribe(client, token, session_id, &stream_names).await {
        eprintln!("{} {}", "Warning: Failed to unsubscribe:".yellow(), e);
    }

    println!("{}", "LSL streaming stopped".green());
    Ok(())
}
