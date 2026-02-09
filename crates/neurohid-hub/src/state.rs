//! # Hub State
//!
//! Unified application state for the hub. Contains storage handles, cached
//! profile/config data, and a snapshot of the running service's state.

use neurohid_types::{
    config::SystemConfig,
    device::DiscoveredStream,
    profile::{ProfileId, ProfileMetadata},
};
use neurohid_storage::{ProfileStore, ConfigStore};

/// Snapshot of the running service state, updated each frame from
/// `Arc<RwLock<ServiceState>>` via non-blocking `try_read()`.
///
/// This is distinct from `neurohid_core::service::ServiceState` (runtime-owned)
/// and `neurohid_types::config::ServiceState` (serializable IPC type).
#[derive(Debug, Clone)]
pub struct ServiceSnapshot {
    pub running: bool,
    pub device_connected: bool,
    pub device_name: Option<String>,
    /// Battery level of connected device(s), if reported.
    pub device_battery: Option<u8>,
    pub signal_quality: f32,
    pub actions_emitted: u64,
    pub errors_detected: u64,
    pub uptime_secs: u64,
    #[allow(dead_code)] // will be used for IPC status display
    pub ipc_connected: bool,
    pub calibration_mode: bool,
    pub active_profile_name: Option<String>,
    /// If a service task failed at runtime, (task_name, error_message).
    pub task_error: Option<(String, String)>,
    /// LSL streams discovered on the network.
    pub discovered_streams: Vec<DiscoveredStream>,
}

impl Default for ServiceSnapshot {
    fn default() -> Self {
        Self {
            running: false,
            device_connected: false,
            device_name: None,
            device_battery: None,
            signal_quality: 0.0,
            actions_emitted: 0,
            errors_detected: 0,
            uptime_secs: 0,
            ipc_connected: false,
            calibration_mode: false,
            active_profile_name: None,
            task_error: None,
            discovered_streams: Vec::new(),
        }
    }
}

/// Central hub state.
pub struct HubState {
    pub profile_store: ProfileStore,
    pub config_store: ConfigStore,
    pub config: SystemConfig,
    pub profiles: Vec<ProfileMetadata>,
    pub active_profile_id: Option<ProfileId>,
    pub service_snapshot: ServiceSnapshot,
    pub init_error: Option<String>,
}

impl HubState {
    /// Initialize hub state by loading configuration and profiles.
    pub fn new(
        profile_store: ProfileStore,
        config_store: ConfigStore,
        config: SystemConfig,
        profiles: Vec<ProfileMetadata>,
    ) -> Self {
        let active_profile_id = profiles.first().map(|p| p.id.clone());
        Self {
            profile_store,
            config_store,
            config,
            profiles,
            active_profile_id,
            service_snapshot: ServiceSnapshot::default(),
            init_error: None,
        }
    }

    /// Refresh the profile list from storage.
    pub fn refresh_profiles(&mut self, runtime: &tokio::runtime::Runtime) {
        match runtime.block_on(self.profile_store.list_profiles()) {
            Ok(profiles) => self.profiles = profiles,
            Err(e) => tracing::warn!("Failed to refresh profiles: {}", e),
        }
    }

    /// Compute the error rate as a percentage.
    pub fn error_rate(&self) -> f32 {
        let snap = &self.service_snapshot;
        let total = snap.actions_emitted + snap.errors_detected;
        if total == 0 {
            0.0
        } else {
            snap.errors_detected as f32 / total as f32 * 100.0
        }
    }
}
