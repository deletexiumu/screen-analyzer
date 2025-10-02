// 屏幕活动分析器 - Tauri应用主库

// 声明模块
pub mod actors;
pub mod capture;
pub mod domains;
pub mod event_bus;
pub mod llm;
pub mod logger;
pub mod models;
pub mod settings;
pub mod storage;
pub mod video;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;
// Actor模式不再需要Mutex和RwLock
// use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

// 导入必要的类型
use capture::{scheduler::CaptureScheduler, ScreenCapture};
use domains::{AnalysisDomain, CaptureDomain, StorageDomain, SystemDomain};
use event_bus::EventBus;
use llm::LLMManager;
use models::*;
use settings::SettingsManager;
use storage::{Database, StorageCleaner};
use video::VideoProcessor;

// 视频帧采样相关常量
/// 帧采样时间间隔（秒）
const FRAME_SAMPLE_INTERVAL_SECONDS: u32 = 5;

/// 应用状态（重构后按领域分组）
///
/// 将原本混乱的11个字段重组为4个领域管理器，实现单一职责原则
/// - 捕获领域：负责屏幕截取和调度
/// - 分析领域：负责LLM分析和视频处理
/// - 存储领域：负责数据库、存储清理和设置管理
/// - 系统领域：负责系统状态、日志和基础设施
/// - 事件总线：用于领域间解耦通信
#[derive(Clone)]
pub struct AppState {
    /// 捕获领域管理器
    pub capture_domain: Arc<CaptureDomain>,
    /// 分析领域管理器
    pub analysis_domain: Arc<AnalysisDomain>,
    /// 存储领域管理器
    pub storage_domain: Arc<StorageDomain>,
    /// 系统领域管理器
    pub system_domain: Arc<SystemDomain>,
    /// 事件总线
    pub event_bus: Arc<EventBus>,
}

/// 文件夹类型枚举（用于安全的路径访问）
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderType {
    /// 截图文件夹
    Frames,
    /// 视频文件夹
    Videos,
}

// ==================== 输入验证辅助函数 ====================

/// 验证会话ID是否有效（防止SQL注入和无效输入）
fn validate_session_id(id: i64) -> Result<(), String> {
    if id < 0 {
        return Err(format!("无效的会话 ID: {}", id));
    }
    Ok(())
}

// ==================== Tauri命令 ====================

/// 获取数据库状态
///
/// # 返回
/// - 数据库初始化状态
#[tauri::command]
async fn get_database_status(
    state: tauri::State<'_, AppState>,
) -> Result<domains::storage::DatabaseStatus, String> {
    Ok(state.storage_domain.get_db_status().await)
}

/// 获取指定日期范围内的活动列表
///
/// # 参数
/// - `state`: 应用状态
/// - `start_date`: 开始日期 (格式: YYYY-MM-DD)
/// - `end_date`: 结束日期 (格式: YYYY-MM-DD)
///
/// # 返回
/// - `Ok(Vec<Activity>)`: 活动列表
/// - `Err(String)`: 错误信息
#[tauri::command]
async fn get_activities(
    state: tauri::State<'_, AppState>,
    start_date: String,
    end_date: String,
) -> Result<Vec<Activity>, String> {
    let db = state.storage_domain.get_db().await?;
    db.get_activities(&start_date, &end_date)
        .await
        .map_err(|e| e.to_string())
}

/// 获取某天的会话列表
#[tauri::command]
async fn get_day_sessions(
    state: tauri::State<'_, AppState>,
    date: String,
) -> Result<Vec<Session>, String> {
    state
        .storage_domain.get_db().await?
        .get_sessions_by_date(&date)
        .await
        .map_err(|e| e.to_string())
}

/// 获取会话详情
#[tauri::command]
async fn get_session_detail(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<SessionDetail, String> {
    validate_session_id(session_id)?;
    state
        .storage_domain.get_db().await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())
}

/// 获取应用配置
#[tauri::command]
async fn get_app_config(state: tauri::State<'_, AppState>) -> Result<PersistedAppConfig, String> {
    Ok(state.storage_domain.get_settings().get().await)
}

/// 更新配置
#[tauri::command]
async fn update_config(
    state: tauri::State<'_, AppState>,
    config: AppConfig,
) -> Result<PersistedAppConfig, String> {
    let updated_config = state
        .storage_domain.get_settings()
        .update(config.clone())
        .await
        .map_err(|e| e.to_string())?;

    // 更新保留天数
    if let Some(retention_days) = config.retention_days {
        // 直接调用cleaner的方法，不需要获取可变引用
        state
            .storage_domain.get_cleaner().await?
            .set_retention_days(retention_days)
            .await
            .map_err(|e| e.to_string())?;
    }

    // 更新LLM配置（现在只有Qwen）
    if let Some(_llm_provider) = config.llm_provider {
        // 现在只支持Qwen，不需要切换provider
        info!("LLM服务使用Qwen");
    }

    // 更新截屏间隔
    if let Some(capture_interval) = config.capture_interval {
        // TODO: 更新调度器配置
        info!("截屏间隔更新为: {}秒", capture_interval);
    }

    // 更新总结间隔
    if let Some(summary_interval) = config.summary_interval {
        // TODO: 更新调度器配置
        info!("总结间隔更新为: {}分钟", summary_interval);
    }

    // 更新截屏配置
    if let Some(capture_settings) = config.capture_settings {
        state.capture_domain.get_capture().update_settings(capture_settings.clone()).await;
        info!("截屏配置已更新: {:?}", capture_settings);
    }

    // 更新日志配置
    if let Some(logger_settings) = config.logger_settings {
        state.system_domain.get_logger().set_enabled(logger_settings.enable_frontend_logging);
        info!("日志配置已更新: 前端日志推送 = {}", logger_settings.enable_frontend_logging);
    }

    Ok(updated_config)
}

