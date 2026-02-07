//! # IPC Client
//!
//! The client side of the IPC connection. This would be used by Python
//! (via PyO3 bindings) or for testing the server from Rust.
//!
//! ## Transport
//!
//! Connects to the Rust core via a TCP socket on localhost. Messages
//! use the same length-prefixed JSON framing as the server side.

use tokio::sync::mpsc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use neurohid_types::error::{IpcError, Result};
use crate::protocol::{RustToPython, PythonToRust, IpcConfig};

/// An IPC client that connects to the Rust core service.
///
/// This is primarily intended for:
/// 1. Testing the IPC layer from Rust
/// 2. Being wrapped with PyO3 for Python access
pub struct IpcClient {
    config: IpcConfig,
    tx: Option<mpsc::Sender<PythonToRust>>,
    rx: Option<mpsc::Receiver<RustToPython>>,
}

impl IpcClient {
    /// Creates a new client with the given configuration.
    pub fn new(config: IpcConfig) -> Self {
        Self {
            config,
            tx: None,
            rx: None,
        }
    }

    /// Connects to the Rust core service.
    ///
    /// Establishes a TCP connection, spawns background read/write
    /// tasks, and populates the internal channels for message passing.
    pub async fn connect(&mut self) -> Result<()> {
        let stream = tokio::net::TcpStream::connect(&self.config.address)
            .await
            .map_err(|e| IpcError::ConnectionFailed(format!(
                "Failed to connect to {}: {}", self.config.address, e
            )))?;

        // Disable Nagle's algorithm for lower latency on small messages
        let _ = stream.set_nodelay(true);

        let (read_half, write_half) = stream.into_split();
        let max_message_size = self.config.max_message_size;

        // Channel for sending messages to Rust core (write side)
        let (write_tx, mut write_rx) = mpsc::channel::<PythonToRust>(64);
        // Channel for receiving messages from Rust core (read side)
        let (read_tx, read_rx) = mpsc::channel::<RustToPython>(64);

        // Spawn writer task
        tokio::spawn(async move {
            let mut writer = write_half;
            while let Some(msg) = write_rx.recv().await {
                let json = match serde_json::to_vec(&msg) {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::warn!("Failed to encode client message: {}", e);
                        continue;
                    }
                };
                let len = json.len() as u32;
                let mut buf = Vec::with_capacity(4 + json.len());
                buf.extend_from_slice(&len.to_le_bytes());
                buf.extend_from_slice(&json);

                if let Err(e) = writer.write_all(&buf).await {
                    tracing::warn!("Client write failed: {}", e);
                    break;
                }
            }
        });

        // Spawn reader task
        tokio::spawn(async move {
            let mut reader = read_half;
            loop {
                // Read 4-byte length prefix
                let mut len_buf = [0u8; 4];
                if let Err(e) = reader.read_exact(&mut len_buf).await {
                    tracing::debug!("Client read failed (server disconnected?): {}", e);
                    break;
                }
                let msg_len = u32::from_le_bytes(len_buf) as usize;

                if msg_len > max_message_size {
                    tracing::warn!(
                        msg_len, max_message_size,
                        "Server message exceeds max size, dropping connection"
                    );
                    break;
                }

                // Read message body
                let mut msg_buf = vec![0u8; msg_len];
                if let Err(e) = reader.read_exact(&mut msg_buf).await {
                    tracing::debug!("Client read failed during body: {}", e);
                    break;
                }

                match serde_json::from_slice::<RustToPython>(&msg_buf) {
                    Ok(msg) => {
                        if read_tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to decode server message: {}", e);
                        continue;
                    }
                }
            }
        });

        self.tx = Some(write_tx);
        self.rx = Some(read_rx);

        Ok(())
    }

    /// Sends a message to the Rust core.
    pub async fn send(&self, msg: PythonToRust) -> Result<()> {
        let tx = self.tx.as_ref()
            .ok_or(IpcError::ConnectionFailed("Not connected".to_string()))?;

        tx.send(msg).await
            .map_err(|_| IpcError::SendFailed("Channel closed".to_string()))?;

        Ok(())
    }

    /// Receives a message from the Rust core.
    pub async fn recv(&mut self) -> Result<RustToPython> {
        let rx = self.rx.as_mut()
            .ok_or(IpcError::ConnectionFailed("Not connected".to_string()))?;

        rx.recv().await
            .ok_or_else(|| IpcError::ConnectionLost.into())
    }

    /// Checks if connected.
    pub fn is_connected(&self) -> bool {
        self.tx.as_ref().map(|tx| !tx.is_closed()).unwrap_or(false)
    }

    /// Disconnects from the server.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.tx = None;
        self.rx = None;
        Ok(())
    }
}
