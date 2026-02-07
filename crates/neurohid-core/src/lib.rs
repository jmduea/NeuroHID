//! # NeuroHID Core Library
//!
//! This library exposes the NeuroHID service for embedding into other binaries
//! (e.g., the hub GUI). The standalone headless binary (`neurohid-service`)
//! also imports from here.
//!
//! ## Usage
//!
//! ```no_run
//! use neurohid_core::service::{NeuroHidService, ServiceHandle};
//! ```

pub mod service;
pub mod tasks;
