use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use super::ServiceManager;
use crate::state::ServiceSnapshot;
#[cfg(windows)]
use neurohid_ipc::{Hello, RuntimeMlRole, TrainerStreamKind};
use neurohid_ipc::{IPC_PROTOCOL_VERSION, IpcChannel, IpcEnvelope};
#[cfg(windows)]
use neurohid_ipc::{IpcConfig, IpcServer, IpcTransport};
use neurohid_types::{
    config::{BrainFlowConfig, DeviceBackend, IpcMode, ServiceRuntimeMode, SystemConfig},
    control::{
        ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload, ControlSnapshot,
        RuntimeModeState,
    },
};

#[test]
fn snapshot_tracks_real_ipc_connect_disconnect_transitions() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let mut manager = ServiceManager::new();
    let mut config = SystemConfig::default();
    config.device.backend = DeviceBackend::BrainFlow;
    config.device.brainflow = Some(BrainFlowConfig::default());
    config.service.ipc_simulation_enabled = false;
    config.service.ipc_mode = IpcMode::TcpLoopback;
    config.service.ipc_endpoint = format!("127.0.0.1:{}", allocate_test_port());

    manager.start(&runtime, config.clone(), None, None);
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);

    let initial = manager.snapshot();
    assert!(!initial.ipc_connected);
    assert!(!initial.ipc_simulated);

    let ipc_handle = manager
        .runtime_handle
        .as_ref()
        .expect("embedded runtime handle should be available")
        .ipc_handle();
    runtime.block_on(async {
        ipc_handle
            .trainer_connected("hub-test-trainer".to_string())
            .await
            .expect("trainer connect event should send");
    });

    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        snap.ipc_connected && !snap.ipc_simulated
    });

    runtime.block_on(async {
        ipc_handle
            .trainer_disconnected()
            .await
            .expect("trainer disconnect event should send");
    });

    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        !snap.ipc_connected && !snap.ipc_simulated
    });

    manager.stop();

    let stopped = manager.snapshot();
    assert!(!stopped.running);
    assert!(!stopped.ipc_connected);
    assert!(!stopped.ipc_simulated);
}

#[test]
fn external_mode_routes_snapshot_and_commands() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let control_port = allocate_test_port();
    let server_join = spawn_mock_control_server(control_port);

    let mut manager = ServiceManager::new();
    let mut config = SystemConfig::default();
    config.service.runtime_mode = ServiceRuntimeMode::External;
    config.service.ipc_mode = IpcMode::TcpLoopback;
    config.service.ipc_endpoint = format!("127.0.0.1:{control_port}");

    manager.configure(&config);
    manager.start(&runtime, config, None, None);

    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);
    manager.enter_calibration_mode();
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        snap.calibration_mode
    });

    manager.set_output_enabled(false);
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        !snap.output_enabled
    });

    manager.update_signal_config(SystemConfig::default().signal);

    manager.stop();
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| !snap.running);

    server_join
        .join()
        .expect("mock control server thread should join");
}

#[test]
fn snapshot_reports_simulated_bridge_with_explicit_tcp_override() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let mut manager = ServiceManager::new();
    let mut config = SystemConfig::default();
    config.device.backend = DeviceBackend::BrainFlow;
    config.device.brainflow = Some(BrainFlowConfig::default());
    config.service.ipc_simulation_enabled = true;
    config.service.ipc_mode = IpcMode::TcpLoopback;
    config.service.ipc_endpoint = format!("127.0.0.1:{}", allocate_test_port());

    manager.start(&runtime, config, None, None);
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        snap.running && snap.ipc_connected && snap.ipc_simulated
    });
    manager.stop();
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| !snap.running);
}

