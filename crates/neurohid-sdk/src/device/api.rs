//! High-level device discovery and connection API over the runtime or discovery-only path.

use neurohid_device::traits::DeviceProvider;
use neurohid_types::device::DiscoveredStream;
use neurohid_types::error::{Error, Result};

use crate::runtime::runtime::{RuntimeCommand, RuntimeHandle, RuntimeIpcHandle};
use crate::types::device::DeviceInfo;

/// List discovered streams from a running runtime.
///
/// Sends `RescanStreams` then reads the current snapshot; the list may lag by one
/// discovery cycle. Use when you have an in-process or IPC-backed runtime handle.
pub fn list_streams_via_runtime(handle: &RuntimeHandle) -> Result<Vec<DiscoveredStream>> {
    handle
        .command(RuntimeCommand::RescanStreams)
        .map_err(|e| Error::Internal(e.to_string()))?;
    let snapshot = handle.snapshot();
    Ok(snapshot.discovered_streams.clone())
}

/// Connect to a stream by id and return a handle that disconnects on drop.
///
/// "Connect" means the connect command is sent; actual connection is asynchronous
/// in the device task. Observe success via a fresh snapshot or listener.
pub fn connect_by_id(
    handle: &RuntimeHandle,
    stream_id: &str,
) -> Result<StreamConnectionHandle> {
    handle
        .command(RuntimeCommand::ConnectStream {
            stream_id: stream_id.to_string(),
        })
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(StreamConnectionHandle {
        ipc: handle.ipc_handle(),
        stream_id: stream_id.to_string(),
    })
}

/// Predicate for selecting a stream by criteria (e.g. first LSL or first EEG).
pub type StreamPredicate = dyn Fn(&DiscoveredStream) -> bool + Send + Sync;

/// Connect to the first stream matching the predicate and return a handle that disconnects on drop.
///
/// Lists streams via the runtime (RescanStreams then Snapshot), then takes the first match.
/// Order is implementation-defined; prefer [`connect_by_id`] when the stream id is known.
pub fn connect_by_criteria(
    handle: &RuntimeHandle,
    predicate: &StreamPredicate,
) -> Result<StreamConnectionHandle> {
    let streams = list_streams_via_runtime(handle)?;
    let stream_id = streams
        .iter()
        .find(|s| predicate(s))
        .map(|s| s.id.clone())
        .ok_or_else(|| Error::Internal("no stream matched criteria".into()))?;
    connect_by_id(handle, &stream_id)
}

/// Scoped connection handle; dropping it sends `DisconnectStream` for the stream.
///
/// When the device disappears, the runtime may invalidate state; further commands
/// using this handle may no longer apply.
pub struct StreamConnectionHandle {
    ipc: RuntimeIpcHandle,
    stream_id: String,
}

impl StreamConnectionHandle {
    /// Return the stream id for this connection.
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }
}

impl Drop for StreamConnectionHandle {
    fn drop(&mut self) {
        let _ = self.ipc.command(RuntimeCommand::DisconnectStream {
            stream_id: self.stream_id.clone(),
        });
    }
}

/// Map a [`DeviceProvider`] discovery result to a list of [`DiscoveredStream`] for scripts.
///
/// Does not start the full runtime; use when you only need a point-in-time list of
/// available devices/streams. Order is implementation-defined.
pub async fn list_streams_discovery(
    provider: &(dyn DeviceProvider + Send + Sync),
) -> Result<Vec<DiscoveredStream>> {
    let devices = provider.discover().await.map_err(|e| {
        Error::Internal(format!("discovery failed: {}", e))
    })?;
    Ok(devices
        .into_iter()
        .map(device_info_to_discovered_stream)
        .collect())
}

fn device_info_to_discovered_stream(info: DeviceInfo) -> DiscoveredStream {
    let name = info.name.unwrap_or_else(|| info.id.0.clone());
    let stream_type = format!("{:?}", info.device_type);
    DiscoveredStream {
        id: info.id.0,
        name,
        stream_type,
        channel_count: 0,
        sample_rate: 0.0,
        connected: false,
        battery_percent: info.battery_percent,
        channel_quality: None,
        source_id: info.source_id,
        effective_sample_rate_hz: None,
        samples_received: None,
        samples_dropped: None,
        drop_rate_pct: None,
        last_sample_age_ms: None,
        preprocessing_summary: None,
        integrity_state: None,
    }
}
