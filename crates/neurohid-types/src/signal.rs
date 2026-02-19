//! # Signal Types
//!
//! Types related to biosignal data: raw samples, channel configurations,
//! and extracted features.

use crate::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Unique identifier for an EEG channel.
/// We use a string-based ID to support different naming conventions
/// (e.g., "AF3", "Cz", "electrode_1").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(pub String);

impl ChannelId {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Configuration for a single channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// The channel identifier
    pub id: ChannelId,

    /// Standard 10-20 position name, if applicable (e.g., "AF3", "Pz")
    pub position_10_20: Option<String>,

    /// Whether this channel is currently enabled for data collection
    pub enabled: bool,

    /// Reference electrode for this channel, if known
    pub reference: Option<ChannelId>,
}

/// Configuration for all channels in a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceChannelConfig {
    /// All available channels
    pub channels: Vec<ChannelConfig>,

    /// Sampling rate in Hz
    pub sampling_rate_hz: f32,

    /// Resolution in bits (e.g., 14 for Emotiv Insight)
    pub resolution_bits: u8,
}

impl DeviceChannelConfig {
    /// Get the number of enabled channels
    pub fn enabled_channel_count(&self) -> usize {
        self.channels.iter().filter(|c| c.enabled).count()
    }

    /// Get the sample period in microseconds
    pub fn sample_period_micros(&self) -> i64 {
        (1_000_000.0 / self.sampling_rate_hz) as i64
    }
}

/// A single multi-channel sample from the device.
/// This is the raw data as received from the hardware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    /// Identifies which stream this sample came from (LSL stream id).
    /// `None` for mock devices or when stream identity is irrelevant.
    pub source_id: Option<String>,

    /// Timestamp when this sample was acquired (device time, if available)
    pub device_timestamp: Option<Timestamp>,

    /// Timestamp when this sample was received by the system
    pub system_timestamp: Timestamp,

    /// Sequence number from the device (for detecting dropped samples)
    pub sequence_number: Option<u64>,

    /// Channel values in microvolts. The order corresponds to the channel
    /// configuration's channel order.
    pub values: Vec<f32>,

    /// Optional quality indicators per channel (0.0 = bad, 1.0 = good)
    pub quality: Option<Vec<f32>>,
}

impl Sample {
    /// Create a new sample with the current system timestamp
    pub fn new(values: Vec<f32>) -> Self {
        Self {
            source_id: None,
            device_timestamp: None,
            system_timestamp: crate::now_micros(),
            sequence_number: None,
            values,
            quality: None,
        }
    }

    /// Get the number of channels in this sample
    pub fn channel_count(&self) -> usize {
        self.values.len()
    }

    /// Get the value for a specific channel index
    pub fn get(&self, channel_index: usize) -> Option<f32> {
        self.values.get(channel_index).copied()
    }
}

/// A batch of samples, typically a time window for processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleBatch {
    /// The samples in this batch, ordered by time (oldest first)
    pub samples: Vec<Sample>,

    /// The channel configuration these samples correspond to
    pub channel_config: DeviceChannelConfig,
}

impl SampleBatch {
    /// Get the duration of this batch in microseconds
    pub fn duration_micros(&self) -> i64 {
        if self.samples.len() < 2 {
            return 0;
        }
        // Length checked above — first()/last() are infallible here.
        let first = self.samples.first().expect("len >= 2").system_timestamp;
        let last = self.samples.last().expect("len >= 2").system_timestamp;
        last - first
    }

    /// Get the approximate sample count
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Extracted features from a signal window.
/// This is what gets passed to the decoder network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    /// The feature values
    pub values: Vec<f32>,

    /// Timestamp of the center of the window these features were extracted from
    pub timestamp: Timestamp,

    /// Optional stream identifier propagated from source samples.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,

    /// Optional window start timestamp in microseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_start_us: Option<Timestamp>,

    /// Optional window end timestamp in microseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_end_us: Option<Timestamp>,

    /// Optional labels for each feature dimension (for debugging/analysis)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

impl FeatureVector {
    /// Create a new feature vector with the current timestamp
    pub fn new(values: Vec<f32>) -> Self {
        Self {
            values,
            timestamp: crate::now_micros(),
            stream_id: None,
            window_start_us: None,
            window_end_us: None,
            labels: None,
        }
    }