#[cfg(windows)]
#[test]
fn snapshot_tracks_named_pipe_reconnect_and_stall_recovery() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let mut manager = ServiceManager::new();
    let mut config = SystemConfig::default();
    config.device.backend = DeviceBackend::BrainFlow;
    config.device.brainflow = Some(BrainFlowConfig::default());
    config.service.ipc_simulation_enabled = false;
    config.service.ipc_mode = IpcMode::LocalSocket;
    config.service.ipc_endpoint = unique_pipe_name("neurohid_ipc_test");
    config.service.ml_stall_timeout_ms = 120;
    config.service.ml_heartbeat_interval_ms = 50;

    manager.start(&runtime, config.clone(), None, None);
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);

    let ipc_handle = manager
        .runtime_handle
        .as_ref()
        .expect("embedded runtime handle should be available")
        .ipc_handle();
    runtime.block_on(async {
        ipc_handle
            .trainer_connected("named-pipe-test".to_string())
            .await
            .expect("trainer ingress connected event should send");
    });
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        snap.ipc_connected && snap.ml_bridge_connected
    });
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        snap.ml_bridge_stalled
    });

    runtime.block_on(async {
        let hello = Hello {
            protocol: "neurohid_runtime_ml_v3".to_string(),
            role: RuntimeMlRole::Trainer,
            capabilities: vec!["errp_result".to_string()],
            profile_id: None,
            feature_schema_version: None,
            action_schema_version: None,
            decoder_model_version: None,
            trainer_name: Some("test-trainer".to_string()),
            trainer_version: Some("0.0.0".to_string()),
        };
        let envelope = IpcEnvelope::new(
            IpcChannel::TrainerStream,
            TrainerStreamKind::Hello.as_msg_type(),
            1,
            None,
            Some("named-pipe-test".to_string()),
            &hello,
        )
        .expect("hello envelope should encode");
        ipc_handle
            .trainer_send_envelope(envelope)
            .await
            .expect("trainer ingress envelope should send");
    });
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        !snap.ml_bridge_stalled
    });

    runtime.block_on(async {
        ipc_handle
            .trainer_disconnected()
            .await
            .expect("trainer ingress disconnected event should send");
    });
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        !snap.ipc_connected
    });

    runtime.block_on(async {
        ipc_handle
            .trainer_connected("named-pipe-reconnect".to_string())
            .await
            .expect("trainer ingress reconnect event should send");
    });
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        snap.ipc_connected
    });
    runtime.block_on(async {
        ipc_handle
            .trainer_disconnected()
            .await
            .expect("trainer ingress disconnected event should send");
    });

    manager.stop();
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| !snap.running);
}

#[cfg(windows)]
#[test]
fn external_mode_routes_snapshot_and_commands_over_named_pipe() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let pipe_name = unique_pipe_name("neurohid_control_test");
    let server_join = spawn_mock_named_pipe_control_server(pipe_name.clone());

    let mut manager = ServiceManager::new();
    let mut config = SystemConfig::default();
    config.service.runtime_mode = ServiceRuntimeMode::External;
    config.service.ipc_mode = IpcMode::LocalSocket;
    config.service.ipc_endpoint = pipe_name;

    manager.configure(&config);
    manager.start(&runtime, config, None, None);

    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);
    manager.enter_calibration_mode();
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        snap.calibration_mode
    });

    manager.set_output_enabled(false);
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
        !snap.output_enabled
    });

    manager.stop();
    wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| !snap.running);

    server_join
        .join()
        .expect("mock named-pipe control server should join");
}

fn wait_for_snapshot(
    manager: &mut ServiceManager,
    timeout: Duration,
    predicate: impl Fn(&ServiceSnapshot) -> bool,
) {
    let start = Instant::now();
    loop {
        let snap = manager.snapshot();
        if predicate(&snap) {
            return;
        }

        if start.elapsed() > timeout {
            panic!("snapshot did not reach expected state before timeout: {snap:?}");
        }

        thread::sleep(Duration::from_millis(20));
    }
}

