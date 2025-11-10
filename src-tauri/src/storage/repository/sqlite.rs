// SQLite 数据库实现

use super::DatabaseRepository;
use crate::storage::config::get_device_info;
use crate::storage::models::*;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use tracing::info;

/// SQLite 数据库实现
pub struct SqliteRepository {
    pool: SqlitePool,
}

impl SqliteRepository {
    /// 创建新的 SQLite 数据库连接
    pub async fn new(db_path: &str) -> Result<Self> {
        info!("初始化 SQLite 数据库: {}", db_path);

        // 确保数据库文件的目录存在
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // 创建连接池
        let pool = SqlitePoolOptions::new()
            .max_connections(20)
            .min_connections(2)
            .idle_timeout(std::time::Duration::from_secs(180))
            .max_lifetime(std::time::Duration::from_secs(1800))
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(&format!("sqlite:{}?mode=rwc", db_path))
            .await?;

        let repo = Self { pool };

        // 初始化表结构
        repo.initialize_tables().await?;

        Ok(repo)
    }

    /// 获取连接池引用（用于向后兼容）
    pub fn get_pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl DatabaseRepository for SqliteRepository {
    // ========== 会话操作 ==========

    async fn insert_session(&self, session: &Session) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO sessions (start_time, end_time, title, summary, video_path, tags, device_name, device_type)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        )
        .bind(&session.start_time)
        .bind(&session.end_time)
        .bind(&session.title)
        .bind(&session.summary)
        .bind(&session.video_path)
        .bind(&session.tags)
        .bind(&session.device_name)
        .bind(&session.device_type)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    async fn insert_sessions(&self, sessions: &[Session]) -> Result<Vec<i64>> {
        let mut ids = Vec::new();
        let mut tx = self.pool.begin().await?;

        for session in sessions {
            let result = sqlx::query(
                r#"
                INSERT INTO sessions (start_time, end_time, title, summary, video_path, tags, device_name, device_type)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            )
            .bind(&session.start_time)
            .bind(&session.end_time)
            .bind(&session.title)
            .bind(&session.summary)
            .bind(&session.video_path)
            .bind(&session.tags)
            .bind(&session.device_name)
            .bind(&session.device_type)
            .execute(&mut *tx)
            .await?;

            ids.push(result.last_insert_rowid());
        }

        tx.commit().await?;
        Ok(ids)
    }

    async fn get_session(&self, session_id: i64) -> Result<Session> {
        let session = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, start_time, end_time, title, summary,
                   video_path, tags, created_at, device_name, device_type
            FROM sessions
            WHERE id = ?
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(session)
    }

    async fn get_session_detail(&self, session_id: i64) -> Result<SessionDetail> {
        let session = self.get_session(session_id).await?;
        let frames = self.get_frames_by_session(session_id).await?;
        let tags = serde_json::from_str(&session.tags).unwrap_or_default();

        Ok(SessionDetail {
            session,
            frames,
            tags,
        })
    }

    async fn get_sessions_by_date(&self, date: &str) -> Result<Vec<Session>> {
        // SQLite 存储的是 UTC 时间（RFC3339 格式），使用 DATE() 函数提取日期部分
        let sessions = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, start_time, end_time, title, summary,
                   video_path, tags, created_at, device_name, device_type
            FROM sessions
            WHERE DATE(start_time) = ?
            ORDER BY start_time DESC
            "#,
        )
        .bind(date)
        .fetch_all(&self.pool)
        .await?;

