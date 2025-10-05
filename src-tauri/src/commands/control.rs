//! 系统控制命令
//!
//! 提供系统状态监控和控制接口，包括：
//! - 系统状态查询
//! - 捕获控制
//! - 分析触发
//! - 标签管理
//! - 会话管理

use crate::analysis::{analyze_unprocessed_videos, analyze_video_once};
use crate::models::{ActivityTag, StorageUsage, SystemStatus};
use crate::storage;
use crate::AppState;
use std::path::PathBuf;
use tracing::{error, info};

/// 验证会话ID是否有效（防止SQL注入和无效输入）
pub fn validate_session_id(id: i64) -> Result<(), String> {
    if id < 0 {
        return Err(format!("无效的会话 ID: {}", id));
    }
    Ok(())
}

/// 获取系统状态
#[tauri::command]
pub async fn get_system_status(
    state: tauri::State<'_, AppState>,
) -> Result<SystemStatus, String> {
    let mut status = state.system_domain.get_status_handle().get().await;

    // 获取存储统计信息
    if let Ok(cleaner) = state.storage_domain.get_cleaner().await {
        if let Ok(storage_stats) = cleaner.get_storage_stats().await {
            status.storage_usage = StorageUsage {
                database_size: storage_stats.database_size as u64,
                frames_size: storage_stats.frames_size as u64,
                videos_size: storage_stats.videos_size as u64,
                total_size: storage_stats.total_size as u64,
                session_count: storage_stats.session_count as u32,
                frame_count: storage_stats.frame_count as u32,
            };
        }
    }

    Ok(status)
}

/// 切换截屏状态（暂停/恢复）
#[tauri::command]
pub async fn toggle_capture(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    state
        .system_domain
        .get_status_handle()
        .set_capturing(enabled)
        .await;

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
pub async fn trigger_analysis(state: tauri::State<'_, AppState>) -> Result<String, String> {
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

/// 添加手动标签
#[tauri::command]
pub async fn add_manual_tag(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    tag: ActivityTag,
) -> Result<(), String> {
    validate_session_id(session_id)?;
    // 获取当前会话
    let session_detail = state
        .storage_domain
        .get_db()
        .await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // 添加新标签
    let mut tags = session_detail.tags;
    tags.push(tag);

    // 更新数据库
    let tags_json = serde_json::to_string(&tags).map_err(|e| e.to_string())?;

    state
        .storage_domain
        .get_db()
        .await?
        .update_session_tags(session_id, &tags_json)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 删除标签
#[tauri::command]
pub async fn remove_tag(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    tag_index: usize,
) -> Result<(), String> {
    validate_session_id(session_id)?;
    // 获取当前会话
    let session_detail = state
        .storage_domain
        .get_db()
        .await?
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
        .storage_domain
        .get_db()
        .await?
        .update_session_tags(session_id, &tags_json)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 删除会话
#[tauri::command]
pub async fn delete_session(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<String, String> {
    validate_session_id(session_id)?;
    info!("删除会话: {}", session_id);

    // 获取会话详情
    let session_detail = state
        .storage_domain
        .get_db()
        .await?
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
    state
        .storage_domain
        .get_db()
        .await?
        .delete_session(session_id)
        .await
        .map_err(|e| format!("删除会话失败: {}", e))?;

    Ok("会话已成功删除".to_string())
}

/// 重新分析指定会话
#[tauri::command]
pub async fn retry_session_analysis(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<String, String> {
    validate_session_id(session_id)?;
    info!("重新分析会话: {}", session_id);

    // 已移除 analysis_lock 临时方案，直接执行分析
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

    let result = async {
        let session_detail = state
            .storage_domain
            .get_db()
            .await?
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
        state
            .storage_domain
            .get_db()
            .await?
            .delete_video_segments_by_session(session_id)
            .await
            .map_err(|e| e.to_string())?;

        state
            .storage_domain
            .get_db()
            .await?
            .delete_timeline_cards_by_session(session_id)
            .await
            .map_err(|e| e.to_string())?;

        state
            .storage_domain
            .get_db()
            .await?
            .update_session(
                session_id,
                &session_detail.session.title,
                "重新分析中...",
                None,
                "[]",
            )
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

    state
        .system_domain
        .get_status_handle()
        .set_processing(false)
        .await;
    state
        .system_domain
        .get_status_handle()
        .update_last_process_time(storage::local_now())
        .await;
    state
        .system_domain
        .get_status_handle()
        .set_error(last_error.clone())
        .await;

    match result {
        Ok(outcome) => Ok(format!(
            "重新分析完成: {} ({} 片段, {} 卡片)",
            outcome.summary.title, outcome.segments_count, outcome.timeline_count
        )),
        Err(err) => Err(err),
    }
}

/// 重新生成timeline
#[tauri::command]
pub async fn regenerate_timeline(
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
        let video_segments: Vec<crate::llm::plugin::VideoSegment> = segments
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

                crate::llm::plugin::VideoSegment {
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
        llm_handle
            .set_provider_database(
                state.storage_domain.get_db().await?.clone(),
                Some(session_id),
            )
            .await
            .map_err(|e| format!("设置数据库失败: {}", e))?;

        // 设置视频速率乘数（虽然generate_timeline不直接使用，但保持一致性）
        let app_config = state.storage_domain.get_settings().get().await;
        let speed_multiplier = app_config.video_config.speed_multiplier;
        llm_handle
            .set_video_speed(speed_multiplier)
            .await
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
                        let start_abs = crate::llm::relative_to_absolute(
                            session_start,
                            session_end,
                            &card.start_time,
                        );
                        let end_abs = crate::llm::relative_to_absolute(
                            session_start,
                            session_end,
                            &card.end_time,
                        );
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
                        created_at: storage::local_now(),
                    }
                })
                .collect();

            if let Err(e) = state
                .storage_domain
                .get_db()
                .await?
                .insert_timeline_cards(&card_records)
                .await
            {
                error!("保存时间线卅片失败: {}", e);
            }
        }
    }

    Ok(format!(
        "重新生成完成：处理了 {} 个分段，生成了 {} 个时间线卡片",
        total_segments, total_cards
    ))
}
