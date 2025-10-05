//! 视频分析器
//!
//! 提供视频文件的LLM分析功能，包括：
//! - 单个视频的分析处理（analyze_video_once）
//! - 批量未处理视频的自动分析（analyze_unprocessed_videos）

use crate::llm;
use crate::storage;
use crate::utils::parse_video_window_from_stem;
use crate::AppState;
use chrono;
use std::path::{Path, PathBuf};
use tracing::{error, info};

#[derive(Default)]
pub struct VideoAnalysisReport {
    pub total_candidates: usize,
    pub processed: usize,
    pub failed: usize,
    pub messages: Vec<String>,
}

pub struct VideoAnalysisOutcome {
    #[allow(dead_code)]
    pub _session_id: i64, // 保留用于未来可能的扩展
    pub segments_count: usize,
    pub timeline_count: usize,
    pub summary: llm::SessionSummary,
}

pub async fn analyze_video_once(
    state: &AppState,
    video_path: &Path,
    session_start: chrono::DateTime<chrono::Utc>,
    session_end: chrono::DateTime<chrono::Utc>,
    duration_minutes: u32,
    reuse_session: Option<i64>,
) -> Result<VideoAnalysisOutcome, String> {
    let video_path_str = video_path.to_string_lossy().to_string();
    let file_stem = video_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("视频");

    let persisted_config = state.storage_domain.get_settings().get().await;
    let llm_handle = state.analysis_domain.get_llm_handle();

    // 根据当前 provider 配置 LLM
    let current_provider = persisted_config.llm_provider.as_str();

    match current_provider {
        "openai" => {
            // 注意：启动时已经根据配置切换到了正确的 provider，这里无需再次切换

            // Qwen provider 需要 API key
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
                return Err("请先在设置中配置 Qwen API Key".to_string());
            }

            if let Err(e) = llm_handle.configure(qwen_config).await {
                return Err(e.to_string());
            }
        }
        "claude" => {
            // Claude provider 无需额外配置
            // 注意：启动时已经根据配置切换到了正确的 provider，这里无需再次切换
            info!("使用 Claude provider 进行视频分析（API key 可选）");
        }
        _ => {
            return Err(format!("不支持的 LLM provider: {}", current_provider));
        }
    }

    // 设置视频路径
    if let Err(e) = llm_handle
        .set_video_path(Some(video_path_str.clone()))
        .await
    {
        return Err(e.to_string());
    }

    let now = storage::local_now();

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

        match state
            .storage_domain
            .get_db()
            .await?
            .insert_session(&temp_session)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                let _ = llm_handle.set_video_path(None).await;
                return Err(e.to_string());
            }
        }
    };

    llm_handle
        .set_provider_database(
            state.storage_domain.get_db().await?.clone(),
            Some(session_id),
        )
        .await
        .map_err(|e| e.to_string())?;

    llm_handle
        .set_session_window(Some(session_start), Some(session_end))
        .await
        .map_err(|e| e.to_string())?;

    // 设置视频速率乘数（从配置获取）
    let speed_multiplier = persisted_config.video_config.speed_multiplier;
    llm_handle
        .set_video_speed(speed_multiplier)
        .await
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
    let _ = llm_handle.set_session_window(None, None).await;

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

        if let Err(e) = state
            .storage_domain
            .get_db()
            .await?
            .insert_video_segments(&segment_records)
            .await
        {
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

        if let Err(e) = state
            .storage_domain
            .get_db()
            .await?
            .insert_timeline_cards(&card_records)
            .await
        {
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

pub async fn analyze_unprocessed_videos(
    state: &AppState,
    limit: Option<usize>,
    mark_status: bool,
) -> Result<VideoAnalysisReport, String> {
    use std::collections::HashSet;

    let videos_dir = state
        .analysis_domain
        .get_video_processor()
        .output_dir
        .clone();

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
        state
            .system_domain
            .get_status_handle()
            .set_processing(true)
            .await;
        state
            .system_domain
            .get_status_handle()
            .set_error(None)
            .await;
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
                let end = storage::local_now();
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
        state
            .system_domain
            .get_status_handle()
            .set_processing(false)
            .await;
    }
    state
        .system_domain
        .get_status_handle()
        .update_last_process_time(storage::local_now())
        .await;
    state
        .system_domain
        .get_status_handle()
        .set_error(processing_error.clone())
        .await;

    if let Some(err) = processing_error {
        return Err(err);
    }

    Ok(report)
}