/// 添加手动标签
#[tauri::command]
async fn add_manual_tag(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    tag: ActivityTag,
) -> Result<(), String> {
    validate_session_id(session_id)?;
    // 获取当前会话
    let session_detail = state
        .storage_domain.get_db().await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // 添加新标签
    let mut tags = session_detail.tags;
    tags.push(tag);

    // 更新数据库
    let tags_json = serde_json::to_string(&tags).map_err(|e| e.to_string())?;

    state
        .storage_domain.get_db().await?
        .update_session_tags(session_id, &tags_json)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 删除标签
#[tauri::command]
async fn remove_tag(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    tag_index: usize,
) -> Result<(), String> {
    validate_session_id(session_id)?;
    // 获取当前会话
    let session_detail = state
        .storage_domain.get_db().await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // 删除指定索引的标签
    let mut tags = session_detail.tags;
    if tag_index >= tags.len() {
        return Err("标签索引超出范围".to_string());
    }
    tags.remove(tag_index);

    // 更新数据库
    let tags_json = serde_json::to_string(&tags).map_err(|e| e.to_string())?;

    state
        .storage_domain.get_db().await?
        .update_session_tags(session_id, &tags_json)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 获取系统状态
#[tauri::command]
async fn get_system_status(state: tauri::State<'_, AppState>) -> Result<SystemStatus, String> {
    let status = state.system_domain.get_status_handle().get().await;
    Ok(status)
}

/// 切换截屏状态（暂停/恢复）
#[tauri::command]
async fn toggle_capture(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.system_domain.get_status_handle().set_capturing(enabled).await;

    if enabled {
        info!("恢复截屏");
        // TODO: 恢复调度器
    } else {
        info!("暂停截屏");
        // TODO: 暂停调度器
    }

    Ok(())
}

/// 手动触发分析 - 分析video文件夹中未分析的视频
#[tauri::command]
async fn trigger_analysis(state: tauri::State<'_, AppState>) -> Result<String, String> {
    info!("手动触发分析 - 分析视频文件");

    // 已移除 analysis_lock 临时方案，直接执行分析
    let report = analyze_unprocessed_videos(&state, None, true).await?;

    if report.total_candidates == 0 {
        return Ok("所有视频已经分析过".to_string());
    }

    let summary = format!(
        "分析完成\n\n总计: {} 个未分析视频\n成功: {} 个\n失败: {} 个\n\n详情：\n{}",
        report.total_candidates,
        report.processed,
        report.failed,
        report.messages.join("\n")
    );

    Ok(summary)
}

/// 重新分析指定会话
#[tauri::command]
async fn retry_session_analysis(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<String, String> {
    validate_session_id(session_id)?;
    info!("重新分析会话: {}", session_id);

    // 已移除 analysis_lock 临时方案，直接执行分析
    state.system_domain.get_status_handle().set_processing(true).await;
    state.system_domain.get_status_handle().set_error(None).await;

    let result = async {
        let session_detail = state
            .storage_domain.get_db().await?
            .get_session_detail(session_id)
            .await
            .map_err(|e| e.to_string())?;

        let video_path = session_detail
            .session
            .video_path
            .clone()
            .ok_or_else(|| "该会话没有关联视频，无法重新分析".to_string())?;

        let session_start = session_detail.session.start_time;
        let session_end = session_detail.session.end_time;
        let diff = session_end.signed_duration_since(session_start);
        let duration_minutes = if diff.num_seconds() > 0 {
            ((diff.num_seconds() as f64) / 60.0).ceil() as u32
        } else {
            1
        };

        // 使用 Database 方法删除和更新
        state.storage_domain.get_db().await?
            .delete_video_segments_by_session(session_id)
            .await
            .map_err(|e| e.to_string())?;

        state.storage_domain.get_db().await?
            .delete_timeline_cards_by_session(session_id)
            .await
            .map_err(|e| e.to_string())?;

        state.storage_domain.get_db().await?
            .update_session(session_id, &session_detail.session.title, "重新分析中...", None, "[]")
            .await
            .map_err(|e| e.to_string())?;

        let video_path_buf = PathBuf::from(&video_path);
        let outcome = analyze_video_once(
            &state,
            &video_path_buf,
            session_start,
            session_end,
            duration_minutes,
            Some(session_id),
        )
        .await?;

        Ok(outcome)
    }
    .await;

    let last_error = result.as_ref().err().cloned();

    state.system_domain.get_status_handle().set_processing(false).await;
    state.system_domain.get_status_handle().update_last_process_time(chrono::Utc::now()).await;
    state.system_domain.get_status_handle().set_error(last_error.clone()).await;

    match result {
        Ok(outcome) => Ok(format!(
            "重新分析完成: {} ({} 片段, {} 卡片)",
            outcome.summary.title, outcome.segments_count, outcome.timeline_count
        )),
        Err(err) => Err(err),
    }
}

fn parse_video_window_from_stem(
    stem: &str,
) -> Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
    use chrono::{NaiveDateTime, TimeZone, Utc};

    // 处理 segment_YYYYMMDDHHMMSS_YYYYMMDDHHMMSS 格式
    if stem.starts_with("segment_") {
        let parts: Vec<&str> = stem
            .strip_prefix("segment_")?
            .split('_')
            .filter(|p| !p.is_empty())
            .collect();

        if parts.len() != 2 {
            return None;
        }

        let start = parts[0];
        let end = parts[1];

        if start.len() != 12 || end.len() != 12 {
            return None;
        }

        let start_naive = NaiveDateTime::parse_from_str(start, "%Y%m%d%H%M").ok()?;
        let end_naive = NaiveDateTime::parse_from_str(end, "%Y%m%d%H%M").ok()?;

        return Some((
            Utc.from_utc_datetime(&start_naive),
            Utc.from_utc_datetime(&end_naive),
        ));
    }

    // 处理带连字符的旧格式 YYYYMMDDHHMMSS-YYYYMMDDHHMMSS
    let cleaned: String = stem
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .collect();

    let mut parts = cleaned.split('-').filter(|p| !p.is_empty());
    let start = parts.next()?;
    let end = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    if start.len() != 12 || end.len() != 12 {
        return None;
    }

    let start_naive = NaiveDateTime::parse_from_str(start, "%Y%m%d%H%M").ok()?;
    let end_naive = NaiveDateTime::parse_from_str(end, "%Y%m%d%H%M").ok()?;

    Some((
        Utc.from_utc_datetime(&start_naive),
        Utc.from_utc_datetime(&end_naive),
    ))
}

/// 获取视频文件内容（返回二进制数据）
#[tauri::command]
async fn get_video_data(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<Vec<u8>, String> {
    validate_session_id(session_id)?;
    use tokio::fs;

    // 获取会话详情
    let session = state
        .storage_domain.get_db().await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(video_path) = session.session.video_path {
        // 读取视频文件
        let data = fs::read(&video_path)
            .await
            .map_err(|e| format!("读取视频文件失败: {}", e))?;
        Ok(data)
    } else {
        Err("该会话没有生成视频".to_string())
    }
}

/// 获取视频文件的URL（处理Windows路径问题）
#[tauri::command]
async fn get_video_url(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<String, String> {
    validate_session_id(session_id)?;
    // 获取会话详情
    let session = state
        .storage_domain.get_db().await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(video_path) = session.session.video_path {
        // 直接返回文件路径，前端使用convertFileSrc处理
        Ok(video_path)
    } else {
        Err("该会话没有生成视频".to_string())
    }
}

/// 生成视频
#[tauri::command]
async fn generate_video(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    speed_multiplier: Option<f32>,
) -> Result<String, String> {
    validate_session_id(session_id)?;
    info!("生成会话视频: session_id={}", session_id);

    // 获取会话详情
    let session_detail = state
        .storage_domain.get_db().await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // 提取帧路径 - 实现抽帧策略：每5秒取一帧
    let all_frames = &session_detail.frames;

    // 如果没有帧，处理错误
    if all_frames.is_empty() {
        error!("会话 {} 没有截图帧，删除该会话", session_id);
        if let Err(e) = state.storage_domain.get_db().await?.delete_session(session_id).await {
            error!("删除空会话失败: {}", e);
        }
        return Err("该会话没有截图帧，已删除该会话".to_string());
    }

    // 应用帧过滤：每5秒选择一张图片（假设原始截图是1fps）
    // 优化：先过滤再克隆，避免克隆所有帧路径
    let interval = FRAME_SAMPLE_INTERVAL_SECONDS as usize;
    let frame_paths: Vec<String> = all_frames
        .iter()
        .enumerate()
        .filter(|(idx, _)| interval <= 1 || idx % interval == 0)
        .map(|(_, frame)| frame.file_path.clone())
        .collect();

    info!(
        "视频抽帧：原始 {} 帧，抽样后 {} 帧（每{}秒取一帧）",
        all_frames.len(),
        frame_paths.len(),
        FRAME_SAMPLE_INTERVAL_SECONDS
    );

    // 生成输出路径
    let output_path = state
        .analysis_domain.get_video_processor()
        .output_dir
        .join(format!("session_{}.mp4", session_id));

    // 从设置中读取视频配置
    let app_config = state.storage_domain.get_settings().get().await;
    let mut config = video::VideoConfig::default();
    config.quality = app_config.video_config.quality;
    config.add_timestamp = app_config.video_config.add_timestamp;

    if let Some(speed) = speed_multiplier {
        config.speed_multiplier = speed;
    } else {
        // 使用配置中的速度，而不是硬编码
        config.speed_multiplier = app_config.video_config.speed_multiplier;
    }

    // 生成视频
    let result = state
        .analysis_domain.get_video_processor()
        .create_summary_video(frame_paths, &output_path, &config)
        .await
        .map_err(|e| e.to_string())?;

    // 更新数据库中的视频路径
    state
        .storage_domain.get_db().await?
        .update_session_video_path(session_id, &result.file_path)
        .await
        .map_err(|e| {
            error!("更新会话视频路径失败: {}", e);
            e.to_string()
        })?;

    // 清理frame文件夹中的图片（视频已生成，不再需要原始图片）
    for frame in all_frames {
        if let Err(e) = tokio::fs::remove_file(&frame.file_path).await {
            debug!("清理frame文件失败 {}: {}", frame.file_path, e);
        }
    }
    info!(
        "视频生成成功并已更新数据库，清理了 {} 个frame文件",
        all_frames.len()
    );

    Ok(result.file_path)
}

/// 测试自动生成视频 - 按 15 分钟区间批量生成
#[tauri::command]
async fn test_generate_videos(
    state: tauri::State<'_, AppState>,
    settings: VideoSettings,
) -> Result<Vec<String>, String> {
    use chrono::{Duration, TimeZone, Timelike, Utc};
    use std::collections::BTreeMap;

    let frames_dir = state.capture_domain.get_capture().frames_dir();
    let now = Utc::now();

    let mut dir = tokio::fs::read_dir(&frames_dir)
        .await
        .map_err(|e| e.to_string())?;

    let mut frames: Vec<(chrono::DateTime<Utc>, PathBuf)> = Vec::new();

    while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();

        if !matches!(ext.as_str(), "jpg" | "jpeg" | "png") {
            continue;
        }

        let file_stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(stem) => stem,
            None => continue,
        };

        let timestamp_ms: i64 = match file_stem.parse() {
            Ok(ts) => ts,
            Err(_) => continue,
        };

        let timestamp = match chrono::DateTime::<Utc>::from_timestamp_millis(timestamp_ms) {
            Some(ts) => ts,
            None => continue,
        };
        frames.push((timestamp, path));
    }

    if frames.is_empty() {
        return Ok(vec![]);
    }

    frames.sort_by_key(|(ts, _)| *ts);

    let mut segments: BTreeMap<chrono::DateTime<Utc>, Vec<PathBuf>> = BTreeMap::new();

    for (timestamp, path) in frames {
        let segment_minute = (timestamp.minute() / 15) * 15;
        let date = timestamp.date_naive();
        let segment_start_naive = match date.and_hms_opt(timestamp.hour(), segment_minute, 0) {
            Some(dt) => dt,
            None => continue,
        };
        let segment_start = Utc.from_utc_datetime(&segment_start_naive);
        let segment_end = segment_start + Duration::minutes(15);

        if segment_end > now {
            continue;
        }

        segments.entry(segment_start).or_default().push(path);
    }

    if segments.is_empty() {
        return Ok(vec![]);
    }

    let mut video_config = video::VideoConfig::default();
    video_config.speed_multiplier = settings.speed_multiplier;
    video_config.quality = settings.quality;
    video_config.add_timestamp = settings.add_timestamp;

    let mut generated_videos = Vec::new();
    let mut failed_segments = Vec::new();

    for (segment_start, frame_paths) in segments.into_iter() {
        if frame_paths.is_empty() {
            continue;
        }

        let output_name = format!(
            "segment_{}.{}",
            segment_start.format("%Y%m%d_%H%M"),
            video_config.format.extension()
        );

        let output_path = state.analysis_domain.get_video_processor().output_dir.join(&output_name);

        let frame_list: Vec<String> = frame_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();

        // 应用帧过滤：每5秒选择一张图片
        let filtered_frame_list = video::filter_frames_by_interval(
            frame_list.clone(),
            FRAME_SAMPLE_INTERVAL_SECONDS as usize
        );

        info!(
            "生成视频段: {} (原始 {} 帧，抽样后 {} 帧)",
            output_name,
            frame_list.len(),
            filtered_frame_list.len()
        );

        match state
            .analysis_domain.get_video_processor()
            .create_summary_video(filtered_frame_list, &output_path, &video_config)
            .await
        {
            Ok(result) => {
                info!("视频生成成功: {}", result.file_path);

                // 删除已使用的帧
                let mut deleted_count = 0;
                let mut failed_count = 0;

                for frame_path in frame_paths {
                    match tokio::fs::remove_file(&frame_path).await {
                        Ok(_) => deleted_count += 1,
                        Err(err) => {
                            failed_count += 1;
                            error!("删除帧失败: {} - {}", frame_path.display(), err);
                        }
                    }
                }

                info!("删除帧: 成功 {}, 失败 {}", deleted_count, failed_count);
                generated_videos.push(result.file_path);
            }
            Err(err) => {
                // ⚠️ 不再直接返回错误，记录后继续处理其他视频段
                error!("视频段 {} 生成失败: {}", output_name, err);
                failed_segments.push((output_name, err.to_string()));
            }
        }
    }

    // 如果所有视频段都失败，返回错误
    if generated_videos.is_empty() && !failed_segments.is_empty() {
        let error_summary = failed_segments
            .iter()
            .map(|(name, err)| format!("{}: {}", name, err))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("所有视频段生成失败: {}", error_summary));
    }

    // 如果有部分成功，记录失败的段但返回成功
    if !failed_segments.is_empty() {
        warn!(
            "部分视频段生成失败 ({}/{}): {:?}",
            failed_segments.len(),
            failed_segments.len() + generated_videos.len(),
            failed_segments.iter().map(|(name, _)| name).collect::<Vec<_>>()
        );
    }

    Ok(generated_videos)
}

/// 清理存储
#[tauri::command]
async fn cleanup_storage(state: tauri::State<'_, AppState>) -> Result<(), String> {
    info!("手动触发存储清理");
    state
        .storage_domain.get_cleaner().await?
        .trigger_cleanup()
        .await
        .map_err(|e| e.to_string())
}

/// 获取存储统计
#[tauri::command]
async fn get_storage_stats(
    state: tauri::State<'_, AppState>,
) -> Result<storage::cleaner::StorageStats, String> {
    state
        .storage_domain.get_cleaner().await?
        .get_storage_stats()
        .await
        .map_err(|e| e.to_string())
}

/// 刷新历史数据的设备信息
#[tauri::command]
async fn refresh_device_info(state: tauri::State<'_, AppState>) -> Result<u64, String> {
    info!("刷新历史数据的设备信息");
    state
        .storage_domain
        .get_db()
        .await?
        .update_device_info_for_all_sessions()
        .await
        .map_err(|e| e.to_string())
}

/// 同步 SQLite 数据到 MariaDB
#[tauri::command]
async fn sync_data_to_mariadb(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    info!("开始同步数据到 MariaDB");

    // 检查当前是否为 MariaDB 模式
    if !state.storage_domain.get_db().await?.is_mariadb() {
        return Err("当前不是 MariaDB 模式，无法同步数据".to_string());
    }

    // 获取 SQLite 数据库路径
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用目录失败: {}", e))?;
    let sqlite_db_path = app_dir.join("data.db");

    if !sqlite_db_path.exists() {
        return Err("本地 SQLite 数据库不存在".to_string());
    }

    // 执行同步
    state
        .storage_domain
        .get_db()
        .await?
        .sync_from_sqlite_to_mariadb(&sqlite_db_path.to_string_lossy())
        .await
        .map_err(|e| format!("同步数据失败: {}", e))?;

    Ok("数据同步成功".to_string())
}

/// 配置Qwen
#[tauri::command]
async fn configure_qwen(
    state: tauri::State<'_, AppState>,
    config: llm::QwenConfig,
) -> Result<(), String> {
    state.analysis_domain.get_llm_handle().configure(config).await.map_err(|e| e.to_string())
}

/// 配置LLM提供商（统一接口）
#[tauri::command]
async fn configure_llm_provider(
    state: tauri::State<'_, AppState>,
    provider: String,
    config: serde_json::Value,
) -> Result<(), String> {
    info!("配置LLM提供商: {}", provider);

    // 目前只支持Qwen（通过OpenAI兼容接口）
    if provider != "openai" {
        return Err(format!("不支持的提供商: {}", provider));
    }

    // 转换为QwenConfig
    let qwen_config = llm::QwenConfig {
        api_key: config
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        model: config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("qwen-vl-max-latest")
            .to_string(),
        base_url: config
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions")
            .to_string(),
        use_video_mode: config
            .get("use_video_mode")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        video_path: config
            .get("video_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };

    // 保存到持久化配置
    let llm_provider_config = models::LLMProviderConfig {
        api_key: qwen_config.api_key.clone(),
        model: qwen_config.model.clone(),
        base_url: qwen_config.base_url.clone(),
        use_video_mode: qwen_config.use_video_mode,
    };

    let update = models::AppConfig {
        retention_days: None,
        llm_provider: Some(provider),
        capture_interval: None,
        summary_interval: None,
        video_config: None,
        capture_settings: None,
        ui_settings: None,
        llm_config: Some(llm_provider_config),
        logger_settings: None,
        database_config: None,
    };

    state
        .storage_domain.get_settings()
        .update(update)
        .await
        .map_err(|e| format!("保存配置失败: {}", e))?;

    // 配置内存中的LLM管理器
    state.analysis_domain.get_llm_handle().configure(qwen_config)
        .await
        .map_err(|e| e.to_string())?;

    info!("LLM配置已保存并应用");
    Ok(())
}

/// 测试截屏功能
#[tauri::command]
async fn test_capture(state: tauri::State<'_, AppState>) -> Result<String, String> {
    info!("测试截屏功能...");
    match state.capture_domain.get_capture().capture_frame().await {
        Ok(frame) => {
            info!("截屏成功: {}", frame.file_path);
            Ok(format!("截屏成功: {}", frame.file_path))
        }
        Err(e) => {
            let error_msg = format!("截屏失败: {}", e);
            error!("{}", error_msg);
            Err(error_msg)
        }
    }
}

/// 删除会话
#[tauri::command]
async fn delete_session(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<String, String> {
    validate_session_id(session_id)?;
    info!("删除会话: {}", session_id);

    // 获取会话详情
    let session_detail = state
        .storage_domain.get_db().await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| format!("获取会话详情失败: {}", e))?;

    // 删除视频文件（如果存在）
    if let Some(video_path) = &session_detail.session.video_path {
        if let Err(e) = tokio::fs::remove_file(video_path).await {
            error!("删除视频文件失败: {}", e);
        }
    }

    // 删除帧文件
    for frame in &session_detail.frames {
        if let Err(e) = tokio::fs::remove_file(&frame.file_path).await {
            error!("删除帧文件失败: {}", e);
        }
    }

    // 删除数据库记录 - 使用 Database 提供的方法
    state.storage_domain.get_db().await?
        .delete_session(session_id)
        .await
        .map_err(|e| format!("删除会话失败: {}", e))?;

    Ok("会话已成功删除".to_string())
}

/// 重新生成timeline
#[tauri::command]
async fn regenerate_timeline(
    state: tauri::State<'_, AppState>,
    date: Option<String>, // 日期格式: YYYY-MM-DD，不提供则为当天
) -> Result<String, String> {
    // 仅在 SQLite 模式下支持
    if !state.storage_domain.get_db().await?.is_sqlite() {
        return Err("重新生成timeline功能仅在 SQLite 模式下支持".to_string());
    }

    info!("重新生成timeline: date={:?}", date);

    // 获取目标日期
    let target_date = if let Some(date_str) = date {
        chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
            .map_err(|e| format!("日期格式错误: {}", e))?
    } else {
        chrono::Local::now().date_naive()
    };

    // 获取当天的所有sessions
    let db = state.storage_domain.get_db().await?;
    let sessions = db
        .get_sessions_by_date(&target_date.to_string())
        .await
        .map_err(|e| format!("获取会话失败: {}", e))?;

    if sessions.is_empty() {
        return Ok("当天没有会话记录".to_string());
    }

    let mut total_segments = 0;
    let mut total_cards = 0;

    // 处理每个session
    for session in sessions {
        let session_id = match session.id {
            Some(id) => id,
            None => continue,
        };
        let session_start = session.start_time;
        let session_end = session.end_time;

        // 获取该session的所有video_segments
        let segments = db
            .get_video_segments_by_session(session_id)
            .await
            .map_err(|e| format!("获取视频分段失败: {}", e))?;

        if segments.is_empty() {
            continue;
        }

        total_segments += segments.len();

        // 转换为LLM需要的格式 - 需要转换为相对时间
        let video_segments: Vec<llm::plugin::VideoSegment> = segments
            .iter()
            .map(|s| {
                // 将ISO时间转换为相对时间（MM:SS格式）
                let start_dt = chrono::DateTime::parse_from_rfc3339(&s.start_timestamp)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or(session_start);
                let end_dt = chrono::DateTime::parse_from_rfc3339(&s.end_timestamp)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or(session_end);

                // 计算相对于session开始的秒数
                let start_seconds = (start_dt - session_start).num_seconds();
                let end_seconds = (end_dt - session_start).num_seconds();

                llm::plugin::VideoSegment {
                    // 格式化为MM:SS（分钟:秒）
                    start_timestamp: format!("{:02}:{:02}", start_seconds / 60, start_seconds % 60),
                    end_timestamp: format!("{:02}:{:02}", end_seconds / 60, end_seconds % 60),
                    description: s.description.clone(),
                }
            })
            .collect();

        // 先删除相关的 LLM 调用记录（避免外键冲突）
        db.delete_llm_calls_by_session(session_id)
            .await
            .map_err(|e| format!("清除LLM调用记录失败: {}", e))?;

        // 清空该session的timeline_cards
        db.delete_timeline_cards_by_session(session_id)
            .await
            .map_err(|e| format!("清除旧时间线失败: {}", e))?;

        // 使用LLM重新生成timeline
        let llm_handle = state.analysis_domain.get_llm_handle();
        // 设置当前的 session_id，以便 LLM 调用记录能正确关联
        llm_handle.set_provider_database(state.storage_domain.get_db().await?.clone(), Some(session_id)).await
            .map_err(|e| format!("设置数据库失败: {}", e))?;

        // 设置视频速率乘数（虽然generate_timeline不直接使用，但保持一致性）
        let app_config = state.storage_domain.get_settings().get().await;
        let speed_multiplier = app_config.video_config.speed_multiplier;
        llm_handle.set_video_speed(speed_multiplier).await
            .map_err(|e| format!("设置视频速率失败: {}", e))?;
        let timeline_cards = match llm_handle.generate_timeline(video_segments, None).await {
            Ok(cards) => cards,
            Err(e) => {
                error!("生成timeline失败: {}", e);
                continue;
            }
        };

        // 获取LLM调用ID
        let timeline_call_id = llm_handle.get_last_call_id("generate_timeline").await;

        total_cards += timeline_cards.len();

        // 保存新的timeline_cards - 需要处理时间格式
        if !timeline_cards.is_empty() {
            let card_records: Vec<storage::TimelineCardRecord> = timeline_cards
                .iter()
                .map(|card| {
                    // 处理时间格式：如果是相对时间（如 "10:00 AM"），需要转换为绝对时间
                    let (start_time, end_time) = if card.start_time.contains("AM")
                        || card.start_time.contains("PM")
                        || !card.start_time.contains("T")
                    {
                        // 是相对时间，需要转换
                        let start_abs =
                            llm::relative_to_absolute(session_start, session_end, &card.start_time);
                        let end_abs =
                            llm::relative_to_absolute(session_start, session_end, &card.end_time);
                        (start_abs.to_rfc3339(), end_abs.to_rfc3339())
                    } else {
                        // 已经是ISO格式，直接使用
                        (card.start_time.clone(), card.end_time.clone())
                    };

                    storage::TimelineCardRecord {
                        id: None,
                        session_id,
                        llm_call_id: timeline_call_id, // 使用实际的LLM调用ID
                        start_time,
                        end_time,
                        category: card.category.clone(),
                        subcategory: card.subcategory.clone(),
                        title: card.title.clone(),
                        summary: card.summary.clone(),
                        detailed_summary: card.detailed_summary.clone(),
                        distractions: Some(
                            serde_json::to_string(&card.distractions).unwrap_or_default(),
                        ),
                        app_sites: serde_json::to_string(&card.app_sites).unwrap_or_default(),
                        video_preview_path: None,
                        created_at: chrono::Utc::now(),
                    }
                })
                .collect();

            if let Err(e) = state.storage_domain.get_db().await?.insert_timeline_cards(&card_records).await {
                error!("保存时间线卅片失败: {}", e);
            }
        }
    }

    Ok(format!(
        "重新生成完成：处理了 {} 个分段，生成了 {} 个时间线卡片",
        total_segments, total_cards
    ))
}

/// 通用的打开文件夹函数，支持跨平台
fn open_folder_in_explorer(path: &Path) -> Result<(), String> {
    // 确保目录存在
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 根据操作系统打开文件夹
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }

    Ok(())
}

/// 打开存储文件夹（使用枚举类型防止路径遍历攻击）
#[tauri::command]
async fn open_storage_folder(
    state: tauri::State<'_, AppState>,
    folder_type: FolderType,
) -> Result<(), String> {
    let path = match folder_type {
        FolderType::Frames => state.capture_domain.get_capture().frames_dir(),
        FolderType::Videos => state.analysis_domain.get_video_processor().output_dir.clone(),
    };

    open_folder_in_explorer(&path)
}

/// 获取日志目录路径
#[tauri::command]
fn get_log_dir() -> Result<String, String> {
    let log_dir = if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("Library/Logs/screen-analyzer")
    } else if cfg!(target_os = "windows") {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata).join("screen-analyzer").join("logs")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join(".local/share/screen-analyzer/logs")
    };

    Ok(log_dir.to_string_lossy().to_string())
}