    /// Create a feature vector with labels
    pub fn with_labels(values: Vec<f32>, labels: Vec<String>) -> Self {
        assert_eq!(
            values.len(),
            labels.len(),
            "Feature values and labels must have same length"
        );
        Self {
            values,
            timestamp: crate::now_micros(),
            stream_id: None,
            window_start_us: None,
            window_end_us: None,
            labels: Some(labels),
        }
    }

    /// Get the dimensionality of the feature vector
    pub fn dim(&self) -> usize {
        self.values.len()
    }
}

/// Frequency bands commonly used in EEG analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrequencyBand {
    /// Delta: 0.5 - 4 Hz (deep sleep, unconscious processes)
    Delta,
    /// Theta: 4 - 8 Hz (drowsiness, light sleep, meditation, error processing)
    Theta,
    /// Alpha: 8 - 13 Hz (relaxed, calm, eyes closed)
    Alpha,
    /// Beta: 13 - 30 Hz (alert, active thinking, focus)
    Beta,
    /// Gamma: 30 - 100+ Hz (high-level cognition, perception)
    Gamma,
    /// Custom frequency range
    Custom { low_hz: u32, high_hz: u32 },
}

impl FrequencyBand {
    /// Get the frequency range for this band in Hz
    pub fn range_hz(&self) -> (f32, f32) {
        match self {
            FrequencyBand::Delta => (0.5, 4.0),
            FrequencyBand::Theta => (4.0, 8.0),
            FrequencyBand::Alpha => (8.0, 13.0),
            FrequencyBand::Beta => (13.0, 30.0),
            FrequencyBand::Gamma => (30.0, 100.0),
            FrequencyBand::Custom { low_hz, high_hz } => (*low_hz as f32, *high_hz as f32),
        }
    }
}

/// Specifies what type of features to extract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureType {
    /// Raw (filtered) signal values
    Raw,
    /// Band power for specific frequency bands
    BandPower(Vec<FrequencyBand>),
    /// Power spectral density
    PSD { num_bins: usize },
    /// Time-domain statistics (mean, variance, etc.)
    Statistics,
    /// Hjorth parameters (activity, mobility, complexity)
    Hjorth,
    /// All of the above combined
    Full,
}

/// Potential types for an XDF stream.
///
/// Reference: XDF stream content type recommendations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum XdfStreamType {
    Eeg,
    Markers,
    Gaze,
    Mocap,
    Nirs,
    Audio,
    VideoRaw,
    VideoCompressed,
    Quality,
    Performance,
    Device,
}

impl XdfStreamType {
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "eeg" => Some(Self::Eeg),
            "markers" => Some(Self::Markers),
            "gaze" => Some(Self::Gaze),
            "mocap" => Some(Self::Mocap),
            "nirs" => Some(Self::Nirs),
            "audio" => Some(Self::Audio),
            "video_raw" | "videoraw" => Some(Self::VideoRaw),
            "video_compressed" | "videocompressed" => Some(Self::VideoCompressed),
            "quality" => Some(Self::Quality),
            "performance" => Some(Self::Performance),
            "device" => Some(Self::Device),
            _ => None,
        }
    }
}

/// A lossless XML-like metadata node used for raw XDF/LSL header capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XdfMetaNode {
    pub name: String,
    pub value: Option<String>,
    pub children: Vec<XdfMetaNode>,
}

/// Flattened lookup index derived from an [`XdfMetaNode`] tree.
pub type XdfPathIndex = BTreeMap<String, Vec<String>>;

/// Parsed convenience fields from the `<info>` stream header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XdfStreamHeaderCore {
    pub name: Option<String>,
    pub stream_type_raw: Option<String>,
    pub stream_type: Option<XdfStreamType>,
    pub channel_count: Option<i32>,
    pub nominal_srate: Option<f64>,
    pub channel_format: Option<String>,
    pub source_id: Option<String>,
    pub version: Option<i32>,
    pub created_at: Option<f64>,
    pub uid: Option<String>,
    pub session_id: Option<String>,
    pub hostname: Option<String>,
}

/// Raw XDF stream header payload with both canonical tree and derived lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XdfStreamHeaderRaw {
    pub info: XdfMetaNode,
    pub core: XdfStreamHeaderCore,
    pub path_index: XdfPathIndex,
}