fn spawn_mock_control_server(port: u16) -> thread::JoinHandle<()> {
    let listener =
        TcpListener::bind(("127.0.0.1", port)).expect("mock control listener bind should succeed");
    thread::spawn(move || {
        let mut running = true;
        let mut calibration_mode = false;
        let mut output_enabled = true;

        while running {
            let (mut stream, _) = listener
                .accept()
                .expect("mock control listener accept should succeed");

            let request = read_control_request(&stream);
            let response = match request.command {
                ControlCommand::Snapshot => ControlResponse::snapshot(
                    request.request_id,
                    ControlSnapshot {
                        running,
                        uptime_secs: 42,
                        calibration_mode,
                        output_enabled,
                        profile_ready: true,
                        decoder_ready: true,
                        decoder_model_version: Some("test-v1".to_string()),
                        active_profile_name: Some("test-profile".to_string()),
                        device_name: Some("Mock Device".to_string()),
                        outlet_name: None,
                        signal_name: None,
                        decoder_name: None,
                        device_battery: Some(88),
                        signal_quality: 0.9,
                        signal_latency_last_us: 100,
                        signal_latency_p95_us: 150,
                        decode_latency_last_us: 200,
                        decode_latency_p95_us: 240,
                        action_latency_last_us: 300,
                        action_latency_p95_us: 360,
                        latency_degraded: false,
                        latency_alert_message: None,
                        actions_emitted: 10,
                        errors_detected: 1,
                        ipc_connected: true,
                        ipc_simulated: false,
                        learning_enabled: true,
                        ml_bridge_connected: true,
                        ml_bridge_stalled: false,
                        runtime_mode_state: RuntimeModeState::Full,
                        enabled_capabilities: vec![
                            "cursor_move".to_string(),
                            "click".to_string(),
                            "keyboard".to_string(),
                        ],
                        limited_capabilities_message: None,
                        fallback_model_kind: Some("onnx".to_string()),
                        trainer_replay_size: Some(200),
                        trainer_step: Some(33),
                        trainer_policy_loss: Some(0.11),
                        trainer_value_loss: Some(0.22),
                        trainer_entropy: Some(0.03),
                        trainer_last_error: None,
                        candidate_promotions_succeeded: 2,
                        candidate_promotions_rejected: 1,
                        candidate_last_outcome: Some("candidate promotion accepted".to_string()),
                        ml_protocol_version: Some(3),
                        device_connected: true,
                        task_error: None,
                        discovered_streams: vec![],
                        routed_eeg_streams: 1,
                        routed_motion_streams: 1,
                        routed_auxiliary_streams: 2,
                        routed_unknown_streams: 0,
                        pipeline_integrity_degraded: false,
                        integrity_issue_count: 0,
                        stage_health_summary: Some("signal:ok".to_string()),
                        recording_active: false,
                        current_session_id: None,
                    },
                ),
                ControlCommand::SetCalibrationMode { enabled } => {
                    calibration_mode = enabled;
                    ControlResponse::ack(request.request_id)
                }
                ControlCommand::SetOutputEnabled { enabled } => {
                    output_enabled = enabled;
                    ControlResponse::ack(request.request_id)
                }
                ControlCommand::Shutdown => {
                    running = false;
                    ControlResponse::ack(request.request_id)
                }
                ControlCommand::TrainerSnapshot => ControlResponse::trainer_snapshot(
                    request.request_id,
                    neurohid_types::control::TrainerSnapshot {
                        trainer_connected: true,
                        trainer_state: "training".to_string(),
                        replay_size: 200,
                        training_step: 33,
                        last_heartbeat_us: Some(1),
                        last_error: None,
                        protocol_version: Some(3),
                    },
                ),
                ControlCommand::SetLearningEnabled { .. }
                | ControlCommand::MlBridgeReconnect
                | ControlCommand::SetFallbackPolicy { .. }
                | ControlCommand::SetSignalConfig { .. } => {
                    ControlResponse::ack(request.request_id)
                }
                _ => ControlResponse {
                    request_id: request.request_id,
                    payload: ControlResponsePayload::Error {
                        message: "unsupported command in mock server".to_string(),
                    },
                },
            };

            write_control_response(&mut stream, &response);
        }
    })
}

