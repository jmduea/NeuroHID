use std::hint::black_box;
use std::time::Instant;

use anyhow::{Context, Result};
use prost::Message;

use neurohid_types::action::{Action, MouseAction, MouseButton, MouseButtonEvent, MouseMovement};
use neurohid_types::ipc::{
    DecisionEvent, ErrpWindow, IPC_PROTOCOL_VERSION, IpcChannel, IpcEnvelope, TrainerStatus,
};
use neurohid_types::now_micros;

const WARMUP_ITERATIONS: usize = 1_500;
const BENCH_ITERATIONS: usize = 15_000;

const GATE_MIN_SIZE_REDUCTION: f64 = 0.35;
const GATE_MIN_ENCODE_REDUCTION: f64 = 0.25;
const GATE_MIN_DECODE_REDUCTION: f64 = 0.25;

const KIND_DECISION_EVENT: u32 = 3;
const KIND_ERRP_WINDOW: u32 = 4;
const KIND_TRAINER_STATUS: u32 = 9;

#[derive(Debug, Clone, Copy)]
struct BenchStats {
    avg_encode_ns: f64,
    avg_decode_ns: f64,
    avg_size_bytes: f64,
}

#[derive(Debug, Clone, Copy)]
struct GateDecision {
    size_reduction: f64,
    encode_reduction: f64,
    decode_reduction: f64,
    migrate_to_protobuf: bool,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoEnvelope {
    #[prost(uint32, tag = "1")]
    v: u32,
    #[prost(uint32, tag = "2")]
    kind: u32,
    #[prost(uint64, tag = "3")]
    seq: u64,
    #[prost(int64, tag = "4")]
    sent_at_us: i64,
    #[prost(string, tag = "5")]
    session_id: String,
    #[prost(oneof = "proto_envelope::Payload", tags = "10, 11, 12")]
    payload: Option<proto_envelope::Payload>,
}

mod proto_envelope {
    use super::{ProtoDecisionEvent, ProtoErrpWindow, ProtoTrainerStatus};
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Payload {
        #[prost(message, tag = "10")]
        DecisionEvent(ProtoDecisionEvent),
        #[prost(message, tag = "11")]
        ErrpWindow(ProtoErrpWindow),
        #[prost(message, tag = "12")]
        TrainerStatus(ProtoTrainerStatus),
    }
}

#[derive(Clone, PartialEq, Message)]
struct ProtoDecisionEvent {
    #[prost(string, tag = "1")]
    decision_id: String,
    #[prost(int64, tag = "2")]
    timestamp_us: i64,
    #[prost(float, repeated, tag = "3")]
    feature_values: Vec<f32>,
    #[prost(message, optional, tag = "4")]
    action: Option<ProtoAction>,
    #[prost(float, tag = "5")]
    decoder_confidence: f32,
    #[prost(float, tag = "6")]
    signal_quality: f32,
    #[prost(string, optional, tag = "7")]
    decoder_model_version: Option<String>,
    #[prost(string, optional, tag = "8")]
    stream_id: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoErrpWindow {
    #[prost(string, tag = "1")]
    decision_id: String,
    #[prost(int64, tag = "2")]
    action_timestamp_us: i64,
    #[prost(int64, tag = "3")]
    window_start_us: i64,
    #[prost(int64, tag = "4")]
    window_end_us: i64,
    #[prost(float, tag = "5")]
    sample_rate_hz: f32,
    #[prost(string, repeated, tag = "6")]
    channel_labels: Vec<String>,
    #[prost(message, repeated, tag = "7")]
    channel_data: Vec<ProtoFloatSeries>,
    #[prost(float, tag = "8")]
    signal_quality: f32,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoTrainerStatus {
    #[prost(string, tag = "1")]
    state: String,
    #[prost(uint64, tag = "2")]
    replay_size: u64,
    #[prost(uint64, tag = "3")]
    training_step: u64,
    #[prost(float, optional, tag = "4")]
    policy_loss: Option<f32>,
    #[prost(float, optional, tag = "5")]
    value_loss: Option<f32>,
    #[prost(float, optional, tag = "6")]
    entropy: Option<f32>,
    #[prost(string, optional, tag = "7")]
    last_error: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoFloatSeries {
    #[prost(float, repeated, tag = "1")]
    values: Vec<f32>,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoAction {
    #[prost(int64, tag = "1")]
    timestamp_us: i64,
    #[prost(message, optional, tag = "2")]
    mouse: Option<ProtoMouseAction>,
    #[prost(float, tag = "3")]
    confidence: f32,
    #[prost(string, optional, tag = "4")]
    decision_id: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoMouseAction {
    #[prost(message, optional, tag = "1")]
    movement: Option<ProtoMouseMovement>,
    #[prost(message, repeated, tag = "2")]
    buttons: Vec<ProtoMouseButtonEvent>,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoMouseMovement {
    #[prost(float, tag = "1")]
    dx: f32,
    #[prost(float, tag = "2")]
    dy: f32,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoMouseButtonEvent {
    #[prost(string, tag = "1")]
    button: String,
    #[prost(bool, tag = "2")]
    pressed: bool,
}

fn main() -> Result<()> {
    let (json_samples, proto_samples) = build_sample_messages()?;

    run_warmup(&json_samples, &proto_samples)?;

    let json_stats = bench_json(&json_samples, BENCH_ITERATIONS)?;
    let protobuf_stats = bench_protobuf(&proto_samples, BENCH_ITERATIONS)?;
    let decision = evaluate_gate(json_stats, protobuf_stats);

    print_report(json_stats, protobuf_stats, decision);
    Ok(())
}

fn build_sample_messages() -> Result<(Vec<IpcEnvelope>, Vec<ProtoEnvelope>)> {
    let session_id = "bench_session";
    let mut json_samples = Vec::new();
    let mut proto_samples = Vec::new();

    let (json_decision, proto_decision) = build_decision_event_sample(1, session_id)?;
    json_samples.push(json_decision);
    proto_samples.push(proto_decision);

    let (json_errp, proto_errp) = build_errp_window_sample(2, session_id)?;
    json_samples.push(json_errp);
    proto_samples.push(proto_errp);

    let (json_status, proto_status) = build_trainer_status_sample(3, session_id)?;
    json_samples.push(json_status);
    proto_samples.push(proto_status);

    Ok((json_samples, proto_samples))
}

fn build_decision_event_sample(seq: u64, session_id: &str) -> Result<(IpcEnvelope, ProtoEnvelope)> {
    let timestamp_us = now_micros();
    let decision_id = format!("dec_{seq}");
    let feature_values: Vec<f32> = (0..64)
        .map(|idx| {
            let x = idx as f32 / 64.0;
            (x * std::f32::consts::TAU).sin() * 0.5 + 0.5
        })
        .collect();

    let mut action = Action::mouse(MouseAction {
        movement: Some(MouseMovement {
            dx: 0.16,
            dy: -0.09,
        }),
        buttons: vec![
            MouseButtonEvent {
                button: MouseButton::Left,
                pressed: true,
            },
            MouseButtonEvent {
                button: MouseButton::Left,
                pressed: false,
            },
        ],
        scroll: None,
    });
    action.timestamp = timestamp_us;
    action.confidence = 0.84;
    action.decision_id = Some(decision_id.clone());

    let payload = DecisionEvent {
        decision_id: decision_id.clone(),
        timestamp_us,
        feature_values: feature_values.clone(),
        action: action.clone(),
        decoder_confidence: 0.84,
        signal_quality: 0.79,
        decoder_model_version: Some("decoder-v1.8.4".to_string()),
        stream_id: Some("eeg_primary".to_string()),
    };

    let json = IpcEnvelope::new(
        IpcChannel::TrainerStream,
        "decision_event",
        seq,
        None,
        Some(session_id.to_string()),
        &payload,
    )
    .map_err(anyhow::Error::msg)
    .context("failed to build JSON decision_event envelope")?;

    let proto = ProtoEnvelope {
        v: IPC_PROTOCOL_VERSION as u32,
        kind: KIND_DECISION_EVENT,
        seq,
        sent_at_us: json.sent_at_us,
        session_id: session_id.to_string(),
        payload: Some(proto_envelope::Payload::DecisionEvent(ProtoDecisionEvent {
            decision_id,
            timestamp_us,
            feature_values,
            action: Some(to_proto_action(&action)),
            decoder_confidence: payload.decoder_confidence,
            signal_quality: payload.signal_quality,
            decoder_model_version: payload.decoder_model_version,
            stream_id: payload.stream_id,
        })),
    };

    Ok((json, proto))
}

fn build_errp_window_sample(seq: u64, session_id: &str) -> Result<(IpcEnvelope, ProtoEnvelope)> {
    let action_timestamp_us = now_micros();
    let channel_labels = ["TP9", "AF7", "AF8", "TP10", "REF"]
        .iter()
        .map(|label| (*label).to_string())
        .collect::<Vec<_>>();
    let channel_data: Vec<Vec<f32>> = (0..channel_labels.len())
        .map(|channel_idx| {
            (0..128)
                .map(|sample_idx| {
                    let phase = (sample_idx as f32 / 128.0) * std::f32::consts::TAU;
                    (phase * (1.0 + channel_idx as f32 * 0.2)).sin()
                })
                .collect()
        })
        .collect();

    let payload = ErrpWindow {
        decision_id: "dec_1".to_string(),
        action_timestamp_us,
        window_start_us: action_timestamp_us + 200_000,
        window_end_us: action_timestamp_us + 550_000,
        sample_rate_hz: 128.0,
        channel_labels: channel_labels.clone(),
        channel_data: channel_data.clone(),
        signal_quality: 0.76,
    };

    let json = IpcEnvelope::new(
        IpcChannel::TrainerStream,
        "errp_window",
        seq,
        None,
        Some(session_id.to_string()),
        &payload,
    )
    .map_err(anyhow::Error::msg)
    .context("failed to build JSON errp_window envelope")?;

    let proto = ProtoEnvelope {
        v: IPC_PROTOCOL_VERSION as u32,
        kind: KIND_ERRP_WINDOW,
        seq,
        sent_at_us: json.sent_at_us,
        session_id: session_id.to_string(),
        payload: Some(proto_envelope::Payload::ErrpWindow(ProtoErrpWindow {
            decision_id: payload.decision_id,
            action_timestamp_us: payload.action_timestamp_us,
            window_start_us: payload.window_start_us,
            window_end_us: payload.window_end_us,
            sample_rate_hz: payload.sample_rate_hz,
            channel_labels,
            channel_data: channel_data
                .into_iter()
                .map(|values| ProtoFloatSeries { values })
                .collect(),
            signal_quality: payload.signal_quality,
        })),
    };

    Ok((json, proto))
}

fn build_trainer_status_sample(seq: u64, session_id: &str) -> Result<(IpcEnvelope, ProtoEnvelope)> {
    let payload = TrainerStatus {
        state: "training".to_string(),
        replay_size: 4_096,
        training_step: 12_345,
        policy_loss: Some(0.083),
        value_loss: Some(0.129),
        entropy: Some(0.041),
        last_error: None,
    };

    let json = IpcEnvelope::new(
        IpcChannel::TrainerStream,
        "trainer_status",
        seq,
        None,
        Some(session_id.to_string()),
        &payload,
    )
    .map_err(anyhow::Error::msg)
    .context("failed to build JSON trainer_status envelope")?;

    let proto = ProtoEnvelope {
        v: IPC_PROTOCOL_VERSION as u32,
        kind: KIND_TRAINER_STATUS,
        seq,
        sent_at_us: json.sent_at_us,
        session_id: session_id.to_string(),
        payload: Some(proto_envelope::Payload::TrainerStatus(ProtoTrainerStatus {
            state: payload.state,
            replay_size: payload.replay_size,
            training_step: payload.training_step,
            policy_loss: payload.policy_loss,
            value_loss: payload.value_loss,
            entropy: payload.entropy,
            last_error: payload.last_error,
        })),
    };

    Ok((json, proto))
}

fn to_proto_action(action: &Action) -> ProtoAction {
    let mouse = action.mouse.as_ref().map(|mouse| ProtoMouseAction {
        movement: mouse.movement.as_ref().map(|movement| ProtoMouseMovement {
            dx: movement.dx,
            dy: movement.dy,
        }),
        buttons: mouse
            .buttons
            .iter()
            .map(|event| ProtoMouseButtonEvent {
                button: mouse_button_name(event.button),
                pressed: event.pressed,
            })
            .collect(),
    });

    ProtoAction {
        timestamp_us: action.timestamp,
        mouse,
        confidence: action.confidence,
        decision_id: action.decision_id.clone(),
    }
}

fn mouse_button_name(button: MouseButton) -> String {
    match button {
        MouseButton::Left => "left".to_string(),
        MouseButton::Right => "right".to_string(),
        MouseButton::Middle => "middle".to_string(),
        MouseButton::Extra(index) => format!("extra_{index}"),
    }
}

fn run_warmup(json_samples: &[IpcEnvelope], proto_samples: &[ProtoEnvelope]) -> Result<()> {
    let _ = bench_json(json_samples, WARMUP_ITERATIONS)?;
    let _ = bench_protobuf(proto_samples, WARMUP_ITERATIONS)?;
    Ok(())
}

fn bench_json(samples: &[IpcEnvelope], iterations: usize) -> Result<BenchStats> {
    let mut encoded_messages = Vec::with_capacity(iterations);
    let mut total_bytes = 0usize;

    let encode_start = Instant::now();
    for i in 0..iterations {
        let sample = &samples[i % samples.len()];
        let bytes = serde_json::to_vec(sample).context("JSON encode failed")?;
        total_bytes = total_bytes.saturating_add(bytes.len());
        encoded_messages.push(bytes);
    }
    let encode_elapsed = encode_start.elapsed();

    let decode_start = Instant::now();
    for bytes in &encoded_messages {
        let decoded: IpcEnvelope = serde_json::from_slice(bytes).context("JSON decode failed")?;
        black_box(decoded);
    }
    let decode_elapsed = decode_start.elapsed();

    Ok(BenchStats {
        avg_encode_ns: nanos_per_message(encode_elapsed, iterations),
        avg_decode_ns: nanos_per_message(decode_elapsed, iterations),
        avg_size_bytes: average_bytes(total_bytes, iterations),
    })
}

fn bench_protobuf(samples: &[ProtoEnvelope], iterations: usize) -> Result<BenchStats> {
    let mut encoded_messages = Vec::with_capacity(iterations);
    let mut total_bytes = 0usize;

    let encode_start = Instant::now();
    for i in 0..iterations {
        let sample = &samples[i % samples.len()];
        let bytes = sample.encode_to_vec();
        total_bytes = total_bytes.saturating_add(bytes.len());
        encoded_messages.push(bytes);
    }
    let encode_elapsed = encode_start.elapsed();

    let decode_start = Instant::now();
    for bytes in &encoded_messages {
        let decoded = ProtoEnvelope::decode(bytes.as_slice()).context("protobuf decode failed")?;
        black_box(decoded);
    }
    let decode_elapsed = decode_start.elapsed();

    Ok(BenchStats {
        avg_encode_ns: nanos_per_message(encode_elapsed, iterations),
        avg_decode_ns: nanos_per_message(decode_elapsed, iterations),
        avg_size_bytes: average_bytes(total_bytes, iterations),
    })
}

fn nanos_per_message(elapsed: std::time::Duration, iterations: usize) -> f64 {
    elapsed.as_nanos() as f64 / iterations as f64
}

fn average_bytes(total_bytes: usize, iterations: usize) -> f64 {
    total_bytes as f64 / iterations as f64
}

fn evaluate_gate(json: BenchStats, proto: BenchStats) -> GateDecision {
    let size_reduction = relative_reduction(json.avg_size_bytes, proto.avg_size_bytes);
    let encode_reduction = relative_reduction(json.avg_encode_ns, proto.avg_encode_ns);
    let decode_reduction = relative_reduction(json.avg_decode_ns, proto.avg_decode_ns);

    let migrate_to_protobuf = size_reduction >= GATE_MIN_SIZE_REDUCTION
        && encode_reduction >= GATE_MIN_ENCODE_REDUCTION
        && decode_reduction >= GATE_MIN_DECODE_REDUCTION;

    GateDecision {
        size_reduction,
        encode_reduction,
        decode_reduction,
        migrate_to_protobuf,
    }
}

fn relative_reduction(baseline: f64, candidate: f64) -> f64 {
    if baseline <= f64::EPSILON {
        return 0.0;
    }
    1.0 - (candidate / baseline)
}

fn print_report(json: BenchStats, proto: BenchStats, decision: GateDecision) {
    println!("NeuroHID Runtime ML Protocol Encoding Gate");
    println!(
        "Iterations: {} (warmup {})",
        BENCH_ITERATIONS, WARMUP_ITERATIONS
    );
    println!();
    println!("JSON:");
    println!("  encode: {:>10.1} ns/msg", json.avg_encode_ns);
    println!("  decode: {:>10.1} ns/msg", json.avg_decode_ns);
    println!("  size:   {:>10.1} bytes/msg", json.avg_size_bytes);
    println!();
    println!("Protobuf-shaped envelope:");
    println!("  encode: {:>10.1} ns/msg", proto.avg_encode_ns);
    println!("  decode: {:>10.1} ns/msg", proto.avg_decode_ns);
    println!("  size:   {:>10.1} bytes/msg", proto.avg_size_bytes);
    println!();
    println!(
        "Observed reductions (protobuf vs JSON): size {:.1}%, encode {:.1}%, decode {:.1}%",
        decision.size_reduction * 100.0,
        decision.encode_reduction * 100.0,
        decision.decode_reduction * 100.0
    );
    println!(
        "Gate thresholds: size >= {:.0}%, encode >= {:.0}%, decode >= {:.0}%",
        GATE_MIN_SIZE_REDUCTION * 100.0,
        GATE_MIN_ENCODE_REDUCTION * 100.0,
        GATE_MIN_DECODE_REDUCTION * 100.0
    );
    println!(
        "Decision: {}",
        if decision.migrate_to_protobuf {
            "MIGRATE_TO_PROTOBUF"
        } else {
            "KEEP_JSON_V2"
        }
    );
}
