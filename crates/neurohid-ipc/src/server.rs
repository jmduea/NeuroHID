//! Runtime ML IPC server.

use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};

use crate::protocol::{IpcConfig, IpcTransport, RuntimeMlEnvelopeV2};
use neurohid_types::error::{IpcError, Result};

#[cfg(windows)]
use tokio::net::windows::named_pipe::ServerOptions;

enum ServerBackend {
    Tcp(tokio::net::TcpListener),
    #[cfg(windows)]
    NamedPipe,
}

/// Runtime ML bridge server.
pub struct IpcServer {
    config: IpcConfig,
    backend: ServerBackend,
}

impl IpcServer {
    /// Create a new IPC server.
    pub async fn new(config: IpcConfig) -> Result<Self> {
        let backend = match config.transport {
            IpcTransport::TcpLoopback => {
                let listener = tokio::net::TcpListener::bind(&config.address)
                    .await
                    .map_err(|e| {
                        IpcError::ConnectionFailed(format!(
                            "Failed to bind TCP socket at {}: {}",
                            config.address, e
                        ))
                    })?;
                tracing::info!(address = %config.address, "IPC server listening (tcp)");
                ServerBackend::Tcp(listener)
            }
            IpcTransport::NamedPipe => {
                #[cfg(windows)]
                {
                    tracing::info!(pipe = %config.pipe_name, "IPC server listening (named pipe)");
                    ServerBackend::NamedPipe
                }
                #[cfg(not(windows))]
                {
                    return Err(IpcError::ConnectionFailed(
                        "named pipes are only available on Windows".to_string(),
                    )
                    .into());
                }
            }
        };

        Ok(Self { config, backend })
    }

    /// Accept one client connection.
    pub async fn accept(&self) -> Result<IpcConnection> {
        match &self.backend {
            ServerBackend::Tcp(listener) => {
                let (stream, addr) = listener
                    .accept()
                    .await
                    .map_err(|e| IpcError::ConnectionFailed(format!("Accept failed: {e}")))?;
                tracing::info!(%addr, "IPC client connected (tcp)");
                let _ = stream.set_nodelay(true);
                Ok(spawn_connection_tasks(stream, self.config.max_message_size))
            }
            #[cfg(windows)]
            ServerBackend::NamedPipe => {
                let server = ServerOptions::new()
                    .create(&self.config.pipe_name)
                    .map_err(|e| {
                        IpcError::ConnectionFailed(format!(
                            "Failed to create named pipe {}: {}",
                            self.config.pipe_name, e
                        ))
                    })?;
                server.connect().await.map_err(|e| {
                    IpcError::ConnectionFailed(format!(
                        "Named pipe connect failed for {}: {}",
                        self.config.pipe_name, e
                    ))
                })?;
                tracing::info!(pipe = %self.config.pipe_name, "IPC client connected (named pipe)");
                Ok(spawn_connection_tasks(server, self.config.max_message_size))
            }
        }
    }

    /// Human-readable endpoint string for diagnostics.
    pub fn endpoint(&self) -> String {
        match self.config.transport {
            IpcTransport::TcpLoopback => self.config.address.clone(),
            IpcTransport::NamedPipe => self.config.pipe_name.clone(),
        }
    }
}

/// Active IPC connection.
#[derive(Clone)]
pub struct IpcConnection {
    tx: mpsc::Sender<RuntimeMlEnvelopeV2>,
    rx: Arc<Mutex<mpsc::Receiver<RuntimeMlEnvelopeV2>>>,
}

impl IpcConnection {
    /// Send one message to the peer.
    pub async fn send(&self, msg: RuntimeMlEnvelopeV2) -> Result<()> {
        self.tx
            .send(msg)
            .await
            .map_err(|_| IpcError::ConnectionLost)?;
        Ok(())
    }

    /// Receive one message from the peer.
    pub async fn recv(&self) -> Result<RuntimeMlEnvelopeV2> {
        let mut rx = self.rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| IpcError::ConnectionLost.into())
    }

    /// Attempt to receive without blocking.
    pub fn try_recv(&self) -> Result<Option<RuntimeMlEnvelopeV2>> {
        match self.rx.try_lock() {
            Ok(mut rx) => match rx.try_recv() {
                Ok(msg) => Ok(Some(msg)),
                Err(mpsc::error::TryRecvError::Empty) => Ok(None),
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    Err(IpcError::ConnectionLost.into())
                }
            },
            Err(_) => Ok(None),
        }
    }

    /// Whether the write side remains open.
    pub fn is_connected(&self) -> bool {
        !self.tx.is_closed()
    }
}