/// 打开日志文件夹
#[tauri::command]
fn open_log_folder() -> Result<(), String> {
    let log_dir = if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("Library/Logs/screen-analyzer")
    } else if cfg!(target_os = "windows") {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata).join("screen-analyzer").join("logs")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join(".local/share/screen-analyzer/logs")
    };

    info!("打开日志文件夹: {:?}", log_dir);
    open_folder_in_explorer(&log_dir)
}

/// 测试LLM API连接
#[tauri::command]
async fn test_llm_api(
    _state: tauri::State<'_, AppState>,
    provider: String,
    config: serde_json::Value,
) -> Result<String, String> {
    info!("测试LLM API连接: provider={}", provider);

    // 对于测试连接，我们使用简单的文本测试而不是图像分析
    // 这样可以避免需要截图权限和图像处理的复杂性
    let test_result = match provider.as_str() {
        "openai" => {
            // 测试OpenAI兼容接口（包括通义千问）
            test_openai_text_api(config).await
        }
        "anthropic" => {
            // 测试Anthropic API
            test_anthropic_text_api(config).await
        }
        _ => Err(format!("不支持的提供商: {}", provider)),
    };

    match test_result {
        Ok(response) => {
            info!("API测试成功");
            Ok(format!("API连接成功！\n\n测试响应：\n{}", response))
        }
        Err(e) => {
            error!("API测试失败: {}", e);
            Err(format!("API连接失败: {}", e))
        }
    }
}

