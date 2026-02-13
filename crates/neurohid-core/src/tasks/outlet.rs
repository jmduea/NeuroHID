//! # Outlet Task
//!
//! Publishes selected runtime streams to external consumers using configurable
//! transport targets.

use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::broadcast;

#[cfg(feature = "device-lsl")]
use lsl::{ChannelFormat, Pushable, StreamInfo, StreamOutlet, IRREGULAR_RATE};

use neurohid_types::{
    action::Action,
    config::{OutletConfig, OutletTarget, OutletTransport},
    error::Result,
    event::StreamMarker,
    signal::{FeatureVector, Sample},
};

#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutletMessage {
    Sample { sample: Sample },
    Feature { feature: FeatureVector },
    Action { action: Action },
    Marker { marker: StreamMarker },
}

struct TcpTargetState {
    target: OutletTarget,
    stream: Option<TcpStream>,
    last_connect_attempt: Instant,
}

impl TcpTargetState {
    fn new(target: OutletTarget) -> Self {
        Self {
            target,
            stream: None,
            last_connect_attempt: Instant::now() - Duration::from_secs(10),
        }
    }

    async fn ensure_connected(&mut self) {
        if self.stream.is_some() {
            return;
        }
        if self.last_connect_attempt.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_connect_attempt = Instant::now();

        match TcpStream::connect(&self.target.address).await {
            Ok(stream) => {
                self.stream = Some(stream);
                tracing::info!(
                    target = %self.target.name,
                    addr = %self.target.address,
                    "Outlet target connected"
                );
            }
            Err(e) => {
                tracing::debug!(
                    target = %self.target.name,
                    addr = %self.target.address,
                    error = %e,
                    "Outlet connect attempt failed"
                );
            }
        }
    }

    async fn send_json_line(&mut self, line: &str) {
        self.ensure_connected().await;
        let Some(stream) = &mut self.stream else {
            return;
        };

        if stream.write_all(line.as_bytes()).await.is_err()
            || stream.write_all(b"\n").await.is_err()
        {
            self.stream = None;
        }
    }
}

#[cfg(feature = "device-lsl")]
struct SendOutlet(StreamOutlet);

#[cfg(feature = "device-lsl")]
// SAFETY: liblsl outlets are thread-safe; the underlying C library manages
// synchronization for publish operations.
unsafe impl Send for SendOutlet {}

#[cfg(feature = "device-lsl")]
struct LslNumericOutlet {
    outlet: SendOutlet,
    channel_count: usize,
}

#[cfg(feature = "device-lsl")]
struct LslStringOutlet {
    outlet: SendOutlet,
}

#[cfg(feature = "device-lsl")]
struct LslTargetState {
    target: OutletTarget,
    sample_outlet: Option<LslNumericOutlet>,
    feature_outlet: Option<LslNumericOutlet>,
    action_outlet: Option<LslStringOutlet>,
    marker_outlet: Option<LslStringOutlet>,
}

#[cfg(feature = "device-lsl")]
impl LslTargetState {
    fn new(target: OutletTarget) -> Self {
        Self {
            target,
            sample_outlet: None,
            feature_outlet: None,
            action_outlet: None,
            marker_outlet: None,
        }
    }

    fn publish_sample(&mut self, sample: &Sample) {
        let channel_count = sample.values.len();
        if channel_count == 0 {
            return;
        }

        let Some(outlet) = self.ensure_numeric_outlet(NumericKind::Sample, channel_count) else {
            return;
        };

        if let Err(e) = outlet.outlet.0.push_sample(&sample.values) {
            tracing::debug!(
                target = %self.target.name,
                error = %e,
                "LSL sample publish failed"
            );
        }
    }

    fn publish_feature(&mut self, feature: &FeatureVector) {
        let channel_count = feature.values.len();
        if channel_count == 0 {
            return;
        }

        let Some(outlet) = self.ensure_numeric_outlet(NumericKind::Feature, channel_count) else {
            return;
        };

        if let Err(e) = outlet.outlet.0.push_sample(&feature.values) {
            tracing::debug!(
                target = %self.target.name,
                error = %e,
                "LSL feature publish failed"
            );
        }
    }

