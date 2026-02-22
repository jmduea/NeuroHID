//! Embedded runtime lifecycle methods.

use neurohid_core::runtime::{RuntimeBuilder, RuntimeCommand};
use neurohid_storage::ProfileStore;
use neurohid_types::{config::SystemConfig, profile::ProfileId};

use super::ServiceManager;

impl ServiceManager {
    pub(super) fn start_embedded(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        config: SystemConfig,
        profile_store: Option<ProfileStore>,
        profile_id: Option<ProfileId>,
    ) {
        // If we have a handle but the service has stopped itself (e.g., task failure),
        // drop the stale handle so the user can restart without clicking "Stop" first.
        let should_drop_handle = self
            .runtime_handle
            .as_ref()
            .is_some_and(|rt| !rt.is_alive());
        if should_drop_handle {
            self.runtime_handle.take();
            self.bus_connected = false;
        }

        if self.runtime_handle.is_some() {
            tracing::warn!("Service already running");
            return;
        }

        let mut builder = RuntimeBuilder::new(config);
        if let Some(store) = profile_store {
            builder = builder.with_profile_store(store);
        }
        if let Some(id) = profile_id {
            builder = builder.with_profile_id(id);
        }

        let result = runtime.block_on(async { builder.start().await });

        match result {
            Ok(handle) => {
                self.runtime_handle = Some(handle);
                self.last_error = None;
                self.bus_connected = false;
                tracing::info!("Service started");
            }
            Err(e) => {
                self.set_last_error(format!("Failed to create service: {}", e));
            }
        }
    }

    pub(super) fn stop_embedded(&mut self) {
        if let Some(rt) = self.runtime_handle.take() {
            let _ = rt.command(RuntimeCommand::Stop);
            self.bus_connected = false;
            tracing::info!("Service stop signal sent");
        }
    }
}
