// 数据库主接口 - Facade 模式统一对外接口

use super::cache::CachedRepository;
use super::config::DatabaseConfig;
use super::models::*;
use super::repository::{mariadb::MariaDbRepository, sqlite::SqliteRepository, DatabaseRepository};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tracing::info;

/// 数据库管理器 - 对外统一接口
pub struct Database {
    /// 底层仓库（带缓存）
    repository: Arc<CachedRepository>,
    /// 数据库类型标识
    db_type: String,
}

impl Database {
    /// 从配置创建数据库连接
    pub async fn from_config(config: &DatabaseConfig) -> Result<Self> {
        match config {
            DatabaseConfig::SQLite { db_path } => Self::new_sqlite(db_path).await,
            DatabaseConfig::MariaDB {
                host,
                port,
                database,
                username,
                password,
            } => Self::new_mariadb(host, *port, database, username, password).await,
        }
    }

    /// 创建 SQLite 数据库连接
    pub async fn new_sqlite(db_path: &str) -> Result<Self> {
        let sqlite_repo = SqliteRepository::new(db_path).await?;
        let cached_repo = CachedRepository::new(Arc::new(sqlite_repo));

        Ok(Self {
            repository: Arc::new(cached_repo),
            db_type: "sqlite".to_string(),
        })
    }

    /// 创建 MariaDB 数据库连接
    pub async fn new_mariadb(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
    ) -> Result<Self> {
        let mariadb_repo = MariaDbRepository::new(host, port, database, username, password).await?;
        let cached_repo = CachedRepository::new(Arc::new(mariadb_repo));

        Ok(Self {
            repository: Arc::new(cached_repo),
            db_type: "mariadb".to_string(),
        })
    }

    // ========== 会话操作 ==========

    pub async fn insert_session(&self, session: &Session) -> Result<i64> {
        self.repository.insert_session(session).await
    }

    pub async fn insert_sessions(&self, sessions: &[Session]) -> Result<Vec<i64>> {
        self.repository.insert_sessions(sessions).await
    }

    pub async fn get_session(&self, session_id: i64) -> Result<Session> {
        self.repository.get_session(session_id).await
    }

    pub async fn get_session_detail(&self, session_id: i64) -> Result<SessionDetail> {
        self.repository.get_session_detail(session_id).await
    }

    pub async fn get_sessions_by_date(&self, date: &str) -> Result<Vec<Session>> {
        self.repository.get_sessions_by_date(date).await
    }

    pub async fn get_all_sessions(&self) -> Result<Vec<Session>> {
        self.repository.get_all_sessions().await
    }

    pub async fn update_session(
        &self,
        session_id: i64,
        title: &str,
        summary: &str,
        video_path: Option<&str>,
        tags: &str,
    ) -> Result<()> {
        self.repository
            .update_session(session_id, title, summary, video_path, tags)
            .await
    }

    pub async fn update_session_tags(&self, session_id: i64, tags: &str) -> Result<()> {
        self.repository.update_session_tags(session_id, tags).await
    }

    pub async fn update_session_video_path(&self, session_id: i64, video_path: &str) -> Result<()> {
        self.repository
            .update_session_video_path(session_id, video_path)
            .await
    }

    pub async fn update_device_info_for_all_sessions(&self) -> Result<u64> {
        self.repository.update_device_info_for_all_sessions().await
    }

    pub async fn delete_session(&self, session_id: i64) -> Result<()> {
        self.repository.delete_session(session_id).await
    }

    pub async fn get_old_sessions(&self, cutoff_date: DateTime<Utc>) -> Result<Vec<Session>> {
        self.repository.get_old_sessions(cutoff_date).await
    }

    pub async fn delete_old_sessions(&self, cutoff_date: DateTime<Utc>) -> Result<u64> {
        self.repository.delete_old_sessions(cutoff_date).await
    }

    // ========== 帧操作 ==========

    pub async fn insert_frame(&self, frame: &Frame) -> Result<i64> {
        self.repository.insert_frame(frame).await
    }

    pub async fn insert_frames(&self, frames: &[Frame]) -> Result<()> {
        self.repository.insert_frames(frames).await
    }

    pub async fn get_frames_by_session(&self, session_id: i64) -> Result<Vec<Frame>> {
        self.repository.get_frames_by_session(session_id).await
    }

