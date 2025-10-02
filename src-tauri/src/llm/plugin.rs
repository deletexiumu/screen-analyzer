// LLM插件系统 - 定义提供商接口和数据结构

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{
    de::{self, Deserializer},
    Deserialize, Serialize,
};
use serde_json::{self, Value};
use std::mem;

/// 活动标签
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityTag {
    /// 活动类别
    pub category: ActivityCategory,
    /// 置信度（0-1）
    pub confidence: f32,
    /// 关键词
    pub keywords: Vec<String>,
}

/// 活动类别（精简为6类，便于人工和AI标注）
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityCategory {
    Work,          // 工作（编程、写作、设计、数据分析、会议、规划等）
    Communication, // 沟通（聊天、邮件、视频会议等）
    Learning,      // 学习（阅读、观看教程、研究等）
    Personal,      // 个人（娱乐、购物、社交媒体、财务等）
    Idle,          // 空闲（有特殊意义，表示无活动或锁屏状态）
    Other,         // 其他（休息、运动等未分类活动）
}

/// 视频分段
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoSegment {
    /// 开始时间戳 (MM:SS格式)
    #[serde(rename = "startTimestamp")]
    pub start_timestamp: String,
    /// 结束时间戳 (MM:SS格式)
    #[serde(rename = "endTimestamp")]
    pub end_timestamp: String,
    /// 活动描述
    pub description: String,
}

/// 干扰活动
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Distraction {
    /// 开始时间
    #[serde(rename = "startTime")]
    pub start_time: String,
    /// 结束时间
    #[serde(rename = "endTime")]
    pub end_time: String,
    /// 标题
    pub title: String,
    /// 摘要
    pub summary: String,
    /// 视频摘要URL（可选）
    #[serde(rename = "videoSummaryURL")]
    pub video_summary_url: Option<String>,
}

/// 应用/网站信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSites {
    /// 主要应用/网站
    pub primary: String,
    /// 次要应用/网站列表
    #[serde(deserialize_with = "deserialize_secondary_apps")]
    pub secondary: Option<Vec<String>>,
}

/// 时间线活动卡片
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimelineCard {
    /// 开始时间
    #[serde(rename = "startTime")]
    pub start_time: String,
    /// 结束时间
    #[serde(rename = "endTime")]
    pub end_time: String,
    /// 类别
    pub category: String,
    /// 子类别
    pub subcategory: String,
    /// 标题
    pub title: String,
    /// 摘要
    pub summary: String,
    /// 详细摘要
    #[serde(rename = "detailedSummary")]
    pub detailed_summary: String,
    /// 干扰活动列表
    #[serde(default, deserialize_with = "deserialize_distractions")]
    pub distractions: Option<Vec<Distraction>>,
    /// 使用的应用和网站
    #[serde(rename = "appSites")]
    pub app_sites: AppSites,
    /// 视频预览路径（本地视频文件）
    #[serde(rename = "videoPreviewPath", skip_serializing_if = "Option::is_none")]
    pub video_preview_path: Option<String>,
}

/// 归一化时间线卡片中的字段（主要处理distractions字符串场景）
pub(crate) fn normalize_timeline_cards_value(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items.iter_mut() {
                normalize_timeline_card_value(item);
            }
        }
        Value::Object(_) => normalize_timeline_card_value(value),
        Value::String(text) => {
            if let Ok(mut inner) = serde_json::from_str::<Value>(text) {
                normalize_timeline_cards_value(&mut inner);
                *value = inner;
            }
        }
        _ => {}
    }
}

fn normalize_timeline_card_value(value: &mut Value) {
    if let Value::Object(map) = value {
        if let Some(distractions_value) = map.get_mut("distractions") {
            let original = mem::take(distractions_value);
            let normalized = normalize_distractions_value(original);
            *distractions_value = normalized;
        }
    }
}