        Ok(sessions)
    }

    async fn get_all_sessions(&self) -> Result<Vec<Session>> {
        let sessions = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, start_time, end_time, title, summary,
                   video_path, tags, created_at, device_name, device_type
            FROM sessions
            ORDER BY start_time
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(sessions)
    }

    async fn update_session(
        &self,
        session_id: i64,
        title: &str,
        summary: &str,
        video_path: Option<&str>,
        tags: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE sessions SET title = ?, summary = ?, video_path = ?, tags = ? WHERE id = ?",
        )
        .bind(title)
        .bind(summary)
        .bind(video_path)
        .bind(tags)
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_session_tags(&self, session_id: i64, tags: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET tags = ? WHERE id = ?")
            .bind(tags)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_session_video_path(&self, session_id: i64, video_path: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET video_path = ? WHERE id = ?")
            .bind(video_path)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_device_info_for_all_sessions(&self) -> Result<u64> {
        let (device_name, device_type) = get_device_info();

        let result = sqlx::query(
            "UPDATE sessions SET device_name = ?, device_type = ? WHERE device_name IS NULL OR device_type = 'desktop'"
        )
        .bind(&device_name)
        .bind(&device_type)
        .execute(&self.pool)
        .await?;

        let updated_count = result.rows_affected();

        if updated_count > 0 {
            info!(
                "已更新 {} 条历史会话的设备信息: device_name={}, device_type={}",
                updated_count, device_name, device_type
            );
        }

        Ok(updated_count)
    }

    async fn delete_session(&self, session_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        info!("删除会话: {}", session_id);
        Ok(())
    }

    async fn get_old_sessions(&self, cutoff_date: DateTime<Utc>) -> Result<Vec<Session>> {
        let sessions = sqlx::query_as::<_, Session>(
            "SELECT id, start_time, end_time, title, summary, video_path, tags, created_at, device_name, device_type
             FROM sessions
             WHERE start_time < ?"
        )
        .bind(cutoff_date)
        .fetch_all(&self.pool)
        .await?;

        Ok(sessions)
    }

    async fn delete_old_sessions(&self, cutoff_date: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query("DELETE FROM sessions WHERE start_time < ?")
            .bind(cutoff_date)
            .execute(&self.pool)
            .await?;

        let deleted_count = result.rows_affected();

        if deleted_count > 0 {
            info!("删除了 {} 个过期会话", deleted_count);
        }

        Ok(deleted_count)
    }

    // ========== 帧操作 ==========

    async fn insert_frame(&self, frame: &Frame) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO frames (session_id, timestamp, file_path)
            VALUES (?1, ?2, ?3)
        "#,
        )
        .bind(frame.session_id)
        .bind(&frame.timestamp)
        .bind(&frame.file_path)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    async fn insert_frames(&self, frames: &[Frame]) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        for frame in frames {
            sqlx::query(
                r#"
                INSERT INTO frames (session_id, timestamp, file_path)
                VALUES (?1, ?2, ?3)
            "#,
            )
            .bind(frame.session_id)
            .bind(&frame.timestamp)
            .bind(&frame.file_path)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn get_frames_by_session(&self, session_id: i64) -> Result<Vec<Frame>> {
        let frames = sqlx::query_as::<_, Frame>(
            r#"
            SELECT id, session_id, timestamp, file_path
            FROM frames
            WHERE session_id = ?
            ORDER BY timestamp
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(frames)
    }

    async fn delete_frames_by_session(&self, session_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM frames WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========== 活动统计 ==========

    async fn get_activities(&self, start_date: &str, end_date: &str) -> Result<Vec<Activity>> {
        // 使用字符串拼接构造时间范围，避免 DATE() 函数的时区转换问题
        let start_datetime = format!("{} 00:00:00", start_date);
        let end_datetime = format!("{} 23:59:59", end_date);

        let rows = sqlx::query(
            r#"
            SELECT
                DATE(start_time) as date,
                COUNT(*) as session_count,
                SUM(CAST((julianday(end_time) - julianday(start_time)) * 24 * 60 AS INTEGER)) as total_duration_minutes,
                GROUP_CONCAT(DISTINCT json_extract(tags, '$[0].category')) as main_categories
            FROM sessions
            WHERE start_time >= ? AND start_time <= ?
            GROUP BY DATE(start_time)
            ORDER BY date DESC
            "#
        )
        .bind(&start_datetime)
        .bind(&end_datetime)
        .fetch_all(&self.pool)
        .await?;

        let mut activities = Vec::new();
        for row in rows {
            let date: String = row.try_get("date")?;
            let session_count: i32 = row.try_get("session_count")?;
            let total_duration_minutes: Option<i32> = row.try_get("total_duration_minutes")?;
            let main_categories_str: Option<String> = row.try_get("main_categories")?;

            let main_categories = main_categories_str
                .map(|s| s.split(',').map(|s| s.to_string()).collect())
                .unwrap_or_default();

            activities.push(Activity {
                date,
                session_count,
                total_duration_minutes: total_duration_minutes.unwrap_or(0),
                main_categories,
            });
        }

        Ok(activities)
    }

    // ========== LLM 调用记录 ==========

    async fn insert_llm_call(&self, record: &LLMCallRecord) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO llm_calls (
                session_id, provider, model, call_type,
                request_headers, request_body, response_headers, response_body,
                status_code, error_message, latency_ms, token_usage, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        )
        .bind(&record.session_id)
        .bind(&record.provider)
        .bind(&record.model)
        .bind(&record.call_type)
        .bind(&record.request_headers)
        .bind(&record.request_body)
        .bind(&record.response_headers)
        .bind(&record.response_body)
        .bind(&record.status_code)
        .bind(&record.error_message)
        .bind(&record.latency_ms)
        .bind(&record.token_usage)
        .bind(&record.created_at)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    async fn get_llm_calls_by_session(&self, session_id: i64) -> Result<Vec<LLMCallRecord>> {
        let records = sqlx::query_as::<_, LLMCallRecord>(
            r#"
            SELECT * FROM llm_calls
            WHERE session_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }

    async fn get_recent_llm_errors(&self, limit: i64) -> Result<Vec<LLMCallRecord>> {
        let records = sqlx::query_as::<_, LLMCallRecord>(
            r#"
            SELECT * FROM llm_calls
            WHERE error_message IS NOT NULL
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }

    async fn delete_llm_calls_by_session(&self, session_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM llm_calls WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========== 视频分段 ==========

    async fn insert_video_segment(&self, segment: &VideoSegmentRecord) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO video_segments (
                session_id, llm_call_id, start_timestamp, end_timestamp,
                description, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        )
        .bind(&segment.session_id)
        .bind(&segment.llm_call_id)
        .bind(&segment.start_timestamp)
        .bind(&segment.end_timestamp)
        .bind(&segment.description)
        .bind(&segment.created_at)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    async fn insert_video_segments(&self, segments: &[VideoSegmentRecord]) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        for segment in segments {
            sqlx::query(
                r#"
                INSERT INTO video_segments (
                    session_id, llm_call_id, start_timestamp, end_timestamp,
                    description, created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            )
            .bind(&segment.session_id)
            .bind(&segment.llm_call_id)
            .bind(&segment.start_timestamp)
            .bind(&segment.end_timestamp)
            .bind(&segment.description)
            .bind(&segment.created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn get_video_segments_by_session(
        &self,
        session_id: i64,
    ) -> Result<Vec<VideoSegmentRecord>> {
        let segments = sqlx::query_as::<_, VideoSegmentRecord>(
            r#"
            SELECT * FROM video_segments
            WHERE session_id = ?
            ORDER BY start_timestamp
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(segments)
    }

    async fn delete_video_segments_by_session(&self, session_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM video_segments WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========== 时间线卡片 ==========

    async fn insert_timeline_card(&self, card: &TimelineCardRecord) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO timeline_cards (
                session_id, llm_call_id, start_time, end_time,
                category, subcategory, title, summary, detailed_summary,
                distractions, app_sites, video_preview_path, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        )
        .bind(&card.session_id)
        .bind(&card.llm_call_id)
        .bind(&card.start_time)
        .bind(&card.end_time)
        .bind(&card.category)
        .bind(&card.subcategory)
        .bind(&card.title)
        .bind(&card.summary)
        .bind(&card.detailed_summary)
        .bind(&card.distractions)
        .bind(&card.app_sites)
        .bind(&card.video_preview_path)
        .bind(&card.created_at)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    async fn insert_timeline_cards(&self, cards: &[TimelineCardRecord]) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        for card in cards {
            sqlx::query(
                r#"
                INSERT INTO timeline_cards (
                    session_id, llm_call_id, start_time, end_time,
                    category, subcategory, title, summary, detailed_summary,
                    distractions, app_sites, video_preview_path, created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            )
            .bind(&card.session_id)
            .bind(&card.llm_call_id)
            .bind(&card.start_time)
            .bind(&card.end_time)
            .bind(&card.category)
            .bind(&card.subcategory)
            .bind(&card.title)
            .bind(&card.summary)
            .bind(&card.detailed_summary)
            .bind(&card.distractions)
            .bind(&card.app_sites)
            .bind(&card.video_preview_path)
            .bind(&card.created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn get_timeline_cards_by_session(
        &self,
        session_id: i64,
    ) -> Result<Vec<TimelineCardRecord>> {
        let cards = sqlx::query_as::<_, TimelineCardRecord>(
            r#"
            SELECT * FROM timeline_cards
            WHERE session_id = ?
            ORDER BY start_time
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(cards)
    }

    async fn get_recent_timeline_cards(&self, limit: i64) -> Result<Vec<TimelineCardRecord>> {
        let cards = sqlx::query_as::<_, TimelineCardRecord>(
            r#"
            SELECT * FROM timeline_cards
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(cards)
    }

    async fn delete_timeline_cards_by_session(&self, session_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM timeline_cards WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========== 统计信息 ==========

    async fn get_stats(&self) -> Result<(i64, i64, i64)> {
        let session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&self.pool)
            .await?;

        let frame_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM frames")
            .fetch_one(&self.pool)
            .await?;

        let total_size: i64 = sqlx::query_scalar(
            "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok((session_count, frame_count, total_size))
    }

    async fn get_analyzed_video_paths(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT video_path
            FROM sessions
            WHERE video_path IS NOT NULL
              AND summary != '{}'
              AND summary != ''
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut paths = Vec::new();
        for row in rows {
            if let Ok(Some(path)) = row.try_get::<Option<String>, _>("video_path") {
                paths.push(path);
            }
        }

        Ok(paths)
    }

    // ========== 数据库初始化 ==========

    async fn initialize_tables(&self) -> Result<()> {
        // 创建会话表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start_time DATETIME NOT NULL,
                end_time DATETIME NOT NULL,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                video_path TEXT,
                tags TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                device_name TEXT,
                device_type TEXT
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // 创建帧表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS frames (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                timestamp DATETIME NOT NULL,
                file_path TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // 创建索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_start_time ON sessions(start_time)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_frames_session_id ON frames(session_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sessions_start_end ON sessions(start_time, end_time)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_frames_session_timestamp ON frames(session_id, timestamp)")
            .execute(&self.pool)
            .await?;

        // 创建LLM调用记录表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS llm_calls (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                call_type TEXT NOT NULL,
                request_headers TEXT NOT NULL,
                request_body TEXT NOT NULL,
                response_headers TEXT,
                response_body TEXT,
                status_code INTEGER,
                error_message TEXT,
                latency_ms INTEGER,
                token_usage TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // 创建视频分段表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS video_segments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                llm_call_id INTEGER,
                start_timestamp TEXT NOT NULL,
                end_timestamp TEXT NOT NULL,
                description TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                FOREIGN KEY (llm_call_id) REFERENCES llm_calls(id) ON DELETE SET NULL
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // 创建时间线卡片表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS timeline_cards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                llm_call_id INTEGER,
                start_time TEXT NOT NULL,
                end_time TEXT NOT NULL,
                category TEXT NOT NULL,
                subcategory TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                detailed_summary TEXT NOT NULL,
                distractions TEXT,
                app_sites TEXT NOT NULL,
                video_preview_path TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                FOREIGN KEY (llm_call_id) REFERENCES llm_calls(id) ON DELETE SET NULL
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // 创建每日总结表（缓存）
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS day_summaries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date DATE NOT NULL UNIQUE,
                summary_text TEXT NOT NULL,
                device_stats TEXT NOT NULL,
                parallel_work TEXT NOT NULL,
                usage_patterns TEXT NOT NULL,
                active_device_count INTEGER NOT NULL,
                llm_call_id INTEGER,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (llm_call_id) REFERENCES llm_calls(id) ON DELETE SET NULL
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // 创建额外的索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_llm_calls_session_id ON llm_calls(session_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_llm_calls_created_at ON llm_calls(created_at)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_video_segments_session_id ON video_segments(session_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_timeline_cards_session_id ON timeline_cards(session_id)")
            .execute(&self.pool)
            .await?;

        // 数据库迁移: 为已存在的sessions表添加设备字段
        let check_device_name = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='device_name'",
        )
        .fetch_one(&self.pool)
        .await?;

        if check_device_name == 0 {
            info!("迁移数据库: 添加device_name和device_type字段");
            sqlx::query("ALTER TABLE sessions ADD COLUMN device_name TEXT")
                .execute(&self.pool)
                .await?;
            sqlx::query("ALTER TABLE sessions ADD COLUMN device_type TEXT")
                .execute(&self.pool)
                .await?;

            let (device_name, device_type) = get_device_info();

            sqlx::query(
                "UPDATE sessions SET device_name = ?, device_type = ? WHERE device_name IS NULL",
            )
            .bind(&device_name)
            .bind(&device_type)
            .execute(&self.pool)
            .await?;

            info!(
                "设备信息迁移完成: device_name={}, device_type={}",
                device_name, device_type
            );
        }

        info!("SQLite 数据库表初始化完成");
        Ok(())
    }

    async fn save_day_summary(&self, date: &str, summary: &DaySummaryRecord) -> Result<()> {
        // 使用 INSERT OR REPLACE 实现 upsert
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO day_summaries (
                date, summary_text, device_stats, parallel_work, usage_patterns,
                active_device_count, llm_call_id, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(date)
        .bind(&summary.summary_text)
        .bind(&summary.device_stats)
        .bind(&summary.parallel_work)
        .bind(&summary.usage_patterns)
        .bind(summary.active_device_count)
        .bind(summary.llm_call_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_day_summary(&self, date: &str) -> Result<Option<DaySummaryRecord>> {
        let result = sqlx::query_as::<_, DaySummaryRecord>(
            r#"
            SELECT * FROM day_summaries WHERE date = ?
            "#,
        )
        .bind(date)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    async fn delete_day_summary(&self, date: &str) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM day_summaries WHERE date = ?
            "#,
        )
        .bind(date)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    fn db_type(&self) -> &str {
        "sqlite"
    }

    async fn migrate_timezone_to_local(&self) -> Result<(u64, u64, u64, u64, u64, u64)> {
        use chrono::Local;

        // 计算时区偏移量（秒）
        let local_offset_seconds = Local::now().offset().local_minus_utc();

        info!(
            "开始时区迁移：将 UTC 时间转换为本地时间（偏移 {} 秒 / {} 小时）",
            local_offset_seconds,
            local_offset_seconds / 3600
        );

        // SQLite 使用 datetime() 函数和字符串偏移
        let offset_str = format!("{} seconds", local_offset_seconds);

        // 更新 sessions 表
        let sessions_updated = sqlx::query(&format!(
            "UPDATE sessions SET
             start_time = datetime(start_time, '{}'),
             end_time = datetime(end_time, '{}'),
             created_at = datetime(created_at, '{}')",
            offset_str, offset_str, offset_str
        ))
        .execute(&self.pool)
        .await?
        .rows_affected();

        // 更新 frames 表
        let frames_updated = sqlx::query(&format!(
            "UPDATE frames SET timestamp = datetime(timestamp, '{}')",
            offset_str
        ))
        .execute(&self.pool)
        .await?
        .rows_affected();

        // 更新 llm_calls 表
        let llm_calls_updated = sqlx::query(&format!(
            "UPDATE llm_calls SET created_at = datetime(created_at, '{}')",
            offset_str
        ))
        .execute(&self.pool)
        .await?
        .rows_affected();

        // 更新 video_segments 表
        let video_segments_updated = sqlx::query(&format!(
            "UPDATE video_segments SET created_at = datetime(created_at, '{}')",
            offset_str
        ))
        .execute(&self.pool)
        .await?
        .rows_affected();

        // 更新 timeline_cards 表
        let timeline_cards_updated = sqlx::query(&format!(
            "UPDATE timeline_cards SET created_at = datetime(created_at, '{}')",
            offset_str
        ))
        .execute(&self.pool)
        .await?
        .rows_affected();

        // 更新 day_summaries 表
        let day_summaries_updated = sqlx::query(&format!(
            "UPDATE day_summaries SET
             created_at = datetime(created_at, '{}'),
             updated_at = datetime(updated_at, '{}')",
            offset_str, offset_str
        ))
        .execute(&self.pool)
        .await?
        .rows_affected();

        info!(
            "时区迁移完成：sessions={}, frames={}, llm_calls={}, video_segments={}, timeline_cards={}, day_summaries={}",
            sessions_updated, frames_updated, llm_calls_updated,
            video_segments_updated, timeline_cards_updated, day_summaries_updated
        );

        Ok((
            sessions_updated,
            frames_updated,
            llm_calls_updated,
            video_segments_updated,
            timeline_cards_updated,
            day_summaries_updated,
        ))
    }
}
