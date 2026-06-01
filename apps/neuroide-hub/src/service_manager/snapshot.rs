//! Snapshot and telemetry helpers for service manager state.

use std::time::Instant;

use neurohid_types::control::{
    ControlCommand, ControlRequest, ControlResponsePayload, TrainerSnapshot,
};

use crate::state::ServiceSnapshot;

use super::ServiceManager;
use super::{EXTERNAL_EVENT_STREAM_STALE_TIMEOUT, EXTERNAL_SNAPSHOT_POLL_INTERVAL};

impl ServiceManager {
    pub(super) fn snapshot_embedded(&mut self) -> ServiceSnapshot {
        let Some(rt) = &self.runtime_handle else {
            return ServiceSnapshot::default();
        };

        let snap = rt.snapshot();
        self.cached_snapshot = snap.clone();
        snap
    }

    pub(super) fn snapshot_external(&mut self) -> ServiceSnapshot {
        let now = Instant::now();
        if let Ok(state) = self.external_event_state.lock() {
            let stream_fresh = state
                .last_event_at
                .is_some_and(|ts| now.duration_since(ts) <= EXTERNAL_EVENT_STREAM_STALE_TIMEOUT);
            if state.stream_connected
                && !state.replay_miss
                && stream_fresh
                && let Some(snapshot) = state.latest_snapshot.clone()
            {
                self.cached_snapshot = snapshot.clone();
                self.last_error = state.last_error.clone();
                return snapshot;
            }
        }

        if let Some(last_poll) = self.last_external_poll
            && now.duration_since(last_poll) < EXTERNAL_SNAPSHOT_POLL_INTERVAL
        {
            return self.cached_snapshot.clone();
        }
        self.last_external_poll = Some(now);

        let endpoint = self.control_endpoint_label();
        match self.send_control_request(ControlRequest::new(ControlCommand::Snapshot)) {
            Ok(response) => match response.payload {
                ControlResponsePayload::Snapshot { snapshot } => {
                    self.cached_snapshot = snapshot;
                    self.last_error = None;
                    if let Ok(mut state) = self.external_event_state.lock() {
                        state.latest_snapshot = Some(self.cached_snapshot.clone());
                        state.replay_miss = false;
                        state.last_error = None;
                    }
                }
                ControlResponsePayload::Error { message } => {
                    self.set_last_error(format!(
                        "External runtime error from {}: {}",
                        endpoint, message
                    ));
                    self.cached_snapshot = ServiceSnapshot {
                        task_error: Some(("control".to_string(), message)),
                        ..ServiceSnapshot::default()
                    };
                    if let Ok(mut state) = self.external_event_state.lock() {
                        state.last_error = self.last_error.clone();
                    }
                }
                ControlResponsePayload::Ack => {
                    self.set_last_error(format!(
                        "External runtime at {} returned unexpected ACK for snapshot",
                        endpoint
                    ));
                    self.cached_snapshot = ServiceSnapshot::default();
                    if let Ok(mut state) = self.external_event_state.lock() {
                        state.last_error = self.last_error.clone();
                    }
                }
                ControlResponsePayload::TrainerSnapshot { .. }
                | ControlResponsePayload::RecordingStarted { .. }
                | ControlResponsePayload::RecordingStopped { .. } => {
                    self.set_last_error(format!(
                        "External runtime at {} returned unexpected payload for snapshot",
                        endpoint
                    ));
                    self.cached_snapshot = ServiceSnapshot::default();
                    if let Ok(mut state) = self.external_event_state.lock() {
                        state.last_error = self.last_error.clone();
                    }
                }
            },
            Err(error) => {
                self.set_last_error(format!(
                    "Failed to reach external runtime at {}: {}",
                    endpoint, error
                ));
                self.cached_snapshot = ServiceSnapshot {
                    task_error: Some(("control".to_string(), error)),
                    ..ServiceSnapshot::default()
                };
                if let Ok(mut state) = self.external_event_state.lock() {
                    state.last_error = self.last_error.clone();
                }
            }
        }

        self.cached_snapshot.clone()
    }

    /// Query the trainer-side snapshot exposed by the runtime.
    pub fn trainer_snapshot(&mut self) -> Option<TrainerSnapshot> {
        match self.runtime_mode {
            neurohid_types::config::ServiceRuntimeMode::Embedded => {
                let rt = self.runtime_handle.as_ref()?;
                Some(rt.trainer_snapshot())
            }
            neurohid_types::config::ServiceRuntimeMode::External => {
                let endpoint = self.control_endpoint_label();
                match self
                    .send_control_request(ControlRequest::new(ControlCommand::TrainerSnapshot))
                {
                    Ok(response) => match response.payload {
                        ControlResponsePayload::TrainerSnapshot { snapshot } => Some(snapshot),
                        ControlResponsePayload::Error { message } => {
                            self.set_last_error(format!(
                                "External runtime trainer snapshot failed from {}: {}",
                                endpoint, message
                            ));
                            None
                        }
                        ControlResponsePayload::Ack
                        | ControlResponsePayload::Snapshot { .. }
                        | ControlResponsePayload::RecordingStarted { .. }
                        | ControlResponsePayload::RecordingStopped { .. } => {
                            self.set_last_error(format!(
                                "External runtime at {} returned unexpected payload for trainer snapshot",
                                endpoint
                            ));
                            None
                        }
                    },
                    Err(error) => {
                        self.set_last_error(format!(
                            "Failed to query trainer snapshot from external runtime at {}: {}",
                            endpoint, error
                        ));
                        None
                    }
                }
            }
        }
    }
}
