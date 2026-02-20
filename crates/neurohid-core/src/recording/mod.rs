//! # Recording
//!
//! Session folder layout (from the recording task) and export to XDF.
//! The recording task lives in `crate::tasks::recording`; this module provides
//! offline export of a session directory to XDF 1.0.

mod xdf_writer;

use std::path::Path;

use neurohid_types::error::Result;

/// Export a session folder to a single XDF 1.0 file.
///
/// Reads the session directory (manifest.json, config.json, streams/*.jsonl)
/// and writes one .xdf file suitable for EEGLAB, MNE-Python, pyxdf, etc.
/// No running service is required.
pub fn export_session_to_xdf(session_dir: &Path, out_path: &Path) -> Result<()> {
    xdf_writer::export_session_to_xdf(session_dir, out_path)
}
