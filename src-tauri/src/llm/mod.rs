// LLM模块 - 管理AI分析服务

pub mod plugin;
pub mod qwen;

pub use plugin::{
    ActivityCategory, ActivityTag, AppSites, Distraction, KeyMoment, LLMProvider, SessionSummary,
    TimelineCard, VideoSegment,
};
pub use qwen::QwenProvider;

use crate::settings::SettingsManager;
use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};

/// LLM管理器
pub struct LLMManager {
    /// Qwen提供商实例
    provider: QwenProvider,
    /// 配置锁
    config_lock: Arc<RwLock<LLMConfig>>,
}

/// LLM配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LLMConfig {
    /// Qwen配置
    pub qwen: QwenConfig,
    /// 分析参数
    pub analysis_params: AnalysisParams,
}

/// Qwen配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QwenConfig {
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_video_mode")]
    pub use_video_mode: bool, // 是否使用视频模式处理多张图片
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_path: Option<String>, // 可选的视频文件路径
}

fn default_model() -> String {
    "qwen-vl-max-latest".to_string()
}

fn default_base_url() -> String {
    "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string()
}

fn default_video_mode() -> bool {
    true
}

/// 分析参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalysisParams {
    /// 帧采样间隔（秒）
    pub frame_sampling_interval: u64,
    /// 最大分析帧数
    pub max_frames_per_analysis: usize,
    /// 是否包含详细描述
    pub include_detailed_description: bool,
    /// 置信度阈值
    pub confidence_threshold: f32,
}

impl Default for AnalysisParams {
    fn default() -> Self {
        Self {
            frame_sampling_interval: 30, // 每30秒采样一帧
            max_frames_per_analysis: 30, // 最多30帧
            include_detailed_description: true,
            confidence_threshold: 0.5,
        }
    }
}

impl LLMManager {
    /// 创建新的LLM管理器
    pub fn new() -> Self {
        Self {
            provider: QwenProvider::new(),
            config_lock: Arc::new(RwLock::new(LLMConfig {
                qwen: QwenConfig {
                    api_key: String::new(),
                    model: default_model(),
                    base_url: default_base_url(),
                    use_video_mode: default_video_mode(),
                    video_path: None,
                },
                analysis_params: AnalysisParams::default(),
            })),
        }
    }

    /// 配置Qwen
    pub async fn configure(&mut self, config: QwenConfig) -> Result<()> {
        // 如果有视频路径，设置到provider
        if let Some(ref video_path) = config.video_path {
            if let Some(provider) = self.provider.as_any().downcast_mut::<QwenProvider>() {
                provider.set_video_path(Some(video_path.clone()));
            }
        }

        // 更新provider配置
        self.provider.configure(serde_json::to_value(&config)?)?;

        // 更新配置锁
        let mut current_config = self.config_lock.write().await;
        current_config.qwen = config;

        info!("Qwen配置已更新");
        Ok(())
    }

    pub fn set_video_path(&mut self, video_path: Option<String>) {
        if let Some(provider) = self.provider.as_any().downcast_mut::<QwenProvider>() {
            provider.set_video_path(video_path);
        }
    }

    /// 设置视频速率乘数
    pub fn set_video_speed(&mut self, speed_multiplier: f32) {
        if let Some(provider) = self.provider.as_any().downcast_mut::<QwenProvider>() {
            provider.set_video_speed(speed_multiplier);
        }
    }

    /// 分析帧数据
    pub async fn analyze_frames(&mut self, frames: Vec<String>) -> Result<SessionSummary> {
        info!("使用Qwen分析 {} 帧", frames.len());

        match self.provider.analyze_frames(frames).await {
            Ok(summary) => {
                info!("分析成功: {}", summary.title);
                Ok(summary)
            }
            Err(e) => {
                error!("分析失败: {}", e);
                Err(e)
            }
        }
    }

    /// 更新配置
    pub async fn update_config(&self, config: LLMConfig) -> Result<()> {
        let mut current_config = self.config_lock.write().await;
        *current_config = config;
        info!("LLM配置已更新");
        Ok(())
    }

