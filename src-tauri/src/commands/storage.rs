//! 存储管理命令
//!
//! 提供存储相关的管理接口，包括：
//! - 存储清理
//! - 存储统计
//! - 数据迁移
//! - 文件夹访问

use tracing::info;
use tauri::Manager;

use crate::utils::file_system::{get_log_dir as get_log_dir_impl, open_log_folder_impl};
use crate::AppState;
use crate::FolderType;

/// 手动触发存储清理
#[tauri::command]
pub async fn cleanup_storage(state: tauri::State<'_, AppState>) -> Result<(), String> {
    info!("手动触发存储清理");
    state
        .storage_domain
        .get_cleaner()
        .await?
        .trigger_cleanup()
        .await
        .map_err(|e| e.to_string())
}

/// 获取存储统计
#[tauri::command]
pub async fn get_storage_stats(
    state: tauri::State<'_, AppState>,
) -> Result<crate::storage::cleaner::StorageStats, String> {
    state
        .storage_domain
        .get_cleaner()
        .await?
        .get_storage_stats()
        .await
        .map_err(|e| e.to_string())
}

/// 迁移数据库时区：将 UTC 时间转换为本地时间
#[tauri::command]
pub async fn migrate_timezone_to_local(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    info!("开始数据库时区迁移");

    let db = state.storage_domain.get_db().await?;
    let (sessions, frames, llm_calls, video_segments, timeline_cards, day_summaries) = db
        .migrate_timezone_to_local()
        .await
        .map_err(|e| format!("时区迁移失败: {}", e))?;

    let message = format!(
        "时区迁移完成！\n\
         - 会话记录: {} 条\n\
         - 帧记录: {} 条\n\
         - LLM 调用记录: {} 条\n\
         - 视频分段记录: {} 条\n\
         - 时间线卡片: {} 条\n\
         - 每日总结: {} 条",
        sessions, frames, llm_calls, video_segments, timeline_cards, day_summaries
    );

    info!("{}", message);
    Ok(message)
}

/// 刷新历史数据的设备信息
#[tauri::command]
pub async fn refresh_device_info(state: tauri::State<'_, AppState>) -> Result<u64, String> {
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
pub async fn sync_data_to_mariadb(
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

/// 打开存储文件夹（使用枚举类型防止路径遍历攻击）
#[tauri::command]
pub async fn open_storage_folder(
    state: tauri::State<'_, AppState>,
    folder_type: FolderType,
) -> Result<(), String> {
    use crate::utils::file_system::open_folder_in_explorer;

    let path = match folder_type {
        FolderType::Frames => state.capture_domain.get_capture().frames_dir(),
        FolderType::Videos => state
            .analysis_domain
            .get_video_processor()
            .output_dir
            .clone(),
    };

    open_folder_in_explorer(&path)
}

/// 获取日志目录路径
#[tauri::command]
pub fn get_log_dir() -> Result<String, String> {
    let log_dir = get_log_dir_impl()?;
    Ok(log_dir.to_string_lossy().to_string())
}

/// 打开日志文件夹
#[tauri::command]
pub fn open_log_folder() -> Result<(), String> {
    open_log_folder_impl()
}
