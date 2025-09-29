// 屏幕活动分析器 - Tauri应用主库

// 声明模块
pub mod capture;
pub mod llm;
pub mod models;
pub mod settings;
pub mod storage;
pub mod video;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, trace};

// 导入必要的类型
use capture::{scheduler::CaptureScheduler, ScreenCapture};
use llm::{LLMManager, LLMProcessor};
use models::*;
use settings::SettingsManager;
use storage::{Database, StorageCleaner};
use video::VideoProcessor;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    /// 截屏管理器
    pub capture: Arc<ScreenCapture>,
    /// 数据库
    pub db: Arc<Database>,
    /// LLM管理器
    pub llm_manager: Arc<Mutex<LLMManager>>,
    /// 存储清理器
    pub cleaner: Arc<StorageCleaner>,
    /// 视频处理器
    pub video_processor: Arc<VideoProcessor>,
    /// 系统状态
    pub system_status: Arc<RwLock<SystemStatus>>,
    /// 调度器
    pub scheduler: Arc<CaptureScheduler>,
    /// 设置管理器
    pub settings: Arc<SettingsManager>,
    /// 视频分析锁
    pub analysis_lock: Arc<tokio::sync::Mutex<()>>,
}

// ==================== Tauri命令 ====================

/// 获取活动列表
#[tauri::command]
async fn get_activities(
    state: tauri::State<'_, AppState>,
    start_date: String,
    end_date: String,
) -> Result<Vec<Activity>, String> {
    state
        .db
        .get_activities(&start_date, &end_date)
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
        .db
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
    state
        .db
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())
}

/// 获取应用配置
#[tauri::command]
async fn get_app_config(state: tauri::State<'_, AppState>) -> Result<PersistedAppConfig, String> {
    Ok(state.settings.get().await)
}

