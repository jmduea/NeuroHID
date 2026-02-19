//! IPC v3 client for trainer-side and integration-test usage.

use ipckit::AsyncLocalSocketStream;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use crate::protocol::{
    ControlRpcRequest, ControlRpcResponse, IpcChannel, IpcConfig, IpcEnvelope, IpcTransport,
};
use neurohid_types::control::{ControlRequest, ControlResponse};
use neurohid_types::error::{IpcError, Result};

/// Client used by trainer-side processes and integration tests.
pub struct IpcClient {
    config: IpcConfig,
    tx: Option<mpsc::Sender<IpcEnvelope>>,
    rx: Option<mpsc::Receiver<IpcEnvelope>>,
}

impl IpcClient {
    pub fn new(config: IpcConfig) -> Self {
        Self {
            config,
            tx: None,
            rx: None,
        }
    }

    /// Establish connection and spawn async read/write tasks.
    pub async fn connect(&mut self) -> Result<()> {
        match self.config.transport {
            IpcTransport::TcpLoopback => {
                let connect = tokio::net::TcpStream::connect(&self.config.endpoint);
                let stream = timeout(
                    Duration::from_millis(self.config.connect_timeout_ms),
                    connect,
                )
                .await
                .map_err(|_| IpcError::Timeout)?
                .map_err(|e| {
                    IpcError::ConnectionFailed(format!(
                        "failed to connect to {}: {}",
                        self.config.endpoint, e
                    ))
                })?;
                let _ = stream.set_nodelay(true);
                self.install_stream(stream);
                Ok(())
            }
            IpcTransport::LocalSocket => {
                let connect = AsyncLocalSocketStream::connect(&self.config.endpoint);
                let stream = timeout(
                    Duration::from_millis(self.config.connect_timeout_ms),
                    connect,
                )
                .await
                .map_err(|_| IpcError::Timeout)?
                .map_err(|error| IpcError::ConnectionFailed(error.to_string()))?;
                self.install_stream(stream);
                Ok(())
            }
        }
    }

    /// Send one IPC v3 envelope to the server.
    pub async fn send(&self, message: IpcEnvelope) -> Result<()> {
        let tx = self
            .tx
            .as_ref()
            .ok_or(IpcError::ConnectionFailed("not connected".to_string()))?;
        tx.send(message)
            .await
            .map_err(|_| IpcError::SendFailed("channel closed".to_string()))?;
        Ok(())
    }

    /// Receive one IPC v3 envelope from the server.
    pub async fn recv(&mut self) -> Result<IpcEnvelope> {
        let rx = self
            .rx
            .as_mut()
            .ok_or(IpcError::ConnectionFailed("not connected".to_string()))?;
        rx.recv()
            .await
            .ok_or_else(|| IpcError::ConnectionLost.into())
    }

    /// Whether the client write side remains open.
    pub fn is_connected(&self) -> bool {
        self.tx.as_ref().is_some_and(|tx| !tx.is_closed())
    }

    /// Drop active transport channels.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.tx = None;
        self.rx = None;
        Ok(())
    }

    fn install_stream<S>(&mut self, stream: S)
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (read_half, write_half) = tokio::io::split(stream);
        let max_message_size = self.config.max_message_size;
        let channel_capacity = self.config.channel_capacity;

        let (write_tx, mut write_rx) = mpsc::channel::<IpcEnvelope>(channel_capacity);
        let (read_tx, read_rx) = mpsc::channel::<IpcEnvelope>(channel_capacity);

        tokio::spawn(async move {
            let mut writer = write_half;
            while let Some(message) = write_rx.recv().await {
                let payload = match serde_json::to_vec(&message) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!("failed to serialize IPC client envelope: {}", error);
                        continue;
                    }
                };
                let len = payload.len() as u32;
                if writer.write_all(&len.to_le_bytes()).await.is_err() {
                    break;
                }
                if writer.write_all(&payload).await.is_err() {
                    break;
                }
                if writer.flush().await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            let mut reader = read_half;
            loop {
                let mut len_buf = [0_u8; 4];
                if let Err(error) = reader.read_exact(&mut len_buf).await {
                    tracing::debug!("IPC client read failed (disconnect?): {}", error);
                    break;
                }
                let message_len = u32::from_le_bytes(len_buf) as usize;
                if message_len > max_message_size {
                    tracing::warn!(
                        message_len,
                        max_message_size,
                        "IPC server message exceeds max size, closing connection"
                    );
                    break;
                }

                let mut payload = vec![0_u8; message_len];
                if let Err(error) = reader.read_exact(&mut payload).await {
                    tracing::debug!("IPC client read failed during payload: {}", error);
                    break;
                }

                match serde_json::from_slice::<IpcEnvelope>(&payload) {
                    Ok(message) => {
                        if read_tx.send(message).await.is_err() {
                            break;
                        }
                    }
                    Err(error) => tracing::warn!("failed to decode IPC client envelope: {}", error),
                }
            }
        });

        self.tx = Some(write_tx);
        self.rx = Some(read_rx);
    }

    /// Send one `control.rpc` request and await a typed response.
    pub async fn send_control_request(
        &mut self,
        request: ControlRequest,
        session_id: &str,
        seq: u64,
    ) -> Result<ControlResponse> {
        let request_id = request.request_id.clone();
        let request_payload = ControlRpcRequest::from(request);
        let envelope = IpcEnvelope::new(
            IpcChannel::ControlRpc,
            "request",
            seq,
            request_id.clone(),
            Some(session_id.to_string()),
            &request_payload,
        )
        .map_err(IpcError::InvalidMessage)?;
        self.send(envelope).await?;

        let response = self.recv().await?;
        decode_control_response_envelope(response, &request_id)
    }
}

/// Connect to an IPC endpoint, send one control request, and disconnect.
pub async fn send_control_request_once(
    config: IpcConfig,
    request: ControlRequest,
    session_id: &str,
    seq: u64,
) -> Result<ControlResponse> {
    let mut client = IpcClient::new(config);
    client.connect().await?;
    let response = client
        .send_control_request(request, session_id, seq)
        .await?;
    client.disconnect().await?;
    Ok(response)
}

/// Blocking helper for sync callers (CLI/UI paths outside async context).
pub fn send_control_request_blocking(
    config: IpcConfig,
    request: ControlRequest,
    session_id: &str,
    seq: u64,
) -> Result<ControlResponse> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| IpcError::ConnectionFailed(error.to_string()))?;
    runtime.block_on(send_control_request_once(config, request, session_id, seq))
}

/// Decode a framed control response envelope into a typed payload.
pub fn decode_control_response_envelope(
    envelope: IpcEnvelope,
    expected_request_id: &Option<String>,
) -> Result<ControlResponse> {
    if envelope.channel != IpcChannel::ControlRpc || envelope.msg_type != "response" {
        return Err(IpcError::InvalidMessage(format!(
            "unexpected control response envelope channel/msg_type: {:?}/{}",
            envelope.channel, envelope.msg_type
        ))
        .into());
    }

    let response_payload: ControlRpcResponse = envelope
        .decode_payload()
        .map_err(IpcError::InvalidMessage)?;
    let response = ControlResponse::from(response_payload);
    if expected_request_id.is_some() && response.request_id != *expected_request_id {
        tracing::debug!(
            expected_request_id = ?expected_request_id,
            actual_request_id = ?response.request_id,
            "control response request_id mismatch"
        );
    }
    Ok(response)
}
