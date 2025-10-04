// Notion 集成模块
// 提供与 Notion 的数据同步功能

pub mod client;

pub use client::{NotionClient, NotionPage};

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::models::{NotionConfig, Session};

/// Notion 同步管理器
pub struct NotionManager {
    client: Arc<RwLock<Option<NotionClient>>>,
}

impl NotionManager {
    /// 创建新的 Notion 管理器
    pub fn new() -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
        }
    }

    /// 初始化或更新 Notion 客户端
    pub async fn initialize(&self, config: NotionConfig) -> Result<()> {
        let mut client = self.client.write().await;

        if config.enabled {
            match NotionClient::new(config.clone()) {
                Ok(notion_client) => {
                    *client = Some(notion_client);
                    info!("Notion 客户端已初始化");
                    Ok(())
                }
                Err(e) => {
                    error!("初始化 Notion 客户端失败: {}", e);
                    *client = None;
                    Err(e)
                }
            }
        } else {
            *client = None;
            info!("Notion 同步已禁用");
            Ok(())
        }
    }

    /// 测试连接
    pub async fn test_connection(&self) -> Result<String> {
        let client = self.client.read().await;
        match &*client {
            Some(c) => c.test_connection().await,
            None => Ok("Notion 客户端未初始化".to_string()),
        }
    }

    /// 同步会话（异步，不阻塞主流程）
    pub async fn sync_session_async(&self, session: Session) {
        let client = self.client.read().await;
        if let Some(c) = &*client {
            let c = c.clone();
            drop(client); // 释放锁

            tokio::spawn(async move {
                // 记录当前配置（用于调试）
                let cfg = c.get_config();
                info!(
                    "Notion 同步配置: sync_videos={}, video_size_limit={}MB",
                    cfg.sync_options.sync_videos, cfg.sync_options.video_size_limit_mb
                );

                match c.sync_session(&session).await {
                    Ok(page_id) => {
                        info!(
                            "会话 {:?} 成功同步到 Notion，页面 ID: {}",
                            session.id, page_id
                        );
                    }
                    Err(e) => {
                        error!("同步会话 {:?} 到 Notion 失败: {}", session.id, e);
                    }
                }
            });
        }
    }

    /// 同步会话（同步方式，等待结果）
    pub async fn sync_session(&self, session: &Session) -> Result<String> {
        let client = self.client.read().await;
        match &*client {
            Some(c) => c.sync_session(session).await,
            None => Ok("Notion 客户端未初始化".to_string()),
        }
    }

    /// 同步每日总结
    pub async fn sync_daily_summary(&self, date: &str, summary: &str) -> Result<String> {
        let client = self.client.read().await;
        match &*client {
            Some(c) => c.sync_daily_summary(date, summary).await,
            None => Ok("Notion 客户端未初始化".to_string()),
        }
    }

    /// 检查是否已启用
    pub async fn is_enabled(&self) -> bool {
        self.client.read().await.is_some()
    }

    /// 搜索可用的页面和数据库
    pub async fn search_pages(&self, api_token: &str) -> Result<Vec<client::NotionPage>> {
        // 创建临时客户端用于搜索
        let temp_config = NotionConfig {
            enabled: true,
            api_token: api_token.to_string(),
            database_id: String::new(), // 搜索时不需要 database_id
            sync_options: Default::default(),
            max_retries: 3,
        };

        let temp_client = NotionClient::new(temp_config)?;
        temp_client.search_pages().await
    }

    /// 在指定页面下创建数据库
    pub async fn create_database(
        &self,
        api_token: &str,
        parent_page_id: &str,
        database_name: &str,
    ) -> Result<String> {
        // 创建临时客户端用于创建数据库
        let temp_config = NotionConfig {
            enabled: true,
            api_token: api_token.to_string(),
            database_id: String::new(),
            sync_options: Default::default(),
            max_retries: 3,
        };

        let temp_client = NotionClient::new(temp_config)?;
        temp_client
            .create_database(parent_page_id, database_name)
            .await
    }
}