#[cfg(windows)]
fn spawn_mock_named_pipe_control_server(pipe_name: String) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("named-pipe mock runtime should build");

        runtime.block_on(async move {
            let server = IpcServer::new(IpcConfig {
                transport: IpcTransport::LocalSocket,
                endpoint: pipe_name,
                ..IpcConfig::default()
            })
            .await
            .expect("named-pipe mock server should start");

            let mut running = true;
            let mut calibration_mode = false;
            let mut output_enabled = true;

            while running {
                let connection = server
                    .accept()
                    .await
                    .expect("named-pipe mock server accept should succeed");

                loop {
                    let envelope = match connection.recv().await {
                        Ok(envelope) => envelope,
                        Err(_) => break,
                    };
                    let parsed = if envelope.v == IPC_PROTOCOL_VERSION
                        && envelope.channel == IpcChannel::ControlRpc
                        && envelope.msg_type == "request"
                    {
                        envelope.decode_payload::<ControlRequest>()
                    } else {
                        Err("invalid control envelope channel/msg_type".to_string())
                    };
                    let response = match parsed {
                        Ok(request) => match request.command {
                            ControlCommand::Snapshot => ControlResponse::snapshot(
                                request.request_id,
                                ControlSnapshot {
                                    running,
                                    uptime_secs: 42,
                                    calibration_mode,
                                    output_enabled,
                                    profile_ready: true,
                                    decoder_ready: true,
                                    decoder_model_version: Some("test-v1".to_string()),
                                    active_profile_name: Some("test-profile".to_string()),
                                    device_name: Some("Mock Device".to_string()),
                                    outlet_name: None,
                                    signal_name: None,
                                    decoder_name: None,
                                    device_battery: Some(88),
                                    signal_quality: 0.9,
                                    signal_latency_last_us: 100,
                                    signal_latency_p95_us: 150,
                                    decode_latency_last_us: 200,
                                    decode_latency_p95_us: 240,
                                    action_latency_last_us: 300,
                                    action_latency_p95_us: 360,
                                    latency_degraded: false,
                                    latency_alert_message: None,
                                    actions_emitted: 10,
                                    errors_detected: 1,
                                    ipc_connected: true,
                                    ipc_simulated: false,
                                    learning_enabled: true,
                                    ml_bridge_connected: true,
                                    ml_bridge_stalled: false,
                                    runtime_mode_state: RuntimeModeState::Full,
                                    enabled_capabilities: vec![
                                        "cursor_move".to_string(),
                                        "click".to_string(),
                                        "keyboard".to_string(),
                                    ],
                                    limited_capabilities_message: None,
                                    fallback_model_kind: Some("onnx".to_string()),
                                    trainer_replay_size: Some(200),
                                    trainer_step: Some(33),
                                    trainer_policy_loss: Some(0.11),
                                    trainer_value_loss: Some(0.22),
                                    trainer_entropy: Some(0.03),
                                    trainer_last_error: None,
                                    candidate_promotions_succeeded: 2,
                                    candidate_promotions_rejected: 1,
                                    candidate_last_outcome: Some(
                                        "candidate promotion accepted".to_string(),
                                    ),
                                    ml_protocol_version: Some(3),
                                    device_connected: true,
                                    task_error: None,
                                    discovered_streams: vec![],
                                    routed_eeg_streams: 1,
                                    routed_motion_streams: 1,
                                    routed_auxiliary_streams: 2,
                                    routed_unknown_streams: 0,
                                    pipeline_integrity_degraded: false,
                                    integrity_issue_count: 0,
                                    stage_health_summary: Some("signal:ok".to_string()),
                                    recording_active: false,
                                    current_session_id: None,
                                },
                            ),
                            ControlCommand::SetCalibrationMode { enabled } => {
                                calibration_mode = enabled;
                                ControlResponse::ack(request.request_id)
                            }
                            ControlCommand::SetOutputEnabled { enabled } => {
                                output_enabled = enabled;
                                ControlResponse::ack(request.request_id)
                            }
                            ControlCommand::Shutdown => {
                                running = false;
                                ControlResponse::ack(request.request_id)
                            }
                            ControlCommand::SetLearningEnabled { .. }
                            | ControlCommand::MlBridgeReconnect
                            | ControlCommand::SetFallbackPolicy { .. }
                            | ControlCommand::SetSignalConfig { .. } => {
                                ControlResponse::ack(request.request_id)
                            }
                            ControlCommand::TrainerSnapshot => ControlResponse::trainer_snapshot(
                                request.request_id,
                                neurohid_types::control::TrainerSnapshot {
                                    trainer_connected: true,
                                    trainer_state: "training".to_string(),
                                    replay_size: 200,
                                    training_step: 33,
                                    last_heartbeat_us: Some(1),
                                    last_error: None,
                                    protocol_version: Some(3),
                                },
                            ),
                            _ => ControlResponse {
                                request_id: request.request_id,
                                payload: ControlResponsePayload::Error {
                                    message: "unsupported command in named-pipe mock server"
                                        .to_string(),
                                },
                            },
                        },
                        Err(error) => ControlResponse::error(
                            None,
                            format!("invalid control request payload: {}", error),
                        ),
                    };

                    let response_payload = response.clone();
                    let response_envelope = IpcEnvelope::new(
                        IpcChannel::ControlRpc,
                        "response",
                        envelope.seq.saturating_add(1),
                        response.request_id.clone(),
                        Some("mock-control".to_string()),
                        &response_payload,
                    )
                    .expect("named-pipe control response should encode");

                    connection
                        .send(response_envelope)
                        .await
                        .expect("named-pipe control response write should succeed");

                    if !running {
                        return;
                    }
                }
            }
        });
    })
}

