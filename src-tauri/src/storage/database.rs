// 数据库操作模块 - SQLite数据库管理

use super::{
    Activity, Frame, LLMCallRecord, Session, SessionDetail, TimelineCardRecord, VideoSegmentRecord,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use tracing::{error, info};

/// 数据库管理器
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// 创建新的数据库连接
    pub async fn new(db_path: &str) -> Result<Self> {
        info!("初始化数据库: {}", db_path);

        // 确保数据库文件的目录存在
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // 创建连接池 - 添加 ?mode=rwc 参数确保创建数据库
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .idle_timeout(std::time::Duration::from_secs(300))      // 5分钟空闲超时
            .max_lifetime(std::time::Duration::from_secs(1800))     // 30分钟最大生命周期
            .acquire_timeout(std::time::Duration::from_secs(30))    // 30秒获取超时
            .connect(&format!("sqlite:{}?mode=rwc", db_path))
            .await?;

        let db = Self { pool };

        // 初始化表结构
        db.initialize_tables().await?;

        Ok(db)
    }

    /// 初始化数据库表
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
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
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

        // 为 get_activities 查询优化添加组合索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_start_end ON sessions(start_time, end_time)")
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

        info!("数据库表初始化完成");
        Ok(())
    }

    /// 插入新会话
    pub async fn insert_session(&self, session: &Session) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO sessions (start_time, end_time, title, summary, video_path, tags)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        )
        .bind(&session.start_time)
        .bind(&session.end_time)
        .bind(&session.title)
        .bind(&session.summary)
        .bind(&session.video_path)
        .bind(&session.tags)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// 插入帧数据
    pub async fn insert_frame(&self, frame: &Frame) -> Result<i64> {
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

    /// 批量插入帧数据
    pub async fn insert_frames(&self, frames: &[Frame]) -> Result<()> {
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

    /// 获取指定日期范围的活动
    pub async fn get_activities(&self, start_date: &str, end_date: &str) -> Result<Vec<Activity>> {
        let rows = sqlx::query(
            r#"
            SELECT
                DATE(datetime(start_time, 'localtime')) as date,
                COUNT(*) as session_count,
                SUM(CAST((julianday(end_time) - julianday(start_time)) * 24 * 60 AS INTEGER)) as total_duration_minutes,
                GROUP_CONCAT(DISTINCT json_extract(tags, '$[0].category')) as main_categories
            FROM sessions
            WHERE DATE(datetime(start_time, 'localtime')) BETWEEN DATE(?) AND DATE(?)
            GROUP BY DATE(datetime(start_time, 'localtime'))
            ORDER BY date DESC
            "#
        )
        .bind(start_date)
        .bind(end_date)
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

    /// 获取某一天的所有会话
    pub async fn get_sessions_by_date(&self, date: &str) -> Result<Vec<Session>> {
        // 将日期转换为UTC的开始和结束时间戳
        // 假设date格式为YYYY-MM-DD，需要转换为当地时间对应的UTC时间范围
        let start_of_day = format!("{} 00:00:00", date);
        let end_of_day = format!("{} 23:59:59", date);

        let sessions = sqlx::query_as::<_, Session>(
            r#"
            SELECT
                id, start_time, end_time, title, summary,
                video_path, tags, created_at
            FROM sessions
            WHERE datetime(start_time, 'localtime') >= ?
              AND datetime(start_time, 'localtime') <= ?
            ORDER BY start_time DESC
            "#,
        )
        .bind(start_of_day)
        .bind(end_of_day)
        .fetch_all(&self.pool)
        .await?;

        Ok(sessions)
    }

    /// 获取会话详情
    pub async fn get_session_detail(&self, session_id: i64) -> Result<SessionDetail> {
        // 获取会话信息
        let session = sqlx::query_as::<_, Session>(
            r#"
            SELECT
                id, start_time, end_time, title, summary,
                video_path, tags, created_at
            FROM sessions
            WHERE id = ?
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        // 获取相关帧
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

        // 解析标签
        let tags = serde_json::from_str(&session.tags).unwrap_or_default();

        Ok(SessionDetail {
            session,
            frames,
            tags,
        })
    }

    /// 更新会话标签
    pub async fn update_session_tags(&self, session_id: i64, tags: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE sessions
            SET tags = ?
            WHERE id = ?
            "#,
        )
        .bind(tags)
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 更新会话视频路径
    pub async fn update_session_video_path(&self, session_id: i64, video_path: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE sessions
            SET video_path = ?
            WHERE id = ?
            "#,
        )
        .bind(video_path)
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 删除过期的会话和关联数据
    pub async fn delete_old_sessions(&self, cutoff_date: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM sessions
            WHERE start_time < ?
            "#,
        )
        .bind(cutoff_date)
        .execute(&self.pool)
        .await?;

        let deleted_count = result.rows_affected();
        if deleted_count > 0 {
            info!("删除了 {} 个过期会话", deleted_count);
        }

        Ok(deleted_count)
    }

    /// 删除单个会话
    pub async fn delete_session(&self, session_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        info!("删除会话: {}", session_id);
        Ok(())
    }

    /// 获取数据库统计信息
    pub async fn get_stats(&self) -> Result<(i64, i64, i64)> {
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

    /// 获取连接池（供其他模块使用）
    pub fn get_pool(&self) -> &SqlitePool {
        &self.pool
    }

    // ========== LLM调用记录相关方法 ==========

    /// 插入LLM调用记录
    pub async fn insert_llm_call(&self, record: &LLMCallRecord) -> Result<i64> {
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

    /// 获取会话的LLM调用记录
    pub async fn get_llm_calls_by_session(&self, session_id: i64) -> Result<Vec<LLMCallRecord>> {
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

    /// 获取最近的LLM调用错误
    pub async fn get_recent_llm_errors(&self, limit: i64) -> Result<Vec<LLMCallRecord>> {
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

    // ========== 视频分段相关方法 ==========

    /// 插入视频分段
    pub async fn insert_video_segment(&self, segment: &VideoSegmentRecord) -> Result<i64> {
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

    /// 批量插入视频分段
    pub async fn insert_video_segments(&self, segments: &[VideoSegmentRecord]) -> Result<()> {
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

    /// 获取会话的视频分段
    pub async fn get_video_segments_by_session(
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

    // ========== 时间线卡片相关方法 ==========

    /// 插入时间线卡片
    pub async fn insert_timeline_card(&self, card: &TimelineCardRecord) -> Result<i64> {
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

    /// 批量插入时间线卡片
    pub async fn insert_timeline_cards(&self, cards: &[TimelineCardRecord]) -> Result<()> {
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

    /// 获取会话的时间线卡片
    pub async fn get_timeline_cards_by_session(
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

    /// 获取最近的时间线卡片（用于继续生成timeline）
    pub async fn get_recent_timeline_cards(&self, limit: i64) -> Result<Vec<TimelineCardRecord>> {
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
}
