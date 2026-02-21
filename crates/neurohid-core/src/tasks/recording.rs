//! # Recording task
//!
//! Subscribes to sample and action broadcasts and writes a full session trace
//! to a session folder (manifest, config snapshot, streams, actions.jsonl).
//! Does not block the pipeline; runs as a separate task.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::{broadcast, mpsc, oneshot};

use neurohid_storage::ProfileStore;
use neurohid_types::{
    action::Action,
    config::SystemConfig,
    error::Result,
    profile::ProfileId,
    recording::{RecordingConfig, SessionManifest},
    signal::Sample,
};

use crate::service::ServiceState;

/// Command sent to the recording task.
#[derive(Debug)]
pub enum RecordingCommand {
    /// Start recording; optional path overrides config default.
    Start {
        output_path_override: Option<PathBuf>,
    },
    /// Stop current recording.
    Stop,
}

/// Result of a recording command.
#[derive(Debug)]
pub enum RecordingCommandResult {
    Started {
        session_id: String,
        output_path: String,
    },
    Stopped { session_id: String },
    Error(String),
}

/// Request sent to recording task: command + oneshot to reply with result.
pub type RecordingRequest = (RecordingCommand, oneshot::Sender<std::result::Result<RecordingCommandResult, String>>);

/// Recording task: writes session folder with manifest, config snapshot, streams, actions.jsonl.
pub struct RecordingTask {
    recording_config: RecordingConfig,
    system_config: SystemConfig,
    profile_id: Option<ProfileId>,
    profile_store: Option<ProfileStore>,
    state: Arc<tokio::sync::RwLock<ServiceState>>,
    sample_rx: broadcast::Receiver<Sample>,
    action_rx: broadcast::Receiver<Action>,
    command_rx: mpsc::Receiver<RecordingRequest>,
}

