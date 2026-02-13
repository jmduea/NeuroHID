//! # IPC Server
//!
//! The Rust side of the IPC connection. This runs as part of the core service
//! and accepts connections from the Python ML process.
//!
//! ## Transport
//!
//! Uses a TCP socket bound to localhost (127.0.0.1). This is cross-platform
//! and works identically on Linux, Windows, and macOS.
//!
//! ## Framing
//!
//! Messages are length-prefixed: 4-byte LE u32 length followed by a JSON body.
//! This simple framing avoids delimiter issues and makes parsing straightforward.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};

use crate::protocol::{IpcConfig, PythonToRust, RustToPython};
use neurohid_types::error::{IpcError, Result};

/// The IPC server that accepts connections from the Python ML process.
pub struct IpcServer {
    config: IpcConfig,
    listener: tokio::net::TcpListener,
}

impl IpcServer {
    /// Creates a new IPC server with the given configuration.
    ///
    /// This binds to the configured TCP address and prepares to accept connections.
    pub async fn new(config: IpcConfig) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind(&config.address)
            .await
            .map_err(|e| {
                IpcError::ConnectionFailed(format!(
                    "Failed to bind TCP socket at {}: {}",
                    config.address, e
                ))
            })?;

        tracing::info!(address = %config.address, "IPC server listening");

        Ok(Self { config, listener })
    }

    /// Accepts a single client connection.
    ///
    /// Blocks until a client connects. On connection, spawns background tasks
    /// for reading and writing messages, and returns an `IpcConnection` handle
    /// that communicates through async channels.
    pub async fn accept(&self) -> Result<IpcConnection> {
        let (stream, addr) = self
            .listener
            .accept()
            .await
            .map_err(|e| IpcError::ConnectionFailed(format!("Accept failed: {}", e)))?;

        tracing::info!(%addr, "Python ML process connected");

        // Disable Nagle's algorithm for lower latency on small messages
        let _ = stream.set_nodelay(true);

        let (read_half, write_half) = stream.into_split();
        let max_message_size = self.config.max_message_size;

        // Channels between the connection handle and the I/O tasks
        let (write_tx, mut write_rx) = mpsc::channel::<RustToPython>(64);
        let (read_tx, read_rx) = mpsc::channel::<PythonToRust>(64);

        // Spawn writer task: drains the channel and writes framed messages
        tokio::spawn(async move {
            let mut writer = write_half;
            while let Some(msg) = write_rx.recv().await {
                let encoded = match encode_message(&msg) {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::warn!("Failed to encode message: {}", e);
                        continue;
                    }
                };
                if let Err(e) = writer.write_all(&encoded).await {
                    tracing::warn!("Write failed, closing connection: {}", e);
                    break;
                }
            }
        });

        // Spawn reader task: reads framed messages and pushes to the channel
        tokio::spawn(async move {
            let mut reader = read_half;
            loop {
                // Read 4-byte length prefix
                let mut len_buf = [0u8; 4];
                if let Err(e) = reader.read_exact(&mut len_buf).await {
                    tracing::debug!("Read failed (client disconnected?): {}", e);
                    break;
                }
                let msg_len = u32::from_le_bytes(len_buf) as usize;

                // Guard against oversized messages
                if msg_len > max_message_size {
                    tracing::warn!(
                        msg_len,
                        max_message_size,
                        "Message exceeds max size, dropping connection"
                    );
                    break;
                }

                // Read message body
                let mut msg_buf = vec![0u8; msg_len];
                if let Err(e) = reader.read_exact(&mut msg_buf).await {
                    tracing::debug!("Read failed during body: {}", e);
                    break;
                }

                // Decode and forward
                match decode_message::<PythonToRust>(&msg_buf) {
                    Ok(msg) => {
                        if read_tx.send(msg).await.is_err() {
                            tracing::debug!("Receiver dropped, stopping read task");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to decode message: {}", e);
                        continue;
                    }
                }
            }
        });

        Ok(IpcConnection {
            tx: write_tx,
            rx: Arc::new(Mutex::new(read_rx)),
        })
    }

    /// Returns the address this server is listening on.
    pub fn address(&self) -> &str {
        &self.config.address
    }
}

/// A connection to a Python client.
///
/// This handle is used to send and receive messages with the Python process.
/// It's safe to clone and share across tasks.
#[derive(Clone)]
pub struct IpcConnection {
    // Channel for sending messages to the write task
    tx: mpsc::Sender<RustToPython>,
    // Channel for receiving messages from the read task
    rx: Arc<Mutex<mpsc::Receiver<PythonToRust>>>,
}

impl IpcConnection {
    /// Sends a message to Python.
    ///
    /// This is non-blocking; the message is queued for sending.
    /// Returns an error if the connection is closed.
    pub async fn send(&self, msg: RustToPython) -> Result<()> {
        self.tx
            .send(msg)
            .await
            .map_err(|_| IpcError::ConnectionLost)?;
        Ok(())
    }

    /// Receives a message from Python.
    ///
    /// Blocks until a message is available or the connection is closed.
    pub async fn recv(&self) -> Result<PythonToRust> {
        let mut rx = self.rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| IpcError::ConnectionLost.into())
    }

    /// Tries to receive a message without blocking.
    ///
    /// Returns `Ok(None)` if no message is available yet or if the receive
    /// lock is currently held by another task calling `recv()`.
    pub fn try_recv(&self) -> Result<Option<PythonToRust>> {
        match self.rx.try_lock() {
            Ok(mut rx) => match rx.try_recv() {
                Ok(msg) => Ok(Some(msg)),
                Err(mpsc::error::TryRecvError::Empty) => Ok(None),
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    Err(IpcError::ConnectionLost.into())
                }
            },
            // Mutex is held by an active recv() call — no message for us right now
            Err(_) => Ok(None),
        }
    }

    /// Checks if the connection is still alive.
    pub fn is_connected(&self) -> bool {
        !self.tx.is_closed()
    }
}

/// Encodes a message for transmission.
///
/// Format: 4-byte little-endian length prefix + JSON body
fn encode_message<T: serde::Serialize>(msg: &T) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(msg).map_err(|e| IpcError::InvalidMessage(e.to_string()))?;

    let len = json.len() as u32;
    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&json);

    Ok(buf)
}

/// Decodes a message from a buffer.
fn decode_message<T: serde::de::DeserializeOwned>(buf: &[u8]) -> Result<T> {
    serde_json::from_slice(buf).map_err(|e| IpcError::InvalidMessage(e.to_string()).into())
}