fn normalize_distractions_value(value: Value) -> Value {
    match value {
        Value::Null => Value::Null,
        Value::Array(items) => {
            let mut normalized = Vec::new();
            for item in items {
                match item {
                    Value::Null => {}
                    Value::Object(_) => normalized.push(item),
                    Value::String(text) => {
                        if let Some(parsed) = parse_distraction_text(&text) {
                            for distraction in parsed {
                                if let Ok(val) = serde_json::to_value(distraction) {
                                    normalized.push(val);
                                }
                            }
                        }
                    }
                    Value::Array(nested) => {
                        let nested_value = normalize_distractions_value(Value::Array(nested));
                        match nested_value {
                            Value::Array(nested_items) => {
                                normalized.extend(nested_items);
                            }
                            Value::Null => {}
                            other => normalized.push(other),
                        }
                    }
                    other => {
                        if let Some(parsed) = parse_distraction_text(&other.to_string()) {
                            for distraction in parsed {
                                if let Ok(val) = serde_json::to_value(distraction) {
                                    normalized.push(val);
                                }
                            }
                        }
                    }
                }
            }
            Value::Array(normalized)
        }
        Value::Object(_) => value,
        Value::String(text) => {
            if let Some(parsed) = parse_distraction_text(&text) {
                serde_json::to_value(parsed).unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        }
        other => {
            if let Some(parsed) = parse_distraction_text(&other.to_string()) {
                serde_json::to_value(parsed).unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        }
    }
}

fn deserialize_secondary_apps<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;

    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            // 如果是单个字符串，转换为包含一个元素的数组
            Ok(Some(vec![s]))
        }
        Some(Value::Array(arr)) => {
            // 如果是数组，提取所有字符串元素
            let strings: Vec<String> = arr
                .into_iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s),
                    _ => None,
                })
                .collect();

            if strings.is_empty() {
                Ok(None)
            } else {
                Ok(Some(strings))
            }
        }
        _ => Ok(None),
    }
}

fn deserialize_distractions<'de, D>(deserializer: D) -> Result<Option<Vec<Distraction>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;

    match value {
        None => Ok(None),
        Some(Value::Null) => Ok(None),
        Some(raw) => {
            let mut distractions = Vec::new();
            collect_distractions_from_value(raw, &mut distractions).map_err(de::Error::custom)?;

            Ok(Some(distractions))
        }
    }
}

fn collect_distractions_from_value(value: Value, acc: &mut Vec<Distraction>) -> Result<(), String> {
    match value {
        Value::Null => Ok(()),
        Value::Array(items) => {
            for item in items {
                collect_distractions_from_value(item, acc)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            let distraction: Distraction =
                serde_json::from_value(Value::Object(map)).map_err(|err| err.to_string())?;
            acc.push(distraction);
            Ok(())
        }
        Value::String(text) => {
            if let Some(mut parsed) = parse_distraction_text(&text) {
                acc.append(&mut parsed);
            }
            Ok(())
        }
        other => Err(format!("无法解析distractions字段: {}", other)),
    }
}

fn parse_distraction_text(raw: &str) -> Option<Vec<Distraction>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lowered = trimmed.to_ascii_lowercase();
    if matches!(trimmed, "无" | "无干扰" | "没有") || lowered == "none" {
        return Some(Vec::new());
    }

    let mut results = Vec::new();
    for segment in trimmed.split(|c| matches!(c, '；' | ';' | '、' | '\n')) {
        let cleaned = segment
            .trim()
            .trim_end_matches(['。', '.', '！', '!'])
            .trim();

        if cleaned.is_empty() {
            continue;
        }

        if let Some(distraction) = build_distraction_from_segment(cleaned) {
            results.push(distraction);
        }
    }

    if results.is_empty() {
        build_distraction_from_segment(trimmed).map(|d| vec![d])
    } else {
        Some(results)
    }
}

