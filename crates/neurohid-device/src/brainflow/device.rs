//! # BrainFlow Device
//!
//! Implements the `Device` trait for any BrainFlow-supported board.
//!
//! ## Lifetime of a BrainFlowDevice
//!
//! ```text
//! Provider::connect()
//!     │
//!     ├─ BoardShim::new(board_id, params)
//!     ├─ board.prepare_session()      ← allocates BrainFlow resources
//!     └─ return BrainFlowDevice { board, ... }
//!         │
//!         ├─ start_streaming()
//!         │      ├─ board.start_stream()
//!         │      └─ spawn polling thread → BrainFlowStream
//!         │
//!         ├─ stop_streaming()
//!         │      ├─ signal polling thread to stop
//!         │      └─ board.stop_stream()
//!         │
//!         └─ disconnect() / Drop
//!                └─ board.release_session()  ← frees BrainFlow resources
//! ```
//!
//! ## Thread Safety
//!
//! BrainFlow's `BoardShim` is wrapped in `Arc` to share between the device
//! struct and the polling thread. All BrainFlow operations are internally
//! synchronized by BrainFlow's C library. The `Arc` only needs to be `Send`,
//! which it is.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use brainflow::board_shim::BoardShim;
use brainflow::BoardIds;
use futures::Stream;
use tokio::sync::watch;

use neurohid_types::device::{
    ConnectionState, ConnectionSettings, DeviceId, DeviceInfo, DeviceStatus, DeviceType,
};
use neurohid_types::error::{DeviceError, Result};
use neurohid_types::signal::DeviceChannelConfig;

use crate::traits::{Device, SampleStream};

use super::board_map;
use super::stream::{BoardChannelMap, BrainFlowStream, StreamConfig};

/// A connected BrainFlow device.
///
/// Wraps a `BoardShim` instance and implements the NeuroHID `Device` trait.
/// Created by `BrainFlowDeviceProvider::connect()`.
pub struct BrainFlowDevice {
    id: DeviceId,
    info: DeviceInfo,
    board_id: BoardIds,
    board: Arc<BoardShim>,
    channel_config: DeviceChannelConfig,
    channel_map: BoardChannelMap,
    stream_config: StreamConfig,

    // State
    connected: AtomicBool,
    streaming: AtomicBool,
    /// Handle to stop the polling thread. `None` when not streaming.
    polling_alive: Option<Arc<AtomicBool>>,

    // Status broadcasting
    status_tx: watch::Sender<DeviceStatus>,
    status_rx: watch::Receiver<DeviceStatus>,
}

impl BrainFlowDevice {
    /// Create a new BrainFlowDevice.
    ///
    /// The board must have already been prepared (`prepare_session` called)
    /// by the provider. Streaming has not yet started.
    pub(crate) fn new(
        board_id: BoardIds,
        board: BoardShim,
        channel_map: BoardChannelMap,
        settings: Option<ConnectionSettings>,
    ) -> Self {
        let _ = settings; // Reserved for future use (reconnect policy, etc.)

        let device_type = board_map::board_id_to_device_type(board_id);
        let display_name = board_map::board_display_name(board_id);
        let channel_config = channel_map.to_channel_config();

        let id = DeviceId::new(format!("brainflow_{:?}", board_id));
        let info = DeviceInfo {
            id: id.clone(),
            device_type,
            name: Some(display_name),
            firmware_version: None,
            channel_config: Some(channel_config.clone()),
            battery_percent: None,
        };

        let initial_status = DeviceStatus {
            device_id: id.clone(),
            connection_state: ConnectionState::Connected,
            is_streaming: false,
            samples_received: 0,
            samples_dropped: 0,
            battery_percent: None,
            channel_quality: None,
            message: Some("BrainFlow device ready".into()),
        };
        let (status_tx, status_rx) = watch::channel(initial_status);

        Self {
            id,
            info,
            board_id,
            board: Arc::new(board),
            channel_config,
            channel_map,
            stream_config: StreamConfig::default(),
            connected: AtomicBool::new(true),
            streaming: AtomicBool::new(false),
            polling_alive: None,
            status_tx,
            status_rx,
        }
    }

