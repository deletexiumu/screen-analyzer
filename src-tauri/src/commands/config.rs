//! 配置管理命令
//!
//! 提供应用配置的读取和更新接口，包括：
//! - 应用配置的获取和更新
//! - LLM 服务商配置
//! - Qwen 配置

use crate::llm;
use crate::models::{self, AppConfig, PersistedAppConfig};
use crate::AppState;
use tracing::info;

/// 获取应用配置
#[tauri::command]
pub async fn get_app_config(
    state: tauri::State<'_, AppState>,
) -> Result<PersistedAppConfig, String> {
    Ok(state.storage_domain.get_settings().get().await)
}

/// 更新配置
#[tauri::command]
pub async fn update_config(
    state: tauri::State<'_, AppState>,
    config: AppConfig,
) -> Result<PersistedAppConfig, String> {
    let updated_config = state
        .storage_domain
        .get_settings()
        .update(config.clone())
        .await
        .map_err(|e| e.to_string())?;

    // 更新保留天数
    if let Some(retention_days) = config.retention_days {
        // 直接调用cleaner的方法，不需要获取可变引用
        state
            .storage_domain
            .get_cleaner()
            .await?
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
        state
            .capture_domain
            .get_capture()
            .update_settings(capture_settings.clone())
            .await;
        info!("截屏配置已更新: {:?}", capture_settings);
    }

    // 更新日志配置
    if let Some(logger_settings) = config.logger_settings {
        state
            .system_domain
            .get_logger()
            .set_enabled(logger_settings.enable_frontend_logging);
        info!(
            "日志配置已更新: 前端日志推送 = {}",
            logger_settings.enable_frontend_logging
        );
    }

    Ok(updated_config)
}

/// 配置Qwen
#[tauri::command]
pub async fn configure_qwen(
    state: tauri::State<'_, AppState>,
    config: llm::QwenConfig,
) -> Result<(), String> {
    state
        .analysis_domain
        .get_llm_handle()
        .configure(config)
        .await
        .map_err(|e| e.to_string())
}

/// 配置LLM提供商（统一接口）
#[tauri::command]
pub async fn configure_llm_provider(
    state: tauri::State<'_, AppState>,
    provider: String,
    config: serde_json::Value,
) -> Result<(), String> {
    info!("配置LLM提供商: {}", provider);

    // 根据 provider 构建配置
    let llm_provider_config = match provider.as_str() {
        "openai" => {
            // Qwen (通过 OpenAI 兼容接口)
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

            models::LLMProviderConfig {
                api_key: qwen_config.api_key.clone(),
                model: qwen_config.model.clone(),
                base_url: qwen_config.base_url.clone(),
                use_video_mode: qwen_config.use_video_mode,
                auth_token: String::new(), // Qwen 不使用 auth_token
            }
        }
        "claude" => {
            // Claude (使用 claude-agent-sdk，API key 可选)
            let api_key = config
                .get("api_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let model = config
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("claude-sonnet-4-5")
                .to_string();

            let auth_token = config
                .get("auth_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let base_url = config
                .get("base_url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if !auth_token.trim().is_empty() {
                std::env::set_var("ANTHROPIC_AUTH_TOKEN", &auth_token);
                std::env::set_var("ANTHROPIC_API_KEY", &auth_token);
            } else {
                std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
                std::env::remove_var("ANTHROPIC_API_KEY");
            }

            if !base_url.trim().is_empty() {
                std::env::set_var("ANTHROPIC_BASE_URL", &base_url);
            } else {
                std::env::remove_var("ANTHROPIC_BASE_URL");
            }

            models::LLMProviderConfig {
                api_key,
                model,
                base_url,
                use_video_mode: true, // Claude 支持视频模式
                auth_token,
            }
        }
        _ => {
            return Err(format!("不支持的提供商: {}", provider));
        }
    };

    let update = models::AppConfig {
        retention_days: None,
        llm_provider: Some(provider.clone()),
        capture_interval: None,
        summary_interval: None,
        video_config: None,
        capture_settings: None,
        ui_settings: None,
        llm_config: Some(llm_provider_config),
        logger_settings: None,
        database_config: None,
        notion_config: None,
    };

    state
        .storage_domain
        .get_settings()
        .update(update)
        .await
        .map_err(|e| format!("保存配置失败: {}", e))?;

    // 配置内存中的LLM管理器
    // 注意：目前 LLM Manager 的 configure 方法只支持 QwenConfig
    // Claude 的配置在切换 provider 时自动处理
    if provider == "openai" {
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

        state
            .analysis_domain
            .get_llm_handle()
            .configure(qwen_config)
            .await
            .map_err(|e| e.to_string())?;
    }
    // Claude 的配置会在应用启动时或切换 provider 时自动加载

    info!("LLM配置已保存并应用");
    Ok(())
}