fn build_distraction_from_segment(segment: &str) -> Option<Distraction> {
    let cleaned = segment.trim();
    if cleaned.is_empty() {
        return None;
    }

    let mut title_part = cleaned;
    let mut time_part: Option<&str> = None;

    for (open, close) in [('（', '）'), ('(', ')')] {
        if title_part.ends_with(close) {
            if let Some(open_idx) = title_part.rfind(open) {
                let close_len = close.len_utf8();
                let open_len = open.len_utf8();
                if open_idx + open_len <= title_part.len() && close_len <= title_part.len() {
                    let start = open_idx + open_len;
                    let end = title_part.len() - close_len;
                    if start <= end {
                        let range = &title_part[start..end];
                        if !range.trim().is_empty() {
                            time_part = Some(range.trim());
                        }
                    }
                }
                title_part = title_part[..open_idx].trim();
                break;
            }
        }
    }

    let title = if title_part.is_empty() {
        cleaned.to_string()
    } else {
        title_part.to_string()
    };

    let (start_time, end_time) = parse_time_range(time_part);

    Some(Distraction {
        start_time,
        end_time,
        title,
        summary: cleaned.to_string(),
        video_summary_url: None,
    })
}

fn parse_time_range(range: Option<&str>) -> (String, String) {
    if let Some(raw_range) = range {
        let normalized = raw_range
            .replace('～', "-")
            .replace('~', "-")
            .replace('—', "-")
            .replace('－', "-")
            .replace('：', ":");

        let parts: Vec<&str> = normalized
            .split('-')
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();

        if !parts.is_empty() {
            let start = normalize_timestamp(parts.first().unwrap());
            let end = if parts.len() >= 2 {
                normalize_timestamp(parts.last().unwrap())
            } else {
                start.clone()
            };
            return (start, end);
        }
    }

    ("00:00".to_string(), "00:00".to_string())
}

fn normalize_timestamp(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "00:00".to_string();
    }

    let ascii = trimmed.replace('：', ":");

    if ascii.contains(':') {
        let parts = ascii.split(':').collect::<Vec<&str>>();
        if parts.len() == 2 {
            let minutes = parts[0].trim().parse::<u32>().unwrap_or(0);
            let seconds = parts[1].trim().parse::<u32>().unwrap_or(0);
            return format!("{:02}:{:02}", minutes, seconds);
        }
        if parts.len() == 3 {
            let hours = parts[0].trim().parse::<u32>().unwrap_or(0);
            let minutes = parts[1].trim().parse::<u32>().unwrap_or(0);
            let seconds = parts[2].trim().parse::<u32>().unwrap_or(0);
            return format!("{:02}:{:02}:{:02}", hours, minutes, seconds);
        }
        return ascii;
    }

    if let Ok(minutes) = ascii.parse::<u32>() {
        return format!("{:02}:00", minutes);
    }

    "00:00".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_app_sites_deserialization() {
        // 测试数组格式
        let json_array = json!({
            "primary": "vscode",
            "secondary": ["github.com", "database-manager", "chat-app"]
        });

        let app_sites: AppSites = serde_json::from_value(json_array).unwrap();
        assert_eq!(app_sites.primary, "vscode");
        assert!(app_sites.secondary.is_some());
        assert_eq!(app_sites.secondary.unwrap().len(), 3);

        // 测试字符串格式
        let json_string = json!({
            "primary": "browser",
            "secondary": "google.com"
        });

        let app_sites: AppSites = serde_json::from_value(json_string).unwrap();
        assert_eq!(app_sites.primary, "browser");
        assert!(app_sites.secondary.is_some());
        assert_eq!(app_sites.secondary.unwrap().len(), 1);

        // 测试 null 格式
        let json_null = json!({
            "primary": "terminal",
            "secondary": null
        });

        let app_sites: AppSites = serde_json::from_value(json_null).unwrap();
        assert_eq!(app_sites.primary, "terminal");
        assert!(app_sites.secondary.is_none());
    }

    #[test]
    fn test_parse_string_distraction() {
        let json = r#"[
            {
                "startTime": "10:00 AM",
                "endTime": "10:15 AM",
                "category": "Work",
                "subcategory": "Coding",
                "title": "Python代码开发与调试",
                "summary": "编写和调试涉及API调用与文件上传功能的Python代码，并处理SSL连接异常。",
                "detailedSummary": "用户在VS Code中编辑Python代码，实现API调用和文件上传功能，参考阿里云文档进行开发。运行代码后出现SSL证书验证失败的错误，开始排查HTTP连接问题。期间短暂切换至Google Gemini浏览器查询如何在Python中忽略SSL检查，以寻找解决方案。",
                "distractions": [
                  "查阅Google Gemini搜索结果以解决SSL问题"
                ],
                "appSites": {
                  "primary": "vscode",
                  "secondary": "github.com, gemini.google.com"
                }
            }
        ]"#;

        let cards: Vec<TimelineCard> = serde_json::from_str(json).expect("解析失败");
        assert_eq!(cards.len(), 1);
        let distractions = cards[0].distractions.as_ref().unwrap();
        assert_eq!(distractions.len(), 1);
        assert!(distractions[0].summary.contains("查阅Google"));
    }
}

