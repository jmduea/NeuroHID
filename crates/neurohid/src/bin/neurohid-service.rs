//! # NeuroHID Service (Headless)
//!
//! This is the standalone headless service binary. It runs continuously in the
//! background, connecting to your EEG device, processing signals, communicating
//! with the Python ML layer, and emitting HID events based on decoded intentions.
//!
//! For the unified GUI experience, use `neurohid` (the hub binary) instead.

use clap::Parser;
use std::path::PathBuf;
use tokio::sync::broadcast;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use neurohid_core::service::NeuroHidService;

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let log_level = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(format!("neurohid={}", log_level).parse().unwrap()),
        )
        .init();

    if !args.foreground {
        tracing::warn!("Background daemon mode is not implemented yet; running in foreground");
    }

    tracing::info!("Starting NeuroHID service");

    let (profile_store, config_store) = neurohid_storage::initialize()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize storage: {}", e))?;

    let config = config_store
        .load()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load configuration: {}", e))?;

    tracing::info!("Configuration loaded");

    let profile_id = if let Some(profile_name) = &args.profile {
        Some(neurohid_types::profile::ProfileId::new(profile_name))
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
                        "Profile '{}' is not fully calibrated. \
                         HID actions will not be emitted until calibration is complete.",
                        pid
                    );
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load profile metadata for {}: {}", pid, e);
            }
        }
    } else {
        tracing::info!("Running without a profile");
    }

    if let Some(source_dir) = &args.import_candidate_dir {
        if args.export_session_logs_dir.is_some() {
            return Err(anyhow::anyhow!(
                "--import-candidate-dir and --export-session-logs-dir are mutually exclusive"
            ));
        }
        let Some(pid) = profile_id.as_ref() else {
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
        return Ok(());
    }

    if let Some(output_dir) = &args.export_session_logs_dir {
        let Some(pid) = profile_id.as_ref() else {
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
        return Ok(());
    }

    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let shutdown_tx_clone = shutdown_tx.clone();

    ctrlc::set_handler(move || {
        tracing::info!("Shutdown signal received");
        let _ = shutdown_tx_clone.send(());
    })?;

    let service = NeuroHidService::new(
        config,
        Some(profile_store),
        profile_id,
        shutdown_tx.subscribe(),
    )
    .await?;

    tracing::info!("Service initialized, starting main loop");

    service.run().await?;

    tracing::info!("NeuroHID service stopped");
    Ok(())
}
