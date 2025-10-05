//! 测试工具命令
//!
//! 提供各种功能测试接口，包括：
//! - 截屏功能测试
//! - LLM API 测试
//! - OpenAI 文本 API 测试
//! - Claude SDK API 测试

use crate::AppState;
use tracing::{error, info};

/// 测试截屏功能
#[tauri::command]
pub async fn test_capture(state: tauri::State<'_, AppState>) -> Result<String, String> {
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

/// 测试LLM API连接
#[tauri::command]
pub async fn test_llm_api(
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
        "claude" | "anthropic" => {
            // 测试 Claude (使用 claude-agent-sdk)
            test_claude_sdk_api(config).await
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
pub async fn test_openai_text_api(config: serde_json::Value) -> Result<String, String> {
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

/// 测试 Claude Agent SDK API
pub async fn test_claude_sdk_api(config: serde_json::Value) -> Result<String, String> {
    use claude_agent_sdk::{
        message::parse_message,
        transport::{PromptInput, SubprocessTransport},
        types::{ClaudeAgentOptions, ContentBlock as AgentContentBlock, Message as AgentMessage},
        Transport,
    };
    use serde_json::json;

    let api_key = config.get("api_key").and_then(|v| v.as_str());

    let model = config
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("claude-sonnet-4-5");

    // 构建选项
    let mut options = ClaudeAgentOptions::builder()
        .system_prompt("You are a helpful assistant.".to_string())
        .max_turns(1)
        .build();
    options.model = Some(model.to_string());
    options.include_partial_messages = true;

    // 如果提供了 API key，设置环境变量
    if let Some(key) = api_key {
        if !key.is_empty() {
            options
                .env
                .insert("ANTHROPIC_API_KEY".to_string(), key.to_string());
        }
    }

    // 创建传输层
    let mut transport = SubprocessTransport::new(PromptInput::Stream, options, None)
        .map_err(|e| format!("初始化 Claude Agent 失败: {}", e))?;

    transport
        .connect()
        .await
        .map_err(|e| format!("连接 Claude Agent 失败: {}", e))?;

    // 发送测试消息
    let message_payload = json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [{
                "type": "text",
                "text": "你好，这是一个API连接测试。请简单回复确认连接成功。"
            }]
        }
    });

    let message_line = format!("{}\n", serde_json::to_string(&message_payload).unwrap());
    transport
        .write(&message_line)
        .await
        .map_err(|e| format!("发送消息失败: {}", e))?;

    transport
        .end_input()
        .await
        .map_err(|e| format!("关闭输入流失败: {}", e))?;

    // 读取响应
    let mut message_rx = transport.read_messages();
    let mut response_text = String::new();

    while let Some(message_result) = message_rx.recv().await {
        match message_result {
            Ok(raw_value) => match parse_message(raw_value) {
                Ok(agent_message) => match agent_message {
                    AgentMessage::Assistant { message, .. } => {
                        for block in message.content {
                            if let AgentContentBlock::Text { text } = block {
                                response_text.push_str(&text);
                            }
                        }
                        break;
                    }
                    AgentMessage::StreamEvent { event, .. } => {
                        if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                            if event_type == "content_block_delta" {
                                if let Some(delta) = event.get("delta") {
                                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                        response_text.push_str(text);
                                    }
                                }
                            }
                        }
                    }
                    AgentMessage::Result { .. } => break,
                    _ => {}
                },
                Err(e) => {
                    return Err(format!("解析消息失败: {}", e));
                }
            },
            Err(e) => {
                return Err(format!("读取消息失败: {}", e));
            }
        }
    }

    let _ = transport.close().await;

    if response_text.is_empty() {
        return Err("未收到响应".to_string());
    }

    Ok(response_text)
}
