use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const INFO_WINDOW_SECS: u64 = 60;
const DEBUG_WINDOW_SECS: u64 = 1;

/// Shared stage identifiers for runtime/control observability.
pub mod stage {
    pub const SIGNAL: &str = "signal";
    pub const DECODER: &str = "decoder";
    pub const ACTION: &str = "action";
    pub const IPC: &str = "ipc";
    pub const CONTROL: &str = "control";
}

/// Shared span names used across runtime/control pipeline tasks.
pub mod span {
    pub const SIGNAL_RUN: &str = "runtime.signal.run";
    pub const DECODER_RUN: &str = "runtime.decoder.run";
    pub const ACTION_RUN: &str = "runtime.action.run";
    pub const IPC_RUN: &str = "runtime.ipc.run";
    pub const CONTROL_REQUEST: &str = "runtime.control.request";
}

/// Shared event names for task lifecycle and hot-path telemetry.
pub mod event {
    pub const TASK_STARTED: &str = "task.started";
    pub const TASK_STOPPED: &str = "task.stopped";
    pub const TASK_SUMMARY: &str = "task.summary";
    pub const FEATURE_WINDOW_EMITTED: &str = "signal.feature_window_emitted";
    pub const DECISION_EMITTED: &str = "decoder.decision_emitted";
    pub const ACTION_EMITTED: &str = "action.emitted";
    pub const IPC_DECISION_SENT: &str = "ipc.decision_sent";
    pub const IPC_TELEMETRY_PUBLISHED: &str = "ipc.telemetry_published";
    pub const CONTROL_REQUEST_RECEIVED: &str = "control.request_received";
    pub const CONTROL_RESPONSE_SENT: &str = "control.response_sent";
}

/// Required cross-boundary correlation keys.
pub mod field {
    pub const DECISION_ID: &str = "decision_id";
    pub const STREAM_ID: &str = "stream_id";
    pub const UNKNOWN: &str = "n/a";
}

/// Runtime observability components that can be tuned independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservabilityComponent {
    Signal,
    Decoder,
    Action,
    Ipc,
    Control,
}

/// Sampling/rate-limit policy for one component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitPolicyConfig {
    /// Deterministic sample ratio in [0.0, 1.0] for high-volume debug events.
    #[serde(default = "default_sample_ratio")]
    pub sample_ratio: f32,
    /// Maximum emitted info events per minute for gated emit points.
    #[serde(default = "default_info_max_per_minute")]
    pub info_max_per_minute: u32,
    /// Maximum emitted debug events per second for gated emit points.
    #[serde(default = "default_debug_max_per_second")]
    pub debug_max_per_second: u32,
}

fn default_sample_ratio() -> f32 {
    1.0
}

fn default_info_max_per_minute() -> u32 {
    120
}

fn default_debug_max_per_second() -> u32 {
    10
}

impl Default for EmitPolicyConfig {
    fn default() -> Self {
        Self {
            sample_ratio: default_sample_ratio(),
            info_max_per_minute: default_info_max_per_minute(),
            debug_max_per_second: default_debug_max_per_second(),
        }
    }
}

impl EmitPolicyConfig {
    fn clamp(&self) -> Self {
        Self {
            sample_ratio: self.sample_ratio.clamp(0.0, 1.0),
            info_max_per_minute: self.info_max_per_minute,
            debug_max_per_second: self.debug_max_per_second,
        }
    }

    fn merged_with(&self, component: &Self) -> Self {
        let global = self.clamp();
        let component = component.clamp();

        Self {
            sample_ratio: (global.sample_ratio * component.sample_ratio).clamp(0.0, 1.0),
            info_max_per_minute: global
                .info_max_per_minute
                .min(component.info_max_per_minute),
            debug_max_per_second: global
                .debug_max_per_second
                .min(component.debug_max_per_second),
        }
    }
}

