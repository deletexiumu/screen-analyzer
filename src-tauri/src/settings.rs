use std::path::PathBuf;

use anyhow::Result;
use tokio::sync::RwLock;

use crate::models::{AppConfig, PersistedAppConfig};

pub struct SettingsManager {
    path: PathBuf,
    data: RwLock<PersistedAppConfig>,
}

impl SettingsManager {
    pub async fn new(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let initial = match tokio::fs::read(&path).await {
            Ok(bytes) if !bytes.is_empty() => {
                serde_json::from_slice::<PersistedAppConfig>(&bytes).unwrap_or_default()
            }
            _ => {
                let default = PersistedAppConfig::default();
                let json = serde_json::to_string_pretty(&default)?;
                tokio::fs::write(&path, json).await?;
                default
            }
        };

        Ok(Self {
            path,
            data: RwLock::new(initial),
        })
    }

    pub async fn get(&self) -> PersistedAppConfig {
        self.data.read().await.clone()
    }

    pub async fn update(&self, update: AppConfig) -> Result<PersistedAppConfig> {
        let mut config = self.data.write().await;

        if let Some(value) = update.retention_days {
            config.retention_days = value;
        }
        if let Some(provider) = update.llm_provider {
            config.llm_provider = provider;
        }
        if let Some(interval) = update.capture_interval {
            config.capture_interval = interval;
        }
        if let Some(interval) = update.summary_interval {
            config.summary_interval = interval;
        }
        if let Some(video) = update.video_config {
            config.video_config = video;
        }
        if let Some(ui) = update.ui_settings {
            config.ui_settings = Some(ui);
        }
        if let Some(llm) = update.llm_config {
            config.llm_config = Some(llm);
        }
        if let Some(capture) = update.capture_settings {
            config.capture_settings = Some(capture);
        }

        self.save(&config).await?;
        Ok(config.clone())
    }

    async fn save(&self, config: &PersistedAppConfig) -> Result<()> {
        let json = serde_json::to_string_pretty(config)?;
        tokio::fs::write(&self.path, json).await?;
        Ok(())
    }
}
