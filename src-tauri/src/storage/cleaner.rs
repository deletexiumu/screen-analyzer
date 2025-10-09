// 存储清理模块 - 自动清理过期数据

use super::Database;
use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{error, info};

/// 存储清理器
pub struct StorageCleaner {
    /// 数据库实例
    db: Arc<Database>,
    /// 数据保留天数（使用RwLock实现内部可变性）
    retention_days: Arc<RwLock<i64>>,
    /// 最大保留天数
    max_retention_days: i64,
    /// 框架文件目录
    frames_dir: PathBuf,
    /// 视频文件目录
    videos_dir: PathBuf,
}

impl StorageCleaner {
    /// 创建新的清理器
    pub fn new(db: Arc<Database>, frames_dir: PathBuf, videos_dir: PathBuf) -> Self {
        Self {
            db,
            retention_days: Arc::new(RwLock::new(7)), // 默认保留7天
            max_retention_days: 30,                   // 最大保留30天
            frames_dir,
            videos_dir,
        }
    }

    /// 设置保留天数
    pub async fn set_retention_days(&self, days: i64) -> Result<()> {
        if days < 1 {
            return Err(anyhow::anyhow!("保留天数必须至少为1天"));
        }
        if days > self.max_retention_days {
            return Err(anyhow::anyhow!(
                "保留天数不能超过{}天",
                self.max_retention_days
            ));
        }

        let mut retention_days = self.retention_days.write().await;
        *retention_days = days;
        info!("数据保留天数已更新为: {}天", days);
        Ok(())
    }

    /// 获取当前保留天数
    pub async fn get_retention_days(&self) -> i64 {
        *self.retention_days.read().await
    }

