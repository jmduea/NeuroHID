//! # Device discovery and connection API
//!
//! High-level API for listing streams (via runtime or discovery-only), connecting by id or
//! by criteria, and a scoped handle that disconnects on drop. Also re-exports the device
//! provider layer for low-level discovery and connection.
//!
//! ## Lifecycle
//!
//! - **List:** Use [`list_streams_via_runtime`] when you have a [`RuntimeHandle`] (e.g. embedded
//!   runtime), or [`list_streams_discovery`] with a [`DeviceProvider`] for a point-in-time list
//!   without starting the full runtime.
//! - **Connect:** [`connect_by_id`] sends a connect command for a known stream id; success can be
//!   observed via a fresh snapshot or listener. [`connect_by_criteria`] lists streams (runtime or
//!   discovery), takes the first match (order implementation-defined), then connects.
//! - **Handle:** [`StreamConnectionHandle`] holds the stream id and runtime reference; **on drop it
//!   sends `DisconnectStream`**, so drop = disconnect. When a device disappears, the runtime may
//!   invalidate state; further use of the handle sends commands that may no longer apply.

pub use neurohid_device::*;

#[cfg(all(feature = "device", feature = "runtime"))]
mod api;
#[cfg(all(feature = "device", feature = "runtime"))]
pub use api::{connect_by_criteria, connect_by_id, list_streams_discovery, list_streams_via_runtime, StreamConnectionHandle};
