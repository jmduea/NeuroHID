//! Client (trainer protocol) side of the IPC task.
//!
//! Message building and handling for the trainer stream (Hello, DecisionEvent,
//! ErrpWindow, etc.) are implemented as methods on IpcTask in the parent module.
//! Protocol types are re-exported from `neurohid_ipc`.