    fn publish_action(&mut self, action: &Action) {
        let Ok(json) = serde_json::to_string(action) else {
            return;
        };

        let Some(outlet) = self.ensure_string_outlet(StringKind::Action) else {
            return;
        };

        let payload = vec![json];
        if let Err(e) = outlet.outlet.0.push_sample(&payload) {
            tracing::debug!(
                target = %self.target.name,
                error = %e,
                "LSL action publish failed"
            );
        }
    }

    fn publish_marker(&mut self, marker: &StreamMarker) {
        let Ok(json) = serde_json::to_string(marker) else {
            return;
        };

        let Some(outlet) = self.ensure_string_outlet(StringKind::Marker) else {
            return;
        };

        let payload = vec![json];
        if let Err(e) = outlet.outlet.0.push_sample(&payload) {
            tracing::debug!(
                target = %self.target.name,
                error = %e,
                "LSL marker publish failed"
            );
        }
    }

    fn ensure_numeric_outlet(
        &mut self,
        kind: NumericKind,
        channel_count: usize,
    ) -> Option<&mut LslNumericOutlet> {
        let target = self.target.clone();
        let slot = match kind {
            NumericKind::Sample => &mut self.sample_outlet,
            NumericKind::Feature => &mut self.feature_outlet,
        };

        if let Some(existing) = slot.as_ref()
            && existing.channel_count != channel_count {
                tracing::warn!(
                    target = %target.name,
                    kind = %kind.stream_suffix(),
                    expected = existing.channel_count,
                    got = channel_count,
                    "LSL outlet channel count changed; dropping payload"
                );
                return None;
            }

        if slot.is_none() {
            match Self::create_numeric_outlet(&target, kind, channel_count) {
                Ok(outlet) => {
                    tracing::info!(
                        target = %target.name,
                        stream = %kind.stream_name(&target),
                        channels = channel_count,
                        "LSL outlet stream created"
                    );
                    *slot = Some(outlet);
                }
                Err(e) => {
                    tracing::warn!(
                        target = %target.name,
                        kind = %kind.stream_suffix(),
                        error = %e,
                        "Failed to create LSL numeric outlet stream"
                    );
                    return None;
                }
            }
        }

        slot.as_mut()
    }

    fn ensure_string_outlet(&mut self, kind: StringKind) -> Option<&mut LslStringOutlet> {
        let target = self.target.clone();
        let slot = match kind {
            StringKind::Action => &mut self.action_outlet,
            StringKind::Marker => &mut self.marker_outlet,
        };

        if slot.is_none() {
            match Self::create_string_outlet(&target, kind) {
                Ok(outlet) => {
                    tracing::info!(
                        target = %target.name,
                        stream = %kind.stream_name(&target),
                        "LSL outlet stream created"
                    );
                    *slot = Some(outlet);
                }
                Err(e) => {
                    tracing::warn!(
                        target = %target.name,
                        kind = %kind.stream_suffix(),
                        error = %e,
                        "Failed to create LSL string outlet stream"
                    );
                    return None;
                }
            }
        }

        slot.as_mut()
    }

    fn create_numeric_outlet(
        target: &OutletTarget,
        kind: NumericKind,
        channel_count: usize,
    ) -> std::result::Result<LslNumericOutlet, lsl::Error> {
        let stream_name = kind.stream_name(target);
        let source_id = kind.stream_source_id(target);
        let info = StreamInfo::new(
            &stream_name,
            kind.stream_type(),
            channel_count as u32,
            IRREGULAR_RATE,
            ChannelFormat::Float32,
            &source_id,
        )?;
        let outlet = SendOutlet(StreamOutlet::new(&info, 0, 360)?);
        Ok(LslNumericOutlet {
            outlet,
            channel_count,
        })
    }