/// 更新配置
#[tauri::command]
async fn update_config(
    state: tauri::State<'_, AppState>,
    config: AppConfig,
) -> Result<PersistedAppConfig, String> {
    let updated_config = state
        .settings
        .update(config.clone())
        .await
        .map_err(|e| e.to_string())?;

    // 更新保留天数
    if let Some(retention_days) = config.retention_days {
        // 直接调用cleaner的方法，不需要获取可变引用
        state
            .cleaner
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
        state.capture.update_settings(capture_settings.clone()).await;
        info!("截屏配置已更新: {:?}", capture_settings);
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
    // 获取当前会话
    let session_detail = state
        .db
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // 添加新标签
    let mut tags = session_detail.tags;
    tags.push(tag);

    // 更新数据库
    let tags_json = serde_json::to_string(&tags).map_err(|e| e.to_string())?;

    state
        .db
        .update_session_tags(session_id, &tags_json)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 获取系统状态
#[tauri::command]
async fn get_system_status(state: tauri::State<'_, AppState>) -> Result<SystemStatus, String> {
    let status = state.system_status.read().await;
    Ok(status.clone())
}

/// 切换截屏状态（暂停/恢复）
#[tauri::command]
async fn toggle_capture(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let mut status = state.system_status.write().await;
    status.is_capturing = enabled;

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

    let analysis_lock = state.analysis_lock.clone();
    let _guard = analysis_lock.lock().await;

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
    info!("重新分析会话: {}", session_id);

    let analysis_lock = state.analysis_lock.clone();
    let _guard = analysis_lock.lock().await;

    {
        let mut status = state.system_status.write().await;
        status.is_processing = true;
        status.last_error = None;
    }

    let result = async {
        let session_detail = state
            .db
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

        sqlx::query("DELETE FROM video_segments WHERE session_id = ?")
            .bind(session_id)
            .execute(state.db.get_pool())
            .await
            .map_err(|e| e.to_string())?;

        sqlx::query("DELETE FROM timeline_cards WHERE session_id = ?")
            .bind(session_id)
            .execute(state.db.get_pool())
            .await
            .map_err(|e| e.to_string())?;

        sqlx::query(
            r#"
            UPDATE sessions
            SET title = ?1, summary = ?2, tags = ?3
            WHERE id = ?4
            "#,
        )
        .bind(&session_detail.session.title)
        .bind("重新分析中...")
        .bind("[]")
        .bind(session_id)
        .execute(state.db.get_pool())
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

    {
        let mut status = state.system_status.write().await;
        status.is_processing = false;
        status.last_process_time = Some(chrono::Utc::now());
        status.last_error = last_error.clone();
    }

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
    use tokio::fs;

    // 获取会话详情
    let session = state
        .db
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
    // 获取会话详情
    let session = state
        .db
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
    info!("生成会话视频: session_id={}", session_id);

    // 获取会话详情
    let session_detail = state
        .db
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // 提取帧路径 - 实现抽帧策略：每5秒取一帧
    let all_frames = &session_detail.frames;

    // 如果没有帧，处理错误
    if all_frames.is_empty() {
        error!("会话 {} 没有截图帧，删除该会话", session_id);
        if let Err(e) = state.db.delete_session(session_id).await {
            error!("删除空会话失败: {}", e);
        }
        return Err("该会话没有截图帧，已删除该会话".to_string());
    }

    // 实现智能抽帧：基于时间戳每5秒取一帧，并确保关键时间点（00,15,30,45秒）的帧被包含
    let frame_paths: Vec<String> = if all_frames.len() <= 12 {
        // 如果帧数少于12帧（1分钟内），使用全部帧
        all_frames.iter().map(|f| f.file_path.clone()).collect()
    } else {
        use chrono::Timelike;
        use std::collections::HashSet;

        let mut selected_indices = HashSet::new();
        let mut sampled = Vec::new();

        // 1. 添加第一帧和最后一帧
        selected_indices.insert(0);
        selected_indices.insert(all_frames.len() - 1);

        // 2. 找到每个关键时间点（00,15,30,45秒）最近的帧
        let key_seconds = vec![0, 15, 30, 45];

        // 按分钟分组，找每分钟内关键时间点最近的帧
        let mut minute_groups = std::collections::HashMap::new();
        for (i, frame) in all_frames.iter().enumerate() {
            let minute = frame.timestamp.minute();
            minute_groups
                .entry(minute)
                .or_insert(Vec::new())
                .push((i, frame));
        }

        // 对每个分钟组，找到距离关键时间点最近的帧
        for (_minute, frames) in minute_groups.iter() {
            for &key_sec in &key_seconds {
                if let Some((idx, _frame)) = frames.iter().min_by_key(|(_, f)| {
                    let sec = f.timestamp.second() as i32;
                    // 计算距离关键时间点的最小距离
                    (sec - key_sec as i32).abs()
                }) {
                    selected_indices.insert(*idx);
                }
            }
        }

        // 3. 基于5秒间隔选择帧
        let mut last_added_timestamp: Option<chrono::DateTime<chrono::Utc>> = None;

        for (i, frame) in all_frames.iter().enumerate() {
            // 如果已经被选中（第一帧、最后一帧或关键时间点），跳过间隔检查
            if selected_indices.contains(&i) {
                sampled.push((i, frame.file_path.clone()));
                last_added_timestamp = Some(frame.timestamp);
                continue;
            }

            // 检查与上一个添加帧的时间间隔
            if let Some(last_time) = last_added_timestamp {
                let time_diff_ms =
                    frame.timestamp.timestamp_millis() - last_time.timestamp_millis();
                if time_diff_ms >= 5000 {
                    // 间隔大于等于5秒，添加此帧
                    sampled.push((i, frame.file_path.clone()));
                    last_added_timestamp = Some(frame.timestamp);
                }
            }
        }

        // 按索引排序，确保帧的顺序正确
        sampled.sort_by_key(|(idx, _)| *idx);

        // 去重并提取文件路径
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        for (_, path) in sampled {
            if seen.insert(path.clone()) {
                result.push(path);
            }
        }

        info!(
            "视频抽帧：原始 {} 帧，抽样后 {} 帧（基于时间戳，5秒间隔+关键时间点）",
            all_frames.len(),
            result.len()
        );

        // 打印调试信息：显示选中的帧的时间戳
        for (i, frame) in all_frames.iter().enumerate() {
            if result.contains(&frame.file_path) {
                debug!(
                    "选中帧 {}: 时间 {}:{}:{}",
                    i,
                    frame.timestamp.minute(),
                    frame.timestamp.second(),
                    frame.timestamp.timestamp_subsec_millis()
                );
            }
        }

        result
    };

    // 生成输出路径
    let output_path = state
        .video_processor
        .output_dir
        .join(format!("session_{}.mp4", session_id));

    // 配置视频参数
    let mut config = video::VideoConfig::default();
    if let Some(speed) = speed_multiplier {
        config.speed_multiplier = speed;
    } else {
        // 默认4倍速播放
        config.speed_multiplier = 8.0;
    }

    // 生成视频
    let result = state
        .video_processor
        .create_summary_video(frame_paths, &output_path, &config)
        .await
        .map_err(|e| e.to_string())?;

    // 更新数据库中的视频路径
    state
        .db
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

    let frames_dir = state.capture.frames_dir();
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

    for (segment_start, frame_paths) in segments.into_iter() {
        if frame_paths.is_empty() {
            continue;
        }

        let output_name = format!(
            "segment_{}.{}",
            segment_start.format("%Y%m%d_%H%M"),
            video_config.format.extension()
        );

        let output_path = state.video_processor.output_dir.join(output_name);

        let frame_list: Vec<String> = frame_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();

        match state
            .video_processor
            .create_summary_video(frame_list, &output_path, &video_config)
            .await
        {
            Ok(result) => {
                for frame_path in frame_paths {
                    if let Err(err) = tokio::fs::remove_file(&frame_path).await {
                        error!("删除帧失败: {} - {}", frame_path.display(), err);
                    }
                }
                generated_videos.push(result.file_path);
            }
            Err(err) => {
                error!("测试生成视频失败: {}", err);
                return Err(err.to_string());
            }
        }
    }

    Ok(generated_videos)
}

/// 清理存储
#[tauri::command]
async fn cleanup_storage(state: tauri::State<'_, AppState>) -> Result<(), String> {
    info!("手动触发存储清理");
    state
        .cleaner
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
        .cleaner
        .get_storage_stats()
        .await
        .map_err(|e| e.to_string())
}

/// 配置Qwen
#[tauri::command]
async fn configure_qwen(
    state: tauri::State<'_, AppState>,
    config: llm::QwenConfig,
) -> Result<(), String> {
    let mut llm = state.llm_manager.lock().await;
    llm.configure(config).await.map_err(|e| e.to_string())
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
    };

    state
        .settings
        .update(update)
        .await
        .map_err(|e| format!("保存配置失败: {}", e))?;

    // 配置内存中的LLM管理器
    let mut llm = state.llm_manager.lock().await;
    llm.configure(qwen_config)
        .await
        .map_err(|e| e.to_string())?;

    info!("LLM配置已保存并应用");
    Ok(())
}

/// 测试截屏功能
#[tauri::command]
async fn test_capture(state: tauri::State<'_, AppState>) -> Result<String, String> {
    info!("测试截屏功能...");
    match state.capture.capture_frame().await {
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
    info!("删除会话: {}", session_id);

    // 获取会话详情
    let session_detail = state
        .db
        .get_session_detail(session_id)
        .await
        .map_err(|e| format!("获取会话详情失败: {}", e))?;

    // 删除视频文件（如果存在）
    if let Some(video_path) = &session_detail.session.video_path {
        if let Err(e) = std::fs::remove_file(video_path) {
            error!("删除视频文件失败: {}", e);
        }
    }

    // 删除帧文件
    for frame in &session_detail.frames {
        if let Err(e) = std::fs::remove_file(&frame.file_path) {
            error!("删除帧文件失败: {}", e);
        }
    }

    // 删除数据库记录
    // 删除 video_segments
    sqlx::query("DELETE FROM video_segments WHERE session_id = ?")
        .bind(session_id)
        .execute(state.db.get_pool())
        .await
        .map_err(|e| format!("删除视频分段失败: {}", e))?;

    // 删除 timeline_cards
    sqlx::query("DELETE FROM timeline_cards WHERE session_id = ?")
        .bind(session_id)
        .execute(state.db.get_pool())
        .await
        .map_err(|e| format!("删除时间线卡片失败: {}", e))?;

    // 删除 frames (正确的表名)
    sqlx::query("DELETE FROM frames WHERE session_id = ?")
        .bind(session_id)
        .execute(state.db.get_pool())
        .await
        .map_err(|e| format!("删除帧记录失败: {}", e))?;

    // 删除 sessions
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(session_id)
        .execute(state.db.get_pool())
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
    info!("重新生成timeline: date={:?}", date);

    // 获取目标日期
    let target_date = if let Some(date_str) = date {
        chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
            .map_err(|e| format!("日期格式错误: {}", e))?
    } else {
        chrono::Local::now().date_naive()
    };

    // 获取当天的所有sessions
    let sessions = sqlx::query_as::<_, (i64, String, String)>(
        r#"
        SELECT id, start_time, end_time
        FROM sessions
        WHERE DATE(start_time) = DATE(?)
        ORDER BY start_time ASC
        "#,
    )
    .bind(target_date.to_string())
    .fetch_all(state.db.get_pool())
    .await
    .map_err(|e| format!("获取会话失败: {}", e))?;

    if sessions.is_empty() {
        return Ok("当天没有会话记录".to_string());
    }

    let mut total_segments = 0;
    let mut total_cards = 0;

    // 处理每个session
    for (session_id, start_time_str, end_time_str) in sessions {
        // 获取该session的所有video_segments
        let segments = sqlx::query_as::<_, storage::VideoSegmentRecord>(
            "SELECT * FROM video_segments WHERE session_id = ? ORDER BY start_timestamp ASC",
        )
        .bind(session_id)
        .fetch_all(state.db.get_pool())
        .await
        .map_err(|e| format!("获取视频分段失败: {}", e))?;

        if segments.is_empty() {
            continue;
        }

        total_segments += segments.len();

        // 解析session的时间范围
        let session_start = chrono::DateTime::parse_from_rfc3339(&start_time_str)
            .map_err(|e| format!("解析开始时间失败: {}", e))?
            .with_timezone(&chrono::Utc);
        let session_end = chrono::DateTime::parse_from_rfc3339(&end_time_str)
            .map_err(|e| format!("解析结束时间失败: {}", e))?
            .with_timezone(&chrono::Utc);

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
        sqlx::query("DELETE FROM llm_calls WHERE session_id = ?")
            .bind(session_id)
            .execute(state.db.get_pool())
            .await
            .map_err(|e| format!("清除LLM调用记录失败: {}", e))?;

        // 清空该session的timeline_cards
        sqlx::query("DELETE FROM timeline_cards WHERE session_id = ?")
            .bind(session_id)
            .execute(state.db.get_pool())
            .await
            .map_err(|e| format!("清除旧时间线失败: {}", e))?;

        // 使用LLM重新生成timeline
        let mut llm_manager = state.llm_manager.lock().await;
        // 设置当前的 session_id，以便 LLM 调用记录能正确关联
        llm_manager.set_provider_database(state.db.clone(), Some(session_id));

        // 设置视频速率乘数（虽然generate_timeline不直接使用，但保持一致性）
        let app_config = state.settings.get().await;
        let speed_multiplier = app_config.video_config.speed_multiplier;
        llm_manager.set_video_speed(speed_multiplier);
        let timeline_cards = match llm_manager.generate_timeline(video_segments, None).await {
            Ok(cards) => cards,
            Err(e) => {
                error!("生成timeline失败: {}", e);
                continue;
            }
        };

        // 获取LLM调用ID
        let timeline_call_id = llm_manager.get_last_call_id("generate_timeline");

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

            if let Err(e) = state.db.insert_timeline_cards(&card_records).await {
                error!("保存时间线卅片失败: {}", e);
            }
        }
    }

    Ok(format!(
        "重新生成完成：处理了 {} 个分段，生成了 {} 个时间线卡片",
        total_segments, total_cards
    ))
}

/// 打开存储文件夹
#[tauri::command]
async fn open_storage_folder(
    state: tauri::State<'_, AppState>,
    folder_type: String,
) -> Result<(), String> {
    let path = match folder_type.as_str() {
        "frames" => state.capture.frames_dir(),
        "videos" => state.video_processor.output_dir.clone(),
        _ => return Err(format!("未知的文件夹类型: {}", folder_type)),
    };

    // 确保目录存在
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 根据操作系统打开文件夹
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }

    Ok(())
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
async fn process_historical_frames(state: &AppState) {
    info!("开始处理历史图片");

    // 查询所有未生成视频的会话
    let pool = state.db.get_pool();
    let sessions_without_video = sqlx::query(
        r#"
        SELECT id, start_time, end_time
        FROM sessions
        WHERE video_path IS NULL OR video_path = ''
        ORDER BY start_time DESC
        LIMIT 10
        "#,
    )
    .fetch_all(pool)
    .await;

    if let Ok(sessions) = sessions_without_video {
        info!("找到 {} 个未生成视频的会话", sessions.len());

        for row in sessions {
            if let (Ok(session_id), Ok(start_time), Ok(end_time)) = (
                sqlx::Row::try_get::<i64, _>(&row, "id"),
                sqlx::Row::try_get::<chrono::DateTime<chrono::Utc>, _>(&row, "start_time"),
                sqlx::Row::try_get::<chrono::DateTime<chrono::Utc>, _>(&row, "end_time"),
            ) {
                info!("处理会话 {}: {} - {}", session_id, start_time, end_time);

                // 获取该会话的所有帧
                let frames = sqlx::query(
                    r#"
                    SELECT file_path
                    FROM frames
                    WHERE session_id = ?1
                    ORDER BY timestamp ASC
                    "#,
                )
                .bind(session_id)
                .fetch_all(pool)
                .await;

                if let Ok(frame_rows) = frames {
                    let mut frame_paths = Vec::new();
                    for frame_row in frame_rows {
                        if let Ok(path) = sqlx::Row::try_get::<String, _>(&frame_row, "file_path") {
                            frame_paths.push(path);
                        }
                    }

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
                            start_time.format("%Y%m%d%H%M"),
                            end_time.format("%Y%m%d%H%M")
                        );

                        let video_path_buf = state.video_processor.output_dir.join(&video_filename);
                        match state
                            .video_processor
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
                                let _ = sqlx::query(
                                    r#"
                                    UPDATE sessions
                                    SET video_path = ?1
                                    WHERE id = ?2
                                    "#,
                                )
                                .bind(video_path_str.as_ref())
                                .bind(session_id)
                                .execute(pool)
                                .await;

                                // 删除已合并到视频的图片文件
                                let mut deleted_count = 0;
                                for frame_path in &frame_paths {
                                    if std::path::Path::new(frame_path).exists() {
                                        if let Err(e) = std::fs::remove_file(frame_path) {
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
        }
    }

    info!("历史图片处理完成");
}

// ==================== 应用入口 ====================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tauri::Builder::default()
        .setup(|app| {
            info!("初始化屏幕活动分析器...");

            let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;

            // 创建必要的目录
            let frames_dir = app_dir.join("frames");
            let videos_dir = app_dir.join("videos");
            let temp_dir = app_dir.join("temp");

            std::fs::create_dir_all(&frames_dir).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&videos_dir).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

            // 初始化运行时
            let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;

            let state = runtime.block_on(async {
                // 初始化数据库
                let db = Database::new(&app_dir.join("data.db").to_string_lossy())
                    .await
                    .expect("数据库初始化失败");
                let db = Arc::new(db);

                // 初始化设置管理器
                let settings = Arc::new(
                    SettingsManager::new(app_dir.join("config.json"))
                        .await
                        .expect("设置管理器初始化失败"),
                );

                // 初始化截屏管理器
                let capture =
                    Arc::new(ScreenCapture::new(frames_dir.clone()).expect("截屏管理器初始化失败"));

                // 初始化LLM管理器
                let llm_manager = Arc::new(Mutex::new(LLMManager::new()));

                // 初始化存储清理器
                let cleaner = Arc::new(StorageCleaner::new(
                    db.clone(),
                    frames_dir.clone(),
                    videos_dir.clone(),
                ));

                // 读取初始配置
                let initial_config = settings.get().await;

                // 从配置加载截屏设置
                if let Some(capture_settings) = initial_config.capture_settings.clone() {
                    capture.update_settings(capture_settings.clone()).await;
                    info!("已加载截屏配置: {:?}", capture_settings);
                }

                // 从配置加载LLM设置
                if let Some(llm_config) = initial_config.llm_config.clone() {
                    let qwen_config = llm::QwenConfig {
                        api_key: llm_config.api_key,
                        model: llm_config.model,
                        base_url: llm_config.base_url,
                        use_video_mode: llm_config.use_video_mode,
                        video_path: None,
                    };

                    let mut llm = llm_manager.lock().await;
                    if let Err(e) = llm.configure(qwen_config).await {
                        error!("加载LLM配置失败: {}", e);
                    } else {
                        info!("已从配置文件加载LLM设置");
                    }
                }

                if let Err(e) = cleaner
                    .set_retention_days(initial_config.retention_days)
                    .await
                {
                    error!("设置保留天数失败: {}", e);
                }

                // 初始化视频处理器
                let video_processor = Arc::new(
                    VideoProcessor::new(videos_dir, temp_dir).expect("视频处理器初始化失败"),
                );

                // 初始化调度器
                let mut scheduler_inner = CaptureScheduler::new(capture.clone());
                scheduler_inner.configure(
                    initial_config.capture_interval,
                    initial_config.summary_interval,
                );
                let scheduler = Arc::new(scheduler_inner);

                // 初始化系统状态
                let system_status = Arc::new(RwLock::new(SystemStatus::default()));

                let analysis_lock = Arc::new(tokio::sync::Mutex::new(()));

                AppState {
                    capture,
                    db,
                    llm_manager,
                    cleaner,
                    video_processor,
                    system_status,
                    scheduler,
                    settings,
                    analysis_lock,
                }
            });

            // 启动后台任务
            {
                let state_clone = state.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        info!("启动后台任务...");

                        // 程序启动时，先处理历史图片
                        info!("开始处理历史图片，生成视频...");
                        process_historical_frames(&state_clone).await;

                        // 创建LLM处理器（带视频处理器）
                        let llm_processor = Arc::new(LLMProcessor::with_video_processor(
                            state_clone.llm_manager.clone(),
                            state_clone.db.clone(),
                            state_clone.video_processor.clone(),
                            state_clone.settings.clone(),
                        ));

                        // 启动调度器
                        state_clone.scheduler.clone().start(llm_processor);

                        // 启动存储清理任务
                        state_clone.cleaner.clone().start_cleanup_task().await;

                        // 周期性扫描视频目录，处理未分析的视频
                        {
                            let video_state = state_clone.clone();
                            let analysis_lock = video_state.analysis_lock.clone();
                            tokio::spawn(async move {
                                loop {
                                    match analysis_lock.try_lock() {
                                        Ok(_guard) => {
                                            match analyze_unprocessed_videos(
                                                &video_state,
                                                None,
                                                false,
                                            )
                                            .await
                                            {
                                                Ok(report) => {
                                                    if report.processed > 0
                                                        || report.failed > 0
                                                    {
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
                                        }
                                        Err(_) => {
                                            trace!("跳过本轮自动视频分析：已有任务执行中");
                                        }
                                    }
                                    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                                }
                            });
                        }

                        // 更新系统状态
                        {
                            let mut status = state_clone.system_status.write().await;
                            status.is_capturing = true;
                        }

                        info!("所有后台任务已启动");

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
            get_activities,
            get_day_sessions,
            get_session_detail,
            get_app_config,
            update_config,
            add_manual_tag,
            get_system_status,
            toggle_capture,
            trigger_analysis,
            generate_video,
            get_video_url,
            get_video_data,
            test_generate_videos,
            cleanup_storage,
            get_storage_stats,
            configure_qwen,
            configure_llm_provider,
            test_capture,
            test_llm_api,
            retry_session_analysis,
            regenerate_timeline,
            delete_session,
            open_storage_folder,
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
    _session_id: i64,
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

    let persisted_config = state.settings.get().await;
    let mut llm = state.llm_manager.lock().await;

    let qwen_config = if let Some(llm_config) = persisted_config.llm_config {
        llm::QwenConfig {
            api_key: llm_config.api_key,
            model: llm_config.model,
            base_url: llm_config.base_url,
            use_video_mode: llm_config.use_video_mode,
            video_path: Some(video_path_str.clone()),
        }
    } else {
        let config = llm.get_config().await;
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

    if let Err(e) = llm.configure(qwen_config).await {
        return Err(e.to_string());
    }

    let now = Utc::now();

    // 准备会话
    let session_id = if let Some(existing_id) = reuse_session {
        if let Err(e) = sqlx::query("DELETE FROM video_segments WHERE session_id = ?")
            .bind(existing_id)
            .execute(state.db.get_pool())
            .await
        {
            llm.set_video_path(None);
            return Err(format!("清理历史视频分段失败: {}", e));
        }

        if let Err(e) = sqlx::query("DELETE FROM timeline_cards WHERE session_id = ?")
            .bind(existing_id)
            .execute(state.db.get_pool())
            .await
        {
            llm.set_video_path(None);
            return Err(format!("清理历史时间线卡片失败: {}", e));
        }

        existing_id
    } else {
        let temp_session = storage::Session {
            id: None,
            start_time: session_start,
            end_time: session_end,
            title: format!("视频分析中: {}", file_stem),
            summary: "正在分析...".to_string(),
            video_path: Some(video_path_str.clone()),
            tags: "[]".to_string(),
            created_at: Some(now),
        };

        match state.db.insert_session(&temp_session).await {
            Ok(id) => id,
            Err(e) => {
                llm.set_video_path(None);
                return Err(e.to_string());
            }
        }
    };

    llm.set_provider_database(state.db.clone(), Some(session_id));

    // 设置视频速率乘数（从配置获取）
    let speed_multiplier = persisted_config.video_config.speed_multiplier;
    llm.set_video_speed(speed_multiplier);

    let analysis = match llm
        .segment_video_and_generate_timeline(vec![], duration_minutes, None)
        .await
    {
        Ok(res) => res,
        Err(e) => {
            llm.set_video_path(None);
            let error_msg = e.to_string();
            // 检测是否是视频过短的错误
            if error_msg.contains("The video file is too short") {
                return Err(format!("VIDEO_TOO_SHORT:{}", error_msg));
            }
            return Err(error_msg);
        }
    };

    llm.set_video_path(None);
    drop(llm);

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

        if let Err(e) = state.db.insert_video_segments(&segment_records).await {
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

        if let Err(e) = state.db.insert_timeline_cards(&card_records).await {
            return Err(format!("保存时间线卡片失败: {}", e));
        }
    }

    let summary =
        llm::build_session_summary(session_start, session_end, &segments, &timeline_cards);

    if let Err(e) = sqlx::query(
        r#"
        UPDATE sessions
        SET title = ?1, summary = ?2, tags = ?3, video_path = ?4
        WHERE id = ?5
        "#,
    )
    .bind(&summary.title)
    .bind(&summary.summary)
    .bind(serde_json::to_string(&summary.tags).unwrap_or_else(|_| "[]".to_string()))
    .bind(&video_path_str)
    .bind(session_id)
    .execute(state.db.get_pool())
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

    let videos_dir = state.video_processor.output_dir.clone();

    let mut video_files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&videos_dir) {
        for entry in entries.flatten() {
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

    let pool = state.db.get_pool();
    let analyzed_videos = sqlx::query(
        r#"        SELECT DISTINCT video_path        FROM sessions        WHERE video_path IS NOT NULL        AND summary != '{}'        AND summary != ''        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut analyzed_paths = HashSet::new();
    for row in analyzed_videos {
        if let Ok(path) = sqlx::Row::try_get::<Option<String>, _>(&row, "video_path") {
            if let Some(p) = path {
                analyzed_paths.insert(p);
            }
        }
    }

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

    if mark_status {
        let mut status = state.system_status.write().await;
        status.is_processing = true;
        status.last_error = None;
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

    let error_clone = processing_error.clone();
    {
        let mut status = state.system_status.write().await;
        if mark_status {
            status.is_processing = false;
        }
        status.last_process_time = Some(chrono::Utc::now());
        status.last_error = error_clone.clone();
    }

    if let Some(err) = processing_error {
        return Err(err);
    }

    Ok(report)
}