impl ActivityCategory {
    /// 获取类别的中文名称
    pub fn to_chinese(&self) -> &str {
        match self {
            Self::Work => "工作",
            Self::Communication => "沟通",
            Self::Learning => "学习",
            Self::Personal => "个人",
            Self::Idle => "空闲",
            Self::Other => "其他",
        }
    }

    /// 获取类别的颜色（用于UI显示）
    pub fn color(&self) -> &str {
        match self {
            Self::Work => "#409EFF",          // 蓝色（专业工作）
            Self::Communication => "#FFC107", // 黄色（沟通交流）
            Self::Learning => "#67C23A",      // 绿色（学习成长）
            Self::Personal => "#FF69B4",      // 粉色（个人活动）
            Self::Idle => "#909399",          // 灰色（空闲状态）
            Self::Other => "#6C757D",         // 深灰（其他活动）
        }
    }
}

/// 关键时刻
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyMoment {
    /// 时间点（格式: "HH:MM:SS"）
    pub time: String,
    /// 描述
    pub description: String,
    /// 重要性（1-5）
    pub importance: u8,
}

/// 用于每日总结的会话简要信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionBrief {
    /// 会话开始时间
    pub start_time: DateTime<Utc>,
    /// 会话结束时间
    pub end_time: DateTime<Utc>,
    /// 会话标题
    pub title: String,
    /// 会话摘要
    pub summary: String,
}

/// 会话总结
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionSummary {
    /// 标题
    pub title: String,
    /// 摘要
    pub summary: String,
    /// 活动标签
    pub tags: Vec<ActivityTag>,
    /// 开始时间
    pub start_time: DateTime<Utc>,
    /// 结束时间
    pub end_time: DateTime<Utc>,
    /// 关键时刻
    pub key_moments: Vec<KeyMoment>,
    /// 生产力评分（0-100）
    pub productivity_score: Option<f32>,
    /// 专注度评分（0-100）
    pub focus_score: Option<f32>,
}

impl Default for SessionSummary {
    fn default() -> Self {
        let now = crate::storage::local_now();
        Self {
            title: "未命名会话".to_string(),
            summary: "".to_string(),
            tags: vec![],
            start_time: now,
            end_time: now,
            key_moments: vec![],
            productivity_score: None,
            focus_score: None,
        }
    }
}

/// LLM提供商接口
#[async_trait]
pub trait LLMProvider: Send + Sync + std::any::Any {
    /// 转换为Any trait（用于向下转型）
    fn as_any(&mut self) -> &mut dyn std::any::Any;
    /// 分析屏幕截图帧（旧接口，保持兼容）
    ///
    /// # 参数
    /// * `frames` - 帧图片的文件路径列表
    ///
    /// # 返回
    /// * 会话总结
    async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary>;

