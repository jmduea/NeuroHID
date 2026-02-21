//! # NeuroHID Hub
//!
//! The unified GUI application for NeuroHID. Combines device management,
//! calibration, profile management, and service control into a single window.

use eframe::egui;

use neurohid_hub::HubApp;

#[path = "../tracing_init.rs"]
mod tracing_init;

struct CombinedLogger {
    egui_logger: egui_logger::EguiLogger,
    tracing_logger: tracing_log::LogTracer,
}

impl log::Log for CombinedLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.egui_logger.enabled(metadata) || self.tracing_logger.enabled(metadata)
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.egui_logger.enabled(record.metadata()) {
            self.egui_logger.log(record);
        }
        if self.tracing_logger.enabled(record.metadata()) {
            self.tracing_logger.log(record);
        }
    }

    fn flush(&self) {
        self.egui_logger.flush();
        self.tracing_logger.flush();
    }
}

fn init_hub_logger() -> anyhow::Result<()> {
    let logger = CombinedLogger {
        egui_logger: egui_logger::builder()
            .max_level(log::LevelFilter::Info)
            .build(),
        tracing_logger: tracing_log::LogTracer::new(),
    };

    log::set_max_level(log::LevelFilter::Trace);
    log::set_boxed_logger(Box::new(logger))
        .map_err(|error| anyhow::anyhow!("Failed to initialize combined logger: {}", error))
}

const CLI_SUBCOMMANDS: &[&str] = &["device", "config", "pipeline", "control", "daemon", "record"];

/// Handle `neurohid extensions list` and `neurohid extensions refresh`. Uses the same
/// discovery/registry as core; exits 0 with list or non-zero on discovery failure.
fn run_extensions_cli(args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }
    let sub = args[0].as_str();
    if sub != "list" && sub != "refresh" {
        return false;
    }
    let mut registry =
        neurohid_core::extension_registry::ExtensionRegistry::new(
            neurohid_core::extension_registry::default_extension_paths(),
        );
    if let Err(e) = registry.scan() {
        eprintln!("neurohid extensions: discovery failed: {}", e);
        std::process::exit(1);
    }
    let outlets = registry.list_outlets();
    let devices = registry.list_devices();
    let signal = registry.list_signal_preprocessors();
    let decoders = registry.list_decoders();
    for e in &outlets {
        println!("outlet\t{}\t{}", e.name, e.path.display());
    }
    for e in &devices {
        println!("device\t{}\t{}", e.name, e.path.display());
    }
    for e in &signal {
        println!("signal_preprocessing\t{}\t{}", e.name, e.path.display());
    }
    for e in &decoders {
        println!("decoder\t{}\t{}", e.name, e.path.display());
    }
    true
}

/// If argv suggests a CLI subcommand (device list, control snapshot, etc.), run neurohid-service
/// with the same args and exit with its code. No GUI or heavy init. Returns only when not dispatching.
fn maybe_dispatch_to_service() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1].as_str() == "extensions" {
        if run_extensions_cli(args.get(2..).unwrap_or_default()) {
            std::process::exit(0);
        }
        if args.len() == 2 {
            eprintln!("neurohid extensions: use 'list' or 'refresh'");
            std::process::exit(1);
        }
    }
    if args.len() < 2 {
        return;
    }
    let first = args[1].as_str();
    if !CLI_SUBCOMMANDS.contains(&first) {
        return;
    }
    let service_exe = locate_service_binary();
    let status = std::process::Command::new(service_exe)
        .args(args.iter().skip(1))
        .status();
    match status {
        Ok(s) => std::process::exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("neurohid: failed to run service binary: {}", e);
            std::process::exit(1);
        }
    }
}

fn locate_service_binary() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
        let name = if cfg!(windows) { "neurohid-service.exe" } else { "neurohid-service" };
        let candidate = dir.join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    let name = if cfg!(windows) { "neurohid-service.exe" } else { "neurohid-service" };
    std::path::PathBuf::from(name)
}

fn main() {
    maybe_dispatch_to_service();

    init_hub_logger().expect("Failed to initialize Hub logger");
    tracing_init::init_tracing("info").expect("Failed to initialize tracing");

    tracing::info!("Starting NeuroHID Hub");

    // On Unix, ignore SIGPIPE so that broken clipboard connections (arboard/x11rb)
    // return EPIPE errors instead of killing the process.
    #[cfg(unix)]
    {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        let _ = signal_hook::flag::register(
            signal_hook::consts::SIGPIPE,
            Arc::new(AtomicBool::new(false)),
        );
    }

    // On WSL2, force the X11 backend.
    #[cfg(unix)]
    if std::env::var("WSL_DISTRO_NAME").is_ok() || std::env::var("WSLENV").is_ok() {
        if std::env::var("WINIT_UNIX_BACKEND").is_err() {
            // SAFETY: called before any threads are spawned (single-threaded main).
            unsafe { std::env::set_var("WINIT_UNIX_BACKEND", "x11") };
        }
        tracing::info!("WSL2 detected, forcing X11 backend");
    }

    // Create the tokio runtime for async operations (storage, service, etc.)
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    // Configure the native window
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("NeuroHID Hub"),
        ..Default::default()
    };

    // Run the application
    match eframe::run_native(
        "NeuroHID Hub",
        options,
        Box::new(move |cc| Ok(Box::new(HubApp::new(cc, runtime)))),
    ) {
        Ok(()) => {}
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("ExitFailure") {
                tracing::warn!("eframe exited with non-zero status (likely harmless on WSL2): {e}");
            } else {
                tracing::error!("Fatal eframe error: {e}");
                std::process::exit(1);
            }
        }
    }
}