/// 测试OpenAI兼容的文本API
async fn test_openai_text_api(config: serde_json::Value) -> Result<String, String> {
    use reqwest::Client;
    use serde_json::json;

    let api_key = config
        .get("api_key")
        .and_then(|v| v.as_str())
        .ok_or("API Key未配置")?;

    let model = config
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("qwen-vl-max-latest");

    let base_url = config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions");

    let client = Client::new();
    let endpoint = base_url.to_string();

    let request_body = json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": "你好，这是一个API连接测试。请简单回复确认连接成功。"
            }
        ],
        "max_tokens": 100,
        "temperature": 0.3
    });

    let response = client
        .post(&endpoint)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    if !response.status().is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "无法读取错误信息".to_string());
        return Err(format!("API返回错误: {}", error_text));
    }

    let response_data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let content = response_data
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|content| content.as_str())
        .ok_or("无法从响应中提取内容")?;

    Ok(content.to_string())
}

/// 测试Anthropic文本API
async fn test_anthropic_text_api(config: serde_json::Value) -> Result<String, String> {
    use reqwest::Client;
    use serde_json::json;

    let api_key = config
        .get("api_key")
        .and_then(|v| v.as_str())
        .ok_or("API Key未配置")?;

    let model = config
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("claude-3-haiku-20240307");

    let base_url = config
        .get("base_url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("https://api.anthropic.com");

    let client = Client::new();
    let endpoint = format!("{}/v1/messages", base_url);

    let request_body = json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": "你好，这是一个API连接测试。请简单回复确认连接成功。"
            }
        ],
        "max_tokens": 100
    });

    let response = client
        .post(&endpoint)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    if !response.status().is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "无法读取错误信息".to_string());
        return Err(format!("API返回错误: {}", error_text));
    }

    let response_data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let content = response_data
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|content| content.get("text"))
        .and_then(|text| text.as_str())
        .ok_or("无法从响应中提取内容")?;

    Ok(content.to_string())
}

