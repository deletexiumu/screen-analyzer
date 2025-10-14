// 数据模型模块 - 定义所有的数据结构

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// 重新导出其他模块的类型
pub use crate::llm::plugin::{ActivityCategory, ActivityTag, KeyMoment};
pub use crate::storage::{Activity, DatabaseConfig, Frame, Session, SessionDetail};

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// 数据保留天数
    pub retention_days: Option<i64>,
    /// LLM提供商
    pub llm_provider: Option<String>,
    /// 截屏间隔（秒）
    pub capture_interval: Option<u64>,
    /// 总结间隔（分钟）
    pub summary_interval: Option<u64>,
    /// 视频配置
    pub video_config: Option<VideoSettings>,
    /// UI设置
    pub ui_settings: Option<UISettings>,
    /// LLM配置
    pub llm_config: Option<LLMProviderConfig>,
    /// 截屏配置
    pub capture_settings: Option<CaptureSettings>,
    /// 日志配置
    pub logger_settings: Option<LoggerSettings>,
    /// 数据库配置
    pub database_config: Option<DatabaseConfig>,
    /// Notion 配置
    pub notion_config: Option<NotionConfig>,
}

/// 日志设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggerSettings {
    /// 是否启用日志推送到前端
    pub enable_frontend_logging: bool,
    /// 日志级别 (trace, debug, info, warn, error)
    pub log_level: String,
    /// 最大日志缓存条数
    pub max_log_buffer: usize,
}

impl Default for LoggerSettings {
    fn default() -> Self {
        Self {
            enable_frontend_logging: true,
            log_level: "info".to_string(),
            max_log_buffer: 1000,
        }
    }
}

/// 截屏设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSettings {
    /// 截屏分辨率
    pub resolution: CaptureResolution,
    /// 图片质量(1-100)
    pub image_quality: u8,
    /// 是否启用黑屏检测
    pub detect_black_screen: bool,
    /// 黑屏检测阈值(0-255)
    pub black_screen_threshold: u8,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            resolution: CaptureResolution::FHD,
            image_quality: 85,
            detect_black_screen: true,
            black_screen_threshold: 5,
        }
    }
}

/// 截屏分辨率枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureResolution {
    #[serde(rename = "1080p")]
    FHD, // 1920x1080 (Full HD)
    #[serde(rename = "2k")]
    QHD, // 2560x1440 (2K/Quad HD)
    #[serde(rename = "4k")]
    UHD, // 3840x2160 (4K/Ultra HD)
    #[serde(rename = "original")]
    Original, // 原始分辨率
}

impl CaptureResolution {
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        match self {
            Self::FHD => Some((1920, 1080)),
            Self::QHD => Some((2560, 1440)),
            Self::UHD => Some((3840, 2160)),
            Self::Original => None,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::FHD => "1080P (1920×1080)",
            Self::QHD => "2K (2560×1440)",
            Self::UHD => "4K (3840×2160)",
            Self::Original => "原始分辨率",
        }
    }
}

/// 视频设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    /// 是否自动生成视频
    pub auto_generate: bool,
    /// 播放速度倍数
    pub speed_multiplier: f32,
    /// 视频质量(0-51)
    pub quality: u8,
    /// 是否添加时间戳
    pub add_timestamp: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            auto_generate: true,
            speed_multiplier: 8.0,
            quality: 23,
            add_timestamp: true,
        }
    }
}

/// 持久化的应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedAppConfig {
    /// 数据保留天数
    pub retention_days: i64,
    /// LLM提供商
    pub llm_provider: String,
    /// 截屏间隔（秒）
    pub capture_interval: u64,
    /// 总结间隔（分钟）
    pub summary_interval: u64,
    /// 视频配置
    pub video_config: VideoSettings,
    /// UI设置
    pub ui_settings: Option<UISettings>,
    /// LLM配置
    pub llm_config: Option<LLMProviderConfig>,
    /// 截屏配置
    pub capture_settings: Option<CaptureSettings>,
    /// 日志配置
    pub logger_settings: Option<LoggerSettings>,
    /// 数据库配置
    pub database_config: Option<DatabaseConfig>,
    /// Notion 配置
    pub notion_config: Option<NotionConfig>,
}

impl Default for PersistedAppConfig {
    fn default() -> Self {
        Self {
            retention_days: 7,
            llm_provider: "openai".to_string(),
            capture_interval: 1,
            summary_interval: 15,
            video_config: VideoSettings::default(),
            ui_settings: Some(UISettings::default()),
            llm_config: None,
            capture_settings: Some(CaptureSettings::default()),
            logger_settings: Some(LoggerSettings::default()),
            database_config: None,
            notion_config: Some(NotionConfig::default()),
        }
    }
}

/// LLM提供商配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMProviderConfig {
    /// API密钥
    pub api_key: String,
    /// 模型名称
    pub model: String,
    /// API基础URL
    pub base_url: String,
    /// 是否使用视频模式
    pub use_video_mode: bool,
    /// Anthropic 认证令牌（用于替代 API Key）
    pub auth_token: String,
}

/// UI设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UISettings {
    /// 主题（light/dark）
    pub theme: String,
    /// 语言
    pub language: String,
    /// 是否显示系统托盘
    pub show_tray: bool,
    /// 是否开机自启
    pub auto_start: bool,
    /// 快捷键设置
    pub hotkeys: HotkeySettings,
}

impl Default for UISettings {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            language: "zh-CN".to_string(),
            show_tray: true,
            auto_start: false,
            hotkeys: HotkeySettings::default(),
        }
    }
}

