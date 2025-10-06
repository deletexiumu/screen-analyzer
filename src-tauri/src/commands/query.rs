//! 数据查询命令
//!
//! 提供各类数据查询接口，包括：
//! - 数据库状态查询
//! - 活动列表查询
//! - 会话信息查询
//! - 日总结查询

use crate::models::*;
use crate::domains;
use crate::AppState;

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
pub async fn get_database_status(
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
pub async fn get_activities(
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
pub async fn get_day_sessions(
    state: tauri::State<'_, AppState>,
    date: String,
) -> Result<Vec<Session>, String> {
    state
        .storage_domain
        .get_db()
        .await?
        .get_sessions_by_date(&date)
        .await
        .map_err(|e| e.to_string())
}

/// 获取某天的总结数据
///
/// # 参数
/// * `date` - 日期 (YYYY-MM-DD)
/// * `force_refresh` - 是否强制重新生成（默认 false，使用缓存）
#[tauri::command]
pub async fn get_day_summary(
    state: tauri::State<'_, AppState>,
    date: String,
    force_refresh: Option<bool>,
) -> Result<domains::summary::DaySummary, String> {
    let db = state.storage_domain.get_db().await?;
    let llm_handle = state.analysis_domain.get_llm_handle();
    let generator = domains::summary::SummaryGenerator::with_llm(db, llm_handle.clone());
    generator
        .generate_day_summary(&date, force_refresh.unwrap_or(false))
        .await
}

/// 获取会话详情
#[tauri::command]
pub async fn get_session_detail(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<SessionDetail, String> {
    validate_session_id(session_id)?;
    state
        .storage_domain
        .get_db()
        .await?
        .get_session_detail(session_id)
        .await
        .map_err(|e| e.to_string())
}