fn read_control_request(stream: &TcpStream) -> ControlRequest {
    let mut reader = stream
        .try_clone()
        .expect("mock stream clone should succeed");
    let envelope = read_control_envelope_sync(&mut reader)
        .expect("mock control request should be readable")
        .expect("mock control request should not be empty");
    assert_eq!(
        envelope.v, IPC_PROTOCOL_VERSION,
        "mock control request should use ipc v3"
    );
    assert_eq!(
        envelope.channel,
        IpcChannel::ControlRpc,
        "mock control request should target control.rpc channel"
    );
    assert_eq!(
        envelope.msg_type, "request",
        "mock control request should be request msg_type"
    );
    let request = envelope
        .decode_payload::<ControlRequest>()
        .expect("mock control request payload should parse");
    request
}

fn write_control_response(stream: &mut TcpStream, response: &ControlResponse) {
    let envelope = IpcEnvelope::new(
        IpcChannel::ControlRpc,
        "response",
        1,
        response.request_id.clone(),
        Some("mock-control".to_string()),
        response,
    )
    .expect("mock control response should encode");
    write_control_envelope_sync(stream, &envelope).expect("mock control response should write");
}

fn read_control_envelope_sync<R>(reader: &mut R) -> Result<Option<IpcEnvelope>, String>
where
    R: Read,
{
    let mut len_buf = [0_u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(format!("failed to read control frame length: {}", error)),
    }
    let frame_len = u32::from_le_bytes(len_buf) as usize;
    if frame_len == 0 {
        return Ok(None);
    }
    let mut payload = vec![0_u8; frame_len];
    reader
        .read_exact(&mut payload)
        .map_err(|e| format!("failed to read control frame payload: {}", e))?;
    let envelope = serde_json::from_slice::<IpcEnvelope>(&payload)
        .map_err(|e| format!("failed to decode control envelope: {}", e))?;
    Ok(Some(envelope))
}

fn write_control_envelope_sync<W>(writer: &mut W, envelope: &IpcEnvelope) -> Result<(), String>
where
    W: Write,
{
    let payload = serde_json::to_vec(envelope)
        .map_err(|e| format!("failed to encode control envelope: {}", e))?;
    let frame_len = payload.len() as u32;
    writer
        .write_all(&frame_len.to_le_bytes())
        .map_err(|e| format!("failed to write control frame length: {}", e))?;
    writer
        .write_all(&payload)
        .map_err(|e| format!("failed to write control frame payload: {}", e))?;
    writer
        .flush()
        .map_err(|e| format!("failed to flush control frame payload: {}", e))?;
    Ok(())
}

fn allocate_test_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("ephemeral port bind should succeed")
        .local_addr()
        .expect("local addr should resolve")
        .port()
}

#[cfg(windows)]
fn unique_pipe_name(prefix: &str) -> String {
    format!(
        r"\\.\pipe\{}_{}_{}",
        prefix,
        std::process::id(),
        allocate_test_port()
    )
}
