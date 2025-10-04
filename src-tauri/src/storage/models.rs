// 数据模型定义 - 数据库实体结构

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

/// 获取当前本地时间（以 DateTime<Utc> 类型表示，但值为本地时间）
/// 用于将本地时间存储到数据库中
pub fn local_now() -> DateTime<Utc> {
    Local::now().naive_local().and_utc()
}

/// 会话数据结构
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: Option<i64>,
    #[serde(serialize_with = "serialize_datetime_as_local")]
    pub start_time: DateTime<Utc>,
    #[serde(serialize_with = "serialize_datetime_as_local")]
    pub end_time: DateTime<Utc>,
    pub title: String,
    pub summary: String,
    pub video_path: Option<String>,
    pub tags: String, // JSON序列化的标签
    #[serde(serialize_with = "serialize_datetime_as_local_option")]
    pub created_at: Option<DateTime<Utc>>,
    pub device_name: Option<String>, // 设备名称
    pub device_type: Option<String>, // 设备类型(desktop, laptop, tablet等)
}

/// 帧数据结构
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Frame {
    pub id: Option<i64>,
    pub session_id: i64,
    #[serde(serialize_with = "serialize_datetime_as_local")]
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
    #[serde(serialize_with = "serialize_datetime_as_local")]
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
    #[serde(serialize_with = "serialize_datetime_as_local")]
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
    #[serde(serialize_with = "serialize_datetime_as_local")]
    pub created_at: DateTime<Utc>,
}

/// 每日总结记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DaySummaryRecord {
    pub id: Option<i64>,
    #[serde(
        serialize_with = "serialize_naive_date",
        deserialize_with = "deserialize_naive_date"
    )]
    pub date: chrono::NaiveDate, // 日期
    pub summary_text: String,     // LLM 生成的总结文本
    pub device_stats: String,     // JSON 格式的设备统计
    pub parallel_work: String,    // JSON 格式的并行工作
    pub usage_patterns: String,   // JSON 格式的使用模式
    pub active_device_count: i32, // 活跃设备数量
    pub llm_call_id: Option<i64>, // 关联的 LLM 调用记录
    #[serde(serialize_with = "serialize_datetime_as_local")]
    pub created_at: DateTime<Utc>, // 创建时间
    #[serde(serialize_with = "serialize_datetime_as_local")]
    pub updated_at: DateTime<Utc>, // 更新时间
}

// 自定义序列化：NaiveDate -> String (YYYY-MM-DD)
fn serialize_naive_date<S>(date: &chrono::NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&date.format("%Y-%m-%d").to_string())
}

// 自定义反序列化：String (YYYY-MM-DD) -> NaiveDate
fn deserialize_naive_date<'de, D>(deserializer: D) -> Result<chrono::NaiveDate, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(serde::de::Error::custom)
}

/// 自定义序列化：DateTime<Utc> -> 不带时区标记的字符串
/// 将数据库中的本地时间序列化为 "YYYY-MM-DD HH:MM:SS" 格式（不带Z后缀）
/// 这样前端 dayjs 会将其视为本地时间，不会再进行时区转换
fn serialize_datetime_as_local<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    // 注意：数据库中存储的已经是本地时间（虽然类型是DateTime<Utc>）
    // 直接格式化为不带时区标记的字符串
    serializer.serialize_str(&dt.format("%Y-%m-%dT%H:%M:%S").to_string())
}

/// 自定义序列化：Option<DateTime<Utc>> -> Option<不带时区标记的字符串>
fn serialize_datetime_as_local_option<S>(
    dt: &Option<DateTime<Utc>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match dt {
        Some(dt) => serializer.serialize_some(&dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
        None => serializer.serialize_none(),
    }
}