impl RecordingTask {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        recording_config: RecordingConfig,
        system_config: SystemConfig,
        profile_id: Option<ProfileId>,
        profile_store: Option<ProfileStore>,
        state: Arc<tokio::sync::RwLock<ServiceState>>,
        sample_rx: broadcast::Receiver<Sample>,
        action_rx: broadcast::Receiver<Action>,
        command_rx: mpsc::Receiver<RecordingRequest>,
    ) -> Self {
        Self {
            recording_config,
            system_config,
            profile_id,
            profile_store,
            state,
            sample_rx,
            action_rx,
            command_rx,
        }
    }

    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        let mut active_session_id: Option<String> = None;
        let mut active_path: Option<PathBuf> = None;
        let mut stream_files: HashMap<String, BufWriter<fs::File>> = HashMap::new();
        let mut stream_index: HashMap<String, usize> = HashMap::new();
        let mut next_stream_index: usize = 0;
        let mut actions_writer: Option<BufWriter<fs::File>> = None;
        let mut _total_bytes: u64 = 0;
        let max_size_bytes: Option<u64> = self
            .recording_config
            .max_size_mb
            .map(|mb| mb.saturating_mul(1024 * 1024));

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    if active_session_id.is_some() {
                        let _ = self.stop_session(
                            &active_session_id,
                            &active_path,
                            &mut actions_writer,
                            &mut stream_files,
                        ).await;
                    }
                    break;
                }
                maybe_req = self.command_rx.recv() => {
                    let Some((cmd, reply)) = maybe_req else {
                        break;
                    };
                    let _ = match cmd {
                        RecordingCommand::Start { output_path_override } => {
                            if active_session_id.is_some() {
                                let _ = reply.send(Ok(RecordingCommandResult::Error(
                                    "recording already active".to_string(),
                                )));
                            } else {
                                match self.start_session(output_path_override).await {
                                    Ok((session_id, path, _started_us)) => {
                                        let actions_path = path.join("actions.jsonl");
                                        match fs::File::create(&actions_path).await {
                                            Ok(f) => {
                                                actions_writer = Some(BufWriter::new(f));
                                                active_session_id = Some(session_id.clone());
                                                active_path = Some(path.clone());
                                                _total_bytes = 0;
                                                let _ = reply.send(Ok(RecordingCommandResult::Started {
                                                    session_id,
                                                    output_path: path.display().to_string(),
                                                }));
                                            }
                                            Err(e) => {
                                                let _ = reply.send(Ok(RecordingCommandResult::Error(
                                                    e.to_string(),
                                                )));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = reply.send(Ok(RecordingCommandResult::Error(
                                            e.to_string(),
                                        )));
                                    }
                                }
                            }
                        }
                        RecordingCommand::Stop => {
                            if let Some(ref sid) = active_session_id {
                                let path = active_path.clone();
                                if let Err(e) = self.stop_session(
                                    &active_session_id,
                                    &path,
                                    &mut actions_writer,
                                    &mut stream_files,
                                ).await {
                                    tracing::warn!("recording stop: {}", e);
                                }
                                let sid = sid.clone();
                                active_session_id = None;
                                active_path = None;
                                stream_index.clear();
                                next_stream_index = 0;
                                let _ = reply.send(Ok(RecordingCommandResult::Stopped {
                                    session_id: sid,
                                }));
                            } else {
                                let _ = reply.send(Ok(RecordingCommandResult::Error(
                                    "no active recording".to_string(),
                                )));
                            }
                        }
                    };
                }
                sample_result = self.sample_rx.recv() => {
                    if let (Some(_), Some(path)) = (&active_session_id, &active_path) {
                        match sample_result {
                            Ok(sample) => {
                                let source_key = sample.source_id.clone().unwrap_or_else(|| "default".to_string());
                                let need_new = !stream_files.contains_key(&source_key);
                                if need_new {
                                    let i = *stream_index.entry(source_key.clone()).or_insert_with(|| {
                                        let idx = next_stream_index;
                                        next_stream_index += 1;
                                        idx
                                    });
                                    let stream_path = path.join("streams").join(format!("stream_{}.jsonl", i));
                                    if let Ok(f) = fs::File::create(&stream_path).await {
                                        stream_files.insert(source_key.clone(), BufWriter::new(f));
                                    }
                                }
                                if let Some(w) = stream_files.get_mut(&source_key) {
                                    let line = serde_json::to_string(&sample).unwrap_or_default();
                                    if let Err(e) = w.write_all(line.as_bytes()).await {
                                        tracing::warn!("recording sample write failed: {}", e);
                                        let _ = self.stop_session(
                                            &active_session_id,
                                            &active_path,
                                            &mut actions_writer,
                                            &mut stream_files,
                                        ).await;
                                        active_session_id = None;
                                        active_path = None;
                                        actions_writer = None;
                                        stream_files.clear();
                                    } else if let Err(_) = w.write_all(b"\n").await {
                                        // best-effort
                                    } else if let Some(max) = max_size_bytes {
                                        _total_bytes += line.len() as u64 + 1;
                                        if _total_bytes >= max {
                                            let _ = self.stop_session(
                                                &active_session_id,
                                                &active_path,
                                                &mut actions_writer,
                                                &mut stream_files,
                                            ).await;
                                            active_session_id = None;
                                            active_path = None;
                                            actions_writer = None;
                                            stream_files.clear();
                                        }
                                    }
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                tracing::debug!("recording sample lagged {} messages", n);
                            }
                            Err(_) => {}
                        }
                    }
                }
                action_result = self.action_rx.recv() => {
                    if active_session_id.is_some() && active_path.is_some() {
                        if let Some(w) = &mut actions_writer {
                        match action_result {
                            Ok(action) => {
                                let line = serde_json::to_string(&action).unwrap_or_default();
                                if let Err(e) = w.write_all(line.as_bytes()).await {
                                    tracing::warn!("recording action write failed: {}", e);
                                } else if let Err(e) = w.write_all(b"\n").await {
                                    tracing::warn!("recording newline failed: {}", e);
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(_) => {}
                        }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn start_session(
        &self,
        output_path_override: Option<PathBuf>,
    ) -> Result<(String, PathBuf, i64)> {
        let base = output_path_override
            .or_else(|| {
                self.recording_config
                    .default_output_path
                    .as_ref()
                    .map(PathBuf::from)
            })
            .unwrap_or_else(|| PathBuf::from("./recordings"));
        let started_us = neurohid_types::now_micros();
        let session_id = format!("session_{}", started_us);
        let session_dir = base.join(&session_id);
        fs::create_dir_all(&session_dir)
            .await
            .map_err(|e| neurohid_types::Error::internal(format!("create session dir: {}", e)))?;
        let streams_dir = session_dir.join("streams");
        fs::create_dir_all(&streams_dir)
            .await
            .map_err(|e| neurohid_types::Error::internal(format!("create streams dir: {}", e)))?;

        let manifest = SessionManifest {
            session_id: session_id.clone(),
            started_at_us: started_us,
            ended_at_us: None,
            config_ref: Some("config.json".to_string()),
            format_version: "1".to_string(),
            runtime_version: None,
            sdk_version: None,
            profile_id: self.profile_id.as_ref().map(|p| p.to_string()),
            device_stream_summary: None,
        };
        let manifest_path = session_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| neurohid_types::Error::internal(e.to_string()))?;
        fs::write(&manifest_path, manifest_json)
            .await
            .map_err(|e| neurohid_types::Error::internal(format!("write manifest: {}", e)))?;

        let config_path = session_dir.join("config.json");
        let config_json = serde_json::to_string_pretty(&self.system_config)
            .map_err(|e| neurohid_types::Error::internal(e.to_string()))?;
        fs::write(&config_path, config_json)
            .await
            .map_err(|e| neurohid_types::Error::internal(format!("write config snapshot: {}", e)))?;

        if let (Some(store), Some(profile_id)) = (self.profile_store.as_ref(), self.profile_id.as_ref()) {
            if let Ok(meta) = store.get_metadata(profile_id).await {
                let profile_path = session_dir.join("profile_meta.json");
                let profile_json = serde_json::to_string_pretty(&meta)
                    .map_err(|e| neurohid_types::Error::internal(e.to_string()))?;
                let _ = fs::write(&profile_path, profile_json).await;
            }
        }

        {
            let mut state = self.state.write().await;
            state.recording_active = true;
            state.current_session_id = Some(session_id.clone());
        }

        tracing::info!(
            session_id = %session_id,
            path = %session_dir.display(),
            "recording started"
        );

        Ok((session_id, session_dir, started_us))
    }

    async fn stop_session(
        &self,
        session_id: &Option<String>,
        session_path: &Option<PathBuf>,
        actions_writer: &mut Option<BufWriter<fs::File>>,
        stream_files: &mut HashMap<String, BufWriter<fs::File>>,
    ) -> Result<()> {
        let path = match session_path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };
        if let Some(ref mut w) = actions_writer.take() {
            let _ = w.flush().await;
        }
        for (_, mut w) in stream_files.drain() {
            let _ = w.flush().await;
        }
        let ended_us = neurohid_types::now_micros();
        let manifest_path = path.join("manifest.json");
        if let Ok(contents) = fs::read_to_string(&manifest_path).await {
            if let Ok(mut manifest) = serde_json::from_str::<SessionManifest>(&contents) {
                manifest.ended_at_us = Some(ended_us);
                if let Ok(updated) = serde_json::to_string_pretty(&manifest) {
                    let _ = fs::write(&manifest_path, updated).await;
                }
            }
        }
        {
            let mut state = self.state.write().await;
            state.recording_active = false;
            state.current_session_id = None;
        }
        if let Some(sid) = session_id {
            tracing::info!(session_id = %sid, "recording stopped");
        }
        Ok(())
    }
}
