//! # Event and Marker Types
//!
//! Lightweight event annotations that can be attached to real-time streams
//! and rendered by hub widgets on shared timelines.

use serde::{Deserialize, Serialize};

use crate::Timestamp;

/// A marker/event aligned to stream time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMarker {
    /// Marker timestamp in microseconds.
    pub timestamp: Timestamp,
    /// Optional source stream identifier.
    pub source_id: Option<String>,
    /// Marker category.
    pub marker_type: MarkerType,
    /// Optional marker payload.
    pub payload: Option<MarkerPayload>,
}

impl StreamMarker {
    /// Build a marker with the current timestamp.
    pub fn now(marker_type: MarkerType) -> Self {
        Self {
            timestamp: crate::now_micros(),
            source_id: None,
            marker_type,
            payload: None,
        }
    }

    /// Attach a source ID to this marker.
    pub fn with_source_id(mut self, source_id: impl Into<String>) -> Self {
        self.source_id = Some(source_id.into());
        self
    }

    /// Attach payload data to this marker.
    pub fn with_payload(mut self, payload: MarkerPayload) -> Self {
        self.payload = Some(payload);
        self
    }
}

/// Marker kinds emitted by core tasks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MarkerType {
    EyeBlink,
    MouseClick,
    CursorMovement,
    HeadMovement,
    ErrpWindowStart,
    ErrpWindowResult,
    Custom(String),
}

impl MarkerType {
    /// Human readable label used by UI widgets.
    pub fn label(&self) -> &str {
        match self {
            MarkerType::EyeBlink => "Eye Blink",
            MarkerType::MouseClick => "Mouse Click",
            MarkerType::CursorMovement => "Cursor Movement",
            MarkerType::HeadMovement => "Head Movement",
            MarkerType::ErrpWindowStart => "ErrP Window Start",
            MarkerType::ErrpWindowResult => "ErrP Window Result",
            MarkerType::Custom(s) => s.as_str(),
        }
    }
}

/// Optional marker details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarkerPayload {
    MouseClick {
        button: String,
        pressed: bool,
    },
    CursorMovement {
        dx: f32,
        dy: f32,
        magnitude: f32,
    },
    HeadMovement {
        magnitude: f32,
    },
    ErrpWindow {
        sequence: u64,
        action_timestamp: Timestamp,
    },
    ErrpResult {
        sequence: u64,
        error_probability: f32,
    },
    Text {
        text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{MarkerPayload, MarkerType, StreamMarker};

    #[test]
    fn marker_json_roundtrip() {
        let marker = StreamMarker::now(MarkerType::CursorMovement)
            .with_source_id("stream-1")
            .with_payload(MarkerPayload::CursorMovement {
                dx: 1.0,
                dy: -2.0,
                magnitude: (1.0f32.powi(2) + (-2.0f32).powi(2)).sqrt(),
            });

        let json = serde_json::to_string(&marker).expect("serialize marker");
        let decoded: StreamMarker = serde_json::from_str(&json).expect("deserialize marker");

        assert_eq!(decoded.source_id.as_deref(), Some("stream-1"));
        assert_eq!(decoded.marker_type, MarkerType::CursorMovement);
        assert!(matches!(
            decoded.payload,
            Some(MarkerPayload::CursorMovement { .. })
        ));
    }
}
