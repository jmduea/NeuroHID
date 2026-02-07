//! # Cortex WebSocket JSON-RPC Client
//!
//! Low-level transport for communicating with the Emotiv Cortex API.
//! Handles WebSocket connection, TLS (self-signed cert for localhost),
//! JSON-RPC request/response correlation, and the authentication flow.
//!
//! ## Architecture
//!
//! The WebSocket connection is split into reader/writer halves using
//! `tokio-tungstenite`'s `StreamExt::split()`. This allows concurrent
//! API calls and data streaming on the same WebSocket:
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                 CortexClient                     │
//! │                                                  │
//! │  writer: Arc<Mutex<SplitSink>>  ◄── call()       │
//! │                                  ◄── subscribe() │
//! │                                                  │
//! │  reader_loop (spawned task):                     │
//! │    SplitStream ─┬─► RPC response → oneshot tx    │
//! │                 ├─► eeg event    → eeg_tx        │
//! │                 ├─► dev event    → dev_tx        │
//! │                 ├─► mot event    → mot_tx        │
//! │                 └─► pow event    → pow_tx        │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! ## TLS Note
//!
//! The Emotiv Cortex service runs at `wss://localhost:6868` with a
//! self-signed TLS certificate. We configure `native-tls` to accept
//! this certificate since it's a localhost-only connection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::{stream::SplitSink, stream::SplitStream, SinkExt, StreamExt};
use native_tls::TlsConnector as NativeTlsConnector;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::{
    connect_async_tls_with_config,
    tungstenite::{http, Message},
    Connector, MaybeTlsStream, WebSocketStream,
};

use neurohid_types::error::{DeviceError, Error as NeurohidError, Result};

use super::protocol::{
    CortexRequest, CortexResponse, DetectionInfo, DetectionType, ErrorCodes, ExportFormat,
    HeadsetInfo, MarkerInfo, Methods, ProfileAction, ProfileInfo, RecordInfo, SessionInfo, Streams,
    TrainingStatus, UserLoginInfo,
};

/// Default timeout for Cortex API calls.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Connection timeout for the initial WebSocket handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Channel buffer size for data stream events.
const STREAM_CHANNEL_BUFFER: usize = 1024;

/// Type alias for the write half of the WebSocket connection.
type WsWriter = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

/// Type alias for the read half of the WebSocket connection.
type WsReader = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

/// A pending RPC response awaiting its matching JSON-RPC response by `id`.
type PendingResponse = oneshot::Sender<Result<serde_json::Value>>;

/// Senders for dispatching stream data events to consumers.
pub(crate) type StreamSenders = HashMap<&'static str, mpsc::Sender<serde_json::Value>>;

/// Receivers for consuming stream data events.
pub(crate) type StreamReceivers = HashMap<&'static str, mpsc::Receiver<serde_json::Value>>;

/// WebSocket JSON-RPC client for the Emotiv Cortex API.
///
/// This client manages a single WebSocket connection, split into reader
/// and writer halves. The writer is shared (behind `Arc<Mutex>`) so that
/// API calls can be made concurrently with data streaming. The reader
/// runs in a background task that dispatches:
///
/// - **RPC responses** → matched by `id` to pending `oneshot` channels
/// - **Data events** → routed by stream type to `mpsc` channels
pub struct CortexClient {
    /// Shared write half of the WebSocket.
    writer: Arc<Mutex<WsWriter>>,

    /// Map of pending RPC requests awaiting responses, keyed by request ID.
    pending_responses: Arc<Mutex<HashMap<u64, PendingResponse>>>,

    /// Auto-incrementing request ID counter.
    next_id: AtomicU64,

    /// Handle to the background reader loop task.
    reader_handle: Option<JoinHandle<()>>,

    /// Whether the reader loop is currently running.
    reader_running: Arc<std::sync::atomic::AtomicBool>,

    /// Shared stream senders, dynamically updatable without restarting
    /// the reader loop. The reader holds a clone of this Arc and checks
    /// it on each data message.
    stream_senders: Arc<std::sync::Mutex<Option<StreamSenders>>>,
}