    fn create_string_outlet(
        target: &OutletTarget,
        kind: StringKind,
    ) -> std::result::Result<LslStringOutlet, lsl::Error> {
        let stream_name = kind.stream_name(target);
        let source_id = kind.stream_source_id(target);
        let info = StreamInfo::new(
            &stream_name,
            kind.stream_type(),
            1,
            IRREGULAR_RATE,
            ChannelFormat::String,
            &source_id,
        )?;
        let outlet = SendOutlet(StreamOutlet::new(&info, 0, 360)?);
        Ok(LslStringOutlet { outlet })
    }
}

#[cfg(feature = "device-lsl")]
#[derive(Clone, Copy)]
enum NumericKind {
    Sample,
    Feature,
}

#[cfg(feature = "device-lsl")]
impl NumericKind {
    fn stream_suffix(self) -> &'static str {
        match self {
            NumericKind::Sample => "samples",
            NumericKind::Feature => "features",
        }
    }

    fn stream_type(self) -> &'static str {
        match self {
            NumericKind::Sample => "EEG",
            NumericKind::Feature => "NeuroHIDFeature",
        }
    }

    fn stream_name(self, target: &OutletTarget) -> String {
        format!(
            "neurohid_{}_{}",
            sanitize_token(&target.name),
            self.stream_suffix()
        )
    }

    fn stream_source_id(self, target: &OutletTarget) -> String {
        format!(
            "neurohid:{}:{}:{}",
            sanitize_token(&target.name),
            sanitize_token(&target.address),
            self.stream_suffix()
        )
    }
}

#[cfg(feature = "device-lsl")]
#[derive(Clone, Copy)]
enum StringKind {
    Action,
    Marker,
}

#[cfg(feature = "device-lsl")]
impl StringKind {
    fn stream_suffix(self) -> &'static str {
        match self {
            StringKind::Action => "actions",
            StringKind::Marker => "markers",
        }
    }

    fn stream_type(self) -> &'static str {
        match self {
            StringKind::Action => "NeuroHIDAction",
            StringKind::Marker => "NeuroHIDMarker",
        }
    }

    fn stream_name(self, target: &OutletTarget) -> String {
        format!(
            "neurohid_{}_{}",
            sanitize_token(&target.name),
            self.stream_suffix()
        )
    }

    fn stream_source_id(self, target: &OutletTarget) -> String {
        format!(
            "neurohid:{}:{}:{}",
            sanitize_token(&target.name),
            sanitize_token(&target.address),
            self.stream_suffix()
        )
    }
}

#[cfg(feature = "device-lsl")]
fn sanitize_token(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "default".to_string();
    }

    trimmed
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

pub struct OutletTask {
    config: OutletConfig,
    sample_rx: Option<broadcast::Receiver<Sample>>,
    feature_rx: Option<broadcast::Receiver<FeatureVector>>,
    action_rx: Option<broadcast::Receiver<Action>>,
    marker_rx: Option<broadcast::Receiver<StreamMarker>>,
}

impl OutletTask {
    pub fn new(
        config: OutletConfig,
        sample_rx: Option<broadcast::Receiver<Sample>>,
        feature_rx: Option<broadcast::Receiver<FeatureVector>>,
        action_rx: Option<broadcast::Receiver<Action>>,
        marker_rx: Option<broadcast::Receiver<StreamMarker>>,
    ) -> Self {
        Self {
            config,
            sample_rx,
            feature_rx,
            action_rx,
            marker_rx,
        }
    }

    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut targets: Vec<TcpTargetState> = self
            .config
            .targets
            .iter()
            .filter(|t| t.enabled && matches!(t.transport, OutletTransport::TcpJson))
            .cloned()
            .map(TcpTargetState::new)
            .collect();

        #[cfg(feature = "device-lsl")]
        let mut lsl_targets: Vec<LslTargetState> = self
            .config
            .targets
            .iter()
            .filter(|t| t.enabled && matches!(t.transport, OutletTransport::Lsl))
            .cloned()
            .map(LslTargetState::new)
            .collect();

        #[cfg(not(feature = "device-lsl"))]
        let mut lsl_targets: Vec<()> = Vec::new();

