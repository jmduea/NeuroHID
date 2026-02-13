//! Example host application embedding the NeuroHID managed runtime.
//!
//! Run with:
//!   cargo run -p neurohid-sdk --example embedded_runtime --features "runtime,types"

#[cfg(all(feature = "runtime", feature = "types"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    use neurohid_sdk::runtime::runtime::{RuntimeBuilder, RuntimeCommand};
    use neurohid_sdk::types::config::{DeviceBackend, SystemConfig};

    let mut config = SystemConfig::default();
    config.device.backend = DeviceBackend::Mock;
    config.service.ipc_simulation_enabled = true;

    let runtime = RuntimeBuilder::new(config).start().await?;
    runtime.command(RuntimeCommand::RescanStreams)?;

    for _ in 0..5 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let snapshot = runtime.snapshot();
        println!(
            "running={} device_connected={} quality={:.2} actions={} decode_p95={}us",
            snapshot.running,
            snapshot.device_connected,
            snapshot.signal_quality,
            snapshot.actions_emitted,
            snapshot.decode_latency_p95_us
        );
    }

    runtime.command(RuntimeCommand::Stop)?;
    runtime.wait().await?;
    Ok(())
}

#[cfg(not(all(feature = "runtime", feature = "types")))]
fn main() {
    eprintln!(
        "This example requires features 'runtime' and 'types'.\n\
         Run: cargo run -p neurohid-sdk --example embedded_runtime --features \"runtime,types\""
    );
}