impl CortexClient {
    /// Connect to the Cortex API WebSocket service.
    ///
    /// The Cortex service must be running on the local machine.
    /// This configures TLS to accept the self-signed localhost certificate.
    pub async fn connect(url: &str) -> Result<Self> {
        // Configure TLS to accept self-signed certificates (Cortex uses one
        // for its localhost WebSocket server)
        let tls_connector = NativeTlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| DeviceError::ConnectionFailed {
                reason: format!("TLS configuration failed: {}", e),
            })?;

        let connector = Connector::NativeTls(tls_connector);

        // Parse the WebSocket URL as a URI for the connection.
        let uri: http::Uri =
            url.parse()
                .map_err(|e: http::uri::InvalidUri| DeviceError::ConnectionFailed {
                    reason: format!("Invalid Cortex URL '{}': {}", url, e),
                })?;

        let connect_fut = connect_async_tls_with_config(
            uri,
            None, // WebSocket config
            true, // disable_nagle
            Some(connector),
        );

        let (ws, response) = tokio::time::timeout(CONNECT_TIMEOUT, connect_fut)
            .await
            .map_err(|_| DeviceError::Timeout)?
            .map_err(|e| DeviceError::ConnectionFailed {
                reason: format!(
                    "WebSocket connection to Cortex failed: {}. \
                     Is the Emotiv Cortex service running?",
                    e
                ),
            })?;

        tracing::info!(url, status = %response.status(), "Connected to Cortex API");
        tracing::debug!(
            url,
            status = %response.status(),
            headers = ?response.headers(),
            "WebSocket upgrade response details"
        );

        // Split the WebSocket into reader and writer halves.
        // The writer is shared for concurrent API calls; the reader
        // is consumed by the background reader loop.
        let (writer, reader) = ws.split();

        let pending_responses: Arc<Mutex<HashMap<u64, PendingResponse>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let reader_running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let stream_senders: Arc<std::sync::Mutex<Option<StreamSenders>>> =
            Arc::new(std::sync::Mutex::new(None));