    /// 分析视频并分段
    ///
    /// # 参数
    /// * `frames` - 帧图片的文件路径列表
    /// * `duration` - 视频时长（分钟）
    ///
    /// # 返回
    /// * 视频分段列表
    async fn segment_video(&self, frames: Vec<String>, duration: u32) -> Result<Vec<VideoSegment>> {
        // 默认实现：调用旧接口，生成单个分段
        let summary = self.analyze_frames(frames).await?;
        Ok(vec![VideoSegment {
            start_timestamp: "00:00".to_string(),
            end_timestamp: format!("{:02}:00", duration),
            description: summary.summary,
        }])
    }

    /// 生成时间线卡片
    ///
    /// # 参数
    /// * `segments` - 视频分段列表
    /// * `previous_cards` - 之前的时间线卡片（可选）
    ///
    /// # 返回
    /// * 时间线卡片列表
    async fn generate_timeline(
        &self,
        segments: Vec<VideoSegment>,
        _previous_cards: Option<Vec<TimelineCard>>,
    ) -> Result<Vec<TimelineCard>> {
        // 默认实现：将segment转换为简单的timeline卡片
        let mut cards = Vec::new();
        for segment in segments {
            cards.push(TimelineCard {
                start_time: segment.start_timestamp.clone(),
                end_time: segment.end_timestamp.clone(),
                category: "Work".to_string(),
                subcategory: "General".to_string(),
                title: "活动".to_string(),
                summary: segment.description.clone(),
                detailed_summary: segment.description.clone(),
                distractions: None,
                app_sites: AppSites {
                    primary: "unknown".to_string(),
                    secondary: None,
                },
                video_preview_path: None,
            });
        }
        Ok(cards)
    }

    /// 获取提供商名称
    fn name(&self) -> &str;

    /// 配置提供商
    ///
    /// # 参数
    /// * `config` - JSON格式的配置
    fn configure(&mut self, config: serde_json::Value) -> Result<()>;

    /// 检查提供商是否已配置
    fn is_configured(&self) -> bool;

    /// 获取支持的功能
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    /// 生成每日总结文本
    ///
    /// # 参数
    /// * `date` - 日期 (YYYY-MM-DD)
    /// * `sessions` - 当天的所有会话简要信息（包含时间段和摘要）
    ///
    /// # 返回
    /// * 生成的总结文本
    async fn generate_day_summary(
        &self,
        _date: &str,
        _sessions: &[SessionBrief],
    ) -> Result<String> {
        // 默认实现：返回简单的基于规则的总结
        let total_minutes: i64 = _sessions
            .iter()
            .map(|s| (s.end_time - s.start_time).num_minutes())
            .sum();
        Ok(format!(
            "今天共记录了 {} 个工作会话，总计 {} 分钟。",
            _sessions.len(),
            total_minutes
        ))
    }
}

/// 提供商能力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// 是否支持视觉分析
    pub vision_support: bool,
    /// 是否支持批量分析
    pub batch_analysis: bool,
    /// 是否支持流式响应
    pub streaming: bool,
    /// 最大输入token数
    pub max_input_tokens: usize,
    /// 支持的图片格式
    pub supported_image_formats: Vec<String>,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            vision_support: true,
            batch_analysis: true,
            streaming: false,
            max_input_tokens: 128000,
            supported_image_formats: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
                "gif".to_string(),
                "webp".to_string(),
            ],
        }
    }
}

/// 分析请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRequest {
    /// 帧文件路径
    pub frames: Vec<String>,
    /// 分析选项
    pub options: AnalysisOptions,
}

/// 分析选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisOptions {
    /// 语言（zh-CN, en-US等）
    pub language: String,
    /// 详细程度（简单、标准、详细）
    pub detail_level: DetailLevel,
    /// 是否包含技术细节
    pub include_technical_details: bool,
    /// 自定义提示词
    pub custom_prompt: Option<String>,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            language: "zh-CN".to_string(),
            detail_level: DetailLevel::Standard,
            include_technical_details: false,
            custom_prompt: None,
        }
    }
}

/// 详细程度
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetailLevel {
    Simple,   // 简单
    Standard, // 标准
    Detailed, // 详细
}