    /// 获取当前配置
    pub async fn get_config(&self) -> LLMConfig {
        self.config_lock.read().await.clone()
    }

    /// 设置provider的数据库连接
    pub fn set_provider_database(
        &mut self,
        db: Arc<crate::storage::Database>,
        session_id: Option<i64>,
    ) {
        self.provider.set_database(db);
        if let Some(sid) = session_id {
            self.provider.set_session_id(sid);
        }
        info!("为Qwen provider设置数据库连接");
    }

    /// 生成时间线卡片（公开方法）
    pub async fn generate_timeline(
        &mut self,
        segments: Vec<VideoSegment>,
        previous_cards: Option<Vec<TimelineCard>>,
    ) -> Result<Vec<TimelineCard>> {
        self.provider
            .generate_timeline(segments, previous_cards)
            .await
    }

    /// 获取最后一次LLM调用的ID
    pub fn get_last_call_id(&self, call_type: &str) -> Option<i64> {
        self.provider.last_llm_call_id(call_type)
    }

    /// 分析视频并生成时间线（两阶段处理）
    pub async fn segment_video_and_generate_timeline(
        &mut self,
        frames: Vec<String>,
        duration: u32,
        previous_cards: Option<Vec<TimelineCard>>,
    ) -> Result<TimelineAnalysis> {
        info!(
            "使用Qwen进行视频分段分析: {} 帧, 时长 {} 分钟",
            frames.len(),
            duration
        );

        // 第一阶段：分段视频
        let segments = match self.provider.segment_video(frames.clone(), duration).await {
            Ok(segs) => {
                info!("视频分段成功: {} 个segment", segs.len());
                segs
            }
            Err(e) => {
                error!("视频分段失败: {}", e);
                // 检查是否是视频过短错误，如果是则保留特殊标记
                if e.to_string().contains("VIDEO_TOO_SHORT") {
                    return Err(anyhow::anyhow!("VIDEO_TOO_SHORT: {}", e));
                }
                return Err(e);
            }
        };

        // 第二阶段：生成时间线
        let timeline_cards = match self
            .provider
            .generate_timeline(segments.clone(), previous_cards)
            .await
        {
            Ok(cards) => {
                info!("时间线生成成功: {} 个卡片", cards.len());
                cards
            }
            Err(e) => {
                error!("时间线生成失败: {}", e);
                return Err(e);
            }
        };

        let segment_call_id = self.provider.last_llm_call_id("segment_video");
        let timeline_call_id = self.provider.last_llm_call_id("generate_timeline");

        Ok(TimelineAnalysis {
            segments,
            timeline_cards,
            segment_call_id,
            timeline_call_id,
        })
    }
}