        // Start the reader loop immediately — it needs to be running before
        // any API calls so that responses can be dispatched.
        let reader_handle = Self::spawn_reader_loop(
            reader,
            Arc::clone(&pending_responses),
            Arc::clone(&reader_running),
            Arc::clone(&stream_senders),
        );

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            pending_responses,
            next_id: AtomicU64::new(1),
            reader_handle: Some(reader_handle),
            reader_running,
            stream_senders,
        })
    }

    /// Spawn the background reader loop that dispatches WebSocket messages.
    ///
    /// This task reads every message from the WebSocket and routes it:
    /// - Messages with a JSON-RPC `id` field → matched to `pending_responses`
    /// - Messages with stream data fields (eeg, dev, mot, etc.) → forwarded to stream channels
    fn spawn_reader_loop(
        mut reader: WsReader,
        pending_responses: Arc<Mutex<HashMap<u64, PendingResponse>>>,
        running: Arc<std::sync::atomic::AtomicBool>,
        stream_senders: Arc<std::sync::Mutex<Option<StreamSenders>>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                let msg = tokio::select! {
                    msg = reader.next() => msg,
                    // Check running flag every 100ms to allow clean shutdown
                    _ = tokio::time::sleep(Duration::from_millis(100)) => continue,
                };

                match msg {
                    Some(Ok(Message::Text(text))) => {
                        tracing::debug!(raw = %text, "Reader loop received message");

                        let value: serde_json::Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::warn!("Failed to parse WebSocket message as JSON: {}", e);
                                continue;
                            }
                        };

                        // Check if this is an RPC response (has an `id` field)
                        if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
                            let response: std::result::Result<CortexResponse, _> =
                                serde_json::from_value(value);

                            let mut pending = pending_responses.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                match response {
                                    Ok(resp) => {
                                        let result = if let Some(error) = resp.error {
                                            Err(DeviceError::CortexApiError {
                                                code: error.code,
                                                message: error.message,
                                            }
                                            .into())
                                        } else {
                                            resp.result.ok_or_else(|| {
                                                DeviceError::CommunicationError(
                                                    "Response has no result or error".into(),
                                                )
                                                .into()
                                            })
                                        };
                                        let _ = tx.send(result);
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Err(DeviceError::CommunicationError(
                                            format!("Failed to parse RPC response: {}", e),
                                        )
                                        .into()));
                                    }
                                }
                            } else {
                                tracing::debug!(id, "Received response for unknown request ID");
                            }
                            continue;
                        }

                        // Not an RPC response — route as a stream data event.
                        // Lock the shared senders briefly to check and forward.
                        // Uses std::sync::Mutex (not tokio) so the lock is held
                        // only for the duration of the try_send call.
                        if let Ok(guard) = stream_senders.lock() {
                            if let Some(ref senders) = *guard {
                                for (key, tx) in senders.iter() {
                                    if value.get(*key).is_some() {
                                        let _ = tx.try_send(value);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::info!("Cortex WebSocket closed by server");
                        let mut pending = pending_responses.lock().await;
                        for (_, tx) in pending.drain() {
                            let _ = tx.send(Err(DeviceError::ConnectionLost {
                                reason: "Cortex WebSocket closed".into(),
                            }
                            .into()));
                        }
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!("WebSocket read error: {}", e);
                        let mut pending = pending_responses.lock().await;
                        for (_, tx) in pending.drain() {
                            let _ = tx.send(Err(DeviceError::CommunicationError(format!(
                                "WebSocket error: {}",
                                e
                            ))
                            .into()));
                        }
                        break;
                    }
                    None => {
                        tracing::info!("Cortex WebSocket stream ended");
                        break;
                    }
                    _ => {
                        // Binary messages, pings, pongs — skip
                    }
                }
            }

            tracing::debug!("Reader loop exiting");
            running.store(false, Ordering::SeqCst);
        })
    }

    // ─── Core RPC ───────────────────────────────────────────────────────

    /// Send a JSON-RPC request and wait for the matching response.
    ///
    /// This registers a oneshot channel in `pending_responses`, sends the
    /// request via the shared writer, and awaits the response from the
    /// reader loop.
    async fn call(
        &self,
        method: &'static str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = CortexRequest::new(id, method, params);

        let json = serde_json::to_string(&request)
            .map_err(|e| DeviceError::CommunicationError(format!("serialize error: {}", e)))?;

        tracing::debug!(method, id, json = %json, "Sending Cortex request");

        // Register the pending response before sending (to avoid race conditions)
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_responses.lock().await;
            pending.insert(id, tx);
        }

        // Send the request via the shared writer
        {
            let mut writer = self.writer.lock().await;
            writer.send(Message::Text(json.into())).await.map_err(|e| {
                DeviceError::CommunicationError(format!("WebSocket send error: {}", e))
            })?;
        }

        // Wait for the reader loop to deliver the response
        let result = tokio::time::timeout(DEFAULT_TIMEOUT, rx)
            .await
            .map_err(|_| {
                // Clean up the pending entry on timeout
                let pending = self.pending_responses.clone();
                tokio::spawn(async move {
                    pending.lock().await.remove(&id);
                });
                DeviceError::Timeout
            })?
            .map_err(|_| {
                DeviceError::CommunicationError(
                    "Response channel dropped (reader loop died)".into(),
                )
            })??;

        Ok(result)
    }

    // ─── Streaming ──────────────────────────────────────────────────────

    /// Stream name validation and mapping to static keys for senders/receivers.
    fn stream_key(name: &str) -> &'static str {
        match name {
            Streams::EEG => "eeg",
            Streams::DEV => "dev",
            Streams::MOT => "mot",
            Streams::POW => "pow",
            Streams::MET => "met",
            Streams::COM => "com",
            Streams::FAC => "fac",
            Streams::SYS => "sys",
            other => {
                tracing::warn!(stream = other, "Unknown stream type");
                "unknown"
            }
        }
    }

    /// Create data stream channels for the specified streams.
    ///
    /// This creates mpsc channels and installs the senders into the
    /// shared senders structure that the reader loop checks. The reader
    /// loop does NOT need to be restarted — the senders are updated
    /// in-place behind the shared `Arc<Mutex>`.
    ///
    /// Call this before `subscribe_streams()`. Returns receivers that
    /// the device layer uses to consume stream events.
    pub(crate) fn create_stream_channels(&self, streams: &[&str]) -> StreamReceivers {
        let mut senders = StreamSenders::new();
        let mut receivers = StreamReceivers::new();

        for &stream in streams {
            let (tx, rx) = mpsc::channel(STREAM_CHANNEL_BUFFER);
            senders.insert(Self::stream_key(stream), tx);
            receivers.insert(Self::stream_key(stream), rx);
        }

        // Install the senders into the shared structure that the reader
        // loop already holds. This is an in-place update — no restart needed.
        if let Ok(mut guard) = self.stream_senders.lock() {
            *guard = Some(senders);
        }

        receivers
    }

    /// Add a single stream channel without disturbing existing ones.
    ///
    /// This is used to incrementally subscribe to additional data streams
    /// (e.g., motion, band power) after the initial EEG+DEV channels have
    /// been set up by `create_stream_channels()`.
    ///
    /// Returns a receiver for the new channel, or `None` if the stream
    /// name is unknown.
    pub(crate) fn add_stream_channel(
        &self,
        stream: &str,
    ) -> Option<mpsc::Receiver<serde_json::Value>> {
        let (tx, rx) = mpsc::channel(STREAM_CHANNEL_BUFFER);
        if let Ok(mut guard) = self.stream_senders.lock() {
            let senders = guard.get_or_insert_with(StreamSenders::new);
            senders.insert(Self::stream_key(stream), tx);
            Some(rx)
        } else {
            None
        }
    }

    /// Remove a single stream channel sender.
    ///
    /// The corresponding receiver will see the channel close.
    pub(crate) fn remove_stream_channel(&self, stream: &str) {
        if let Ok(mut guard) = self.stream_senders.lock() {
            if let Some(ref mut senders) = *guard {
                senders.remove(stream);
            }
        }
    }

    /// Clear all stream senders, causing the reader loop to stop routing
    /// data events. Existing receivers will see the channel close.
    pub(crate) fn clear_stream_channels(&self) {
        if let Ok(mut guard) = self.stream_senders.lock() {
            *guard = None;
        }
    }

    // ─── Authentication ─────────────────────────────────────────────────

    /// Query Cortex service version and build info.
    ///
    /// This is the simplest API call — no authentication required. Useful as
    /// a connection health check and for logging the running Cortex version.
    pub async fn get_cortex_info(&self) -> Result<serde_json::Value> {
        self.call(Methods::GET_CORTEX_INFO, serde_json::json!({}))
            .await
    }

    /// Check if the application has been granted access rights.
    pub async fn has_access_right(&self, client_id: &str, client_secret: &str) -> Result<bool> {
        let result = self
            .call(
                Methods::HAS_ACCESS_RIGHT,
                serde_json::json!({
                    "clientId": client_id,
                    "clientSecret": client_secret,
                }),
            )
            .await?;

        Ok(result
            .get("accessGranted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    /// Get the currently logged-in Emotiv user.
    pub async fn get_user_login(&self) -> Result<Vec<UserLoginInfo>> {
        let result = self
            .call(Methods::GET_USER_LOGIN, serde_json::json!({}))
            .await?;

        serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("Failed to parse user login info: {}", e))
                .into()
        })
    }

    /// Authenticate with the Cortex API.
    ///
    /// Performs: `getCortexInfo` → `requestAccess` → `authorize`.
    ///
    /// - `getCortexInfo` verifies the API is responding and logs the version.
    /// - `requestAccess` is gracefully skipped if the Cortex version doesn't
    ///   support it (error -32601), since newer Launcher versions handle
    ///   app approval internally.
    /// - `authorize` must succeed — it returns the cortex token for all
    ///   subsequent operations.
    pub async fn authenticate(&self, client_id: &str, client_secret: &str) -> Result<String> {
        // Step 0: getCortexInfo — verify API is alive, log version
        let cortex_info_ok = match self.get_cortex_info().await {
            Ok(info) => {
                tracing::info!("Cortex API info: {}", info);
                true
            }
            Err(e) => {
                tracing::warn!("getCortexInfo failed (continuing anyway): {}", e);
                false
            }
        };

        // Step 1: requestAccess — prompts user approval if first time.
        // Gracefully skip if the method doesn't exist (-32601), which
        // happens on newer Cortex versions where the Launcher handles
        // app approval directly.
        match self
            .call(
                Methods::REQUEST_ACCESS,
                serde_json::json!({
                    "clientId": client_id,
                    "clientSecret": client_secret,
                }),
            )
            .await
        {
            Ok(_) => tracing::debug!("Cortex access requested"),
            Err(e) => {
                if is_cortex_error_code(&e, ErrorCodes::METHOD_NOT_FOUND) {
                    tracing::info!(
                        "requestAccess not available on this Cortex version \
                         (Launcher handles app approval directly)"
                    );
                } else {
                    return Err(e);
                }
            }
        }

        // Step 2: Authorize and get a cortex token
        let auth_result = match self
            .call(
                Methods::AUTHORIZE,
                serde_json::json!({
                    "clientId": client_id,
                    "clientSecret": client_secret,
                }),
            )
            .await
        {
            Ok(result) => result,
            Err(e) => {
                if is_cortex_error_code(&e, ErrorCodes::METHOD_NOT_FOUND) {
                    if !cortex_info_ok {
                        tracing::error!(
                            "Both getCortexInfo and authorize returned 'Method not found'. \
                             The service on the Cortex URL port may not be the Emotiv Cortex API, \
                             or it may be an incompatible version. Verify that:\n\
                             1. The EMOTIV Launcher is installed and running\n\
                             2. You are logged in with your EmotivID\n\
                             3. The Cortex URL (Settings > Device) matches the Emotiv Launcher port\n\
                             4. The Launcher version supports the Cortex API (v2.0+)"
                        );
                    }
                    return Err(DeviceError::ConnectionFailed {
                        reason: "Cortex API 'authorize' method not found (-32601). \
                                 The Emotiv Cortex service may not be running or may be \
                                 an incompatible version. Check that the EMOTIV Launcher is \
                                 running and you are logged in with your EmotivID."
                            .to_string(),
                    }
                    .into());
                }
                return Err(e);
            }
        };

        let cortex_token = auth_result
            .get("cortexToken")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DeviceError::CommunicationError("authorize response missing cortexToken".into())
            })?
            .to_string();

        tracing::info!("Cortex authentication successful");

        Ok(cortex_token)
    }

    // ─── Headset Management ─────────────────────────────────────────────

    /// Query available headsets.
    pub async fn query_headsets(&self) -> Result<Vec<HeadsetInfo>> {
        let result = self
            .call(Methods::QUERY_HEADSETS, serde_json::json!({}))
            .await?;

        let headsets: Vec<HeadsetInfo> = serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse headset list: {}", e))
        })?;

        tracing::info!(count = headsets.len(), "Queried headsets");

        Ok(headsets)
    }

    /// Connect to a specific headset via the Cortex service.
    ///
    /// This is only needed if the headset status is "discovered" but not
    /// yet "connected". If already connected, this is a no-op.
    pub async fn connect_headset(&self, headset_id: &str) -> Result<()> {
        let _result = self
            .call(
                Methods::CONTROL_DEVICE,
                serde_json::json!({
                    "command": "connect",
                    "headset": headset_id,
                }),
            )
            .await?;

        tracing::info!(headset = headset_id, "Headset connection initiated");

        Ok(())
    }

    /// Disconnect a headset from the Cortex service.
    pub async fn disconnect_headset(&self, headset_id: &str) -> Result<()> {
        let _result = self
            .call(
                Methods::CONTROL_DEVICE,
                serde_json::json!({
                    "command": "disconnect",
                    "headset": headset_id,
                }),
            )
            .await?;

        tracing::info!(headset = headset_id, "Headset disconnection initiated");

        Ok(())
    }

    /// Trigger headset scanning / refresh.
    ///
    /// The Cortex API documentation specifies calling `controlDevice` with
    /// `command: "refresh"` before `queryHeadsets` to ensure the service
    /// scans for newly available devices.
    pub async fn refresh_headsets(&self) -> Result<()> {
        let _result = self
            .call(
                Methods::CONTROL_DEVICE,
                serde_json::json!({
                    "command": "refresh",
                }),
            )
            .await?;

        tracing::debug!("Headset refresh/scan triggered");

        Ok(())
    }

    /// Synchronize the system clock with the headset clock.
    ///
    /// Returns the headset's monotonic timestamp for precise time alignment.
    pub async fn sync_with_headset_clock(
        &self,
        cortex_token: &str,
        headset_id: &str,
    ) -> Result<serde_json::Value> {
        self.call(
            Methods::SYNC_WITH_HEADSET_CLOCK,
            serde_json::json!({
                "cortexToken": cortex_token,
                "headset": headset_id,
            }),
        )
        .await
    }

    // ─── Session Management ─────────────────────────────────────────────

    /// Create a session for a headset.
    ///
    /// A session associates a headset with a cortex token and is
    /// required before subscribing to data streams.
    pub async fn create_session(
        &self,
        cortex_token: &str,
        headset_id: &str,
    ) -> Result<SessionInfo> {
        let result = self
            .call(
                Methods::CREATE_SESSION,
                serde_json::json!({
                    "cortexToken": cortex_token,
                    "headset": headset_id,
                    "status": "active",
                }),
            )
            .await?;

        let session: SessionInfo = serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse session info: {}", e))
        })?;

        tracing::info!(session_id = %session.id, "Session created");

        Ok(session)
    }

    /// Query existing sessions.
    pub async fn query_sessions(&self, cortex_token: &str) -> Result<Vec<SessionInfo>> {
        let result = self
            .call(
                Methods::QUERY_SESSIONS,
                serde_json::json!({
                    "cortexToken": cortex_token,
                }),
            )
            .await?;

        serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse sessions: {}", e)).into()
        })
    }

    /// Close a session.
    pub async fn close_session(&self, cortex_token: &str, session_id: &str) -> Result<()> {
        let _result = self
            .call(
                Methods::UPDATE_SESSION,
                serde_json::json!({
                    "cortexToken": cortex_token,
                    "session": session_id,
                    "status": "close",
                }),
            )
            .await;

        // Ignore errors — the session may already be closed
        tracing::info!(session_id, "Session closed");

        Ok(())
    }

    // ─── Data Streams ───────────────────────────────────────────────────

    /// Subscribe to one or more data streams.
    ///
    /// The streams parameter should contain stream names from [`Streams`].
    /// The reader loop must have been configured with matching stream
    /// senders via `setup_stream_channels()` before calling this.
    pub async fn subscribe_streams(
        &self,
        cortex_token: &str,
        session_id: &str,
        streams: &[&str],
    ) -> Result<()> {
        let _result = self
            .call(
                Methods::SUBSCRIBE,
                serde_json::json!({
                    "cortexToken": cortex_token,
                    "session": session_id,
                    "streams": streams,
                }),
            )
            .await?;

        tracing::info!(session_id, ?streams, "Subscribed to data streams");

        Ok(())
    }

    /// Unsubscribe from one or more data streams.
    pub async fn unsubscribe_streams(
        &self,
        cortex_token: &str,
        session_id: &str,
        streams: &[&str],
    ) -> Result<()> {
        let _result = self
            .call(
                Methods::UNSUBSCRIBE,
                serde_json::json!({
                    "cortexToken": cortex_token,
                    "session": session_id,
                    "streams": streams,
                }),
            )
            .await?;

        tracing::info!(session_id, ?streams, "Unsubscribed from data streams");

        Ok(())
    }

    // ─── Records ────────────────────────────────────────────────────────

    /// Start a new recording.
    pub async fn create_record(
        &self,
        cortex_token: &str,
        session_id: &str,
        title: &str,
    ) -> Result<RecordInfo> {
        let result = self
            .call(
                Methods::CREATE_RECORD,
                serde_json::json!({
                    "cortexToken": cortex_token,
                    "session": session_id,
                    "title": title,
                }),
            )
            .await?;

        let record: RecordInfo = serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse record info: {}", e))
        })?;

        tracing::info!(record_id = %record.uuid, "Recording started");

        Ok(record)
    }

    /// Stop an active recording.
    pub async fn stop_record(&self, cortex_token: &str, session_id: &str) -> Result<RecordInfo> {
        let result = self
            .call(
                Methods::UPDATE_RECORD,
                serde_json::json!({
                    "cortexToken": cortex_token,
                    "session": session_id,
                    "status": "stop",
                }),
            )
            .await?;

        let record: RecordInfo = serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse record info: {}", e))
        })?;

        tracing::info!(record_id = %record.uuid, "Recording stopped");

        Ok(record)
    }

    /// Query recorded sessions.
    pub async fn query_records(
        &self,
        cortex_token: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<RecordInfo>> {
        let mut params = serde_json::json!({
            "cortexToken": cortex_token,
        });

        if let Some(limit) = limit {
            params["limit"] = serde_json::json!(limit);
        }
        if let Some(offset) = offset {
            params["offset"] = serde_json::json!(offset);
        }

        let result = self.call(Methods::QUERY_RECORDS, params).await?;

        // queryRecords returns { "records": [...], "count": N }
        let records = result
            .get("records")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));

        serde_json::from_value(records).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse records: {}", e)).into()
        })
    }

    /// Export a recording to CSV or EDF format.
    pub async fn export_record(
        &self,
        cortex_token: &str,
        record_ids: &[String],
        folder: &str,
        format: ExportFormat,
    ) -> Result<()> {
        let _result = self
            .call(
                Methods::EXPORT_RECORD,
                serde_json::json!({
                    "cortexToken": cortex_token,
                    "recordIds": record_ids,
                    "folder": folder,
                    "format": format.as_str(),
                }),
            )
            .await?;

        tracing::info!(
            ?record_ids,
            folder,
            format = format.as_str(),
            "Export initiated"
        );

        Ok(())
    }

    // ─── Markers ────────────────────────────────────────────────────────

    /// Inject a time-stamped marker during an active recording.
    ///
    /// Returns marker info including the UUID for later reference.
    pub async fn inject_marker(
        &self,
        cortex_token: &str,
        session_id: &str,
        label: &str,
        value: i32,
        time: Option<f64>,
    ) -> Result<MarkerInfo> {
        let mut params = serde_json::json!({
            "cortexToken": cortex_token,
            "session": session_id,
            "label": label,
            "value": value,
        });

        if let Some(t) = time {
            params["time"] = serde_json::json!(t);
        }

        let result = self.call(Methods::INJECT_MARKER, params).await?;

        let marker: MarkerInfo = serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse marker info: {}", e))
        })?;

        tracing::debug!(marker_id = %marker.uuid, label, "Marker injected");

        Ok(marker)
    }

    /// Update a marker to convert it from an instance marker to an interval marker.
    pub async fn update_marker(
        &self,
        cortex_token: &str,
        session_id: &str,
        marker_id: &str,
        time: Option<f64>,
    ) -> Result<()> {
        let mut params = serde_json::json!({
            "cortexToken": cortex_token,
            "session": session_id,
            "markerId": marker_id,
        });

        if let Some(t) = time {
            params["time"] = serde_json::json!(t);
        }

        let _result = self.call(Methods::UPDATE_MARKER, params).await?;

        tracing::debug!(marker_id, "Marker updated");

        Ok(())
    }

    // ─── Profiles ───────────────────────────────────────────────────────

    /// List all profiles for the current user.
    pub async fn query_profiles(&self, cortex_token: &str) -> Result<Vec<ProfileInfo>> {
        let result = self
            .call(
                Methods::QUERY_PROFILE,
                serde_json::json!({
                    "cortexToken": cortex_token,
                }),
            )
            .await?;

        serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse profiles: {}", e)).into()
        })
    }

    /// Get the profile currently loaded for a headset.
    pub async fn get_current_profile(
        &self,
        cortex_token: &str,
        headset_id: &str,
    ) -> Result<Option<ProfileInfo>> {
        let result = self
            .call(
                Methods::GET_CURRENT_PROFILE,
                serde_json::json!({
                    "cortexToken": cortex_token,
                    "headset": headset_id,
                }),
            )
            .await?;

        // Returns null/empty if no profile loaded
        if result.is_null() {
            return Ok(None);
        }

        let profile: ProfileInfo = serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse profile info: {}", e))
        })?;

        Ok(Some(profile))
    }

    /// Manage a profile (create, load, unload, save, rename, delete).
    pub async fn setup_profile(
        &self,
        cortex_token: &str,
        headset_id: &str,
        profile_name: &str,
        action: ProfileAction,
    ) -> Result<()> {
        let _result = self
            .call(
                Methods::SETUP_PROFILE,
                serde_json::json!({
                    "cortexToken": cortex_token,
                    "headset": headset_id,
                    "profile": profile_name,
                    "status": action.as_str(),
                }),
            )
            .await?;

        tracing::info!(
            profile = profile_name,
            action = action.as_str(),
            "Profile action completed"
        );

        Ok(())
    }

    // ─── BCI / Training ─────────────────────────────────────────────────

    /// Get detection info for a specific detection type.
    ///
    /// Returns available actions, controls, and events for mental command
    /// or facial expression detection.
    pub async fn get_detection_info(&self, detection: DetectionType) -> Result<DetectionInfo> {
        let result = self
            .call(
                Methods::GET_DETECTION_INFO,
                serde_json::json!({
                    "detection": detection.as_str(),
                }),
            )
            .await?;

        serde_json::from_value(result).map_err(|e| {
            DeviceError::CommunicationError(format!("failed to parse detection info: {}", e)).into()
        })
    }

    /// Control the training lifecycle for mental commands or facial expressions.
    pub async fn training(
        &self,
        cortex_token: &str,
        session_id: &str,
        detection: DetectionType,
        status: TrainingStatus,
        action: &str,
    ) -> Result<serde_json::Value> {
        self.call(
            Methods::TRAINING,
            serde_json::json!({
                "cortexToken": cortex_token,
                "session": session_id,
                "detection": detection.as_str(),
                "status": status.as_str(),
                "action": action,
            }),
        )
        .await
    }

    /// Get or set the active mental command actions.
    pub async fn mental_command_active_action(
        &self,
        cortex_token: &str,
        session_id: &str,
        actions: Option<&[&str]>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({
            "cortexToken": cortex_token,
            "session": session_id,
            "status": if actions.is_some() { "set" } else { "get" },
        });

        if let Some(actions) = actions {
            params["actions"] = serde_json::json!(actions);
        }

        self.call(Methods::MENTAL_COMMAND_ACTIVE_ACTION, params)
            .await
    }

    /// Get or set the mental command action sensitivity.
    pub async fn mental_command_action_sensitivity(
        &self,
        cortex_token: &str,
        session_id: &str,
        values: Option<&[i32]>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({
            "cortexToken": cortex_token,
            "session": session_id,
            "status": if values.is_some() { "set" } else { "get" },
        });

        if let Some(values) = values {
            params["values"] = serde_json::json!(values);
        }

        self.call(Methods::MENTAL_COMMAND_ACTION_SENSITIVITY, params)
            .await
    }

    /// Get the mental command brain map.
    pub async fn mental_command_brain_map(
        &self,
        cortex_token: &str,
        session_id: &str,
    ) -> Result<serde_json::Value> {
        self.call(
            Methods::MENTAL_COMMAND_BRAIN_MAP,
            serde_json::json!({
                "cortexToken": cortex_token,
                "session": session_id,
            }),
        )
        .await
    }

    /// Get or set the mental command training threshold.
    pub async fn mental_command_training_threshold(
        &self,
        cortex_token: &str,
        session_id: &str,
    ) -> Result<serde_json::Value> {
        self.call(
            Methods::MENTAL_COMMAND_TRAINING_THRESHOLD,
            serde_json::json!({
                "cortexToken": cortex_token,
                "session": session_id,
            }),
        )
        .await
    }

    // ─── Connection Management ──────────────────────────────────────────

    /// Stop the reader loop.
    pub(crate) async fn stop_reader(&mut self) {
        self.reader_running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.reader_handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }
    }

    /// Close the WebSocket connection.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.stop_reader().await;

        let mut writer = self.writer.lock().await;
        let _ = writer.close().await;

        Ok(())
    }
}

/// Check if an error is a Cortex API error with a specific error code.
///
/// Used to gracefully handle known error codes (e.g., -32601 for deprecated
/// methods) without propagating them as hard failures.
fn is_cortex_error_code(err: &NeurohidError, target_code: i32) -> bool {
    match err {
        NeurohidError::Device(DeviceError::CortexApiError { code, .. }) => *code == target_code,
        _ => false,
    }
}
