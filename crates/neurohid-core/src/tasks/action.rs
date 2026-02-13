//! # Action Task
//!
//! This task is the final step in the pipeline: it takes decoded actions from
//! the Python decoder and translates them into actual HID events that the
//! operating system will treat as mouse movements, clicks, and keystrokes.
//!
//! This is where the "magic" becomes visible to the user. When they think
//! "move left" and the cursor actually moves left, it's this task that made
//! it happen. That's a big responsibility, so we need to be careful about:
//!
//! 1. **Timing**: Actions should feel responsive, not laggy
//! 2. **Smoothing**: Jerky movements are uncomfortable; we smooth them out
//! 3. **Safety**: We respect confidence thresholds and debouncing to prevent
//!    accidental clicks or key presses

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};

use neurohid_platform::{create_platform, MouseMovement, Platform};
use neurohid_types::{
    action::{Action, MouseButton},
    config::ActionConfig,
    control::RuntimeModeState,
    error::Result,
    event::{MarkerPayload, MarkerType, StreamMarker},
    observability::{self as obs, EmitGate, ObservabilityComponent, ObservabilityConfig},
};

use crate::service::ServiceState;
use crate::tasks::latency::RollingLatency;

const ACTION_SUMMARY_EVERY_EMITTED: u64 = 256;

#[derive(Debug, Clone, Copy)]
enum CapabilityKind {
    CursorMove,
    Click,
    Keyboard,
}

#[derive(Default)]
struct CapabilityGate {
    samples: VecDeque<(Instant, f32, f32)>,
    eligible_since: Option<Instant>,
    enabled: bool,
}

impl CapabilityGate {
    #[expect(
        clippy::too_many_arguments,
        reason = "Gate updates require explicit threshold and timing parameters for each capability"
    )]
    fn update(
        &mut self,
        now: Instant,
        confidence: f32,
        success: f32,
        min_confidence: f32,
        min_success: f32,
        window: Duration,
        reenable_hold: Duration,
    ) {
        self.samples.push_back((now, confidence, success));
        while let Some((ts, _, _)) = self.samples.front() {
            if now.duration_since(*ts) > window {
                let _ = self.samples.pop_front();
            } else {
                break;
            }
        }

        if self.samples.is_empty() {
            self.enabled = false;
            self.eligible_since = None;
            return;
        }

        let count = self.samples.len() as f32;
        let avg_conf = self.samples.iter().map(|(_, c, _)| *c).sum::<f32>() / count;
        let avg_success = self.samples.iter().map(|(_, _, s)| *s).sum::<f32>() / count;
        let passing = avg_conf >= min_confidence && avg_success >= min_success;

        if !passing {
            self.enabled = false;
            self.eligible_since = None;
            return;
        }

        if self.enabled {
            return;
        }

        let eligible_since = self.eligible_since.get_or_insert(now);
        if now.duration_since(*eligible_since) >= reenable_hold {
            self.enabled = true;
        }
    }
}

/// The action task emits HID events based on decoded intentions.
pub struct ActionTask {
    config: ActionConfig,
    action_rx: mpsc::Receiver<Action>,
    state: Arc<RwLock<ServiceState>>,

    /// Optional calibration mode flag — when set, HID emission is paused.
    calibration_mode: Option<Arc<AtomicBool>>,
    /// Optional runtime output toggle.
    output_enabled: Option<Arc<AtomicBool>>,

    /// Broadcast channel for forwarding actions to hub visualization widgets.
    action_broadcast_tx: Option<broadcast::Sender<Action>>,
    /// Broadcast channel for forwarding marker annotations.
    marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,

    // State for smoothing and debouncing
    // Reserved for future absolute->relative position tracking.
    _last_mouse_pos: (f32, f32),
    last_action_time: Instant,
    smoothed_velocity: (f32, f32),
    action_latency: RollingLatency,
    cursor_gate: CapabilityGate,
    click_gate: CapabilityGate,
    keyboard_gate: CapabilityGate,
    emit_gate: EmitGate,
}

