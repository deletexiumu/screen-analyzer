//! 视频处理命令
//!
//! 提供视频生成和访问接口，包括：
//! - 视频数据读取
//! - 视频URL获取
//! - 视频文件生成
//! - 批量视频生成测试

use crate::models::VideoSettings;
use crate::video;
use crate::{storage, AppState, FRAME_SAMPLE_INTERVAL_SECONDS};
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// 验证会话ID是否有效（防止SQL注入和无效输入）
fn validate_session_id(id: i64) -> Result<(), String> {
    if id < 0 {
        return Err(format!("无效的会话 ID: {}", id));
    }
    Ok(())
}

/// 获取视频数据
#[tauri::command]
pub async fn get_video_data(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<Vec<u8>, String> {
    validate_session_id(session_id)?;
    use tokio::fs;

    // 获取会话详情
    let session = state
        .storage_domain
        .get_db()
        .await?
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
pub async fn get_video_url(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<String, String> {
    validate_session_id(session_id)?;
    // 获取会话详情
    let session = state
        .storage_domain
        .get_db()
        .await?
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
pub async fn generate_video(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    speed_multiplier: Option<f32>,
) -> Result<String, String> {
    validate_session_id(session_id)?;
    info!("生成会话视频: session_id={}", session_id);

    // 获取会话详情
    let session_detail = state
        .storage_domain
        .get_db()
        .await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // 提取帧路径 - 实现抽帧策略：每5秒取一帧
    let all_frames = &session_detail.frames;

    // 如果没有帧，处理错误
    if all_frames.is_empty() {
        error!("会话 {} 没有截图帧，删除该会话", session_id);
        if let Err(e) = state
            .storage_domain
            .get_db()
            .await?
            .delete_session(session_id)
            .await
        {
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
        .analysis_domain
        .get_video_processor()
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
        .analysis_domain
        .get_video_processor()
        .create_summary_video(frame_paths, &output_path, &config)
        .await
        .map_err(|e| e.to_string())?;

    // 更新数据库中的视频路径
    state
        .storage_domain
        .get_db()
        .await?
        .update_session_video_path(session_id, &result.file_path)
        .await
        .map_err(|e| {
            error!("更新会话视频路径失败: {}", e);
            e.to_string()
        })?;

    // 清理frame文件夹中的图片（视频已生成，不再需要原始图片）
    let mut deleted_count = 0;
    let mut failed_count = 0;

    for frame in all_frames {
        // 先检查文件是否存在
        if !std::path::Path::new(&frame.file_path).exists() {
            debug!("frame文件不存在（可能已被删除）: {}", frame.file_path);
            continue;
        }

        match tokio::fs::remove_file(&frame.file_path).await {
            Ok(_) => deleted_count += 1,
            Err(e) => {
                failed_count += 1;
                error!("清理frame文件失败 {}: {}", frame.file_path, e);
            }
        }
    }

    info!(
        "视频生成成功并已更新数据库，清理frame文件: 成功 {}, 失败 {}, 总计 {}",
        deleted_count, failed_count, all_frames.len()
    );

    Ok(result.file_path)
}

/// 测试自动生成视频 - 按 15 分钟区间批量生成
#[tauri::command]
pub async fn test_generate_videos(
    state: tauri::State<'_, AppState>,
    settings: VideoSettings,
) -> Result<Vec<String>, String> {
    use chrono::{Duration, TimeZone, Timelike, Utc};
    use std::collections::BTreeMap;

    let frames_dir = state.capture_domain.get_capture().frames_dir();
    let now = storage::local_now();

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

        let output_path = state
            .analysis_domain
            .get_video_processor()
            .output_dir
            .join(&output_name);

        let frame_list: Vec<String> = frame_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();

        // 应用帧过滤：每5秒选择一张图片
        let filtered_frame_list = video::filter_frames_by_interval(
            frame_list.clone(),
            FRAME_SAMPLE_INTERVAL_SECONDS as usize,
        );

        info!(
            "生成视频段: {} (原始 {} 帧，抽样后 {} 帧)",
            output_name,
            frame_list.len(),
            filtered_frame_list.len()
        );

        match state
            .analysis_domain
            .get_video_processor()
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
            failed_segments
                .iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
        );
    }

    Ok(generated_videos)
}