/// Build a flattened path index from a raw XDF metadata tree.
#[must_use]
pub fn build_xdf_path_index(info: &XdfMetaNode) -> XdfPathIndex {
    let mut out = BTreeMap::new();
    index_node(info, &info.name, &mut out);
    out
}

fn index_node(node: &XdfMetaNode, path: &str, out: &mut XdfPathIndex) {
    if let Some(value) = &node.value {
        out.entry(path.to_string()).or_default().push(value.clone());
    }

    let mut total_by_name: BTreeMap<&str, usize> = BTreeMap::new();
    for child in &node.children {
        *total_by_name.entry(child.name.as_str()).or_insert(0) += 1;
    }

    let mut seen_by_name: BTreeMap<&str, usize> = BTreeMap::new();
    for child in &node.children {
        let seen = seen_by_name.entry(child.name.as_str()).or_insert(0);
        let total = total_by_name.get(child.name.as_str()).copied().unwrap_or(1);
        let child_path = if total > 1 {
            format!("{path}.{}[{seen}]", child.name)
        } else {
            format!("{path}.{}", child.name)
        };
        *seen += 1;
        index_node(child, &child_path, out);
    }
}

#[cfg(test)]
mod xdf_metadata_tests {
    use super::*;

    #[test]
    fn xdf_stream_type_parse_is_case_insensitive() {
        assert_eq!(XdfStreamType::parse("EEG"), Some(XdfStreamType::Eeg));
        assert_eq!(XdfStreamType::parse("eeg"), Some(XdfStreamType::Eeg));
        assert_eq!(
            XdfStreamType::parse("VideoCompressed"),
            Some(XdfStreamType::VideoCompressed)
        );
        assert_eq!(XdfStreamType::parse("unknown_type"), None);
    }

    #[test]
    fn xdf_path_index_preserves_repeated_channel_labels_in_order() {
        let info = XdfMetaNode {
            name: "info".to_string(),
            value: None,
            children: vec![
                XdfMetaNode {
                    name: "name".to_string(),
                    value: Some("MetaTester".to_string()),
                    children: vec![],
                },
                XdfMetaNode {
                    name: "type".to_string(),
                    value: Some("EEG".to_string()),
                    children: vec![],
                },
                XdfMetaNode {
                    name: "desc".to_string(),
                    value: None,
                    children: vec![
                        XdfMetaNode {
                            name: "manufacturer".to_string(),
                            value: Some("SCCN".to_string()),
                            children: vec![],
                        },
                        XdfMetaNode {
                            name: "cap".to_string(),
                            value: None,
                            children: vec![
                                XdfMetaNode {
                                    name: "name".to_string(),
                                    value: Some("EasyCap".to_string()),
                                    children: vec![],
                                },
                                XdfMetaNode {
                                    name: "size".to_string(),
                                    value: Some("54".to_string()),
                                    children: vec![],
                                },
                            ],
                        },
                        XdfMetaNode {
                            name: "channels".to_string(),
                            value: None,
                            children: vec![
                                XdfMetaNode {
                                    name: "channel".to_string(),
                                    value: None,
                                    children: vec![XdfMetaNode {
                                        name: "label".to_string(),
                                        value: Some("C3".to_string()),
                                        children: vec![],
                                    }],
                                },
                                XdfMetaNode {
                                    name: "channel".to_string(),
                                    value: None,
                                    children: vec![XdfMetaNode {
                                        name: "label".to_string(),
                                        value: Some("C4".to_string()),
                                        children: vec![],
                                    }],
                                },
                            ],
                        },
                    ],
                },
            ],
        };

        let path_index = build_xdf_path_index(&info);

        assert_eq!(
            path_index.get("info.desc.manufacturer"),
            Some(&vec!["SCCN".to_string()])
        );
        assert_eq!(
            path_index.get("info.desc.cap.size"),
            Some(&vec!["54".to_string()])
        );
        assert_eq!(
            path_index.get("info.desc.channels.channel[0].label"),
            Some(&vec!["C3".to_string()])
        );
        assert_eq!(
            path_index.get("info.desc.channels.channel[1].label"),
            Some(&vec!["C4".to_string()])
        );
    }
}