pub fn build_session_summary(
    window_start: chrono::DateTime<chrono::Utc>,
    window_end: chrono::DateTime<chrono::Utc>,
    segments: &[VideoSegment],
    timeline_cards: &[TimelineCard],
) -> SessionSummary {
    use std::collections::HashMap;

    // 统计各个类别的时间占比
    let mut category_duration: HashMap<String, f32> = HashMap::new();
    let mut total_duration = 0.0f32;

    // 遍历所有timeline cards计算时间
    for card in timeline_cards {
        // 解析时间并计算持续时间（简化处理，假设MM:SS格式）
        let duration = parse_duration(&card.start_time, &card.end_time).unwrap_or(15.0);
        total_duration += duration;

        let category = card.category.to_lowercase();
        *category_duration.entry(category).or_insert(0.0) += duration;
    }

    // 生成标签列表（最多3个，按比重排序）
    let mut tags: Vec<ActivityTag> = Vec::new();

    if total_duration > 0.0 {
        // 计算每个类别的比重并排序
        let mut category_weights: Vec<(String, f32)> = category_duration
            .into_iter()
            .map(|(cat, duration)| (cat, duration / total_duration))
            .collect();

        category_weights.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // 取前3个类别生成标签
        for (category_str, weight) in category_weights.iter().take(3) {
            // 只添加比重超过10%的标签
            if *weight < 0.1 {
                break;
            }

            let category = match category_str.as_str() {
                "work" | "coding" | "writing" | "design" | "planning" | "data_analysis" => {
                    ActivityCategory::Work
                }
                "communication" | "meeting" => ActivityCategory::Communication,
                "learning" | "research" => ActivityCategory::Learning,
                "personal" | "entertainment" | "social_media" | "shopping" | "finance" => {
                    ActivityCategory::Personal
                }
                "idle" => ActivityCategory::Idle,
                _ => ActivityCategory::Other,
            };

            // 根据类别收集关键词
            let keywords = timeline_cards
                .iter()
                .filter(|card| card.category.to_lowercase() == *category_str)
                .map(|card| card.subcategory.clone())
                .collect::<Vec<_>>();

            tags.push(ActivityTag {
                category,
                confidence: *weight, // 使用时间占比作为confidence
                keywords,
            });
        }
    }

    // 如果没有有效标签，至少添加一个默认标签
    if tags.is_empty() && !timeline_cards.is_empty() {
        let first_card = &timeline_cards[0];
        tags.push(ActivityTag {
            category: map_category(&first_card.category),
            confidence: 1.0,
            keywords: vec![first_card.subcategory.clone()],
        });
    }

    // 生成总结
    let title = timeline_cards
        .first()
        .map(|c| c.title.clone())
        .unwrap_or_else(|| "活动会话".to_string());

    let summary = if timeline_cards.len() > 1 {
        format!(
            "本次会话包含{}个主要活动阶段。{}",
            timeline_cards.len(),
            timeline_cards
                .iter()
                .map(|c| c.title.clone())
                .collect::<Vec<_>>()
                .join("、")
        )
    } else {
        timeline_cards
            .first()
            .map(|c| c.detailed_summary.clone())
            .unwrap_or_else(|| {
                segments
                    .first()
                    .map(|s| s.description.clone())
                    .unwrap_or_default()
            })
    };

    SessionSummary {
        title,
        summary,
        tags,
        start_time: window_start,
        end_time: window_end,
        key_moments: segments
            .iter()
            .map(|seg| KeyMoment {
                time: seg.start_timestamp.clone(),
                description: seg.description.clone(),
                importance: 3,
            })
            .collect(),
        productivity_score: Some(75.0),
        focus_score: Some(80.0),
    }
}

// 辅助函数：解析持续时间（MM:SS格式）
fn parse_duration(start: &str, end: &str) -> Option<f32> {
    // 简单处理MM:SS格式
    let parse_time = |s: &str| -> Option<f32> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 2 {
            let minutes = parts[0].parse::<f32>().ok()?;
            let seconds = parts[1].parse::<f32>().ok()?;
            Some(minutes * 60.0 + seconds)
        } else {
            None
        }
    };

    let start_seconds = parse_time(start)?;
    let end_seconds = parse_time(end)?;

    Some((end_seconds - start_seconds).abs())
}

// 辅助函数：映射类别
fn map_category(category_str: &str) -> ActivityCategory {
    match category_str.to_lowercase().as_str() {
        "work" | "coding" | "writing" | "design" | "planning" | "data_analysis" => {
            ActivityCategory::Work
        }
        "communication" | "meeting" => ActivityCategory::Communication,
        "learning" | "research" => ActivityCategory::Learning,
        "personal" | "entertainment" | "social_media" | "shopping" | "finance" => {
            ActivityCategory::Personal
        }
        "idle" => ActivityCategory::Idle,
        _ => ActivityCategory::Other,
    }
}

fn parse_relative_duration(label: &str) -> Option<chrono::Duration> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Some(chrono::Duration::zero());
    }

    let mut total_seconds: i64 = 0;
    let mut multiplier: i64 = 1;

    for part in trimmed.split(':').rev() {
        let component = part.trim();
        if component.is_empty() {
            continue;
        }
        let integer_part = component.split('.').next().unwrap_or(component).trim();
        let value = integer_part.parse::<i64>().ok()?;
        total_seconds = total_seconds.saturating_add(value.saturating_mul(multiplier));
        multiplier = multiplier.saturating_mul(60);
    }

    Some(chrono::Duration::seconds(total_seconds))
}