fn spawn_connection_tasks<S>(stream: S, max_message_size: usize) -> IpcConnection
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (read_half, write_half) = tokio::io::split(stream);
    let (write_tx, mut write_rx) = mpsc::channel::<RuntimeMlEnvelopeV2>(64);
    let (read_tx, read_rx) = mpsc::channel::<RuntimeMlEnvelopeV2>(64);

    tokio::spawn(async move {
        let mut writer = write_half;
        while let Some(msg) = write_rx.recv().await {
            let encoded = match encode_message(&msg) {
                Ok(data) => data,
                Err(e) => {
                    tracing::warn!("Failed to encode IPC message: {}", e);
                    continue;
                }
            };
            if let Err(e) = writer.write_all(&encoded).await {
                tracing::warn!("IPC write failed, closing connection: {}", e);
                break;
            }
        }
    });

    tokio::spawn(async move {
        let mut reader = read_half;
        loop {
            let mut len_buf = [0u8; 4];
            if let Err(e) = reader.read_exact(&mut len_buf).await {
                tracing::debug!("IPC read failed (client disconnected?): {}", e);
                break;
            }
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            if msg_len > max_message_size {
                tracing::warn!(
                    msg_len,
                    max_message_size,
                    "IPC message exceeds max size, dropping connection"
                );
                break;
            }

            let mut msg_buf = vec![0u8; msg_len];
            if let Err(e) = reader.read_exact(&mut msg_buf).await {
                tracing::debug!("IPC read failed during body: {}", e);
                break;
            }

            match decode_message::<RuntimeMlEnvelopeV2>(&msg_buf) {
                Ok(msg) => {
                    if read_tx.send(msg).await.is_err() {
                        tracing::debug!("IPC receiver dropped, stopping read task");
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to decode IPC message: {}", e);
                }
            }
        }
    });

    IpcConnection {
        tx: write_tx,
        rx: Arc::new(Mutex::new(read_rx)),
    }
}

fn encode_message<T: serde::Serialize>(msg: &T) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(msg).map_err(|e| IpcError::InvalidMessage(e.to_string()))?;
    let len = json.len() as u32;
    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&json);
    Ok(buf)
}

fn decode_message<T: serde::de::DeserializeOwned>(buf: &[u8]) -> Result<T> {
    serde_json::from_slice(buf).map_err(|e| IpcError::InvalidMessage(e.to_string()).into())
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use crate::client::IpcClient;
    use crate::protocol::{PingV2, RuntimeMlKindV2};

    use super::{IpcConfig, IpcServer, IpcTransport, RuntimeMlEnvelopeV2};

    fn allocate_test_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral bind should succeed")
            .local_addr()
            .expect("socket address should resolve")
            .port()
    }

    #[tokio::test]
    async fn explicit_tcp_transport_roundtrips_messages() {
        let address = format!("127.0.0.1:{}", allocate_test_port());
        let config = IpcConfig {
            transport: IpcTransport::TcpLoopback,
            address: address.clone(),
            ..IpcConfig::default()
        };
        let server = IpcServer::new(config.clone())
            .await
            .expect("TCP IPC server should start");

        let server_task = tokio::spawn(async move {
            let connection = server.accept().await.expect("server accept should succeed");
            let message = connection.recv().await.expect("server recv should succeed");
            connection
                .send(message)
                .await
                .expect("server send should succeed");
        });

        let mut client = IpcClient::new(config);
        client.connect().await.expect("client connect should succeed");

        let ping = PingV2 {
            ping_id: "test-ping".to_string(),
            timestamp_us: 123,
        };
        let envelope = RuntimeMlEnvelopeV2::new(
            RuntimeMlKindV2::Ping,
            1,
            "test-session",
            &ping,
        )
        .expect("envelope should encode");

        client
            .send(envelope.clone())
            .await
            .expect("client send should succeed");
        let echoed = client.recv().await.expect("client recv should succeed");

        assert_eq!(echoed.kind, RuntimeMlKindV2::Ping);
        assert_eq!(echoed.seq, envelope.seq);
        assert_eq!(echoed.session_id, envelope.session_id);

        server_task
            .await
            .expect("server task should complete successfully");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn named_pipe_transport_is_rejected_on_non_windows() {
        let result = IpcServer::new(IpcConfig {
            transport: IpcTransport::NamedPipe,
            ..IpcConfig::default()
        })
        .await;

        match result {
            Ok(_) => panic!("named pipes should be unsupported on non-Windows"),
            Err(error) => assert!(
                error
                    .to_string()
                    .contains("named pipes are only available on Windows"),
                "unexpected error: {error}"
            ),
        }
    }
}
