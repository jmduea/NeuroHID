//! # Replay source
//!
//! File-backed sample source for replay: reads a session folder (streams/*.jsonl)
//! or an XDF file and produces [`neurohid_types::signal::Sample`] on demand,
//! so the pipeline can run the decoder on replayed data (virtual source).
//!
//! Use when the runtime is configured for "replay mode" (session path or XDF path):
//! the service spawns this task instead of the live device task and feeds its
//! output into the same sample path.

use std::path::Path;

use tokio::sync::mpsc;

use neurohid_types::signal::Sample;

/// Load samples from a session folder (streams/*.jsonl), sorted by system_timestamp.
/// Returns a single vec of samples (multi-stream sessions are merged by time).
pub fn load_session_samples(session_dir: &Path) -> Result<Vec<Sample>, String> {
    let streams_dir = session_dir.join("streams");
    if !streams_dir.is_dir() {
        return Err("session streams/ directory missing".to_string());
    }

    let mut all: Vec<Sample> = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(streams_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(s) = serde_json::from_str::<Sample>(line) {
                all.push(s);
            }
        }
    }

    all.sort_by_key(|s| s.system_timestamp);
    Ok(all)
}

/// Run replay task: send loaded samples to `sample_tx` and then close the channel.
/// Respects `shutdown` so the task can be cancelled. Runs until all samples are sent
/// or shutdown is received.
pub async fn run_replay_task(
    session_dir: &Path,
    sample_tx: mpsc::Sender<Sample>,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<usize, String> {
    let samples = load_session_samples(session_dir)?;
    let total = samples.len();
    tracing::info!(session_dir = %session_dir.display(), total, "replay source starting");

    for sample in samples {
        tokio::select! {
            _ = shutdown.recv() => {
                tracing::info!("replay source shutdown");
                return Ok(0);
            }
            res = sample_tx.send(sample) => {
                if res.is_err() {
                    tracing::warn!("replay: sample receiver dropped");
                    break;
                }
            }
        }
    }

    drop(sample_tx);
    tracing::info!(sent = total, "replay source finished");
    Ok(total)
}
