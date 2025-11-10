// Repository 抽象层 - 定义数据库操作接口

pub mod mariadb;
pub mod sqlite;

use super::models::*;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// 数据库操作接口 - 所有数据库实现必须实现此 trait
#[async_trait]
pub trait DatabaseRepository: Send + Sync {
    // ========== 会话操作 ==========

    /// 插入新会话
    async fn insert_session(&self, session: &Session) -> Result<i64>;

    /// 批量插入会话
    async fn insert_sessions(&self, sessions: &[Session]) -> Result<Vec<i64>>;

    /// 获取会话详情
    async fn get_session(&self, session_id: i64) -> Result<Session>;

    /// 获取会话详情（包含帧和标签）
    async fn get_session_detail(&self, session_id: i64) -> Result<SessionDetail>;

    /// 获取某一天的所有会话
    async fn get_sessions_by_date(&self, date: &str) -> Result<Vec<Session>>;

    /// 获取所有会话（用于数据同步）
    async fn get_all_sessions(&self) -> Result<Vec<Session>>;

    /// 更新会话信息
    async fn update_session(
        &self,
        session_id: i64,
        title: &str,
        summary: &str,
        video_path: Option<&str>,
        tags: &str,
    ) -> Result<()>;

    /// 更新会话标签
    async fn update_session_tags(&self, session_id: i64, tags: &str) -> Result<()>;

    /// 更新会话视频路径
    async fn update_session_video_path(&self, session_id: i64, video_path: &str) -> Result<()>;

    /// 更新所有会话的设备信息
    async fn update_device_info_for_all_sessions(&self) -> Result<u64>;

    /// 删除会话
    async fn delete_session(&self, session_id: i64) -> Result<()>;

    /// 获取过期会话（用于清理前获取文件路径）
    async fn get_old_sessions(&self, cutoff_date: DateTime<Utc>) -> Result<Vec<Session>>;

    /// 删除过期会话
    async fn delete_old_sessions(&self, cutoff_date: DateTime<Utc>) -> Result<u64>;

    // ========== 帧操作 ==========

    /// 插入单个帧
    async fn insert_frame(&self, frame: &Frame) -> Result<i64>;

    /// 批量插入帧
    async fn insert_frames(&self, frames: &[Frame]) -> Result<()>;

    /// 获取会话的所有帧
    async fn get_frames_by_session(&self, session_id: i64) -> Result<Vec<Frame>>;

    /// 删除会话的所有帧
    async fn delete_frames_by_session(&self, session_id: i64) -> Result<()>;

    // ========== 活动统计 ==========

    /// 获取指定日期范围的活动统计
    async fn get_activities(&self, start_date: &str, end_date: &str) -> Result<Vec<Activity>>;

    // ========== LLM 调用记录 ==========

    /// 插入 LLM 调用记录
    async fn insert_llm_call(&self, record: &LLMCallRecord) -> Result<i64>;

    /// 获取会话的 LLM 调用记录
    async fn get_llm_calls_by_session(&self, session_id: i64) -> Result<Vec<LLMCallRecord>>;

    /// 获取最近的 LLM 调用错误
    async fn get_recent_llm_errors(&self, limit: i64) -> Result<Vec<LLMCallRecord>>;

    /// 删除会话的 LLM 调用记录
    async fn delete_llm_calls_by_session(&self, session_id: i64) -> Result<()>;

    // ========== 视频分段 ==========

    /// 插入视频分段
    async fn insert_video_segment(&self, segment: &VideoSegmentRecord) -> Result<i64>;

    /// 批量插入视频分段
    async fn insert_video_segments(&self, segments: &[VideoSegmentRecord]) -> Result<()>;

    /// 获取会话的视频分段
    async fn get_video_segments_by_session(
        &self,
        session_id: i64,
    ) -> Result<Vec<VideoSegmentRecord>>;

    /// 删除会话的视频分段
    async fn delete_video_segments_by_session(&self, session_id: i64) -> Result<()>;

    // ========== 时间线卡片 ==========

    /// 插入时间线卡片
    async fn insert_timeline_card(&self, card: &TimelineCardRecord) -> Result<i64>;

    /// 批量插入时间线卡片
    async fn insert_timeline_cards(&self, cards: &[TimelineCardRecord]) -> Result<()>;

    /// 获取会话的时间线卡片
    async fn get_timeline_cards_by_session(
        &self,
        session_id: i64,
    ) -> Result<Vec<TimelineCardRecord>>;

    /// 获取最近的时间线卡片
    async fn get_recent_timeline_cards(&self, limit: i64) -> Result<Vec<TimelineCardRecord>>;

    /// 删除会话的时间线卡片
    async fn delete_timeline_cards_by_session(&self, session_id: i64) -> Result<()>;

    // ========== 统计信息 ==========

    /// 获取数据库统计信息 (会话数, 帧数, 数据库大小)
    async fn get_stats(&self) -> Result<(i64, i64, i64)>;

    /// 获取已分析的视频路径列表
    async fn get_analyzed_video_paths(&self) -> Result<Vec<String>>;

    // ========== 每日总结 ==========

    /// 保存每日总结（插入或更新）
    async fn save_day_summary(&self, date: &str, summary: &DaySummaryRecord) -> Result<()>;

    /// 获取某一天的总结
    async fn get_day_summary(&self, date: &str) -> Result<Option<DaySummaryRecord>>;

    /// 删除某一天的总结
    async fn delete_day_summary(&self, date: &str) -> Result<()>;

    // ========== 数据库初始化和元数据 ==========

    /// 初始化数据库表结构
    async fn initialize_tables(&self) -> Result<()>;

    /// 获取数据库类型标识
    fn db_type(&self) -> &str;

    /// 迁移时间字段：将 UTC 时间转换为本地时间格式存储
    ///
    /// 此方法用于将旧的 UTC 时间数据迁移为本地时间格式。
    /// 它会将所有时间字段加上时区偏移量，使数据库中显示的时间为本地时间。
    ///
    /// 返回值：(更新的会话数, 更新的帧数, 更新的 LLM 调用数, 更新的视频分段数, 更新的时间线卡片数, 更新的每日总结数)
    async fn migrate_timezone_to_local(&self) -> Result<(u64, u64, u64, u64, u64, u64)>;
}
