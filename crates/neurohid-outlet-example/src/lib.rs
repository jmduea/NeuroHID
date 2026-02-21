//! # NeuroHID Outlet Example
//!
//! Minimal outlet extension that implements the outlet contract from `neurohid-types`.
//! Receives config and channels, runs until shutdown (log-only / no-op). Used to validate
//! the extension path and CI e2e.

use async_trait::async_trait;
use neurohid_types::{
    config::OutletConfig,
    error::Result,
    outlet::{Outlet, OutletChannels},
};
use tokio::sync::broadcast;

/// Minimal outlet: runs until shutdown, optionally logs that it is running.
struct ExampleOutlet {
    _config: OutletConfig,
}

#[async_trait]
impl Outlet for ExampleOutlet {
    async fn run(
        self: Box<Self>,
        mut shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        tracing::info!("neurohid-outlet-example: outlet running until shutdown");
        let _ = shutdown.recv().await;
        tracing::debug!("neurohid-outlet-example: shutdown received");
        Ok(())
    }
}

/// Factory symbol required by the extension loader (see docs/extension-contracts.md and
/// neurohid-core extension_registry). Same toolchain as host required for ABI.
#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn neurohid_outlet_create(
    config: OutletConfig,
    _channels: OutletChannels,
) -> Result<Box<dyn Outlet>> {
    Ok(Box::new(ExampleOutlet { _config: config }))
}