/// Global + per-component observability policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Global baseline policy applied to all components.
    #[serde(default)]
    pub global: EmitPolicyConfig,
    /// Signal task policy.
    #[serde(default = "default_signal_policy")]
    pub signal: EmitPolicyConfig,
    /// Decoder task policy.
    #[serde(default = "default_decoder_policy")]
    pub decoder: EmitPolicyConfig,
    /// Action task policy.
    #[serde(default = "default_action_policy")]
    pub action: EmitPolicyConfig,
    /// Runtime IPC bridge policy.
    #[serde(default = "default_ipc_policy")]
    pub ipc: EmitPolicyConfig,
    /// Control request/response policy.
    #[serde(default = "default_control_policy")]
    pub control: EmitPolicyConfig,
}

fn default_signal_policy() -> EmitPolicyConfig {
    EmitPolicyConfig {
        sample_ratio: 0.25,
        info_max_per_minute: 60,
        debug_max_per_second: 3,
    }
}

fn default_decoder_policy() -> EmitPolicyConfig {
    EmitPolicyConfig {
        sample_ratio: 0.5,
        info_max_per_minute: 120,
        debug_max_per_second: 6,
    }
}

fn default_action_policy() -> EmitPolicyConfig {
    EmitPolicyConfig {
        sample_ratio: 0.5,
        info_max_per_minute: 120,
        debug_max_per_second: 6,
    }
}

fn default_ipc_policy() -> EmitPolicyConfig {
    EmitPolicyConfig {
        sample_ratio: 0.25,
        info_max_per_minute: 60,
        debug_max_per_second: 4,
    }
}

fn default_control_policy() -> EmitPolicyConfig {
    EmitPolicyConfig {
        sample_ratio: 1.0,
        info_max_per_minute: 240,
        debug_max_per_second: 16,
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            global: EmitPolicyConfig::default(),
            signal: default_signal_policy(),
            decoder: default_decoder_policy(),
            action: default_action_policy(),
            ipc: default_ipc_policy(),
            control: default_control_policy(),
        }
    }
}

impl ObservabilityConfig {
    /// Resolve effective policy for a component by combining global+component limits.
    pub fn policy_for(&self, component: ObservabilityComponent) -> EmitPolicyConfig {
        let component_cfg = match component {
            ObservabilityComponent::Signal => &self.signal,
            ObservabilityComponent::Decoder => &self.decoder,
            ObservabilityComponent::Action => &self.action,
            ObservabilityComponent::Ipc => &self.ipc,
            ObservabilityComponent::Control => &self.control,
        };

        self.global.merged_with(component_cfg)
    }
}

#[derive(Debug)]
struct WindowCounter {
    started_at: Instant,
    count: u32,
}

impl WindowCounter {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            count: 0,
        }
    }

    fn allow(&mut self, window: Duration, limit: u32) -> bool {
        if limit == 0 {
            return false;
        }

        if self.started_at.elapsed() >= window {
            self.started_at = Instant::now();
            self.count = 0;
        }

        if self.count >= limit {
            return false;
        }

        self.count = self.count.saturating_add(1);
        true
    }
}

/// Cheap deterministic emit gate for sampling and rate-limiting hot-path logs.
#[derive(Debug)]
pub struct EmitGate {
    policy: EmitPolicyConfig,
    info_counter: WindowCounter,
    debug_counter: WindowCounter,
    sample_seq: u64,
}

impl EmitGate {
    pub fn new(policy: EmitPolicyConfig) -> Self {
        Self {
            policy: policy.clamp(),
            info_counter: WindowCounter::new(),
            debug_counter: WindowCounter::new(),
            sample_seq: 0,
        }
    }

    pub fn allow_info(&mut self) -> bool {
        self.info_counter.allow(
            Duration::from_secs(INFO_WINDOW_SECS),
            self.policy.info_max_per_minute,
        )
    }

    pub fn allow_debug(&mut self) -> bool {
        if !self.sample_pass() {
            return false;
        }
        self.debug_counter.allow(
            Duration::from_secs(DEBUG_WINDOW_SECS),
            self.policy.debug_max_per_second,
        )
    }

    fn sample_pass(&mut self) -> bool {
        let ratio = self.policy.sample_ratio;
        if ratio <= 0.0 {
            return false;
        }
        if ratio >= 1.0 {
            return true;
        }

        self.sample_seq = self.sample_seq.wrapping_add(1);
        let stride = (1.0 / ratio).round().max(1.0) as u64;
        self.sample_seq.is_multiple_of(stride)
    }
}