// ==================== 辅助函数 ====================

/// 处理历史图片，生成视频并清理
async fn process_historical_frames(state: &AppState) -> Result<(), String> {
    // 仅在 SQLite 模式下处理历史图片
    let db = state.storage_domain.get_db().await?;
    if !db.is_sqlite() {
        info!("跳过历史图片处理（仅 SQLite 模式支持）");
        return Ok(());
    }

    info!("开始处理历史图片");

    // 查询所有会话，筛选出未生成视频的
    let all_sessions = db.get_all_sessions().await.map_err(|e| e.to_string())?;

    let sessions_without_video: Vec<_> = all_sessions
        .into_iter()
        .filter(|s| s.video_path.is_none() || s.video_path.as_ref().map_or(false, |p| p.is_empty()))
        .take(10)
        .collect();

    info!("找到 {} 个未生成视频的会话", sessions_without_video.len());

    for session in sessions_without_video {
        let session_id = match session.id {
            Some(id) => id,
            None => continue,
        };

        info!("处理会话 {}: {} - {}", session_id, session.start_time, session.end_time);

        // 获取该会话的所有帧
        let frames = match db.get_frames_by_session(session_id).await {
            Ok(frames) => frames,
            Err(e) => {
                error!("获取会话 {} 的帧失败: {}", session_id, e);
                continue;
            }
        };

        if !frames.is_empty() {
            let frame_paths: Vec<String> = frames.into_iter().map(|f| f.file_path).collect();

            if !frame_paths.is_empty() {
                info!(
                    "为会话 {} 生成视频，共 {} 帧",
                    session_id,
                    frame_paths.len()
                );

                // 生成视频
                let video_config = crate::video::VideoConfig::default();
                let video_filename = format!(
                    "{}-{}.mp4",
                    session.start_time.format("%Y%m%d%H%M"),
                    session.end_time.format("%Y%m%d%H%M")
                );

                let video_path_buf = state.analysis_domain.get_video_processor().output_dir.join(&video_filename);
                match state
                    .analysis_domain.get_video_processor()
                    .create_summary_video(
                        frame_paths.clone(),
                        &video_path_buf,
                        &video_config,
                    )
                    .await
                {
                    Ok(video_result) => {
                        info!(
                            "视频生成成功: {} ({}字节, {}ms)",
                            video_path_buf.display(),
                            video_result.file_size,
                            video_result.processing_time_ms
                        );

                        let video_path_str = video_path_buf.to_string_lossy();
                        // 更新数据库中的视频路径
                        if let Err(e) = db.update_session_video_path(session_id, &video_path_str).await {
                            error!("更新会话 {} 视频路径失败: {}", session_id, e);
                        }

                        // 删除已合并到视频的图片文件（使用异步 I/O）
                        let mut deleted_count = 0;
                        for frame_path in &frame_paths {
                            if std::path::Path::new(frame_path).exists() {
                                if let Err(e) = tokio::fs::remove_file(frame_path).await {
                                    error!("删除图片失败 {}: {}", frame_path, e);
                                } else {
                                    deleted_count += 1;
                                }
                            }
                        }
                        info!("已删除 {} 个图片文件", deleted_count);
                    }
                    Err(e) => {
                        error!("为会话 {} 生成视频失败: {}", session_id, e);
                    }
                }
            }
        }
    }

    info!("历史图片处理完成");
    Ok(())
}

