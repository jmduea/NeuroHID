//! # NeuroHID Hub Library
//!
//! GUI components for the NeuroHID Hub application. This crate provides
//! the HubApp and all associated screens, widgets, and state management.

pub mod app;
pub mod data_bus;
pub mod layout;
pub mod screens;
pub mod service_manager;
pub mod state;
pub mod stream_console;
pub mod theme;
pub mod widgets;
pub mod workbench;

pub use app::HubApp;