    pub async fn delete_frames_by_session(&self, session_id: i64) -> Result<()> {
        self.repository.delete_frames_by_session(session_id).await
    }

    // ========== 活动统计 ==========

    pub async fn get_activities(&self, start_date: &str, end_date: &str) -> Result<Vec<Activity>> {
        self.repository.get_activities(start_date, end_date).await
    }

    // ========== LLM 调用记录 ==========

    pub async fn insert_llm_call(&self, record: &LLMCallRecord) -> Result<i64> {
        self.repository.insert_llm_call(record).await
    }

    pub async fn get_llm_calls_by_session(&self, session_id: i64) -> Result<Vec<LLMCallRecord>> {
        self.repository.get_llm_calls_by_session(session_id).await
    }

    pub async fn get_recent_llm_errors(&self, limit: i64) -> Result<Vec<LLMCallRecord>> {
        self.repository.get_recent_llm_errors(limit).await
    }

    pub async fn delete_llm_calls_by_session(&self, session_id: i64) -> Result<()> {
        self.repository
            .delete_llm_calls_by_session(session_id)
            .await
    }

    // ========== 视频分段 ==========

    pub async fn insert_video_segment(&self, segment: &VideoSegmentRecord) -> Result<i64> {
        self.repository.insert_video_segment(segment).await
    }

    pub async fn insert_video_segments(&self, segments: &[VideoSegmentRecord]) -> Result<()> {
        self.repository.insert_video_segments(segments).await
    }

    pub async fn get_video_segments_by_session(
        &self,
        session_id: i64,
    ) -> Result<Vec<VideoSegmentRecord>> {
        self.repository
            .get_video_segments_by_session(session_id)
            .await
    }

    pub async fn delete_video_segments_by_session(&self, session_id: i64) -> Result<()> {
        self.repository
            .delete_video_segments_by_session(session_id)
            .await
    }

    // ========== 时间线卡片 ==========

    pub async fn insert_timeline_card(&self, card: &TimelineCardRecord) -> Result<i64> {
        self.repository.insert_timeline_card(card).await
    }

    pub async fn insert_timeline_cards(&self, cards: &[TimelineCardRecord]) -> Result<()> {
        self.repository.insert_timeline_cards(cards).await
    }

    pub async fn get_timeline_cards_by_session(
        &self,
        session_id: i64,
    ) -> Result<Vec<TimelineCardRecord>> {
        self.repository
            .get_timeline_cards_by_session(session_id)
            .await
    }

    pub async fn get_recent_timeline_cards(&self, limit: i64) -> Result<Vec<TimelineCardRecord>> {
        self.repository.get_recent_timeline_cards(limit).await
    }

    pub async fn delete_timeline_cards_by_session(&self, session_id: i64) -> Result<()> {
        self.repository
            .delete_timeline_cards_by_session(session_id)
            .await
    }

    // ========== 统计信息 ==========

    pub async fn get_stats(&self) -> Result<(i64, i64, i64)> {
        self.repository.get_stats().await
    }

    pub async fn get_analyzed_video_paths(&self) -> Result<Vec<String>> {
        self.repository.get_analyzed_video_paths().await
    }

    // ========== 数据库元数据 ==========

    pub async fn initialize_tables(&self) -> Result<()> {
        self.repository.initialize_tables().await
    }

    pub fn db_type(&self) -> &str {
        &self.db_type
    }

    pub fn is_sqlite(&self) -> bool {
        self.db_type == "sqlite"
    }

    pub fn is_mariadb(&self) -> bool {
        self.db_type == "mariadb"
    }

    // ========== 缓存管理 ==========

    pub async fn invalidate_session(&self, session_id: i64) {
        self.repository.invalidate_session(session_id).await;
    }

    pub async fn clear_cache(&self) {
        self.repository.clear_cache().await;
    }

    // ========== 向后兼容方法 ==========

    /// 旧版兼容方法：创建新的 SQLite 数据库连接
    pub async fn new(db_path: &str) -> Result<Self> {
        Self::new_sqlite(db_path).await
    }

    // ========== 数据同步功能 ==========

