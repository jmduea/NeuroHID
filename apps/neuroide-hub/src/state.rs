//! # Hub State
//!
//! Unified application state for the hub. Contains storage handles, cached
//! profile/config data, and a snapshot of the running service's state.

use neurohid_storage::{ConfigStore, ProfileStore};
use neurohid_types::{
    config::SystemConfig,
    control::ControlSnapshot,
    profile::{ProfileId, ProfileMetadata},
};

/// Type alias for the canonical runtime snapshot.
///
/// Previously a standalone struct with fields duplicated from
/// [`ControlSnapshot`]. Now unified to eliminate field-drift risk.
pub type ServiceSnapshot = ControlSnapshot;

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
