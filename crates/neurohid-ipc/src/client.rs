//! Runtime ML IPC client.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::protocol::{IpcConfig, IpcTransport, RuntimeMlEnvelopeV2};
use neurohid_types::error::{IpcError, Result};

#[cfg(windows)]
use tokio::net::windows::named_pipe::ClientOptions;

/// Client used by trainer-side processes and integration tests.
pub struct IpcClient {
    config: IpcConfig,
    tx: Option<mpsc::Sender<RuntimeMlEnvelopeV2>>,
    rx: Option<mpsc::Receiver<RuntimeMlEnvelopeV2>>,
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
                let stream = tokio::net::TcpStream::connect(&self.config.address)
                    .await
                    .map_err(|e| {
                        IpcError::ConnectionFailed(format!(
                            "Failed to connect to {}: {}",
                            self.config.address, e
                        ))
                    })?;
                let _ = stream.set_nodelay(true);
                self.install_stream(stream);
                Ok(())
            }
            IpcTransport::NamedPipe => {
                #[cfg(windows)]
                {
                    let client =
                        ClientOptions::new()
                            .open(&self.config.pipe_name)
                            .map_err(|e| {
                                IpcError::ConnectionFailed(format!(
                                    "Failed to open named pipe {}: {}",
                                    self.config.pipe_name, e
                                ))
                            })?;
                    self.install_stream(client);
                    Ok(())
                }
                #[cfg(not(windows))]
                {
                    Err(IpcError::ConnectionFailed(
                        "named pipes are only available on Windows".to_string(),
                    )
                    .into())
                }
            }
        }
    }

    pub async fn send(&self, msg: RuntimeMlEnvelopeV2) -> Result<()> {
        let tx = self
            .tx
            .as_ref()
            .ok_or(IpcError::ConnectionFailed("Not connected".to_string()))?;
        tx.send(msg)
            .await
            .map_err(|_| IpcError::SendFailed("Channel closed".to_string()))?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<RuntimeMlEnvelopeV2> {
        let rx = self
            .rx
            .as_mut()
            .ok_or(IpcError::ConnectionFailed("Not connected".to_string()))?;
        rx.recv()
            .await
            .ok_or_else(|| IpcError::ConnectionLost.into())
    }

    pub fn is_connected(&self) -> bool {
        self.tx.as_ref().is_some_and(|tx| !tx.is_closed())
    }

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

        let (write_tx, mut write_rx) = mpsc::channel::<RuntimeMlEnvelopeV2>(64);
        let (read_tx, read_rx) = mpsc::channel::<RuntimeMlEnvelopeV2>(64);

        tokio::spawn(async move {
            let mut writer = write_half;
            while let Some(msg) = write_rx.recv().await {
                let json = match serde_json::to_vec(&msg) {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::warn!("Failed to encode IPC client message: {}", e);
                        continue;
                    }
                };
                let len = json.len() as u32;
                let mut buf = Vec::with_capacity(4 + json.len());
                buf.extend_from_slice(&len.to_le_bytes());
                buf.extend_from_slice(&json);

                if let Err(e) = writer.write_all(&buf).await {
                    tracing::warn!("IPC client write failed: {}", e);
                    break;
                }
            }
        });

        tokio::spawn(async move {
            let mut reader = read_half;
            loop {
                let mut len_buf = [0u8; 4];
                if let Err(e) = reader.read_exact(&mut len_buf).await {
                    tracing::debug!("IPC client read failed (disconnect?): {}", e);
                    break;
                }
                let msg_len = u32::from_le_bytes(len_buf) as usize;
                if msg_len > max_message_size {
                    tracing::warn!(
                        msg_len,
                        max_message_size,
                        "IPC server message exceeds max size, dropping connection"
                    );
                    break;
                }

                let mut msg_buf = vec![0u8; msg_len];
                if let Err(e) = reader.read_exact(&mut msg_buf).await {
                    tracing::debug!("IPC client read failed during body: {}", e);
                    break;
                }

                match serde_json::from_slice::<RuntimeMlEnvelopeV2>(&msg_buf) {
                    Ok(msg) => {
                        if read_tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to decode IPC server message: {}", e);
                    }
                }
            }
        });

        self.tx = Some(write_tx);
        self.rx = Some(read_rx);
    }
}
