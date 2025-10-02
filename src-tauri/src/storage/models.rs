// 数据模型定义 - 数据库实体结构

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 会话数据结构
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: Option<i64>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub title: String,
    pub summary: String,
    pub video_path: Option<String>,
    pub tags: String, // JSON序列化的标签
    pub created_at: Option<DateTime<Utc>>,
    pub device_name: Option<String>, // 设备名称
    pub device_type: Option<String>, // 设备类型(desktop, laptop, tablet等)
}

/// 帧数据结构
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Frame {
    pub id: Option<i64>,
    pub session_id: i64,
    pub timestamp: DateTime<Utc>,
    pub file_path: String,
}

/// 活动数据结构（用于日历视图）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub date: String,
    pub session_count: i32,
    pub total_duration_minutes: i32,
    pub main_categories: Vec<String>,
}

/// 会话详情数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub session: Session,
    pub frames: Vec<Frame>,
    pub tags: Vec<crate::models::ActivityTag>,
}

/// LLM调用记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LLMCallRecord {
    pub id: Option<i64>,
    pub session_id: Option<i64>,
    pub provider: String, // openai, anthropic等
    pub model: String,
    pub call_type: String, // segment_video, generate_timeline, analyze_frames
    pub request_headers: String, // JSON格式的请求头
    pub request_body: String, // JSON格式的请求体
    pub response_headers: Option<String>, // JSON格式的响应头
    pub response_body: Option<String>, // 响应内容
    pub status_code: Option<i32>,
    pub error_message: Option<String>,
    pub latency_ms: Option<i64>,     // 调用延迟（毫秒）
    pub token_usage: Option<String>, // JSON格式的token使用情况
    pub created_at: DateTime<Utc>,
}

/// 视频分段记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoSegmentRecord {
    pub id: Option<i64>,
    pub session_id: i64,
    pub llm_call_id: Option<i64>, // 关联的LLM调用记录
    pub start_timestamp: String,  // MM:SS格式
    pub end_timestamp: String,    // MM:SS格式
    pub description: String,
    pub created_at: DateTime<Utc>,
}

/// 时间线卡片记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TimelineCardRecord {
    pub id: Option<i64>,
    pub session_id: i64,
    pub llm_call_id: Option<i64>, // 关联的LLM调用记录
    pub start_time: String,
    pub end_time: String,
    pub category: String,
    pub subcategory: String,
    pub title: String,
    pub summary: String,
    pub detailed_summary: String,
    pub distractions: Option<String>,       // JSON格式的干扰活动
    pub app_sites: String,                  // JSON格式的应用/网站信息
    pub video_preview_path: Option<String>, // 本地视频文件路径
    pub created_at: DateTime<Utc>,
}