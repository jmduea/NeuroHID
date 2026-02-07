//! # Service Tasks
//!
//! This module contains the individual tasks that run concurrently within the
//! NeuroHID service. Each task has a single responsibility and communicates
//! with other tasks through channels.
//!
//! Think of each task as a worker on an assembly line: each one has a specific
//! job, receives input from the worker before them, does their work, and passes
//! the result to the next worker. This design keeps each piece simple and makes
//! it easy to test them independently.

mod device;
mod signal;
mod ipc;
mod action;

pub use device::DeviceTask;
pub use signal::SignalTask;
pub use ipc::IpcTask;
pub use action::ActionTask;