/// 快捷键设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeySettings {
    /// 暂停/恢复截屏
    pub toggle_capture: Option<String>,
    /// 手动触发总结
    pub manual_summary: Option<String>,
    /// 显示/隐藏窗口
    pub toggle_window: Option<String>,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            toggle_capture: Some("Cmd+Shift+P".to_string()),
            manual_summary: Some("Cmd+Shift+S".to_string()),
            toggle_window: Some("Cmd+Shift+A".to_string()),
        }
    }
}

/// 系统状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    /// 是否正在截屏
    pub is_capturing: bool,
    /// 是否正在处理
    pub is_processing: bool,
    /// 最后截屏时间
    pub last_capture_time: Option<DateTime<Utc>>,
    /// 最后处理时间
    pub last_process_time: Option<DateTime<Utc>>,
    /// 当前会话帧数
    pub current_session_frames: usize,
    /// 存储使用情况
    pub storage_usage: StorageUsage,
    /// 错误信息
    pub last_error: Option<String>,
    /// CPU占用率（百分比）
    pub cpu_usage: f32,
    /// 内存占用（MB）
    pub memory_usage: f32,
}

impl Default for SystemStatus {
    fn default() -> Self {
        Self {
            is_capturing: false,
            is_processing: false,
            last_capture_time: None,
            last_process_time: None,
            current_session_frames: 0,
            storage_usage: StorageUsage::default(),
            last_error: None,
            cpu_usage: 0.0,
            memory_usage: 0.0,
        }
    }
}

/// 存储使用情况
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageUsage {
    /// 数据库大小（字节）
    pub database_size: u64,
    /// 帧文件总大小（字节）
    pub frames_size: u64,
    /// 视频文件总大小（字节）
    pub videos_size: u64,
    /// 总大小（字节）
    pub total_size: u64,
    /// 会话数量
    pub session_count: u32,
    /// 帧数量
    pub frame_count: u32,
}

impl Default for StorageUsage {
    fn default() -> Self {
        Self {
            database_size: 0,
            frames_size: 0,
            videos_size: 0,
            total_size: 0,
            session_count: 0,
            frame_count: 0,
        }
    }
}

/// 日期范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start_date: String,
    pub end_date: String,
}

/// 分析请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRequest {
    /// 会话ID
    pub session_id: Option<i64>,
    /// 帧路径列表
    pub frame_paths: Option<Vec<String>>,
    /// 分析选项
    pub options: AnalysisOptions,
}

/// 分析选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisOptions {
    /// 是否强制重新分析
    pub force_reanalyze: bool,
    /// 使用的LLM提供商
    pub provider: Option<String>,
    /// 自定义提示词
    pub custom_prompt: Option<String>,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            force_reanalyze: false,
            provider: None,
            custom_prompt: None,
        }
    }
}

/// 导出请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    /// 导出类型
    pub export_type: ExportType,
    /// 日期范围
    pub date_range: DateRange,
    /// 导出格式
    pub format: ExportFormat,
    /// 输出路径
    pub output_path: Option<String>,
}

/// 导出类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportType {
    Sessions,   // 会话数据
    Statistics, // 统计数据
    Report,     // 完整报告
}

/// 导出格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Csv,
    Pdf,
    Html,
}

/// 统计数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    /// 时间段
    pub period: StatisticsPeriod,
    /// 总工作时间（分钟）
    pub total_work_minutes: u32,
    /// 总休息时间（分钟）
    pub total_break_minutes: u32,
    /// 各类别时间分布
    pub category_distribution: Vec<CategoryTime>,
    /// 生产力评分
    pub avg_productivity_score: f32,
    /// 专注度评分
    pub avg_focus_score: f32,
    /// 最高效时段
    pub peak_hours: Vec<u8>,
    /// 关键词云
    pub keyword_cloud: Vec<KeywordFrequency>,
}

/// 统计周期
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatisticsPeriod {
    Daily,
    Weekly,
    Monthly,
    Custom(DateRange),
}

/// 类别时间
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryTime {
    pub category: ActivityCategory,
    pub minutes: u32,
    pub percentage: f32,
}

/// 关键词频率
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordFrequency {
    pub keyword: String,
    pub frequency: u32,
}

/// API响应包装
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(msg: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg),
        }
    }
}

/// 通知消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub title: String,
    pub message: String,
    pub notification_type: NotificationType,
    pub timestamp: DateTime<Utc>,
    pub actions: Vec<NotificationAction>,
}

/// 通知类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

/// 通知动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationAction {
    pub label: String,
    pub action: String,
}

/// Notion 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionConfig {
    /// 是否启用 Notion 同步
    pub enabled: bool,
    /// Notion API Token (Integration Secret)
    pub api_token: String,
    /// Notion Database ID - 存储会话记录
    pub database_id: String,
    /// 同步内容选项
    pub sync_options: NotionSyncOptions,
    /// 失败重试次数
    pub max_retries: u32,
}

impl Default for NotionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_token: String::new(),
            database_id: String::new(),
            sync_options: NotionSyncOptions::default(),
            max_retries: 3,
        }
    }
}

/// Notion 同步选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionSyncOptions {
    /// 同步会话记录
    pub sync_sessions: bool,
    /// 同步视频文件（小于5MB直接上传，大于5MB仅上传链接）
    pub sync_videos: bool,
    /// 同步每日总结
    pub sync_daily_summary: bool,
    /// 同步关键截图
    pub sync_screenshots: bool,
    /// 视频大小限制（MB）
    pub video_size_limit_mb: u32,
}

impl Default for NotionSyncOptions {
    fn default() -> Self {
        Self {
            sync_sessions: true,
            sync_videos: false,        // 默认不同步视频（文件较大）
            sync_daily_summary: false, // 默认不同步每日总结（Notion 会自动总结）
            sync_screenshots: true,
            video_size_limit_mb: 5,
        }
    }
}
