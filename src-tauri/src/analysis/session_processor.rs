//! 会话处理器
//!
//! 负责处理历史会话数据，包括：
//! - 为未生成视频的历史会话生成视频文件
//! - 清理已合并到视频的图片文件

use crate::AppState;
use tracing::{error, info};

pub async fn process_historical_frames(state: &AppState) -> Result<(), String> {
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

        info!(
            "处理会话 {}: {} - {}",
            session_id, session.start_time, session.end_time
        );

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

                let video_path_buf = state
                    .analysis_domain
                    .get_video_processor()
                    .output_dir
                    .join(&video_filename);
                match state
                    .analysis_domain
                    .get_video_processor()
                    .create_summary_video(frame_paths.clone(), &video_path_buf, &video_config)
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
                        if let Err(e) = db
                            .update_session_video_path(session_id, &video_path_str)
                            .await
                        {
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