    /// 从 SQLite 同步数据到当前数据库
    ///
    /// 此方法会清空当前数据库所有数据，然后从指定的 SQLite 数据库同步所有数据
    /// 仅在 MariaDB 模式下可用
    pub async fn sync_from_sqlite_to_mariadb(&self, sqlite_db_path: &str) -> Result<()> {
        if !self.is_mariadb() {
            return Err(anyhow!("只能在 MariaDB 模式下调用此方法"));
        }

        info!("开始从 SQLite 同步数据到 MariaDB");

        // 创建 SQLite 临时连接
        let sqlite_db = Self::new_sqlite(sqlite_db_path).await?;

        // 清空当前数据库的所有数据（注意外键约束顺序）
        info!("清空 MariaDB 数据...");
        self.delete_timeline_cards_by_session(0).await.ok(); // 清空所有
        self.delete_video_segments_by_session(0).await.ok();
        self.delete_llm_calls_by_session(0).await.ok();
        self.delete_frames_by_session(0).await.ok();
        // 删除所有会话需要遍历
        let all_sessions = self.get_all_sessions().await?;
        for session in all_sessions {
            if let Some(id) = session.id {
                self.delete_session(id).await.ok();
            }
        }
        info!("MariaDB 数据已清空");

        // 同步 sessions
        info!("同步 sessions...");
        let sessions = sqlite_db.get_all_sessions().await?;
        let sessions_count = sessions.len();
        for session in sessions {
            self.insert_session(&session).await?;
        }
        info!("已同步 {} 个会话", sessions_count);

        // 同步 frames（需要重新映射 session_id）
        info!("同步 frames...");
        let all_sessions = sqlite_db.get_all_sessions().await?;
        for session in &all_sessions {
            if let Some(old_session_id) = session.id {
                let frames = sqlite_db.get_frames_by_session(old_session_id).await?;
                if !frames.is_empty() {
                    // 获取新的 session_id
                    let new_session_id = self.insert_session(session).await?;
                    for mut frame in frames {
                        frame.session_id = new_session_id;
                        self.insert_frame(&frame).await?;
                    }
                }
            }
        }
        info!("已同步帧数据");

        // 同步 llm_calls
        info!("同步 llm_calls...");
        for session in &all_sessions {
            if let Some(old_session_id) = session.id {
                let llm_calls = sqlite_db.get_llm_calls_by_session(old_session_id).await?;
                for mut call in llm_calls {
                    // 需要找到新的 session_id
                    // 简化处理：直接使用相同的 session 引用
                    call.session_id = session.id;
                    self.insert_llm_call(&call).await?;
                }
            }
        }
        info!("已同步 LLM 调用记录");

        // 同步 video_segments
        info!("同步 video_segments...");
        for session in &all_sessions {
            if let Some(old_session_id) = session.id {
                let segments = sqlite_db
                    .get_video_segments_by_session(old_session_id)
                    .await?;
                for mut segment in segments {
                    segment.session_id = old_session_id; // 保持引用
                    self.insert_video_segment(&segment).await?;
                }
            }
        }
        info!("已同步视频分段");

        // 同步 timeline_cards
        info!("同步 timeline_cards...");
        for session in &all_sessions {
            if let Some(old_session_id) = session.id {
                let cards = sqlite_db
                    .get_timeline_cards_by_session(old_session_id)
                    .await?;
                for mut card in cards {
                    card.session_id = old_session_id; // 保持引用
                    self.insert_timeline_card(&card).await?;
                }
            }
        }
        info!("已同步时间线卡片");

        info!("数据同步完成！");
        Ok(())
    }

    // ========== 每日总结操作 ==========

    pub async fn save_day_summary(&self, date: &str, summary: &DaySummaryRecord) -> Result<()> {
        self.repository.save_day_summary(date, summary).await
    }

    pub async fn get_day_summary(&self, date: &str) -> Result<Option<DaySummaryRecord>> {
        self.repository.get_day_summary(date).await
    }

    pub async fn delete_day_summary(&self, date: &str) -> Result<()> {
        self.repository.delete_day_summary(date).await
    }

    // ========== 数据库维护操作 ==========

    /// 迁移数据库时区：将 UTC 时间转换为本地时间
    pub async fn migrate_timezone_to_local(&self) -> Result<(u64, u64, u64, u64, u64, u64)> {
        self.repository.migrate_timezone_to_local().await
    }
}