impl ActionTask {
    /// Creates a new action task.
    #[expect(
        clippy::too_many_arguments,
        reason = "Task constructor wires runtime channels, state handles, and observability policy"
    )]
    pub fn new(
        config: ActionConfig,
        action_rx: mpsc::Receiver<Action>,
        state: Arc<RwLock<ServiceState>>,
        calibration_mode: Option<Arc<AtomicBool>>,
        output_enabled: Option<Arc<AtomicBool>>,
        action_broadcast_tx: Option<broadcast::Sender<Action>>,
        marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
        observability: ObservabilityConfig,
    ) -> Self {
        Self {
            config,
            action_rx,
            state,
            calibration_mode,
            output_enabled,
            action_broadcast_tx,
            marker_broadcast_tx,
            _last_mouse_pos: (0.0, 0.0),
            last_action_time: Instant::now(),
            smoothed_velocity: (0.0, 0.0),
            action_latency: RollingLatency::new(512),
            cursor_gate: CapabilityGate::default(),
            click_gate: CapabilityGate::default(),
            keyboard_gate: CapabilityGate::default(),
            emit_gate: EmitGate::new(observability.policy_for(ObservabilityComponent::Action)),
        }
    }

    /// Runs the action task until shutdown is signaled.
    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!(
            event = obs::event::TASK_STARTED,
            span = obs::span::ACTION_RUN,
            stage = obs::stage::ACTION,
            "Action task started"
        );

        // Try to create the platform-specific HID emitter. If this fails we
        // continue running in "passthrough" mode: actions are still broadcast
        // to visualization widgets so the console/graphs keep working, but no
        // HID events are emitted. This prevents a platform init failure from
        // taking down the entire data pipeline.
        let mut platform: Option<Box<dyn Platform>> = match create_platform() {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!("Failed to create platform (HID output disabled): {}", e);
                let mut state = self.state.write().await;
                state.task_error = Some((
                    "action".into(),
                    format!("Platform unavailable: {} \u{2014} HID output disabled", e),
                ));
                None
            }
        };

        // Check that we have the necessary permissions for input simulation.
        // On macOS, this might prompt the user to grant accessibility access.
        if let Some(ref p) = platform
            && let Err(e) = p.check_input_permissions() {
                tracing::warn!("Input permission check failed (HID output disabled): {}", e);
                tracing::warn!("Please grant the necessary permissions and restart.");
                let mut state = self.state.write().await;
                state.task_error = Some((
                    "action".into(),
                    format!("Permission denied: {} \u{2014} HID output disabled", e),
                ));
                platform = None;
            }

        if let Some(ref p) = platform {
            tracing::info!("Platform initialized: {}", p.platform_name());
        } else {
            tracing::info!("Running in passthrough mode (no HID output)");
        }

        // Check if action output is enabled in config
        if !self.config.enabled {
            tracing::warn!("Action output is disabled in configuration");
            // We'll still run the loop but won't emit any actions
        }

        let mut actions_emitted: u64 = 0;

        loop {
            tokio::select! {
                // Check for shutdown
                _ = shutdown.recv() => {
                    tracing::info!(event = obs::event::TASK_STOPPED, "Action task received shutdown signal");
                    break;
                }

                // Receive actions from IPC task
                action = self.action_rx.recv() => {
                    match action {
                        Some(action) => {
                            let decision_id = action.decision_id.as_deref().unwrap_or("none");

                            // Broadcast action to hub visualization widgets
                            // (always, regardless of confidence/calibration/platform)
                            if let Some(tx) = &self.action_broadcast_tx {
                                let _ = tx.send(action.clone());
                            }
                            self.emit_markers(&action);

                            // If no platform is available, skip HID emission
                            // but keep broadcasting for visualizations.
                            let Some(ref mut p) = platform else { continue };

                            // Check if output is enabled
                            if !self.config.enabled {
                                continue;
                            }

                            // Runtime output toggle (pause/resume)
                            if let Some(flag) = &self.output_enabled
                                && !flag.load(Ordering::Relaxed) {
                                    continue;
                                }

                            // Do not emit HID events until profile calibration and runtime
                            // decoder readiness are both satisfied.
                            let can_emit = {
                                let state_guard = self.state.read().await;
                                state_guard.profile_ready && state_guard.decoder_ready
                            };
                            if !can_emit {
                                continue;
                            }

                            // Check if calibration mode is active — skip HID emission
                            if let Some(flag) = &self.calibration_mode
                                && flag.load(Ordering::Relaxed) {
                                    continue;
                                }

                            // Check confidence threshold. If the decoder isn't sure,
                            // we'd rather do nothing than make a mistake.
                            if action.confidence < self.config.min_confidence_threshold {
                                tracing::trace!(
                                    decision_id = %decision_id,
                                    "Skipping action with low confidence: {:.2}",
                                    action.confidence
                                );
                                continue;
                            }

                            let mut gated_action = action.clone();
                            if self
                                .apply_fallback_capability_gating(&mut gated_action, action.confidence)
                                .await
                            {
                                // Continue with gated action.
                            } else {
                                continue;
                            }

                            // Check debounce timer for discrete actions
                            let now = Instant::now();
                            let ms_since_last = now.duration_since(self.last_action_time).as_millis() as u32;

                            // Execute the action
                            if let Err(e) = self.execute_action(&mut **p, &gated_action, ms_since_last) {
                                tracing::warn!(decision_id = %decision_id, "Failed to execute action: {}", e);
                            } else {
                                actions_emitted += 1;
                                self.last_action_time = now;

                                if gated_action.timestamp > 0 {
                                    let now_micros = neurohid_types::now_micros();
                                    let latency_us =
                                        now_micros.saturating_sub(gated_action.timestamp) as u64;
                                    self.action_latency.record(latency_us);
                                }

                                // Update shared state
                                let mut state = self.state.write().await;
                                state.actions_emitted = actions_emitted;
                                state.action_latency_last_us = self.action_latency.last_us();
                                state.action_latency_p95_us = self.action_latency.p95_us();

                                if tracing::enabled!(tracing::Level::DEBUG) && self.emit_gate.allow_debug() {
                                    tracing::debug!(
                                        event = obs::event::ACTION_EMITTED,
                                        decision_id = %decision_id,
                                        stream_id = obs::field::UNKNOWN,
                                        confidence = gated_action.confidence,
                                        action_latency_last_us = state.action_latency_last_us,
                                        "Action emitted"
                                    );
                                }

                                if actions_emitted.is_multiple_of(ACTION_SUMMARY_EVERY_EMITTED)
                                    && self.emit_gate.allow_info()
                                {
                                    tracing::info!(
                                        event = obs::event::TASK_SUMMARY,
                                        decision_id = obs::field::UNKNOWN,
                                        stream_id = obs::field::UNKNOWN,
                                        actions_emitted,
                                        action_latency_p95_us = state.action_latency_p95_us,
                                        "Action task periodic summary"
                                    );
                                }
                            }
                        }
                        None => {
                            tracing::info!("Action channel closed");
                            break;
                        }
                    }
                }
            }
        }

        tracing::info!(
            event = obs::event::TASK_STOPPED,
            decision_id = obs::field::UNKNOWN,
            stream_id = obs::field::UNKNOWN,
            actions_emitted,
            "Action task emitted actions"
        );
        Ok(())
    }

    async fn apply_fallback_capability_gating(
        &mut self,
        action: &mut Action,
        confidence: f32,
    ) -> bool {
        let now = Instant::now();
        let (fallback_policy, success_score, ml_connected, ml_stalled, model_kind) = {
            let state = self.state.read().await;
            (
                state.fallback_policy.clone(),
                state.rolling_success_score,
                state.ml_bridge_connected,
                state.ml_bridge_stalled,
                state.fallback_model_kind.clone(),
            )
        };

        let fallback_mode = fallback_policy.enabled
            && (ml_stalled || !ml_connected || model_kind.as_deref() != Some("onnx"));
        if !fallback_mode {
            self.publish_capability_state(
                RuntimeModeState::Full,
                vec![
                    "cursor_move".to_string(),
                    "click".to_string(),
                    "keyboard".to_string(),
                ],
                None,
            )
            .await;
            return !action.is_none();
        }

        let window = Duration::from_secs(fallback_policy.gate_window_secs.max(1));
        let reenable = Duration::from_secs(fallback_policy.capability_reenable_hold_secs.max(1));

        self.update_gate(
            CapabilityKind::CursorMove,
            now,
            confidence,
            success_score,
            fallback_policy.movement_min_confidence,
            fallback_policy.movement_min_success_score,
            window,
            reenable,
        );
        self.update_gate(
            CapabilityKind::Click,
            now,
            confidence,
            success_score,
            fallback_policy.click_min_confidence,
            fallback_policy.click_min_success_score,
            window,
            reenable,
        );
        self.update_gate(
            CapabilityKind::Keyboard,
            now,
            confidence,
            success_score,
            fallback_policy.keyboard_min_confidence,
            fallback_policy.keyboard_min_success_score,
            window,
            reenable,
        );

        let cursor_enabled = self.cursor_gate.enabled;
        let click_enabled = self.click_gate.enabled;
        let keyboard_enabled = self.keyboard_gate.enabled;

        if let Some(mouse) = &mut action.mouse {
            if !cursor_enabled {
                mouse.movement = None;
                mouse.scroll = None;
            }
            if !click_enabled {
                mouse.buttons.clear();
            }
            if mouse.movement.is_none() && mouse.buttons.is_empty() && mouse.scroll.is_none() {
                action.mouse = None;
            }
        }
        if !keyboard_enabled {
            action.keyboard = None;
        }

        let mut enabled = Vec::new();
        if cursor_enabled {
            enabled.push("cursor_move".to_string());
        }
        if click_enabled {
            enabled.push("click".to_string());
        }
        if keyboard_enabled {
            enabled.push("keyboard".to_string());
        }

        let message = if enabled.is_empty() {
            Some(
                "Runtime fallback active; no capabilities meet confidence/success thresholds."
                    .to_string(),
            )
        } else {
            let disabled: Vec<&str> = ["cursor_move", "click", "keyboard"]
                .into_iter()
                .filter(|cap| !enabled.iter().any(|enabled_cap| enabled_cap == cap))
                .collect();
            if disabled.is_empty() {
                Some("Runtime fallback active with full capability set.".to_string())
            } else {
                Some(format!(
                    "Runtime fallback active; limited capabilities (disabled: {}).",
                    disabled.join(", ")
                ))
            }
        };

        let mode = if enabled.is_empty() {
            RuntimeModeState::Degraded
        } else {
            RuntimeModeState::Fallback
        };
        self.publish_capability_state(mode, enabled, message).await;
        !action.is_none()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Fallback capability gate update passes policy thresholds explicitly"
    )]
    fn update_gate(
        &mut self,
        kind: CapabilityKind,
        now: Instant,
        confidence: f32,
        success: f32,
        min_confidence: f32,
        min_success: f32,
        window: Duration,
        reenable_hold: Duration,
    ) {
        let gate = match kind {
            CapabilityKind::CursorMove => &mut self.cursor_gate,
            CapabilityKind::Click => &mut self.click_gate,
            CapabilityKind::Keyboard => &mut self.keyboard_gate,
        };
        gate.update(
            now,
            confidence,
            success,
            min_confidence,
            min_success,
            window,
            reenable_hold,
        );
    }

    async fn publish_capability_state(
        &self,
        runtime_mode_state: RuntimeModeState,
        enabled_capabilities: Vec<String>,
        limited_message: Option<String>,
    ) {
        let mut state = self.state.write().await;
        let previous_mode = state.runtime_mode_state;
        let fallback_policy = state.fallback_policy.clone();
        let now_us = neurohid_types::now_micros();
        state.runtime_mode_state = runtime_mode_state;
        state.enabled_capabilities = enabled_capabilities;
        state.limited_capabilities_message = limited_message.clone();

        let cooldown_us = fallback_policy
            .notification_cooldown_secs
            .saturating_mul(1_000_000) as i64;
        let should_alert = previous_mode != runtime_mode_state
            && state
                .last_runtime_mode_alert_us
                .is_none_or(|last| now_us.saturating_sub(last) >= cooldown_us);

        if should_alert {
            state.last_runtime_mode_alert_us = Some(now_us);
            drop(state);

            match runtime_mode_state {
                RuntimeModeState::Full => {
                    tracing::info!("Runtime recovered to full capability mode");
                }
                RuntimeModeState::Fallback => {
                    tracing::warn!(
                        "{}",
                        limited_message.unwrap_or_else(|| {
                            "Runtime entered fallback mode; capabilities may be limited."
                                .to_string()
                        })
                    );
                }
                RuntimeModeState::Degraded => {
                    tracing::warn!(
                        "{}",
                        limited_message.unwrap_or_else(|| {
                            "Runtime entered degraded mode; HID output is limited or disabled."
                                .to_string()
                        })
                    );
                }
            }
        }
    }

    /// Executes a single action using the platform HID interface.
    fn execute_action(
        &mut self,
        platform: &mut dyn Platform,
        action: &Action,
        ms_since_last: u32,
    ) -> Result<()> {
        // Handle mouse actions
        if let Some(mouse) = &action.mouse {
            // Process movement
            if let Some(movement) = &mouse.movement {
                // Apply smoothing to make movements feel more natural.
                // Without smoothing, decoder noise causes jittery movement.
                let smoothed = if self.config.mouse_smoothing_enabled {
                    self.smooth_movement(movement.dx, movement.dy)
                } else {
                    (movement.dx, movement.dy)
                };

                // Apply sensitivity scaling
                let scaled_dx = smoothed.0 * self.config.mouse_sensitivity;
                let scaled_dy = smoothed.1 * self.config.mouse_sensitivity;

                // Only emit if movement is significant (reduces micro-jitter)
                if scaled_dx.abs() > 0.5 || scaled_dy.abs() > 0.5 {
                    platform.emit_mouse_move(MouseMovement {
                        dx: scaled_dx,
                        dy: scaled_dy,
                    })?;
                }
            }

            // Process button events (with debouncing)
            for button_event in &mouse.buttons {
                // Debounce discrete actions to prevent accidental double-clicks
                if ms_since_last < self.config.action_debounce_ms {
                    tracing::trace!("Debouncing button event");
                    continue;
                }

                if button_event.pressed {
                    platform.emit_mouse_press(button_event.button)?;
                } else {
                    platform.emit_mouse_release(button_event.button)?;
                }
            }

            // Process scroll
            if let Some(scroll) = &mouse.scroll {
                platform.emit_scroll(scroll.dx, scroll.dy)?;
            }
        }

        // Handle keyboard actions
        if let Some(keyboard) = &action.keyboard {
            for key_event in &keyboard.events {
                // Debounce discrete actions
                if ms_since_last < self.config.action_debounce_ms {
                    tracing::trace!("Debouncing key event");
                    continue;
                }

                if key_event.pressed {
                    platform.emit_key_press(key_event.key)?;
                } else {
                    platform.emit_key_release(key_event.key)?;
                }
            }
        }

        Ok(())
    }

    /// Applies exponential smoothing to mouse movement.
    ///
    /// This reduces jitter from noisy decoder output while preserving intentional
    /// movements. The smoothing factor controls how much we blend the new value
    /// with the previous smoothed value: 0 = no smoothing, 1 = maximum smoothing.
    fn smooth_movement(&mut self, dx: f32, dy: f32) -> (f32, f32) {
        let alpha = 1.0 - self.config.mouse_smoothing_factor;

        // Exponential moving average
        self.smoothed_velocity.0 = alpha * dx + (1.0 - alpha) * self.smoothed_velocity.0;
        self.smoothed_velocity.1 = alpha * dy + (1.0 - alpha) * self.smoothed_velocity.1;

        self.smoothed_velocity
    }

    fn emit_markers(&self, action: &Action) {
        let Some(tx) = &self.marker_broadcast_tx else {
            return;
        };

        if let Some(mouse) = &action.mouse {
            if let Some(mv) = &mouse.movement {
                let marker = StreamMarker::now(MarkerType::CursorMovement).with_payload(
                    MarkerPayload::CursorMovement {
                        dx: mv.dx,
                        dy: mv.dy,
                        magnitude: mv.magnitude(),
                    },
                );
                let _ = tx.send(marker);
            }

            for btn in &mouse.buttons {
                let marker = StreamMarker::now(MarkerType::MouseClick).with_payload(
                    MarkerPayload::MouseClick {
                        button: mouse_button_label(btn.button).to_string(),
                        pressed: btn.pressed,
                    },
                );
                let _ = tx.send(marker);
            }
        }
    }
}

fn mouse_button_label(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        MouseButton::Extra(_) => "extra",
    }
}