pub(crate) fn relative_to_absolute(
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
    relative: &str,
) -> chrono::DateTime<chrono::Utc> {
    let offset = parse_relative_duration(relative).unwrap_or_else(|| chrono::Duration::zero());
    let mut ts = start + offset;
    if ts > end {
        ts = end;
    }
    if ts < start {
        ts = start;
    }
    ts
}

/// LLM处理器（实现SessionProcessor trait）
pub struct LLMProcessor {
    manager: Arc<Mutex<LLMManager>>,
    db: Arc<crate::storage::Database>,
    video_processor: Option<Arc<crate::video::VideoProcessor>>,
    settings: Arc<SettingsManager>,
}

/// LLM两阶段分析的聚合结果
pub struct TimelineAnalysis {
    pub segments: Vec<VideoSegment>,
    pub timeline_cards: Vec<TimelineCard>,
    pub segment_call_id: Option<i64>,
    pub timeline_call_id: Option<i64>,
}

impl LLMProcessor {
    pub fn new(
        manager: Arc<Mutex<LLMManager>>,
        db: Arc<crate::storage::Database>,
        settings: Arc<SettingsManager>,
    ) -> Self {
        Self {
            manager,
            db,
            video_processor: None,
            settings,
        }
    }

    pub fn with_video_processor(
        manager: Arc<Mutex<LLMManager>>,
        db: Arc<crate::storage::Database>,
        video_processor: Arc<crate::video::VideoProcessor>,
        settings: Arc<SettingsManager>,
    ) -> Self {
        Self {
            manager,
            db,
            video_processor: Some(video_processor),
            settings,
        }
    }
}