// ==================== 应用入口 ====================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 创建日志广播器
    let log_broadcaster = Arc::new(logger::LogBroadcaster::new());

    // 初始化日志系统（带前端推送功能）
    logger::init_with_broadcaster(log_broadcaster.clone())
        .expect("Failed to initialize logger");

    tauri::Builder::default()
        .setup(move |app| {
            info!("初始化屏幕活动分析器...");

            // 设置日志广播器的 app handle
            log_broadcaster.set_app_handle(app.handle().clone());

            let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;

            // 创建必要的目录
            let frames_dir = app_dir.join("frames");
            let videos_dir = app_dir.join("videos");
            let temp_dir = app_dir.join("temp");

            std::fs::create_dir_all(&frames_dir).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&videos_dir).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

            // 初始化运行时（仅用于初始化，不用于运行 Actor）
            let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;

            let (state, llm_actor, status_actor, llm_config_to_load, db_config_to_load, frames_dir_clone, videos_dir_clone) = runtime.block_on(async {
                // 先初始化设置管理器，以便读取数据库配置
                let settings = Arc::new(
                    SettingsManager::new(app_dir.join("config.json"))
                        .await
                        .expect("设置管理器初始化失败"),
                );

                // 读取初始配置
                let initial_config = settings.get().await;

                // 准备数据库配置（延迟初始化）
                let db_config_to_load = if let Some(db_config) = initial_config.database_config.clone() {
                    info!("将使用配置的数据库: {:?}", db_config);
                    Some(db_config)
                } else {
                    info!("将使用默认 SQLite 数据库");
                    None
                };

                // 初始化截屏管理器
                let capture =
                    Arc::new(ScreenCapture::new(frames_dir.clone()).expect("截屏管理器初始化失败"));

                // 创建共享的 HTTP 客户端（用于 LLM API 调用，复用连接池提升性能）
                let http_client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(300))
                    .pool_max_idle_per_host(10)
                    .build()
                    .expect("无法创建 HTTP 客户端");

                // 初始化LLM管理器（使用Actor模式，无需外层锁）
                // 注意：Actor 不在此处启动，而是在后台任务的运行时中启动
                let llm_manager = LLMManager::new(http_client.clone());
                let (llm_actor, llm_handle) = actors::LLMManagerActor::new(llm_manager);

                // 从配置加载截屏设置
                if let Some(capture_settings) = initial_config.capture_settings.clone() {
                    capture.update_settings(capture_settings.clone()).await;
                    info!("已加载截屏配置: {:?}", capture_settings);
                }

                // 保存 LLM 配置（在 Actor 启动后再配置）
                let llm_config_to_load = initial_config.llm_config.clone().map(|llm_config| {
                    llm::QwenConfig {
                        api_key: llm_config.api_key,
                        model: llm_config.model,
                        base_url: llm_config.base_url,
                        use_video_mode: llm_config.use_video_mode,
                        video_path: None,
                    }
                });

                // 初始化视频处理器
                let video_processor = Arc::new(
                    VideoProcessor::new(videos_dir.clone(), temp_dir).expect("视频处理器初始化失败"),
                );

                // 初始化调度器
                let mut scheduler_inner = CaptureScheduler::new(capture.clone());
                scheduler_inner.configure(
                    initial_config.capture_interval,
                    initial_config.summary_interval,
                );
                let scheduler = Arc::new(scheduler_inner);

                // 初始化系统状态（使用Actor模式，无需锁）
                // 注意：Actor 不在此处启动，而是在后台任务的运行时中启动
                let (status_actor, status_handle) = actors::SystemStatusActor::new();

                // 从配置中读取日志设置并应用
                let initial_logger_settings = initial_config.logger_settings.unwrap_or_default();
                log_broadcaster.set_enabled(initial_logger_settings.enable_frontend_logging);
                info!("日志推送已设置: {}", initial_logger_settings.enable_frontend_logging);

                // 将 HTTP 客户端包装为 Arc 以便在 AppState 中共享
                let http_client = Arc::new(http_client);

                // ==================== 组装领域管理器 ====================

                // 创建捕获领域
                let capture_domain = Arc::new(CaptureDomain::new(
                    capture.clone(),
                    scheduler.clone(),
                ));

                // 创建分析领域（使用LLM Handle）
                let analysis_domain = Arc::new(AnalysisDomain::new(
                    llm_handle.clone(),
                    video_processor.clone(),
                ));

                // 创建存储领域（数据库未初始化）
                let storage_domain = Arc::new(StorageDomain::new_pending(settings.clone()));

                // 创建系统领域（使用SystemStatus Handle）
                let system_domain = Arc::new(SystemDomain::new(
                    status_handle.clone(),
                    log_broadcaster.clone(),
                    http_client,
                ));

                // 创建事件总线（容量1000,足够缓冲）
                let event_bus = Arc::new(EventBus::new(1000));

                info!("领域管理器已初始化完成");

                let app_state = AppState {
                    capture_domain,
                    analysis_domain,
                    storage_domain,
                    system_domain,
                    event_bus,
                };

                // 返回 AppState、两个 Actor、LLM 配置、数据库配置和目录路径
                (
                    app_state,
                    llm_actor,
                    status_actor,
                    llm_config_to_load,
                    db_config_to_load,
                    frames_dir.clone(),
                    videos_dir.clone(),
                )
            });

            // 启动后台任务
            {
                let state_clone = state.clone();
                let app_dir_clone = app_dir.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new()
                        .expect("无法创建 Tokio 运行时，程序无法继续运行");
                    rt.block_on(async {
                        info!("启动后台任务...");

                        // ========== 异步初始化数据库 ==========
                        info!("开始异步初始化数据库...");
                        let db_result = if let Some(db_config) = db_config_to_load {
                            info!("使用配置的数据库: {:?}", db_config);
                            Database::from_config(&db_config).await
                        } else {
                            info!("使用默认 SQLite 数据库");
                            Database::new(&app_dir_clone.join("data.db").to_string_lossy()).await
                        };

                        match db_result {
                            Ok(db) => {
                                let db = Arc::new(db);
                                info!("数据库初始化成功，类型: {}", db.db_type());

                                // 设置数据库到 StorageDomain
                                state_clone.storage_domain.set_database(db.clone()).await;

                                // 初始化存储清理器
                                let cleaner = Arc::new(StorageCleaner::new(
                                    db.clone(),
                                    frames_dir_clone.clone(),
                                    videos_dir_clone.clone(),
                                ));

                                // 从配置读取保留天数
                                let retention_days = state_clone.storage_domain.get_settings().get().await.retention_days;
                                if let Err(e) = cleaner.set_retention_days(retention_days).await {
                                    error!("设置保留天数失败: {}", e);
                                }

                                // 设置清理器到 StorageDomain
                                state_clone.storage_domain.set_cleaner(cleaner).await;

                                info!("数据库和存储清理器已就绪");
                            }
                            Err(e) => {
                                let error_msg = format!("数据库初始化失败: {}", e);
                                error!("{}", error_msg);
                                state_clone.storage_domain.set_database_error(error_msg).await;
                                // 继续运行，但数据库相关功能将不可用
                            }
                        }

                        // 启动 Actor（在这个长期运行的运行时中）
                        info!("启动 LLM Manager Actor 和 System Status Actor...");
                        tokio::spawn(llm_actor.run());
                        tokio::spawn(status_actor.run());
                        info!("Actors 已启动");

                        // 配置 LLM（Actor 启动后才能配置）
                        if let Some(qwen_config) = llm_config_to_load {
                            if let Err(e) = state_clone.analysis_domain.get_llm_handle().configure(qwen_config).await {
                                error!("加载LLM配置失败: {}", e);
                            } else {
                                info!("已从配置文件加载LLM设置");
                            }
                        }

                        // 仅在数据库就绪时启动依赖数据库的组件
                        if let Some(db) = state_clone.storage_domain.try_get_db().await {
                            // 创建LLMProcessor并启动事件监听器
                            let llm_processor = Arc::new(llm::LLMProcessor::with_video_processor(
                                state_clone.analysis_domain.get_llm_handle().clone(),
                                db.clone(),
                                state_clone.analysis_domain.get_video_processor().clone(),
                                state_clone.storage_domain.get_settings().clone(),
                            ));

                            // 启动LLM处理器事件监听器
                            llm_processor.start_event_listener(
                                state_clone.event_bus.clone(),
                                state_clone.capture_domain.get_capture().clone(),
                            ).await;

                            info!("LLM处理器事件监听器已启动");

                            // 启动调度器（事件驱动模式）
                            state_clone.capture_domain.get_scheduler().clone().start(state_clone.event_bus.clone());

                            // 启动存储清理任务
                            if let Ok(cleaner) = state_clone.storage_domain.get_cleaner().await {
                                cleaner.start_cleanup_task().await;
                                info!("存储清理任务已启动");
                            } else {
                                error!("存储清理器未就绪");
                            }
                        } else {
                            error!("数据库未就绪，跳过数据库相关组件的启动");
                        }

                        // 周期性扫描视频目录，处理未分析的视频
                        {
                            let video_state = state_clone.clone();
                            tokio::spawn(async move {
                                loop {
                                    // 直接执行分析，无需 analysis_lock（已移除临时方案）
                                    match analyze_unprocessed_videos(
                                        &video_state,
                                        None,
                                        false,
                                    )
                                    .await
                                    {
                                        Ok(report) => {
                                            if report.processed > 0 || report.failed > 0 {
                                                info!(
                                                    "自动视频分析完成: 处理 {} 个, 失败 {} 个",
                                                    report.processed, report.failed
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            error!("自动视频分析失败: {}", e);
                                        }
                                    }
                                    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                                }
                            });
                        }

                        // 更新系统状态
                        state_clone.system_domain.get_status_handle().set_capturing(true).await;

                        info!("所有后台任务已启动");

                        // 在独立的后台任务中处理历史图片（不阻塞启动）
                        {
                            let history_state = state_clone.clone();
                            tokio::spawn(async move {
                                info!("开始处理历史图片，生成视频...");
                                if let Err(e) = process_historical_frames(&history_state).await {
                                    error!("处理历史图片失败: {}", e);
                                }
                            });
                        }

                        // 保持运行时活跃
                        loop {
                            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                        }
                    });
                });
            }

            app.manage(state);
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_database_status,
            get_activities,
            get_day_sessions,
            get_session_detail,
            get_app_config,
            update_config,
            add_manual_tag,
            remove_tag,
            get_system_status,
            toggle_capture,
            trigger_analysis,
            generate_video,
            get_video_url,
            get_video_data,
            test_generate_videos,
            cleanup_storage,
            get_storage_stats,
            refresh_device_info,
            sync_data_to_mariadb,
            configure_qwen,
            configure_llm_provider,
            test_capture,
            test_llm_api,
            retry_session_analysis,
            regenerate_timeline,
            delete_session,
            open_storage_folder,
            get_log_dir,
            open_log_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
