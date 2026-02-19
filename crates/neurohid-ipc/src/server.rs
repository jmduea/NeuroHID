//! IPC v3 server built on top of `ipckit` local sockets and loopback TCP.

use std::sync::Arc;

use ipckit::AsyncLocalSocketListener;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc};

use crate::protocol::{IpcConfig, IpcEnvelopeV3, IpcTransport};
use neurohid_types::error::{IpcError, Result};

enum ServerBackend {
    Tcp(tokio::net::TcpListener),
    LocalSocket(AsyncLocalSocketListener),
}

/// Runtime IPC server endpoint.
pub struct IpcServer {
    config: IpcConfig,
    backend: ServerBackend,
}

impl IpcServer {
    /// Create a new IPC server.
    pub async fn new(config: IpcConfig) -> Result<Self> {
        let backend = match config.transport {
            IpcTransport::TcpLoopback => {
                let listener = tokio::net::TcpListener::bind(&config.endpoint)
                    .await
                    .map_err(|e| {
                        IpcError::ConnectionFailed(format!(
                            "failed to bind tcp endpoint {}: {}",
                            config.endpoint, e
                        ))
                    })?;
                tracing::info!(endpoint = %config.endpoint, "IPC server listening (tcp)");
                ServerBackend::Tcp(listener)
            }
            IpcTransport::LocalSocket => {
                let listener = AsyncLocalSocketListener::bind(&config.endpoint)
                    .await
                    .map_err(map_ipckit_error)?;
                tracing::info!(endpoint = %config.endpoint, "IPC server listening (local_socket)");
                ServerBackend::LocalSocket(listener)
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
                    .map_err(|e| IpcError::ConnectionFailed(format!("accept failed: {e}")))?;
                let _ = stream.set_nodelay(true);
                tracing::info!(%addr, "IPC client connected (tcp)");
                Ok(spawn_connection_tasks(
                    stream,
                    self.config.max_message_size,
                    self.config.channel_capacity,
                ))
            }
            ServerBackend::LocalSocket(listener) => {
                let stream = listener.accept().await.map_err(map_ipckit_error)?;
                tracing::info!(endpoint = %self.config.endpoint, "IPC client connected (local_socket)");
                Ok(spawn_connection_tasks(
                    stream,
                    self.config.max_message_size,
                    self.config.channel_capacity,
                ))
            }
        }
    }

    /// Human-readable endpoint string for diagnostics.
    pub fn endpoint(&self) -> &str {
        &self.config.endpoint
    }
}

/// Active IPC connection.
#[derive(Clone)]
pub struct IpcConnection {
    tx: mpsc::Sender<IpcEnvelopeV3>,
    rx: Arc<Mutex<mpsc::Receiver<IpcEnvelopeV3>>>,
}

impl IpcConnection {
    /// Send one envelope to the peer.
    pub async fn send(&self, message: IpcEnvelopeV3) -> Result<()> {
        self.tx
            .send(message)
            .await
            .map_err(|_| IpcError::ConnectionLost)?;
        Ok(())
    }

    /// Receive one envelope from the peer.
    pub async fn recv(&self) -> Result<IpcEnvelopeV3> {
        let mut rx = self.rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| IpcError::ConnectionLost.into())
    }

    /// Try receiving without blocking.
    pub fn try_recv(&self) -> Result<Option<IpcEnvelopeV3>> {
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

fn spawn_connection_tasks<S>(
    stream: S,
    max_message_size: usize,
    channel_capacity: usize,
) -> IpcConnection
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (read_half, write_half) = tokio::io::split(stream);
    let (write_tx, mut write_rx) = mpsc::channel::<IpcEnvelopeV3>(channel_capacity);
    let (read_tx, read_rx) = mpsc::channel::<IpcEnvelopeV3>(channel_capacity);

    tokio::spawn(async move {
        let mut writer = write_half;
        while let Some(message) = write_rx.recv().await {
            let payload = match serde_json::to_vec(&message) {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!("failed to serialize IPC envelope: {}", error);
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
                tracing::debug!("IPC read failed (client disconnected?): {}", error);
                break;
            }
            let message_len = u32::from_le_bytes(len_buf) as usize;
            if message_len > max_message_size {
                tracing::warn!(
                    message_len,
                    max_message_size,
                    "IPC message exceeds max size, closing connection"
                );
                break;
            }

            let mut message_buf = vec![0_u8; message_len];
            if let Err(error) = reader.read_exact(&mut message_buf).await {
                tracing::debug!("IPC read failed during payload: {}", error);
                break;
            }

            match serde_json::from_slice::<IpcEnvelopeV3>(&message_buf) {
                Ok(message) => {
                    if read_tx.send(message).await.is_err() {
                        break;
                    }
                }
                Err(error) => {
                    tracing::warn!("failed to decode IPC envelope: {}", error);
                }
            }
        }
    });

    IpcConnection {
        tx: write_tx,
        rx: Arc::new(Mutex::new(read_rx)),
    }
}