#[async_trait::async_trait]
impl crate::capture::scheduler::SessionProcessor for LLMProcessor {
    async fn process_session(
        &self,
        frames: Vec<crate::capture::ScreenFrame>,
        window: crate::capture::scheduler::SessionWindow,
    ) -> Result<()> {
        // 获取配置
        let config = {
            let manager = self.manager.lock().await;
            manager.get_config().await
        };
        let params = &config.analysis_params;

        // 采样帧
        let sampled_frames = self.sample_frames(&frames, params.frame_sampling_interval as usize);

        // 提取文件路径
        let frame_paths: Vec<String> = sampled_frames.iter().map(|f| f.file_path.clone()).collect();

        // 计算视频时长（分钟）
        let duration = window.end - window.start;
        let duration_minutes = (duration.num_seconds().max(0) as f64 / 60.0).ceil() as u32;

        // 提取所有帧路径用于视频生成
        let all_frame_paths: Vec<String> = frames.iter().map(|f| f.file_path.clone()).collect();

        // 先生成视频（如果配置了视频处理器）
        let mut video_path = None;
        let mut should_persist_frames = true;
        if let Some(ref video_processor) = self.video_processor {
            let app_config = self.settings.get().await;
            if app_config.video_config.auto_generate {
                info!("自动生成会话视频...");

                let file_label = format!(
                    "{}-{}",
                    window.start.format("%Y%m%d%H%M"),
                    window.end.format("%Y%m%d%H%M")
                );
                let output_path = video_processor
                    .output_dir
                    .join(format!("{}.mp4", file_label));

                let mut video_config = crate::video::VideoConfig::default();
                video_config.speed_multiplier = app_config.video_config.speed_multiplier;
                video_config.quality = app_config.video_config.quality;
                video_config.add_timestamp = app_config.video_config.add_timestamp;

                match video_processor
                    .create_summary_video(all_frame_paths.clone(), &output_path, &video_config)
                    .await
                {
                    Ok(result) => {
                        info!("视频生成成功: {}", result.file_path);
                        video_path = Some(result.file_path.clone());
                        should_persist_frames = false;

                        info!("删除 {} 个原始图片文件...", all_frame_paths.len());
                        for frame_path in &all_frame_paths {
                            if let Err(e) = tokio::fs::remove_file(frame_path).await {
                                error!("删除图片文件失败 {}: {}", frame_path, e);
                            }
                        }
                        info!("原始图片文件已删除");
                    }
                    Err(e) => {
                        error!("视频生成失败: {}，保留原始图片", e);
                    }
                }
            } else {
                info!("自动生成视频已关闭，跳过视频生成");
            }
        }

        // 检查是否有帧，如果没有帧则不创建会话
        if frame_paths.is_empty() {
            warn!("该时间段没有截图帧，跳过会话创建");
            return Err(anyhow!("没有找到截图帧，无法创建会话"));
        }

        // 先创建会话获取session_id（用于关联LLM调用记录）
        let temp_session = crate::storage::Session {
            id: None,
            start_time: window.start,
            end_time: window.end,
            title: "处理中...".to_string(),
            summary: "正在分析...".to_string(),
            video_path: video_path.clone(), // 如果已生成视频，这里就有路径了
            tags: "[]".to_string(),
            created_at: None,
        };

        let session_id = self.db.insert_session(&temp_session).await?;
        info!("创建临时会话: ID={}", session_id);

        // 记录视频路径，用于错误清理
        let video_path_for_cleanup = video_path.clone();

        // 更新provider的视频路径
        {
            let mut manager = self.manager.lock().await;
            manager.set_video_path(video_path.clone());
        }

        // 设置provider的数据库连接和session_id
        {
            let mut manager = self.manager.lock().await;
            manager.set_provider_database(self.db.clone(), Some(session_id));

            // 设置视频速率乘数
            let app_config = self.settings.get().await;
            let speed_multiplier = app_config.video_config.speed_multiplier;
            manager.set_video_speed(speed_multiplier);
        }

        // 使用两阶段分析：先分段，再生成时间线
        let analysis = {
            let mut manager = self.manager.lock().await;
            match manager
                .segment_video_and_generate_timeline(frame_paths, duration_minutes, None)
                .await {
                Ok(result) => result,
                Err(e) => {
                    // 如果是视频过短错误，清理已创建的资源
                    if e.to_string().contains("VIDEO_TOO_SHORT") {
                        error!("检测到视频过短错误，开始清理资源...");

                        // 1. 删除数据库中的会话记录
                        if let Err(del_err) = self.db.delete_session(session_id).await {
                            error!("删除会话失败 (ID={}): {}", session_id, del_err);
                        } else {
                            info!("已删除会话记录: ID={}", session_id);
                        }

                        // 2. 删除视频文件（如果存在）
                        if let Some(ref vp) = video_path_for_cleanup {
                            if let Err(del_err) = tokio::fs::remove_file(vp).await {
                                error!("删除视频文件失败 {}: {}", vp, del_err);
                            } else {
                                info!("已删除视频文件: {}", vp);
                            }
                        }
                    }

                    return Err(e);
                }
            }
        };

        let TimelineAnalysis {
            mut segments,
            mut timeline_cards,
            segment_call_id,
            timeline_call_id,
        } = analysis;

        for segment in &mut segments {
            let start_abs =
                relative_to_absolute(window.start, window.end, &segment.start_timestamp);
            let end_abs = relative_to_absolute(window.start, window.end, &segment.end_timestamp);
            segment.start_timestamp = start_abs.to_rfc3339();
            segment.end_timestamp = end_abs.to_rfc3339();
        }

        for card in &mut timeline_cards {
            let start_abs = relative_to_absolute(window.start, window.end, &card.start_time);
            let end_abs = relative_to_absolute(window.start, window.end, &card.end_time);
            card.start_time = start_abs.to_rfc3339();
            card.end_time = end_abs.to_rfc3339();

            if let Some(distractions) = card.distractions.as_mut() {
                for distraction in distractions {
                    let d_start =
                        relative_to_absolute(window.start, window.end, &distraction.start_time);
                    let d_end =
                        relative_to_absolute(window.start, window.end, &distraction.end_time);
                    distraction.start_time = d_start.to_rfc3339();
                    distraction.end_time = d_end.to_rfc3339();
                }
            }
        }

        // 保存segments到数据库
        if !segments.is_empty() {
            let segment_records: Vec<crate::storage::VideoSegmentRecord> = segments
                .iter()
                .map(|seg| crate::storage::VideoSegmentRecord {
                    id: None,
                    session_id,
                    llm_call_id: segment_call_id,
                    start_timestamp: seg.start_timestamp.clone(),
                    end_timestamp: seg.end_timestamp.clone(),
                    description: seg.description.clone(),
                    created_at: chrono::Utc::now(),
                })
                .collect();

            self.db.insert_video_segments(&segment_records).await?;
            info!("保存了 {} 个视频分段", segment_records.len());
        }

        // 保存timeline cards到数据库
        if !timeline_cards.is_empty() {
            let card_records: Vec<crate::storage::TimelineCardRecord> = timeline_cards
                .iter()
                .map(|card| {
                    crate::storage::TimelineCardRecord {
                        id: None,
                        session_id,
                        llm_call_id: timeline_call_id,
                        start_time: card.start_time.clone(),
                        end_time: card.end_time.clone(),
                        category: card.category.clone(),
                        subcategory: card.subcategory.clone(),
                        title: card.title.clone(),
                        summary: card.summary.clone(),
                        detailed_summary: card.detailed_summary.clone(),
                        distractions: card
                            .distractions
                            .as_ref()
                            .map(|d| serde_json::to_string(d).unwrap_or_default()),
                        app_sites: serde_json::to_string(&card.app_sites).unwrap_or_default(),
                        video_preview_path: video_path.clone(), // 使用已生成的视频路径
                        created_at: chrono::Utc::now(),
                    }
                })
                .collect();

            self.db.insert_timeline_cards(&card_records).await?;
            info!("保存了 {} 个时间线卡片", card_records.len());
        }

        // 从timeline卡片生成总结（使用第一个卡片的信息）
        let summary = build_session_summary(window.start, window.end, &segments, &timeline_cards);

        // 更新会话信息（之前已经创建了临时会话）
        sqlx::query(
            r#"
            UPDATE sessions
            SET title = ?1, summary = ?2, video_path = ?3, tags = ?4
            WHERE id = ?5
            "#,
        )
        .bind(&summary.title)
        .bind(&summary.summary)
        .bind(&video_path)
        .bind(serde_json::to_string(&summary.tags)?)
        .bind(session_id)
        .execute(self.db.get_pool())
        .await?;

        // 保存帧数据（如果没有生成视频则保存路径，否则路径已被删除）
        if should_persist_frames {
            let db_frames: Vec<crate::storage::Frame> = frames
                .iter()
                .map(|f| crate::storage::Frame {
                    id: None,
                    session_id,
                    timestamp: f.timestamp,
                    file_path: f.file_path.clone(),
                })
                .collect();

            self.db.insert_frames(&db_frames).await?;
        }

        info!(
            "会话已保存到数据库: ID={}, 标题={}",
            session_id, summary.title
        );

        // 清理provider的视频路径，避免影响后续会话
        {
            let mut manager = self.manager.lock().await;
            manager.set_video_path(None);
        }
        Ok(())
    }
}

impl LLMProcessor {
    /// 采样帧数据
    fn sample_frames(
        &self,
        frames: &[crate::capture::ScreenFrame],
        interval: usize,
    ) -> Vec<crate::capture::ScreenFrame> {
        if frames.is_empty() {
            return vec![];
        }

        let mut sampled = vec![frames[0].clone()]; // 始终包含第一帧

        for i in (interval..frames.len()).step_by(interval) {
            sampled.push(frames[i].clone());
        }

        // 如果最后一帧没有被包含，添加它
        if sampled.last().unwrap().timestamp != frames.last().unwrap().timestamp {
            sampled.push(frames.last().unwrap().clone());
        }

        sampled
    }
}
