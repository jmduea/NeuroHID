use std::collections::HashSet;

use tokio::sync::{broadcast, mpsc};

use neurohid_storage::ProfileStore;
use neurohid_types::{
    config::StorageConfig, error::Result, learning::TrainingEpisode, profile::ProfileId,
};

/// One runtime episode destined for session-log persistence.
#[derive(Debug, Clone)]
pub struct EpisodeLogRecord {
    pub profile_id: ProfileId,
    pub episode: TrainingEpisode,
}

/// Background task that persists runtime episodes into encrypted session logs.
pub struct SessionLoggerTask {
    config: StorageConfig,
    profile_store: Option<ProfileStore>,
    episode_rx: mpsc::Receiver<EpisodeLogRecord>,
    session_id: String,
    retention_pruned_profiles: HashSet<String>,
}

impl SessionLoggerTask {
    pub fn new(
        config: StorageConfig,
        profile_store: Option<ProfileStore>,
        episode_rx: mpsc::Receiver<EpisodeLogRecord>,
    ) -> Self {
        Self {
            config,
            profile_store,
            episode_rx,
            session_id: neurohid_types::now_micros().to_string(),
            retention_pruned_profiles: HashSet::new(),
        }
    }

    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        if !self.config.session_logging_enabled {
            tracing::info!("Session logger disabled in config");
            return Ok(());
        }
        if self.profile_store.is_none() {
            tracing::warn!("Session logger has no profile store; episode logging disabled");
            return Ok(());
        }

        tracing::info!(session_id = self.session_id, "Session logger started");

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Session logger received shutdown signal");
                    break;
                }
                maybe_record = self.episode_rx.recv() => {
                    let Some(record) = maybe_record else {
                        tracing::info!("Session logger input channel closed");
                        break;
                    };
                    self.handle_record(record).await;
                }
            }
        }

        tracing::info!("Session logger stopped");
        Ok(())
    }

    async fn handle_record(&mut self, record: EpisodeLogRecord) {
        let Some(store) = self.profile_store.as_ref() else {
            return;
        };

        if !self
            .retention_pruned_profiles
            .contains(&record.profile_id.0)
        {
            let retention_days = i64::from(self.config.session_log_retention_days);
            let retention_us = retention_days
                .saturating_mul(24)
                .saturating_mul(60)
                .saturating_mul(60)
                .saturating_mul(1_000_000);
            if retention_us > 0 {
                let cutoff = neurohid_types::now_micros().saturating_sub(retention_us);
                if let Err(error) = store
                    .prune_training_session_logs(&record.profile_id, cutoff)
                    .await
                {
                    tracing::warn!(
                        profile_id = %record.profile_id,
                        error = %error,
                        "Failed to prune old session logs"
                    );
                }
            }
            let _ = self
                .retention_pruned_profiles
                .insert(record.profile_id.to_string());
        }

        if let Err(error) = store
            .append_training_episode(&record.profile_id, &self.session_id, record.episode)
            .await
        {
            tracing::warn!(
                profile_id = %record.profile_id,
                error = %error,
                "Failed to append training episode"
            );
        }
    }
}