fn map_ipckit_error(error: ipckit::IpcError) -> neurohid_types::error::Error {
    let mapped = if error.is_timeout() {
        IpcError::Timeout
    } else {
        IpcError::ConnectionFailed(error.to_string())
    };
    mapped.into()
}

#[cfg(test)]
mod tests {
    use crate::client::IpcClient;
    use crate::protocol::{IpcChannelV3, IpcEnvelopeV3, PingV2, TrainerStreamKindV3};

    use super::{IpcConfig, IpcServer, IpcTransport};

    fn allocate_test_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral bind should succeed")
            .local_addr()
            .expect("socket address should resolve")
            .port()
    }

    #[tokio::test]
    async fn explicit_tcp_transport_roundtrips_messages() {
        let endpoint = format!("127.0.0.1:{}", allocate_test_port());
        let config = IpcConfig {
            transport: IpcTransport::TcpLoopback,
            endpoint: endpoint.clone(),
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
        client
            .connect()
            .await
            .expect("client connect should succeed");

        let ping = PingV2 {
            ping_id: "test-ping".to_string(),
            timestamp_us: 123,
        };
        let envelope = IpcEnvelopeV3::new(
            IpcChannelV3::TrainerStream,
            TrainerStreamKindV3::Ping.as_msg_type(),
            1,
            None,
            Some("test-session".to_string()),
            &ping,
        )
        .expect("envelope should encode");

        client
            .send(envelope.clone())
            .await
            .expect("client send should succeed");
        let echoed = client.recv().await.expect("client recv should succeed");

        assert_eq!(echoed.msg_type, envelope.msg_type);
        assert_eq!(echoed.seq, envelope.seq);
        assert_eq!(echoed.session_id, envelope.session_id);

        server_task
            .await
            .expect("server task should complete successfully");
    }

    #[tokio::test]
    async fn explicit_local_socket_transport_roundtrips_messages() {
        let endpoint = format!(
            "neurohid_ipc_test_{}_{}",
            std::process::id(),
            neurohid_types::now_micros()
        );
        let config = IpcConfig {
            transport: IpcTransport::LocalSocket,
            endpoint,
            ..IpcConfig::default()
        };
        let server = IpcServer::new(config.clone())
            .await
            .expect("local socket IPC server should start");

        let server_task = tokio::spawn(async move {
            let connection = server.accept().await.expect("server accept should succeed");
            let message = connection.recv().await.expect("server recv should succeed");
            connection
                .send(message)
                .await
                .expect("server send should succeed");
        });

        let mut client = IpcClient::new(config);
        client
            .connect()
            .await
            .expect("client connect should succeed");

        let ping = PingV2 {
            ping_id: "test-local".to_string(),
            timestamp_us: 456,
        };
        let envelope = IpcEnvelopeV3::new(
            IpcChannelV3::TrainerStream,
            TrainerStreamKindV3::Ping.as_msg_type(),
            1,
            None,
            Some("session-local".to_string()),
            &ping,
        )
        .expect("envelope should encode");

        client
            .send(envelope.clone())
            .await
            .expect("client send should succeed");
        let echoed = client.recv().await.expect("client recv should succeed");
        assert_eq!(echoed.msg_type, envelope.msg_type);
        assert_eq!(echoed.seq, envelope.seq);

        server_task
            .await
            .expect("server task should complete successfully");
    }
}