#[derive(Default)]
struct VideoAnalysisReport {
    total_candidates: usize,
    processed: usize,
    failed: usize,
    messages: Vec<String>,
}

struct VideoAnalysisOutcome {
    #[allow(dead_code)]
    _session_id: i64, // 保留用于未来可能的扩展
    segments_count: usize,
    timeline_count: usize,
    summary: llm::SessionSummary,
}

async fn analyze_video_once(
    state: &AppState,
    video_path: &Path,
    session_start: chrono::DateTime<chrono::Utc>,
    session_end: chrono::DateTime<chrono::Utc>,
    duration_minutes: u32,
    reuse_session: Option<i64>,
) -> Result<VideoAnalysisOutcome, String> {
    use chrono::Utc;

    let video_path_str = video_path.to_string_lossy().to_string();
    let file_stem = video_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("视频");

    let persisted_config = state.storage_domain.get_settings().get().await;
    let llm_handle = state.analysis_domain.get_llm_handle();

    let qwen_config = if let Some(llm_config) = persisted_config.llm_config {
        llm::QwenConfig {
            api_key: llm_config.api_key,
            model: llm_config.model,
            base_url: llm_config.base_url,
            use_video_mode: llm_config.use_video_mode,
            video_path: Some(video_path_str.clone()),
        }
    } else {
        let config = llm_handle.get_config().await.map_err(|e| e.to_string())?;
        llm::QwenConfig {
            api_key: config.qwen.api_key.clone(),
            model: config.qwen.model.clone(),
            base_url: config.qwen.base_url.clone(),
            use_video_mode: true,
            video_path: Some(video_path_str.clone()),
        }
    };

    if qwen_config.api_key.is_empty() {
        return Err("请先在设置中配置LLM API Key".to_string());
    }

    if let Err(e) = llm_handle.configure(qwen_config).await {
        return Err(e.to_string());
    }

    let now = Utc::now();

    // 准备会话
    let db = state.storage_domain.get_db().await?;
    let session_id = if let Some(existing_id) = reuse_session {
        if let Err(e) = db.delete_video_segments_by_session(existing_id).await {
            let _ = llm_handle.set_video_path(None).await;
            return Err(format!("清理历史视频分段失败: {}", e));
        }

        if let Err(e) = db.delete_timeline_cards_by_session(existing_id).await {
            let _ = llm_handle.set_video_path(None).await;
            return Err(format!("清理历史时间线卡片失败: {}", e));
        }

        existing_id
    } else {
        let (device_name, device_type) = storage::get_device_info();
        let temp_session = storage::Session {
            id: None,
            start_time: session_start,
            end_time: session_end,
            title: format!("视频分析中: {}", file_stem),
            summary: "正在分析...".to_string(),
            video_path: Some(video_path_str.clone()),
            tags: "[]".to_string(),
            created_at: Some(now),
            device_name: Some(device_name),
            device_type: Some(device_type),
        };

        match state.storage_domain.get_db().await?.insert_session(&temp_session).await {
            Ok(id) => id,
            Err(e) => {
                let _ = llm_handle.set_video_path(None).await;
                return Err(e.to_string());
            }
        }
    };

    llm_handle.set_provider_database(state.storage_domain.get_db().await?.clone(), Some(session_id)).await
        .map_err(|e| e.to_string())?;

    // 设置视频速率乘数（从配置获取）
    let speed_multiplier = persisted_config.video_config.speed_multiplier;
    llm_handle.set_video_speed(speed_multiplier).await
        .map_err(|e| e.to_string())?;

    let analysis = match llm_handle
        .segment_video_and_generate_timeline(vec![], duration_minutes, None)
        .await
    {
        Ok(res) => res,
        Err(e) => {
            let _ = llm_handle.set_video_path(None).await;
            let error_msg = e.to_string();
            // 检测是否是视频过短的错误
            if error_msg.contains("The video file is too short") {
                return Err(format!("VIDEO_TOO_SHORT:{}", error_msg));
            }
            return Err(error_msg);
        }
    };

    let _ = llm_handle.set_video_path(None).await;

    let mut segments = analysis.segments;
    for segment in &mut segments {
        let start_abs =
            llm::relative_to_absolute(session_start, session_end, &segment.start_timestamp);
        let end_abs = llm::relative_to_absolute(session_start, session_end, &segment.end_timestamp);
        segment.start_timestamp = start_abs.to_rfc3339();
        segment.end_timestamp = end_abs.to_rfc3339();
    }

    let mut timeline_cards = analysis.timeline_cards;
    for card in &mut timeline_cards {
        let start_abs = llm::relative_to_absolute(session_start, session_end, &card.start_time);
        let end_abs = llm::relative_to_absolute(session_start, session_end, &card.end_time);
        card.start_time = start_abs.to_rfc3339();
        card.end_time = end_abs.to_rfc3339();

        if let Some(distractions) = card.distractions.as_mut() {
            for distraction in distractions {
                let d_start =
                    llm::relative_to_absolute(session_start, session_end, &distraction.start_time);
                let d_end =
                    llm::relative_to_absolute(session_start, session_end, &distraction.end_time);
                distraction.start_time = d_start.to_rfc3339();
                distraction.end_time = d_end.to_rfc3339();
            }
        }
    }

    if !segments.is_empty() {
        let segment_records: Vec<storage::VideoSegmentRecord> = segments
            .iter()
            .map(|seg| storage::VideoSegmentRecord {
                id: None,
                session_id,
                llm_call_id: analysis.segment_call_id,
                start_timestamp: seg.start_timestamp.clone(),
                end_timestamp: seg.end_timestamp.clone(),
                description: seg.description.clone(),
                created_at: now,
            })
            .collect();

        if let Err(e) = state.storage_domain.get_db().await?.insert_video_segments(&segment_records).await {
            return Err(format!("保存视频分段失败: {}", e));
        }
    }

    if !timeline_cards.is_empty() {
        let card_records: Vec<storage::TimelineCardRecord> = timeline_cards
            .iter()
            .map(|card| storage::TimelineCardRecord {
                id: None,
                session_id,
                llm_call_id: analysis.timeline_call_id,
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
                    .map(|d| serde_json::to_string(d).unwrap_or_else(|_| "[]".to_string())),
                video_preview_path: Some(video_path_str.clone()),
                app_sites: serde_json::to_string(&card.app_sites)
                    .unwrap_or_else(|_| "{}".to_string()),
                created_at: now,
            })
            .collect();

        if let Err(e) = state.storage_domain.get_db().await?.insert_timeline_cards(&card_records).await {
            return Err(format!("保存时间线卡片失败: {}", e));
        }
    }

    let summary =
        llm::build_session_summary(session_start, session_end, &segments, &timeline_cards);

    let tags_json = serde_json::to_string(&summary.tags).unwrap_or_else(|_| "[]".to_string());
    if let Err(e) = db
        .update_session(
            session_id,
            &summary.title,
            &summary.summary,
            Some(&video_path_str),
            &tags_json,
        )
        .await
    {
        return Err(format!("更新会话信息失败: {}", e));
    }

    Ok(VideoAnalysisOutcome {
        _session_id: session_id,
        segments_count: segments.len(),
        timeline_count: timeline_cards.len(),
        summary,
    })
}