    /// 启动自动清理任务
    pub async fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(3600)); // 每小时检查一次
            info!("存储清理任务已启动，每小时检查一次");

            loop {
                interval.tick().await;

                // 执行清理
                if let Err(e) = self.perform_cleanup().await {
                    error!("清理任务执行失败: {}", e);
                }
            }
        });
    }

    /// 执行清理操作
    pub async fn perform_cleanup(&self) -> Result<()> {
        let retention_days = *self.retention_days.read().await;
        let cutoff_date = crate::storage::local_now() - ChronoDuration::days(retention_days);
        info!("开始清理 {} 之前的数据", cutoff_date.format("%Y-%m-%d"));

        // 1. 获取要删除的会话详情（包括关联的文件路径）
        let old_sessions = self.get_old_sessions_with_files(&cutoff_date).await?;

        // 2. 删除数据库中的记录
        let deleted_count = self.db.delete_old_sessions(cutoff_date).await?;

        // 3. 删除关联的文件
        let failed_files = if deleted_count > 0 {
            self.cleanup_files(old_sessions).await?
        } else {
            Vec::new()
        };

        // 4. 清理孤立文件（没有数据库记录的文件）
        self.cleanup_orphaned_files().await?;

        // 5. 记录清理结果
        if !failed_files.is_empty() {
            error!("清理完成，但有 {} 个文件删除失败", failed_files.len());
            for (path, err) in &failed_files {
                error!("  - {}: {}", path, err);
            }
        }

        info!("清理完成，删除了 {} 个会话", deleted_count);
        Ok(())
    }

    /// 获取要删除的会话及其文件信息
    /// 获取旧会话及其关联的文件路径
    ///
    /// TODO: 实现完整的旧会话查询逻辑
    /// 预计在 v1.2 版本实现
    #[allow(unused)]
    async fn get_old_sessions_with_files(
        &self,
        cutoff_date: &chrono::DateTime<Utc>,
    ) -> Result<Vec<SessionFiles>> {
        // 这里应该从数据库获取要删除的会话的文件路径
        // 为了简化，暂时返回空列表
        Ok(vec![])
    }

    /// 清理文件，返回失败列表
    async fn cleanup_files(&self, sessions: Vec<SessionFiles>) -> Result<Vec<(String, String)>> {
        let mut failed_files = Vec::new();

        for session in sessions {
            // 删除帧文件
            for frame_path in session.frame_paths {
                if let Err(e) = tokio::fs::remove_file(&frame_path).await {
                    error!("删除帧文件失败 {}: {}", frame_path, e);
                    failed_files.push((frame_path.clone(), e.to_string()));
                }
            }

            // 删除视频文件
            if let Some(video_path) = session.video_path {
                if let Err(e) = tokio::fs::remove_file(&video_path).await {
                    error!("删除视频文件失败 {}: {}", video_path, e);
                    failed_files.push((video_path.clone(), e.to_string()));
                }
            }
        }

        Ok(failed_files)
    }

    /// 清理孤立文件（数据库中没有记录的文件）
    async fn cleanup_orphaned_files(&self) -> Result<()> {
        // 清理frames目录中的孤立文件
        if self.frames_dir.exists() {
            self.cleanup_orphaned_in_dir(&self.frames_dir).await?;
        }

        // 清理videos目录中的孤立文件
        if self.videos_dir.exists() {
            self.cleanup_orphaned_in_dir(&self.videos_dir).await?;
        }

        Ok(())
    }

    /// 清理指定目录中的孤立文件
    async fn cleanup_orphaned_in_dir(&self, dir: &PathBuf) -> Result<()> {
        let retention_secs = {
            let retention_days = *self.retention_days.read().await;
            let clamped_days = retention_days.max(0) as u64;
            clamped_days.saturating_mul(86_400)
        };

        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // 使用异步方法获取文件元数据，而不是同步的 is_file()
            if let Ok(metadata) = tokio::fs::metadata(&path).await {
                // 检查是否是文件（而不是目录）
                if metadata.is_file() {
                    if let Ok(modified) = metadata.modified() {
                        let age = std::time::SystemTime::now()
                            .duration_since(modified)
                            .unwrap_or_default();

                        // 如果文件超过保留期限，删除它
                        if age.as_secs() > retention_secs {
                            if let Err(e) = tokio::fs::remove_file(&path).await {
                                error!("删除孤立文件失败 {:?}: {}", path, e);
                            } else {
                                info!("删除孤立文件: {:?}", path);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 手动触发清理
    pub async fn trigger_cleanup(&self) -> Result<()> {
        info!("手动触发存储清理");
        self.perform_cleanup().await
    }

    /// 获取存储统计信息
    pub async fn get_storage_stats(&self) -> Result<StorageStats> {
        let (session_count, frame_count, db_size) = self.db.get_stats().await?;

        let frames_size = self.calculate_dir_size(&self.frames_dir).await?;
        let videos_size = self.calculate_dir_size(&self.videos_dir).await?;
        let retention_days = *self.retention_days.read().await;

        Ok(StorageStats {
            session_count,
            frame_count,
            database_size: db_size,
            frames_size,
            videos_size,
            total_size: db_size + frames_size + videos_size,
            retention_days,
        })
    }

    /// 计算目录大小
    async fn calculate_dir_size(&self, dir: &PathBuf) -> Result<i64> {
        if !dir.exists() {
            return Ok(0);
        }

        let mut total_size = 0i64;
        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if let Ok(metadata) = entry.metadata().await {
                if metadata.is_file() {
                    total_size += metadata.len() as i64;
                }
            }
        }

        Ok(total_size)
    }
}

/// 会话文件信息
struct SessionFiles {
    frame_paths: Vec<String>,
    video_path: Option<String>,
}

/// 清理结果
#[derive(Debug, Default)]
pub struct CleanupResult {
    /// 删除的会话数
    pub sessions_deleted: usize,
    /// 释放的空间
    pub space_freed: u64,
    /// 删除失败的文件列表（路径，错误信息）
    pub failed_files: Vec<(String, String)>,
}

/// 存储统计信息
#[derive(Debug, serde::Serialize)]
pub struct StorageStats {
    pub session_count: i64,
    pub frame_count: i64,
    pub database_size: i64,
    pub frames_size: i64,
    pub videos_size: i64,
    pub total_size: i64,
    pub retention_days: i64,
}
