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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

use neurohid_platform::{create_platform, MouseMovement, Platform};
use neurohid_types::{action::Action, config::ActionConfig, error::Result};

use crate::service::ServiceState;

/// The action task emits HID events based on decoded intentions.
pub struct ActionTask {
    config: ActionConfig,
    action_rx: mpsc::Receiver<Action>,
    state: Arc<RwLock<ServiceState>>,

    /// Optional calibration mode flag — when set, HID emission is paused.
    calibration_mode: Option<Arc<AtomicBool>>,

    /// Broadcast channel for forwarding actions to hub visualization widgets.
    action_broadcast_tx: Option<broadcast::Sender<Action>>,

    // State for smoothing and debouncing
    // Reserved for future absolute->relative position tracking.
    _last_mouse_pos: (f32, f32),
    last_action_time: std::time::Instant,
    smoothed_velocity: (f32, f32),
}

impl ActionTask {
    /// Creates a new action task.
    pub fn new(
        config: ActionConfig,
        action_rx: mpsc::Receiver<Action>,
        state: Arc<RwLock<ServiceState>>,
        calibration_mode: Option<Arc<AtomicBool>>,
        action_broadcast_tx: Option<broadcast::Sender<Action>>,
    ) -> Self {
        Self {
            config,
            action_rx,
            state,
            calibration_mode,
            action_broadcast_tx,
            _last_mouse_pos: (0.0, 0.0),
            last_action_time: std::time::Instant::now(),
            smoothed_velocity: (0.0, 0.0),
        }
    }

    /// Runs the action task until shutdown is signaled.
    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!("Action task started");

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
        if let Some(ref p) = platform {
            if let Err(e) = p.check_input_permissions() {
                tracing::warn!("Input permission check failed (HID output disabled): {}", e);
                tracing::warn!("Please grant the necessary permissions and restart.");
                let mut state = self.state.write().await;
                state.task_error = Some((
                    "action".into(),
                    format!("Permission denied: {} \u{2014} HID output disabled", e),
                ));
                platform = None;
            }
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
                    tracing::info!("Action task received shutdown signal");
                    break;
                }

                // Receive actions from IPC task
                action = self.action_rx.recv() => {
                    match action {
                        Some(action) => {
                            // Broadcast action to hub visualization widgets
                            // (always, regardless of confidence/calibration/platform)
                            if let Some(tx) = &self.action_broadcast_tx {
                                let _ = tx.send(action.clone());
                            }

                            // If no platform is available, skip HID emission
                            // but keep broadcasting for visualizations.
                            let Some(ref mut p) = platform else { continue };

                            // Check if output is enabled
                            if !self.config.enabled {
                                continue;
                            }

                            // Check if calibration mode is active — skip HID emission
                            if let Some(flag) = &self.calibration_mode {
                                if flag.load(Ordering::Relaxed) {
                                    continue;
                                }
                            }

                            // Check confidence threshold. If the decoder isn't sure,
                            // we'd rather do nothing than make a mistake.
                            if action.confidence < self.config.min_confidence_threshold {
                                tracing::trace!(
                                    "Skipping action with low confidence: {:.2}",
                                    action.confidence
                                );
                                continue;
                            }

                            // Check debounce timer for discrete actions
                            let now = std::time::Instant::now();
                            let ms_since_last = now.duration_since(self.last_action_time).as_millis() as u32;

                            // Execute the action
                            if let Err(e) = self.execute_action(&mut **p, &action, ms_since_last) {
                                tracing::warn!("Failed to execute action: {}", e);
                            } else {
                                actions_emitted += 1;
                                self.last_action_time = now;

                                // Update shared state
                                let mut state = self.state.write().await;
                                state.actions_emitted = actions_emitted;
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

        tracing::info!("Action task emitted {} actions", actions_emitted);
        Ok(())
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
}