        #[cfg(not(feature = "device-lsl"))]
        if self
            .config
            .targets
            .iter()
            .any(|t| t.enabled && matches!(t.transport, OutletTransport::Lsl))
        {
            tracing::warn!(
                "LSL outlet targets configured, but `device-lsl` feature is disabled in core"
            );
        }

        if targets.is_empty() && lsl_targets.is_empty() {
            tracing::info!("OutletTask enabled with no active targets");
            return Ok(());
        }

        let mut tick = tokio::time::interval(Duration::from_millis(20));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("OutletTask shutdown received");
                    break;
                }
                _ = tick.tick() => {
                    if self.config.publish_samples
                        && let Some(rx) = &mut self.sample_rx {
                            while let Ok(sample) = rx.try_recv() {
                                Self::publish_sample(&mut targets, &mut lsl_targets, sample).await;
                            }
                        }

                    if self.config.publish_features
                        && let Some(rx) = &mut self.feature_rx {
                            while let Ok(feature) = rx.try_recv() {
                                Self::publish_feature(&mut targets, &mut lsl_targets, feature).await;
                            }
                        }

                    if self.config.publish_actions
                        && let Some(rx) = &mut self.action_rx {
                            while let Ok(action) = rx.try_recv() {
                                Self::publish_action(&mut targets, &mut lsl_targets, action).await;
                            }
                        }

                    if self.config.publish_markers
                        && let Some(rx) = &mut self.marker_rx {
                            while let Ok(marker) = rx.try_recv() {
                                Self::publish_marker(&mut targets, &mut lsl_targets, marker).await;
                            }
                        }
                }
            }
        }

        Ok(())
    }

    async fn publish_sample(
        tcp_targets: &mut [TcpTargetState],
        #[cfg(feature = "device-lsl")] lsl_targets: &mut [LslTargetState],
        #[cfg(not(feature = "device-lsl"))] _lsl_targets: &mut [()],
        sample: Sample,
    ) {
        #[cfg(feature = "device-lsl")]
        for target in lsl_targets {
            target.publish_sample(&sample);
        }

        if !tcp_targets.is_empty() {
            Self::publish_tcp(tcp_targets, OutletMessage::Sample { sample }).await;
        }
    }

    async fn publish_feature(
        tcp_targets: &mut [TcpTargetState],
        #[cfg(feature = "device-lsl")] lsl_targets: &mut [LslTargetState],
        #[cfg(not(feature = "device-lsl"))] _lsl_targets: &mut [()],
        feature: FeatureVector,
    ) {
        #[cfg(feature = "device-lsl")]
        for target in lsl_targets {
            target.publish_feature(&feature);
        }

        if !tcp_targets.is_empty() {
            Self::publish_tcp(tcp_targets, OutletMessage::Feature { feature }).await;
        }
    }

    async fn publish_action(
        tcp_targets: &mut [TcpTargetState],
        #[cfg(feature = "device-lsl")] lsl_targets: &mut [LslTargetState],
        #[cfg(not(feature = "device-lsl"))] _lsl_targets: &mut [()],
        action: Action,
    ) {
        #[cfg(feature = "device-lsl")]
        for target in lsl_targets {
            target.publish_action(&action);
        }

        if !tcp_targets.is_empty() {
            Self::publish_tcp(tcp_targets, OutletMessage::Action { action }).await;
        }
    }

    async fn publish_marker(
        tcp_targets: &mut [TcpTargetState],
        #[cfg(feature = "device-lsl")] lsl_targets: &mut [LslTargetState],
        #[cfg(not(feature = "device-lsl"))] _lsl_targets: &mut [()],
        marker: StreamMarker,
    ) {
        #[cfg(feature = "device-lsl")]
        for target in lsl_targets {
            target.publish_marker(&marker);
        }

        if !tcp_targets.is_empty() {
            Self::publish_tcp(tcp_targets, OutletMessage::Marker { marker }).await;
        }
    }

    async fn publish_tcp(targets: &mut [TcpTargetState], msg: OutletMessage) {
        let Ok(line) = serde_json::to_string(&msg) else {
            return;
        };
        for target in targets {
            target.send_json_line(&line).await;
        }
    }
}