    fn update_status(&self) {
        let status = DeviceStatus {
            device_id: self.id.clone(),
            connection_state: if self.connected.load(Ordering::SeqCst) {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            },
            is_streaming: self.streaming.load(Ordering::SeqCst),
            samples_received: 0, // TODO: track via channel_map stats
            samples_dropped: 0,
            battery_percent: None,
            channel_quality: None,
            message: None,
        };
        let _ = self.status_tx.send(status);
    }
}

#[async_trait]
impl Device for BrainFlowDevice {
    fn id(&self) -> &DeviceId {
        &self.id
    }

    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn channel_config(&self) -> &DeviceChannelConfig {
        &self.channel_config
    }

    fn status(&self) -> DeviceStatus {
        self.status_rx.borrow().clone()
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn is_streaming(&self) -> bool {
        self.streaming.load(Ordering::SeqCst)
    }

    async fn start_streaming(&mut self) -> Result<SampleStream> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(DeviceError::NotConnected.into());
        }
        if self.streaming.load(Ordering::SeqCst) {
            return Err(DeviceError::DeviceBusy.into());
        }

        // Start BrainFlow data acquisition (blocking call, run on spawn_blocking)
        let board = Arc::clone(&self.board);
        tokio::task::spawn_blocking(move || {
            board.start_stream(45000, "")
        })
        .await
        .map_err(|e| DeviceError::CommunicationError(format!("task join error: {}", e)))?
        .map_err(|e| DeviceError::CommunicationError(format!("BrainFlow start_stream failed: {}", e)))?;

        self.streaming.store(true, Ordering::SeqCst);

        // Create the polling adapter
        let (stream, alive_handle) = BrainFlowStream::start(
            Arc::clone(&self.board),
            self.channel_map.clone(),
            self.stream_config.clone(),
        );
        self.polling_alive = Some(alive_handle);

        self.update_status();

        Ok(Box::pin(stream))
    }

    async fn stop_streaming(&mut self) -> Result<()> {
        // Signal polling thread to stop
        if let Some(alive) = self.polling_alive.take() {
            alive.store(false, Ordering::Relaxed);
        }

        self.streaming.store(false, Ordering::SeqCst);

        // Stop BrainFlow data acquisition
        let board = Arc::clone(&self.board);
        let _ = tokio::task::spawn_blocking(move || {
            board.stop_stream()
        })
        .await;

        self.update_status();
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // Stop streaming first if active
        if self.streaming.load(Ordering::SeqCst) {
            self.stop_streaming().await?;
        }

        self.connected.store(false, Ordering::SeqCst);

        // Release BrainFlow session (frees C resources)
        let board = Arc::clone(&self.board);
        let _ = tokio::task::spawn_blocking(move || {
            board.release_session()
        })
        .await;

        self.update_status();
        Ok(())
    }

    fn status_stream(&self) -> std::pin::Pin<Box<dyn Stream<Item = DeviceStatus> + Send>> {
        let rx = self.status_rx.clone();
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            rx.changed().await.ok()?;
            Some((rx.borrow().clone(), rx))
        }))
    }
}

impl Drop for BrainFlowDevice {
    fn drop(&mut self) {
        // Best-effort cleanup if disconnect wasn't called explicitly
        if let Some(alive) = self.polling_alive.take() {
            alive.store(false, Ordering::Relaxed);
        }
        // Note: we can't call async disconnect() from Drop, and we can't
        // call blocking release_session() without risking deadlock if we're
        // inside a Tokio runtime. The Arc<BoardShim> will be dropped when
        // the polling thread also finishes, and BrainFlow's destructor
        // handles cleanup at the C level.
    }
}
