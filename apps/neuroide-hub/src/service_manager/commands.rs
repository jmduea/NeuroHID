//! Command-dispatch helpers for external runtime control.

use std::time::Instant;

use neurohid_core::facade::{IpcConfig, IpcTransport, send_control_request_blocking};
use neurohid_types::observability as obs;
use neurohid_types::{
    config::IpcMode,
    control::{ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload},
};

use super::{EXTERNAL_CONNECT_TIMEOUT_MS, EXTERNAL_IO_TIMEOUT_MS, ServiceManager};

impl ServiceManager {
    pub(super) fn send_external_command(&self, command: ControlCommand) {
        let endpoint = self.control_endpoint_label();
        match self.send_control_request(ControlRequest::new(command)) {
            Ok(response) => match response.payload {
                ControlResponsePayload::Error { message } => {
                    tracing::warn!(
                        endpoint = %endpoint,
                        "External runtime rejected command: {}",
                        message
                    );
                }
                ControlResponsePayload::TrainerSnapshot { .. } => {
                    tracing::warn!(
                        endpoint = %endpoint,
                        "External runtime returned unexpected trainer snapshot payload"
                    );
                }
                ControlResponsePayload::Ack
                | ControlResponsePayload::Snapshot { .. }
                | ControlResponsePayload::RecordingStarted { .. }
                | ControlResponsePayload::RecordingStopped { .. } => {}
            },
            Err(error) => {
                tracing::warn!(endpoint = %endpoint, "Failed to send external runtime command: {}", error);
            }
        }
    }

    pub(super) fn send_control_request(
        &self,
        request: ControlRequest,
    ) -> Result<ControlResponse, String> {
        let endpoint = self.control_endpoint_label();
        let request_id = request.request_id.clone();
        let command = Self::control_command_name(&request.command);
        let started = Instant::now();
        let _request_span = tracing::debug_span!(
            obs::span::CONTROL_REQUEST,
            stage = obs::stage::CONTROL,
            decision_id = obs::field::UNKNOWN,
            stream_id = obs::field::UNKNOWN,
            command,
            request_id = request_id.as_deref().unwrap_or("none")
        )
        .entered();

        if self.allow_control_debug() {
            tracing::debug!(
                event = obs::event::CONTROL_REQUEST_RECEIVED,
                endpoint = %endpoint,
                request_id = request_id.as_deref().unwrap_or("none"),
                decision_id = obs::field::UNKNOWN,
                stream_id = obs::field::UNKNOWN,
                command,
                mode = ?self.external_ipc_mode,
                "Sending external control request"
            );
        }

        let config = self.external_control_ipc_config();
        let response = send_control_request_blocking(config, request, "hub-control", 1)
            .map_err(|error| format!("external control request failed: {}", error));

        match &response {
            Ok(ok) => {
                if self.allow_control_debug() {
                    tracing::debug!(
                        event = obs::event::CONTROL_RESPONSE_SENT,
                        endpoint = %endpoint,
                        request_id = request_id.as_deref().unwrap_or("none"),
                        decision_id = obs::field::UNKNOWN,
                        stream_id = obs::field::UNKNOWN,
                        command,
                        payload = %Self::control_response_kind(&ok.payload),
                        duration_ms = started.elapsed().as_millis() as u64,
                        "Received external control response"
                    );
                }
            }
            Err(error) => tracing::warn!(
                endpoint = %endpoint,
                request_id = request_id.as_deref().unwrap_or("none"),
                decision_id = obs::field::UNKNOWN,
                stream_id = obs::field::UNKNOWN,
                command,
                duration_ms = started.elapsed().as_millis() as u64,
                "External control request failed: {}",
                error
            ),
        }

        response
    }

    pub(super) fn allow_control_debug(&self) -> bool {
        self.control_emit_gate
            .lock()
            .map(|mut gate| gate.allow_debug())
            .unwrap_or(true)
    }

    pub(super) fn external_control_ipc_config(&self) -> IpcConfig {
        let transport = match self.external_ipc_mode {
            IpcMode::LocalSocket => IpcTransport::LocalSocket,
            IpcMode::TcpLoopback => IpcTransport::TcpLoopback,
        };
        IpcConfig {
            transport,
            endpoint: self.external_ipc_endpoint.clone(),
            connect_timeout_ms: EXTERNAL_CONNECT_TIMEOUT_MS,
            send_timeout_ms: EXTERNAL_IO_TIMEOUT_MS,
            recv_timeout_ms: EXTERNAL_IO_TIMEOUT_MS,
            ..IpcConfig::default()
        }
    }

    pub(super) fn control_endpoint_label(&self) -> String {
        self.external_ipc_endpoint.clone()
    }

    pub(super) fn control_response_kind(payload: &ControlResponsePayload) -> &'static str {
        match payload {
            ControlResponsePayload::Ack => "ack",
            ControlResponsePayload::Snapshot { .. } => "snapshot",
            ControlResponsePayload::TrainerSnapshot { .. } => "trainer_snapshot",
            ControlResponsePayload::Error { .. } => "error",
            ControlResponsePayload::RecordingStarted { .. } => "recording_started",
            ControlResponsePayload::RecordingStopped { .. } => "recording_stopped",
        }
    }

    pub(super) fn control_command_name(command: &ControlCommand) -> &'static str {
        match command {
            ControlCommand::Snapshot => "snapshot",
            ControlCommand::Shutdown => "shutdown",
            ControlCommand::SetCalibrationMode { .. } => "set_calibration_mode",
            ControlCommand::SetOutputEnabled { .. } => "set_output_enabled",
            ControlCommand::ReloadModel => "reload_model",
            ControlCommand::PromoteCandidateModel => "promote_candidate_model",
            ControlCommand::RescanStreams => "rescan_streams",
            ControlCommand::ConnectStream { .. } => "connect_stream",
            ControlCommand::DisconnectStream { .. } => "disconnect_stream",
            ControlCommand::SetLearningEnabled { .. } => "set_learning_enabled",
            ControlCommand::MlBridgeReconnect => "ml_bridge_reconnect",
            ControlCommand::TrainerSnapshot => "trainer_snapshot",
            ControlCommand::SetFallbackPolicy { .. } => "set_fallback_policy",
            ControlCommand::SetSignalConfig { .. } => "set_signal_config",
            ControlCommand::StartRecording { .. } => "start_recording",
            ControlCommand::StopRecording => "stop_recording",
        }
    }

    pub(super) fn set_last_error(&mut self, message: String) {
        if self.last_error.as_deref() != Some(message.as_str()) {
            tracing::error!("{}", message);
        }
        self.last_error = Some(message);
    }
}