async fn analyze_unprocessed_videos(
    state: &AppState,
    limit: Option<usize>,
    mark_status: bool,
) -> Result<VideoAnalysisReport, String> {
    use std::collections::HashSet;

    let videos_dir = state.analysis_domain.get_video_processor().output_dir.clone();

    // 使用异步 I/O 读取目录
    let mut video_files = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&videos_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("mp4") {
                video_files.push(path);
            }
        }
    }

    video_files.sort();

    if video_files.is_empty() {
        return Ok(VideoAnalysisReport::default());
    }

    // 使用新的抽象方法获取已分析的视频路径（支持 SQLite 和 MariaDB）
    let analyzed_video_paths = state
        .storage_domain
        .get_db()
        .await?
        .get_analyzed_video_paths()
        .await
        .map_err(|e| e.to_string())?;

    let analyzed_paths: HashSet<String> = analyzed_video_paths.into_iter().collect();

    let mut unanalyzed_videos: Vec<PathBuf> = video_files
        .into_iter()
        .filter(|path| {
            let path_str = path.to_string_lossy().to_string();
            !analyzed_paths.contains(&path_str)
        })
        .collect();

    unanalyzed_videos.sort();

    let total_candidates = unanalyzed_videos.len();
    if total_candidates == 0 {
        return Ok(VideoAnalysisReport::default());
    }

    let total_to_process = limit
        .map(|l| l.min(total_candidates))
        .unwrap_or(total_candidates);

    if total_to_process == 0 {
        return Ok(VideoAnalysisReport {
            total_candidates,
            ..Default::default()
        });
    }

    // 使用单一的原子操作更新状态
    if mark_status {
        state.system_domain.get_status_handle().set_processing(true).await;
        state.system_domain.get_status_handle().set_error(None).await;
    }

    let mut report = VideoAnalysisReport {
        total_candidates,
        processed: 0,
        failed: 0,
        messages: Vec::new(),
    };

    let mut processing_error: Option<String> = None;

    for (index, video_path) in unanalyzed_videos.iter().enumerate() {
        if index >= total_to_process {
            break;
        }

        info!("开始分析视频: {:?}", video_path);

        let video_filename = video_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let (session_start, session_end) = parse_video_window_from_stem(video_filename)
            .unwrap_or_else(|| {
                let end = chrono::Utc::now();
                (end - chrono::Duration::minutes(15), end)
            });

        let diff = session_end.signed_duration_since(session_start);
        let duration_minutes = if diff.num_seconds() > 0 {
            ((diff.num_seconds() as f64) / 60.0).ceil() as u32
        } else {
            1
        };

        match analyze_video_once(
            state,
            video_path,
            session_start,
            session_end,
            duration_minutes,
            None,
        )
        .await
        {
            Ok(outcome) => {
                info!(
                    "视频分析成功: {} 个片段, {} 个卡片",
                    outcome.segments_count, outcome.timeline_count
                );
                report.processed += 1;
                report.messages.push(format!(
                    "✅ {}: {} 片段, {} 卡片",
                    video_filename, outcome.segments_count, outcome.timeline_count
                ));
            }
            Err(err) => {
                error!("视频分析失败: {}", err);

                // 如果是视频过短错误，删除视频文件避免反复尝试
                if err.contains("VIDEO_TOO_SHORT") {
                    info!("检测到视频过短错误，删除视频文件: {:?}", video_path);
                    if let Err(e) = tokio::fs::remove_file(video_path).await {
                        error!("删除视频文件失败: {}", e);
                    } else {
                        info!("已删除过短的视频文件: {:?}", video_path);
                    }
                }

                report.failed += 1;
                report
                    .messages
                    .push(format!("❌ {}: 分析失败 - {}", video_filename, err));
                processing_error = Some(err);
                break;
            }
        }

        if total_to_process > 1 && (index + 1) < total_to_process {
            info!("等待2秒后继续分析下一个视频...");
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }

    // 使用单一的原子操作更新所有状态字段
    if mark_status {
        state.system_domain.get_status_handle().set_processing(false).await;
    }
    state.system_domain.get_status_handle().update_last_process_time(chrono::Utc::now()).await;
    state.system_domain.get_status_handle().set_error(processing_error.clone()).await;

    if let Some(err) = processing_error {
        return Err(err);
    }

    Ok(report)
}
