//! Notion 集成命令
//!
//! 提供 Notion API 集成接口，包括：
//! - 连接测试
//! - 配置更新
//! - 页面搜索
//! - 数据库创建

use crate::models::{self, AppConfig};
use crate::notion;
use crate::AppState;
use tracing::info;

/// 测试 Notion API 连接
#[tauri::command]
pub async fn test_notion_connection(
    _state: tauri::State<'_, AppState>,
    api_token: String,
) -> Result<String, String> {
    info!("测试 Notion API 连接");

    // 创建临时配置用于测试
    let temp_config = models::NotionConfig {
        enabled: true,
        api_token: api_token.clone(),
        database_id: String::new(), // 测试时不需要
        sync_options: Default::default(),
        max_retries: 3,
    };

    // 创建临时客户端测试连接
    let temp_client = notion::NotionClient::new(temp_config).map_err(|e| e.to_string())?;

    // 测试连接 - 搜索一下用户空间看是否有权限
    temp_client
        .search_pages()
        .await
        .map(|pages| format!("连接成功！可以访问 {} 个页面/数据库", pages.len()))
        .map_err(|e| e.to_string())
}

/// 更新 Notion 配置
#[tauri::command]
pub async fn update_notion_config(
    state: tauri::State<'_, AppState>,
    config: models::NotionConfig,
) -> Result<(), String> {
    info!("更新 Notion 配置");

    // 保存配置到设置文件
    let update = AppConfig {
        notion_config: Some(config.clone()),
        ..Default::default()
    };

    state
        .storage_domain
        .get_settings()
        .update(update)
        .await
        .map_err(|e| format!("保存配置失败: {}", e))?;

    // 重新初始化 Notion 客户端
    state
        .storage_domain
        .get_notion_manager()
        .initialize(config)
        .await
        .map_err(|e| e.to_string())?;

    info!("Notion 配置已保存并应用");
    Ok(())
}

/// 搜索 Notion 页面和数据库
#[tauri::command]
pub async fn search_notion_pages(
    state: tauri::State<'_, AppState>,
    api_token: String,
) -> Result<Vec<notion::NotionPage>, String> {
    info!("搜索 Notion 页面和数据库");

    state
        .storage_domain
        .get_notion_manager()
        .search_pages(&api_token)
        .await
        .map_err(|e| e.to_string())
}

/// 在指定页面下创建 Notion 数据库
#[tauri::command]
pub async fn create_notion_database(
    state: tauri::State<'_, AppState>,
    api_token: String,
    parent_page_id: String,
    database_name: String,
) -> Result<String, String> {
    info!("在页面 {} 下创建数据库: {}", parent_page_id, database_name);

    state
        .storage_domain
        .get_notion_manager()
        .create_database(&api_token, &parent_page_id, &database_name)
        .await
        .map_err(|e| e.to_string())
}
