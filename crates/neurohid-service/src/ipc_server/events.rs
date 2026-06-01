use std::collections::{HashSet, VecDeque};

use neurohid_ipc::{
    RuntimeComponentCapability, RuntimeEvent, RuntimeEventsSubscribe, RuntimeTelemetry,
    TrainerStatus,
};
use neurohid_types::{
    control::ControlSnapshot,
    observation::{CursorState, Observation, ScreenInfo},
};

pub(super) const RUNTIME_EVENTS_REPLAY_MAX_EVENTS: usize = 10_000;
pub(super) const RUNTIME_EVENTS_REPLAY_RETENTION_US: i64 = 120_000_000;

#[derive(Debug, Clone)]
pub(super) struct RuntimeEventsReplayItem {
    pub(super) seq: u64,
    pub(super) sent_at_us: i64,
    pub(super) family: &'static str,
    pub(super) event: RuntimeEvent,
}

#[derive(Debug, Default, Clone)]
pub(super) struct RuntimeEventsReplayBuffer {
    entries: VecDeque<RuntimeEventsReplayItem>,
}

impl RuntimeEventsReplayBuffer {
    pub(super) fn push(&mut self, item: RuntimeEventsReplayItem) {
        self.entries.push_back(item);
        self.prune();
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn oldest_seq(&self) -> Option<u64> {
        self.entries.front().map(|item| item.seq)
    }

    pub(super) fn newest_seq(&self) -> Option<u64> {
        self.entries.back().map(|item| item.seq)
    }

    pub(super) fn iter_from(
        &self,
        from_seq: u64,
    ) -> impl Iterator<Item = &RuntimeEventsReplayItem> {
        self.entries.iter().filter(move |item| item.seq >= from_seq)
    }

    pub(super) fn prune(&mut self) {
        while self.entries.len() > RUNTIME_EVENTS_REPLAY_MAX_EVENTS {
            let _ = self.entries.pop_front();
        }

        let now_us = neurohid_types::now_micros();
        while self.entries.front().is_some_and(|item| {
            now_us.saturating_sub(item.sent_at_us) > RUNTIME_EVENTS_REPLAY_RETENTION_US
        }) {
            let _ = self.entries.pop_front();
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(super) struct RuntimeEventsState {
    next_seq: u64,
    pub(super) replay: RuntimeEventsReplayBuffer,
}

impl RuntimeEventsState {
    pub(super) fn allocate_seq(&mut self) -> u64 {
        self.next_seq = self.next_seq.saturating_add(1);
        self.next_seq
    }
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeEventsFilter {
    families: Option<HashSet<String>>,
}

impl RuntimeEventsFilter {
    pub(super) fn from_request(request: &RuntimeEventsSubscribe) -> Self {
        if request.families.is_empty() {
            return Self { families: None };
        }
        let families = request
            .families
            .iter()
            .map(|family| family.trim().to_ascii_lowercase())
            .filter(|family| !family.is_empty())
            .collect::<HashSet<_>>();
        if families.is_empty() {
            Self { families: None }
        } else {
            Self {
                families: Some(families),
            }
        }
    }

    pub(super) fn allows(&self, family: &str) -> bool {
        self.families
            .as_ref()
            .is_none_or(|families| families.contains(family))
    }
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeObservationState {
    cursor: CursorState,
    screen: ScreenInfo,
}

impl Default for RuntimeObservationState {
    fn default() -> Self {
        Self {
            cursor: CursorState::centered(),
            screen: ScreenInfo {
                width: 1_920,
                height: 1_080,
                active_monitor: 0,
                monitor_count: 1,
            },
        }
    }
}

impl RuntimeObservationState {
    pub(super) fn update_from_action(&mut self, action: &neurohid_types::Action) {
        if let Some(mouse) = &action.mouse {
            if let Some(movement) = &mouse.movement {
                self.cursor.velocity_x = movement.dx;
                self.cursor.velocity_y = movement.dy;
                self.cursor.x = (self.cursor.x + movement.dx).clamp(0.0, 1.0);
                self.cursor.y = (self.cursor.y + movement.dy).clamp(0.0, 1.0);
            }
            for button in &mouse.buttons {
                self.cursor.button_held = button.pressed;
            }
        } else {
            self.cursor.velocity_x = 0.0;
            self.cursor.velocity_y = 0.0;
        }
    }

    pub(super) fn observation_from_feature(
        &self,
        feature: &neurohid_types::FeatureVector,
    ) -> Observation {
        Observation {
            timestamp: feature.timestamp,
            signal_features: feature.clone(),
            cursor: self.cursor,
            screen: self.screen.clone(),
            enhanced: None,
        }
    }
}

pub(super) fn runtime_event_family(event: &RuntimeEvent) -> &'static str {
    match event {
        RuntimeEvent::Snapshot { .. } => "snapshot",
        RuntimeEvent::TrainerSnapshot { .. } => "trainer_snapshot",
        RuntimeEvent::TrainerStatus { .. } => "trainer_status",
        RuntimeEvent::RuntimeTelemetry { .. } => "runtime_telemetry",
        RuntimeEvent::Sample { .. } => "sample",
        RuntimeEvent::FeatureFrame { .. } => "feature_frame",
        RuntimeEvent::ActionEmitted { .. } => "action_emitted",
        RuntimeEvent::Marker { .. } => "marker",
        RuntimeEvent::ObservationFrame { .. } => "observation_frame",
        RuntimeEvent::DecisionEvent { .. } => "decision_event",
        RuntimeEvent::ErrpWindow { .. } => "errp_window",
        RuntimeEvent::ErrpResult { .. } => "errp_result",
        RuntimeEvent::IntegrityIssue { .. } => "integrity_issue",
        RuntimeEvent::Lifecycle { .. } => "lifecycle",
        RuntimeEvent::BackpressureDrop { .. } => "backpressure_drop",
        RuntimeEvent::Capabilities { .. } => "capabilities",
    }
}

pub(super) fn build_runtime_telemetry(snapshot: &ControlSnapshot) -> RuntimeTelemetry {
    RuntimeTelemetry {
        signal_latency_p95_us: snapshot.signal_latency_p95_us,
        decode_latency_p95_us: snapshot.decode_latency_p95_us,
        action_latency_p95_us: snapshot.action_latency_p95_us,
        decision_queue_depth: 0,
        errp_queue_depth: 0,
        dropped_ml_messages: snapshot.integrity_issue_count,
    }
}

pub(super) fn build_runtime_trainer_status(snapshot: &ControlSnapshot) -> TrainerStatus {
    TrainerStatus {
        state: if snapshot.ml_bridge_stalled {
            "stalled".to_string()
        } else if snapshot.ml_bridge_connected {
            "training".to_string()
        } else {
            "disconnected".to_string()
        },
        replay_size: snapshot.trainer_replay_size.unwrap_or(0),
        training_step: snapshot.trainer_step.unwrap_or(0),
        policy_loss: snapshot.trainer_policy_loss,
        value_loss: snapshot.trainer_value_loss,
        entropy: snapshot.trainer_entropy,
        last_error: snapshot.trainer_last_error.clone(),
    }
}

pub(super) fn build_runtime_capabilities_event(snapshot: &ControlSnapshot) -> RuntimeEvent {
    let runtime_ready = snapshot.running;
    let device_ready = snapshot.device_connected;
    let bridge_ready = snapshot.ml_bridge_connected && !snapshot.ml_bridge_stalled;
    let decoder_ready = snapshot.decoder_ready;
    let profile_ready = snapshot.profile_ready;

    let available = |name: &str| RuntimeComponentCapability {
        name: name.to_string(),
        available: true,
        unavailable_reason: None,
    };
    let unavailable = |name: &str, reason: &str| RuntimeComponentCapability {
        name: name.to_string(),
        available: false,
        unavailable_reason: Some(reason.to_string()),
    };

    let live_observation_reason = if !runtime_ready {
        Some("runtime_not_running")
    } else if !device_ready {
        Some("device_not_connected")
    } else {
        None
    };

    let trainer_event_reason = if !runtime_ready {
        Some("runtime_not_running")
    } else if !profile_ready {
        Some("profile_not_ready")
    } else if !decoder_ready {
        Some("decoder_not_ready")
    } else if !bridge_ready {
        Some("ml_bridge_unavailable")
    } else {
        None
    };

    RuntimeEvent::Capabilities {
        observation_schema_version: 1,
        channels: vec![
            neurohid_ipc::IpcChannel::ControlRpc,
            neurohid_ipc::IpcChannel::TrainerStream,
            neurohid_ipc::IpcChannel::RuntimeEvents,
        ],
        components: vec![
            match live_observation_reason {
                Some(reason) => unavailable("sample", reason),
                None => available("sample"),
            },
            match live_observation_reason {
                Some(reason) => unavailable("feature_frame", reason),
                None => available("feature_frame"),
            },
            match live_observation_reason {
                Some(reason) => unavailable("action_emitted", reason),
                None => available("action_emitted"),
            },
            match live_observation_reason {
                Some(reason) => unavailable("marker", reason),
                None => available("marker"),
            },
            match live_observation_reason {
                Some(reason) => unavailable("observation_frame", reason),
                None => available("observation_frame"),
            },
            available("snapshot"),
            if bridge_ready {
                available("trainer_status")
            } else {
                unavailable("trainer_status", "ml_bridge_unavailable")
            },
            available("runtime_telemetry"),
            match trainer_event_reason {
                Some(reason) => unavailable("decision_event", reason),
                None => available("decision_event"),
            },
            match trainer_event_reason {
                Some(reason) => unavailable("errp_window", reason),
                None => available("errp_window"),
            },
            match trainer_event_reason {
                Some(reason) => unavailable("errp_result", reason),
                None => available("errp_result"),
            },
            if runtime_ready {
                available("integrity_issue")
            } else {
                unavailable("integrity_issue", "runtime_not_running")
            },
            available("resume_from_seq"),
            available("replay_miss"),
            available("sample_every"),
            available("backpressure_drop"),
        ],
    }
}
