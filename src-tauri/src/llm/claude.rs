// Anthropic Claude 提供商实现
//
// 参考 Ollama provider 的图片处理逻辑：将帧图片转为 base64 发送给 Claude API

use super::plugin::*;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Local, Utc};
use claude_agent_sdk::{
    message::parse_message,
    transport::{PromptInput, SubprocessTransport},
    types::{ClaudeAgentOptions, ContentBlock as AgentContentBlock, Message as AgentMessage},
    Transport,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, warn};

/// 读取 Claude CLI 的会话令牌（Windows 平台）
#[cfg(target_os = "windows")]
fn read_claude_cli_session_token() -> Option<String> {
    use std::fs;

    // 尝试多个可能的配置路径
    let possible_paths = vec![
        // 方式1: %USERPROFILE%\.claude\config.json
        std::env::var("USERPROFILE")
            .ok()
            .map(|home| PathBuf::from(home).join(".claude").join("config.json")),
        // 方式2: %APPDATA%\Claude\config.json
        std::env::var("APPDATA")
            .ok()
            .map(|appdata| PathBuf::from(appdata).join("Claude").join("config.json")),
        // 方式3: %LOCALAPPDATA%\Claude\config.json
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|localappdata| PathBuf::from(localappdata).join("Claude").join("config.json")),
    ];

    for path_option in possible_paths {
        if let Some(config_path) = path_option {
            if !config_path.exists() {
                debug!("Claude CLI 配置文件不存在: {:?}", config_path);
                continue;
            }

            info!("找到 Claude CLI 配置文件: {:?}", config_path);

            if let Ok(content) = fs::read_to_string(&config_path) {
                info!("成功读取 Claude CLI 配置文件");

                if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                    // 打印配置文件的所有字段名（不打印值）
                    if let Some(obj) = config.as_object() {
                        let field_names: Vec<&String> = obj.keys().collect();
                        info!("配置文件包含字段: {:?}", field_names);
                    }

                    // 尝试多个可能的字段名
                    let session_key = config
                        .get("sessionKey")
                        .or_else(|| config.get("session_key"))
                        .or_else(|| config.get("token"))
                        .or_else(|| config.get("api_key"))
                        .or_else(|| config.get("anthropic_api_key"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if let Some(ref key) = session_key {
                        info!(
                            "成功从 Claude CLI 配置读取会话令牌 (长度: {} 字符)",
                            key.len()
                        );
                        return Some(key.clone());
                    } else {
                        warn!("Claude CLI 配置文件中未找到会话令牌字段");
                    }
                } else {
                    warn!("无法解析 Claude CLI 配置文件为 JSON");
                }
            } else {
                warn!("无法读取 Claude CLI 配置文件: {:?}", config_path);
            }
        }
    }

    warn!("未能从任何位置读取到 Claude CLI 会话令牌");
    None
}

/// 读取 Claude CLI 的会话令牌（非 Windows 平台）
#[cfg(not(target_os = "windows"))]
fn read_claude_cli_session_token() -> Option<String> {
    // 非 Windows 平台，SDK 应该能正常工作
    None
}

/// Claude Provider - 使用 Messages API 进行视觉分析
pub struct ClaudeProvider {
    api_key: Option<String>,
    model: String,
    db: Option<Arc<crate::storage::Database>>,
    current_session_id: Option<i64>,
    last_call_ids: Mutex<HashMap<String, i64>>,
    /// 当前会话的视频路径（如果设置，会从视频提取帧）
    session_video_path: Option<String>,
    /// 当前分析的绝对时间窗口（UTC）
    session_window_start: Option<DateTime<Utc>>,
    session_window_end: Option<DateTime<Utc>>,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        Self {
            api_key: None,
            model: "claude-sonnet-4-5".to_string(),
            db: None,
            current_session_id: None,
            last_call_ids: Mutex::new(HashMap::new()),
            session_video_path: None,
            session_window_start: None,
            session_window_end: None,
        }
    }

    /// 设置会话视频路径（如果设置，会从视频提取帧）
    pub fn set_video_path(&mut self, video_path: Option<String>) {
        self.session_video_path = video_path;
    }

    /// 设置视频速率（Claude 不需要，保持接口兼容）
    pub fn set_video_speed(&mut self, _speed_multiplier: f32) {
        // Claude 不需要视频速率参数
    }

    pub fn set_session_window(&mut self, start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) {
        self.session_window_start = start;
        self.session_window_end = end;
    }

    pub fn set_database(&mut self, db: Arc<crate::storage::Database>) {
        self.db = Some(db);
    }

    pub fn set_session_id(&mut self, session_id: i64) {
        self.current_session_id = Some(session_id);
    }

    fn reset_call_id(&self, call_type: &str) {
        if let Ok(mut map) = self.last_call_ids.lock() {
            map.remove(call_type);
        }
    }

    fn record_call_id(&self, call_type: &str, id: i64) {
        if let Ok(mut map) = self.last_call_ids.lock() {
            map.insert(call_type.to_string(), id);
        }
    }

    pub fn last_llm_call_id(&self, call_type: &str) -> Option<i64> {
        self.last_call_ids
            .lock()
            .ok()
            .and_then(|map| map.get(call_type).copied())
    }

    /// 将图片文件转换为 base64
    async fn image_to_base64(&self, path: &str) -> Result<String> {
        let image_data = tokio::fs::read(path).await?;
        Ok(general_purpose::STANDARD.encode(&image_data))
    }

    /// 从视频中提取帧（参考 Ollama Provider 的 extractFrames）
    /// 提取策略：每60秒提取1帧，最多30帧
    async fn extract_frames_from_video(
        &self,
        video_path: &str,
        target_frames: usize,
        duration_seconds: Option<u32>,
    ) -> Result<Vec<String>> {
        use crate::video::ffmpeg_helper;
        use std::path::Path;

        let target_frames = target_frames.max(1);
        let duration_seconds = duration_seconds.unwrap_or(0);

        info!("从视频提取帧: {}", video_path);

        let ffmpeg_path = ffmpeg_helper::get_ffmpeg_path()?;

        let random_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let base_dir = std::env::temp_dir().join(format!("claude_frames_{}", random_id));

        async fn extract_with_filter(
            ffmpeg_path: &Path,
            video_path: &str,
            output_dir: &std::path::Path,
            filter: &str,
            target_frames: usize,
        ) -> Result<Vec<String>> {
            if output_dir.exists() {
                let _ = tokio::fs::remove_dir_all(output_dir).await;
            }
            tokio::fs::create_dir_all(output_dir).await?;

            let output_pattern = output_dir.join("frame_%04d.jpg");
            let pattern_str = output_pattern
                .to_str()
                .ok_or_else(|| anyhow!("无法转换输出路径为字符串"))?;

            // 添加锐化滤镜（unsharp=5:5:0.8:5:5:0.0）
            // 参数说明：luma_msize_x:luma_msize_y:luma_amount:chroma_msize_x:chroma_msize_y:chroma_amount
            // luma_amount=0.8 提供适度锐化，避免过度锐化产生噪点
            let filter_with_sharpen = format!("{},unsharp=5:5:0.8:5:5:0.0", filter);

            let mut command = tokio::process::Command::new(ffmpeg_path);
            command
                .arg("-i")
                .arg(video_path)
                .arg("-vf")
                .arg(filter_with_sharpen)
                .arg("-vsync")
                .arg("vfr")
                .arg("-frames:v")
                .arg(target_frames.to_string())
                .arg("-q:v")
                .arg("6") // JPEG质量：2=最高质量，10=最低质量。6提供良好的压缩比和质量平衡
                .arg(pattern_str)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());

            #[cfg(target_os = "windows")]
            {
                // Windows 下隐藏控制台窗口
                const CREATE_NO_WINDOW: u32 = 0x08000000;
                #[allow(unused_imports)]
                use std::os::windows::process::CommandExt;
                command.creation_flags(CREATE_NO_WINDOW);
            }

            let status = command.status().await?;

            if !status.success() {
                return Err(anyhow!("ffmpeg 提取帧失败"));
            }

            let mut frame_paths = Vec::new();
            let mut entries = tokio::fs::read_dir(output_dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("jpg") {
                    frame_paths.push(path.to_string_lossy().to_string());
                }
            }

            frame_paths.sort();
            Ok(frame_paths)
        }

        let primary_filter = if duration_seconds > 0 {
            let fps_value = (target_frames as f32 / duration_seconds as f32).max(0.1);
            format!("fps={:.6}", fps_value)
        } else {
            "fps=1".to_string()
        };

        let primary_dir = base_dir.join("primary");
        let mut frame_paths = extract_with_filter(
            Path::new(&ffmpeg_path),
            video_path,
            &primary_dir,
            &primary_filter,
            target_frames,
        )
        .await?;

        if frame_paths.len() < target_frames {
            info!("主采样仅提取到 {} 帧，尝试回退为 fps=1", frame_paths.len());
            let fallback_dir = base_dir.join("fallback_fps1");
            frame_paths = extract_with_filter(
                Path::new(&ffmpeg_path),
                video_path,
                &fallback_dir,
                "fps=1",
                target_frames,
            )
            .await?;
        }

        if frame_paths.len() > target_frames {
            let total = frame_paths.len();
            let mut sampled = Vec::with_capacity(target_frames);
            for i in 0..target_frames {
                let position =
                    ((i as f32 + 0.5) * total as f32 / target_frames as f32).floor() as usize;
                let index = position.min(total - 1);
                sampled.push(frame_paths[index].clone());
            }
            frame_paths = sampled;
        }

        if frame_paths.len() < target_frames {
            if frame_paths.is_empty() {
                warn!("未能从视频提取到任何帧");
            } else {
                warn!(
                    "最终仅提取到 {} 帧，视频可能过短或帧率过低",
                    frame_paths.len()
                );
            }
        }

        info!("从视频提取了 {} 帧", frame_paths.len());
        Ok(frame_paths)
    }
    /// 获取图片的 media type
    fn get_media_type(path: &str) -> &str {
        if path.ends_with(".png") {
            "image/png"
        } else if path.ends_with(".gif") {
            "image/gif"
        } else if path.ends_with(".webp") {
            "image/webp"
        } else {
            "image/jpeg"
        }
    }

    /// 调用 Claude Agent SDK（带重试机制）
    async fn call_claude_api_with_retry(
        &self,
        system_prompt: String,
        user_content: Vec<Value>,
        call_type: &str,
    ) -> Result<String> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_SECS: u64 = 60;

        let mut last_error = None;

        for attempt in 1..=MAX_RETRIES {
            match self
                .call_claude_api(system_prompt.clone(), user_content.clone(), call_type)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    let error_msg = e.to_string();

                    // 检查是否是超时错误
                    if error_msg.contains("Timeout") || error_msg.contains("timed out") {
                        warn!(
                            "Claude API 调用超时 (尝试 {}/{}): {}",
                            attempt, MAX_RETRIES, error_msg
                        );

                        last_error = Some(e);

                        // 如果不是最后一次尝试，等待后重试
                        if attempt < MAX_RETRIES {
                            info!("等待 {} 秒后重试...", RETRY_DELAY_SECS);
                            tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_DELAY_SECS))
                                .await;
                        }
                    } else {
                        // 非超时错误，直接返回
                        error!("Claude API 调用失败（非超时错误）: {}", error_msg);
                        return Err(e);
                    }
                }
            }
        }

        // 所有重试都失败
        Err(last_error.unwrap_or_else(|| anyhow!("Claude API 调用失败")))
    }

    /// 调用 Claude Agent SDK（流式模式，支持视觉分析）
    ///
    /// 已知限制：在 Windows Release 版本中，SubprocessTransport 会创建临时的
    /// 黑色控制台窗口。这是 claude-agent-sdk 库的限制。
    /// 解决方案：使用 API Key 配置可以避免此问题。
    async fn call_claude_api(
        &self,
        system_prompt: String,
        user_content: Vec<Value>,
        call_type: &str,
    ) -> Result<String> {
        let api_key = self.api_key.clone();
        let auth_mode = if api_key.is_some() {
            "direct-key"
        } else {
            "cli-session"
        };

        let start_time = std::time::Instant::now();
        self.reset_call_id(call_type);

        // 计算请求大小
        let mut total_image_bytes = 0usize;
        let mut image_count = 0usize;
        let mut text_bytes = 0usize;

        for item in &user_content {
            if let Some(item_type) = item.get("type").and_then(|v| v.as_str()) {
                match item_type {
                    "image" => {
                        image_count += 1;
                        if let Some(data) = item
                            .get("source")
                            .and_then(|s| s.get("data"))
                            .and_then(|d| d.as_str())
                        {
                            total_image_bytes += data.len();
                        }
                    }
                    "text" => {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            text_bytes += text.len();
                        }
                    }
                    _ => {}
                }
            }
        }

        let request_body_bytes = serde_json::to_string(&user_content)
            .map(|s| s.len())
            .unwrap_or(0);

        info!(
            "调用 Claude Agent SDK: {} | 图片: {} 张 ({:.2} MB base64) | 文本: {:.2} KB | 总请求体: {:.2} MB",
            call_type,
            image_count,
            total_image_bytes as f64 / 1024.0 / 1024.0,
            text_bytes as f64 / 1024.0,
            request_body_bytes as f64 / 1024.0 / 1024.0
        );

        let message_payload = json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": user_content
            }
        });

        let request_snapshot = json!({
            "model": self.model,
            "system_prompt": system_prompt,
            "message": message_payload["message"],
            "call_type": call_type,
            "transport": "claude-agent-sdk",
            "auth_mode": auth_mode
        });

        let mut llm_record = crate::storage::LLMCallRecord {
            id: None,
            session_id: self.current_session_id,
            provider: "claude".to_string(),
            model: self.model.clone(),
            call_type: call_type.to_string(),
            request_headers: json!({
                "transport": "claude-agent-sdk",
                "mode": "stream"
            })
            .to_string(),
            // 使用 sanitize_request_body 清理图片 base64 数据
            request_body: crate::llm::sanitize_request_body(&request_snapshot),
            response_headers: None,
            response_body: None,
            status_code: None,
            error_message: None,
            latency_ms: None,
            token_usage: None,
            created_at: crate::storage::local_now(),
        };

        let mut options = ClaudeAgentOptions::builder()
            .system_prompt(system_prompt.clone())
            .max_turns(1)
            .build();
        options.model = Some(self.model.clone());
        options.include_partial_messages = true;
        // 确保每次调用都是新会话，不继续之前的对话
        options.continue_conversation = false;
        options.resume = None;
        let stream_timeout_secs = std::env::var("CLAUDE_AGENT_STREAM_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(180);
        options.stream_timeout_secs = Some(stream_timeout_secs);
        info!("Claude Agent 流读取超时: {} 秒", stream_timeout_secs);
        if let Some(api_key) = api_key {
            options.env.insert("ANTHROPIC_API_KEY".to_string(), api_key);
        } else {
            // Windows 下尝试读取 Claude CLI 会话令牌
            #[cfg(target_os = "windows")]
            {
                if let Some(session_token) = read_claude_cli_session_token() {
                    info!("使用 Claude CLI 会话令牌");
                    options.env.insert("ANTHROPIC_API_KEY".to_string(), session_token);
                } else {
                    warn!("Windows 下未找到 Claude CLI 会话令牌，将依赖 SDK 默认行为");
                }
            }
        }

        // 传递代理设置到子进程
        let mut proxy_configured = false;
        if let Ok(http_proxy) = std::env::var("HTTP_PROXY") {
            options
                .env
                .insert("HTTP_PROXY".to_string(), http_proxy.clone());
            options
                .env
                .insert("http_proxy".to_string(), http_proxy.clone());
            info!("Claude Agent 使用 HTTP 代理: {}", http_proxy);
            proxy_configured = true;
        }
        if let Ok(https_proxy) = std::env::var("HTTPS_PROXY") {
            options
                .env
                .insert("HTTPS_PROXY".to_string(), https_proxy.clone());
            options
                .env
                .insert("https_proxy".to_string(), https_proxy.clone());
            info!("Claude Agent 使用 HTTPS 代理: {}", https_proxy);
            proxy_configured = true;
        }
        if let Ok(no_proxy) = std::env::var("NO_PROXY") {
            options.env.insert("NO_PROXY".to_string(), no_proxy.clone());
            options.env.insert("no_proxy".to_string(), no_proxy);
        }
        if !proxy_configured {
            info!("Claude Agent 未检测到代理配置，直连");
        }

        let mut transport = SubprocessTransport::new(PromptInput::Stream, options, None)
            .map_err(|e| anyhow!("初始化 Claude Agent 失败: {e}"))?;
        transport
            .connect()
            .await
            .map_err(|e| anyhow!("连接 Claude Agent 失败: {e}"))?;

        let message_line = format!(
            "{}
",
            serde_json::to_string(&message_payload)?
        );
        transport
            .write(&message_line)
            .await
            .map_err(|e| anyhow!("发送消息失败: {e}"))?;
        transport
            .end_input()
            .await
            .map_err(|e| anyhow!("关闭输入流失败: {e}"))?;

        let mut message_rx = transport.read_messages();
        let mut streamed_text = String::new();
        let mut final_text: Option<String> = None;
        let mut usage_snapshot: Option<String> = None;
        let mut collected_events: Vec<Value> = Vec::new();
        let mut stream_error: Option<anyhow::Error> = None;
        let mut finished = false;

        while let Some(message_result) = message_rx.recv().await {
            match message_result {
                Ok(raw_value) => {
                    collected_events.push(raw_value.clone());
                    match parse_message(raw_value.clone()) {
                        Ok(agent_message) => match agent_message {
                            AgentMessage::Assistant { message, .. } => {
                                let mut pieces = Vec::new();
                                for block in message.content {
                                    if let AgentContentBlock::Text { text } = block {
                                        pieces.push(text);
                                    }
                                }
                                if !pieces.is_empty() {
                                    final_text = Some(pieces.join(
                                        "
",
                                    ));
                                }
                            }
                            AgentMessage::StreamEvent { event, .. } => {
                                if let Some(event_type) = event.get("type").and_then(|v| v.as_str())
                                {
                                    match event_type {
                                        "content_block_delta" => {
                                            if let Some(delta) = event.get("delta") {
                                                if let Some(text) =
                                                    delta.get("text").and_then(|v| v.as_str())
                                                {
                                                    streamed_text.push_str(text);
                                                } else if let Some(nested) = delta.get("delta") {
                                                    if let Some(text) =
                                                        nested.get("text").and_then(|v| v.as_str())
                                                    {
                                                        streamed_text.push_str(text);
                                                    }
                                                }
                                            }
                                        }
                                        "message_delta" => {
                                            if let Some(delta) = event.get("delta") {
                                                if let Some(text) =
                                                    delta.get("text").and_then(|v| v.as_str())
                                                {
                                                    streamed_text.push_str(text);
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            AgentMessage::Result { usage, .. } => {
                                if let Some(value) = usage {
                                    usage_snapshot = Some(value.to_string());
                                }
                                finished = true;
                            }
                            _ => {}
                        },
                        Err(err) => {
                            let parse_err = anyhow!("解析 Claude Agent 消息失败: {err}");
                            error!("{}", parse_err);
                            stream_error = Some(parse_err);
                        }
                    }
                }
                Err(e) => {
                    let read_err = anyhow!("读取 Claude Agent 流失败: {e}");
                    error!("{}", read_err);
                    stream_error = Some(read_err);
                    break;
                }
            }

            if finished {
                break;
            }
        }

        if let Err(e) = transport.close().await {
            warn!("关闭 Claude Agent 进程时出错: {}", e);
        }

        let mut response_text = final_text.unwrap_or_else(|| streamed_text.clone());
        response_text = response_text.trim().to_string();

        if collected_events.is_empty() {
            llm_record.response_body = None;
        } else {
            llm_record.response_body = serde_json::to_string(&collected_events).ok();
        }
        llm_record.token_usage = usage_snapshot;
        llm_record.latency_ms = Some(start_time.elapsed().as_millis() as i64);

        if response_text.is_empty() {
            let err = stream_error
                .take()
                .unwrap_or_else(|| anyhow!("Claude Agent 未返回任何内容"));
            llm_record.error_message = Some(err.to_string());
            if let Some(ref db) = self.db {
                let _ = db.insert_llm_call(&llm_record).await;
            }
            return Err(err);
        }

        llm_record.error_message = stream_error.as_ref().map(|e| e.to_string());

        if let Some(ref db) = self.db {
            match db.insert_llm_call(&llm_record).await {
                Ok(id) => self.record_call_id(call_type, id),
                Err(e) => error!("保存 LLM 调用记录失败: {}", e),
            }
        }

        Ok(response_text)
    }
    /// 解析 JSON 响应
    ///
    /// 支持多种格式：
    /// 1. 纯 JSON: `[{...}]`
    /// 2. Markdown 代码块: ` ```json\n[{...}]\n``` `
    /// 3. Markdown 代码块（无语言标记）: ` ```\n[{...}]\n``` `
    /// 4. 包含其他文本的响应，提取 JSON 部分
    fn extract_json(response: &str) -> Result<serde_json::Value> {
        // 先 trim 去除前后空白字符
        let response = response.trim();

        // 尝试直接解析
        match serde_json::from_str::<serde_json::Value>(response) {
            Ok(value) => return Ok(value),
            Err(e) => {
                debug!("直接解析 JSON 失败: {}", e);
            }
        }

        // 尝试提取 markdown 代码块中的 JSON
        // 这个方法能处理 ```json 和 ``` 两种格式
        if let Some(start_marker) = response.find("```") {
            // 找到代码块开始标记
            let after_marker = &response[start_marker + 3..];

            // 跳过语言标识（如 "json"）到下一行
            let content_start = if let Some(newline_pos) = after_marker.find('\n') {
                start_marker + 3 + newline_pos + 1
            } else {
                start_marker + 3
            };

            // 查找结束标记
            if let Some(end_pos) = response[content_start..].find("```") {
                let json_str = response[content_start..content_start + end_pos].trim();

                // 尝试解析提取的内容
                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(value) => return Ok(value),
                    Err(e) => {
                        debug!("从代码块提取 JSON 失败: {}", e);
                    }
                }
            }
        }

        // 尝试提取第一个 [ 或 { 到最后一个 ] 或 }
        if let Some(start) = response.find('[').or_else(|| response.find('{')) {
            if let Some(end) = response.rfind(']').or_else(|| response.rfind('}')) {
                let json_str = &response[start..=end];

                // 记录提取的内容用于调试
                tracing::debug!("提取的 JSON 字符串长度: {}", json_str.len());
                tracing::debug!(
                    "提取的 JSON 前100字符: {:?}",
                    json_str.chars().take(100).collect::<String>()
                );

                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(value) => return Ok(value),
                    Err(parse_err) => {
                        tracing::warn!("JSON 解析失败: {}, 尝试清理后重试", parse_err);

                        // 尝试清理字符串（移除不可见字符）
                        let cleaned = json_str
                            .chars()
                            .filter(|c| !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t')
                            .collect::<String>();

                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&cleaned) {
                            tracing::info!("清理后的 JSON 解析成功");
                            return Ok(value);
                        }
                    }
                }
            }
        }

        Err(anyhow::anyhow!("无法从响应中提取有效的 JSON"))
    }

    /// 构建视频分段提示词
    fn build_segment_prompt(&self, duration: u32) -> String {
        let session_window_info = if let (Some(start), Some(end)) = (
            self.session_window_start.as_ref(),
            self.session_window_end.as_ref(),
        ) {
            let start_local = start.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S");
            let end_local = end.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S");
            format!(
                r#"## Session Context (for reference only):
- Actual start time: {start_local}
- Actual end time: {end_local}
- Note: These are absolute times for context. Your output should use relative MM:SS format."#
            )
        } else {
            "## Session Context: Not provided.".to_string()
        };

        format!(
            r#"# Video Analysis Task
Analyze these screenshots from a screen recording session and create meaningful activity segments.

{}

## CRITICAL TIME FORMAT:
- This is a {} minute screen recording
- Use relative time format: MM:SS (minutes:seconds)
- Video time 00:00 to {:02}:00 represents the session duration
- Example: "00:00" = start, "05:30" = 5 minutes 30 seconds, "{:02}:00" = end

## Output Format (JSON only):
[
  {{
    "startTimestamp": "00:00",
    "endTimestamp": "05:00",
    "description": "1-3 sentences describing what happened (in Chinese)"
  }}
]

## Requirements:
- Create 2-5 segments that cover the full recording period
- Use relative timestamp format: MM:SS (分钟:秒)
- Group related activities together
- Write descriptions in Chinese
- **CRITICAL**: Return ONLY the JSON array, NO markdown formatting, NO code blocks, NO ```json markers
- Output must be valid JSON that can be parsed directly
- Do NOT wrap the JSON in any markdown syntax"#,
            session_window_info,
            duration,
            duration,
            duration
        )
    }

    /// 构建 timeline 生成提示词
    fn build_timeline_prompt(&self, previous_cards: &Option<Vec<TimelineCard>>) -> String {
        let previous_cards_json = if let Some(cards) = previous_cards {
            serde_json::to_string_pretty(cards).unwrap_or_else(|_| "[]".to_string())
        } else {
            "[]".to_string()
        };

        let session_window_info = if let (Some(start), Some(end)) = (
            self.session_window_start.as_ref(),
            self.session_window_end.as_ref(),
        ) {
            let start_local = start.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S");
            let end_local = end.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S");
            format!(
                "## Session Context (for reference only):
- Actual start time: {start_local}
- Actual end time: {end_local}
- Note: These are absolute times for context. Your output should use relative MM:SS format.",
            )
        } else {
            "## Session Context: Not provided.".to_string()
        };

        format!(
            r#"Create a SINGLE timeline activity card that summarises the entire session.

{}

## CRITICAL TIME FORMAT:
- Input segments use relative time format: MM:SS (minutes:seconds)
- Your output MUST also use MM:SS format
- Example: "00:00", "05:30", "15:00"
- DO NOT use absolute time (YYYY-MM-DD HH:MM:SS)

## Rules:
- Output must be a JSON array with **exactly one** object (数组长度必须为1)。
- 每个字段必须存在：`startTime`、`endTime`、`category`、`subcategory`、`title`、`summary`、`detailedSummary`、`distractions`、`appSites`、`appSites.primary`、`appSites.secondary`、`isUpdated`。
- `startTime` = 各 segment 最早开始时间 (MM:SS 相对时间)，`endTime` = 各 segment 最晚结束时间 (MM:SS 相对时间)。
- `category` 必须从 [work, communication, learning, personal, idle, other] 中选择最符合的一个。
- 所有文本字段使用中文描述，`summary` 为一句话概述，`detailedSummary` 需包含各 segment 的时间点与活动内容，并引用相对时间（例如 "00:00-05:00"）。
- `distractions` 必须是数组，若无干扰请返回 []；如果存在干扰对象，必须包含 `startTime`、`endTime`、`title`、`summary` 字段，均使用 MM:SS 相对时间和中文描述。
- `appSites.secondary` 必须是数组，若无元素返回 []，不要使用字符串。
- 如果识别到主要应用/站点，请填写 `appSites.primary`。
- 输出结果禁止使用 Markdown 或代码块标记（不要包裹 ```json）。
- 可以参考历史卡片（如下），保持字段兼容。

## Previous Cards:
{}

## Output Format (JSON only):
[
  {{
    "startTime": "00:00",
    "endTime": "15:00",
    "category": "work",
    "subcategory": "Development",
    "title": "功能开发",
    "summary": "持续开发新功能模块",
    "detailedSummary": "连续工作，完成了功能模块的开发和测试",
    "distractions": [],
    "appSites": {{
      "primary": "visualstudio.com",
      "secondary": ["github.com"]
    }},
    "isUpdated": false
  }}
]

Return ONLY the JSON array (确保startTime/endTime等字段存在，并使用相对时间格式 MM:SS)。"#,
            session_window_info,
            previous_cards_json
        )
    }

    fn collapse_timeline_cards(cards: Vec<TimelineCard>) -> TimelineCard {
        let mut cards = cards;
        let len = cards.len();
        if len == 0 {
            return TimelineCard {
                start_time: "00:00".to_string(),
                end_time: "00:00".to_string(),
                category: "other".to_string(),
                subcategory: "General".to_string(),
                title: "空时间线".to_string(),
                summary: "本时段无有效活动".to_string(),
                detailed_summary: "未检测到任何活动".to_string(),
                distractions: None,
                app_sites: AppSites {
                    primary: "unknown".to_string(),
                    secondary: Some(vec![]),
                },
                video_preview_path: None,
            };
        }

        cards.sort_by(|a, b| a.start_time.cmp(&b.start_time));
        let first = cards.first().unwrap();
        let last = cards.last().unwrap();

        let title = if len == 1 {
            first.title.clone()
        } else {
            format!("{} 等 {} 项活动", first.title, len)
        };

        let summary = if len == 1 {
            first.summary.clone()
        } else {
            cards
                .iter()
                .map(|c| format!("{}-{} {}", c.start_time, c.end_time, c.title))
                .collect::<Vec<_>>()
                .join("；")
        };

        let detailed_summary = cards
            .iter()
            .map(|c| format!("{}-{}: {}", c.start_time, c.end_time, c.detailed_summary))
            .collect::<Vec<_>>()
            .join("；");

        let mut secondary_set: HashSet<String> = HashSet::new();
        for card in &cards {
            if let Some(ref secs) = card.app_sites.secondary {
                for item in secs {
                    if !item.trim().is_empty() {
                        secondary_set.insert(item.clone());
                    }
                }
            }
        }
        let mut secondary_vec: Vec<String> = secondary_set.into_iter().collect();
        secondary_vec.sort();

        let mut all_distractions = Vec::new();
        for card in &cards {
            if let Some(ref list) = card.distractions {
                all_distractions.extend(list.clone());
            }
        }
        let distractions = if all_distractions.is_empty() {
            None
        } else {
            Some(all_distractions)
        };

        let primary_app = cards
            .iter()
            .find(|c| !c.app_sites.primary.is_empty())
            .map(|c| c.app_sites.primary.clone())
            .unwrap_or_else(|| first.app_sites.primary.clone());

        let video_preview_path = cards.iter().find_map(|c| c.video_preview_path.clone());

        TimelineCard {
            start_time: first.start_time.clone(),
            end_time: last.end_time.clone(),
            category: first.category.clone(),
            subcategory: first.subcategory.clone(),
            title,
            summary,
            detailed_summary,
            distractions,
            app_sites: AppSites {
                primary: primary_app,
                secondary: if secondary_vec.is_empty() {
                    Some(vec![])
                } else {
                    Some(secondary_vec)
                },
            },
            video_preview_path,
        }
    }
}

#[async_trait]
impl LLMProvider for ClaudeProvider {
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn set_session_window(&mut self, start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) {
        self.session_window_start = start;
        self.session_window_end = end;
    }

    async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary> {
        info!("Claude 开始分析 {} 帧图像", frames.len());

        // 采样帧（最多 30 张）
        let sampled_frames = if frames.len() > 30 {
            let step = (frames.len() as f32 / 30.0).ceil() as usize;
            let step = step.max(1);
            frames.iter().step_by(step).take(30).cloned().collect()
        } else {
            frames
        };

        // 构建包含图片的消息内容（参考 Ollama 的处理方式）
        let mut user_content = Vec::new();

        // 添加图片
        for frame_path in &sampled_frames {
            if let Ok(base64) = self.image_to_base64(frame_path).await {
                let media_type = Self::get_media_type(frame_path);
                user_content.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": base64
                    }
                }));
            }
        }

        // 添加文本提示
        user_content.push(json!({
            "type": "text",
            "text": r#"Analyze these screenshots and summarize the activity.
Return JSON:
{
  "title": "Activity title (in Chinese)",
  "summary": "Detailed description (in Chinese)",
  "productivity_score": 75,
  "focus_score": 80
}

Return ONLY the JSON object."#
        }));

        let system_prompt = "You are analyzing computer screen activity.".to_string();

        let response = self
            .call_claude_api_with_retry(system_prompt, user_content, "analyze_frames")
            .await?;

        let json_value = match Self::extract_json(&response) {
            Ok(value) => value,
            Err(err) => {
                info!("Claude analyze_frames 原始响应: {}", response);
                return Err(err);
            }
        };

        let now = crate::storage::local_now();
        Ok(SessionSummary {
            title: json_value["title"]
                .as_str()
                .unwrap_or("未命名会话")
                .to_string(),
            summary: json_value["summary"].as_str().unwrap_or("").to_string(),
            tags: vec![],
            start_time: now - chrono::Duration::minutes(15),
            end_time: now,
            key_moments: vec![],
            productivity_score: json_value["productivity_score"].as_f64().map(|v| v as f32),
            focus_score: json_value["focus_score"].as_f64().map(|v| v as f32),
        })
    }

    async fn segment_video(&self, frames: Vec<String>, duration: u32) -> Result<Vec<VideoSegment>> {
        info!(
            "Claude 开始分析视频 segments: {} 帧, 时长 {} 分钟",
            frames.len(),
            duration
        );

        // 调试：打印 session_window 设置
        if let (Some(start), Some(end)) = (&self.session_window_start, &self.session_window_end) {
            info!("Session window 已设置: {:?} 到 {:?}", start, end);
        } else {
            warn!(
                "Session window 未设置！start={:?}, end={:?}",
                self.session_window_start, self.session_window_end
            );
        }

        // 优先从视频文件提取帧（最多30帧）
        let frames_to_use = if let Some(ref video_path) = self.session_video_path {
            info!("从视频文件提取帧: {}", video_path);
            match self
                .extract_frames_from_video(video_path, 30, Some(duration.saturating_mul(60)))
                .await
            {
                Ok(extracted_frames) => {
                    info!("成功从视频提取 {} 帧", extracted_frames.len());
                    extracted_frames
                }
                Err(e) => {
                    info!("视频提取失败，回退到使用传入的帧: {}", e);
                    frames.clone()
                }
            }
        } else {
            // 没有视频路径，使用传入的帧
            frames.clone()
        };

        // Claude 要求至少 10 帧才能进行有效分析
        if frames_to_use.len() < 10 {
            warn!(
                "Claude 提取的帧数不足 ({} < 10)，认为这段时间没有足够的屏幕活动",
                frames_to_use.len()
            );
            return Err(anyhow::anyhow!(
                "VIDEO_TOO_SHORT: 提取的帧数 {} 少于最小要求 10 帧，这段时间可能没有屏幕活动",
                frames_to_use.len()
            ));
        }

        // 采样帧（最多 30 张）
        let sampled_frames = if frames_to_use.len() > 30 {
            let step = (frames_to_use.len() as f32 / 30.0).ceil() as usize;
            let step = step.max(1);
            frames_to_use
                .iter()
                .step_by(step)
                .take(30)
                .cloned()
                .collect()
        } else {
            frames_to_use
        };

        let mut user_content = Vec::new();

        // 添加图片（参考 Ollama: data:image/jpeg;base64,{base64}）
        for frame_path in &sampled_frames {
            if let Ok(base64) = self.image_to_base64(frame_path).await {
                let media_type = Self::get_media_type(frame_path);
                user_content.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": base64
                    }
                }));
            }
        }

        // 添加文本提示
        let prompt = self.build_segment_prompt(duration);
        user_content.push(json!({
            "type": "text",
            "text": prompt
        }));

        let system_prompt =
            "You are analyzing screen recording to create activity segments.".to_string();

        let response = self
            .call_claude_api_with_retry(system_prompt, user_content, "segment_video")
            .await?;

        let json_value = match Self::extract_json(&response) {
            Ok(value) => value,
            Err(err) => {
                error!("Claude segment_video JSON 提取失败: {}", err);
                // 将完整响应写入文件以便调试
                if let Err(e) = std::fs::write(
                    std::env::temp_dir().join("claude_segment_video_response.txt"),
                    &response
                ) {
                    warn!("无法写入调试文件: {}", e);
                }
                info!("Claude segment_video 原始响应已保存到: {:?}",
                    std::env::temp_dir().join("claude_segment_video_response.txt"));
                info!("Claude segment_video 原始响应: {}", response);
                // 添加更详细的调试信息
                info!("响应长度: {} 字节", response.len());
                info!(
                    "响应前50字符: {:?}",
                    response.chars().take(50).collect::<String>()
                );
                if response.len() > 50 {
                    info!(
                        "响应后50字符: {:?}",
                        response.chars().rev().take(50).collect::<String>()
                    );
                }
                // 尝试手动提取 JSON
                if let Some(start) = response.find('[') {
                    if let Some(end) = response.rfind(']') {
                        let json_str = &response[start..=end];
                        info!("提取的 JSON 长度: {}", json_str.len());
                        if let Err(parse_err) = serde_json::from_str::<serde_json::Value>(json_str)
                        {
                            error!("手动提取的 JSON 解析失败: {}", parse_err);
                        }
                    }
                }
                return Err(err);
            }
        };

        let segments: Vec<VideoSegment> = match serde_json::from_value(json_value.clone()) {
            Ok(segments) => segments,
            Err(err) => {
                error!("无法解析 Claude segment_video JSON: {}", err);
                let raw_json = serde_json::to_string_pretty(&json_value)
                    .unwrap_or_else(|_| json_value.to_string());
                info!("Claude segment_video 原始 JSON: {}", raw_json);
                info!("Claude segment_video 原始文本: {}", response);
                return Err(err.into());
            }
        };

        info!("Claude 视频分段完成: {} 个 segment", segments.len());
        Ok(segments)
    }

    async fn generate_timeline(
        &self,
        segments: Vec<VideoSegment>,
        previous_cards: Option<Vec<TimelineCard>>,
    ) -> Result<Vec<TimelineCard>> {
        info!("Claude 开始生成 timeline: {} 个 segments", segments.len());

        let mut prompt = self.build_timeline_prompt(&previous_cards);
        prompt.push_str("\n\nCurrent video segments:\n");
        prompt.push_str(&serde_json::to_string_pretty(&segments)?);

        let user_content = vec![json!({
            "type": "text",
            "text": prompt
        })];

        let system_prompt = "You are creating timeline cards from video segments.".to_string();

        let response = self
            .call_claude_api_with_retry(system_prompt, user_content, "generate_timeline")
            .await?;

        let mut json_value = match Self::extract_json(&response) {
            Ok(value) => value,
            Err(err) => {
                info!("Claude generate_timeline 原始响应: {}", response);
                return Err(err);
            }
        };
        crate::llm::plugin::normalize_timeline_cards_value(&mut json_value);

        let mut cards: Vec<TimelineCard> = match serde_json::from_value(json_value.clone()) {
            Ok(cards) => cards,
            Err(err) => {
                error!("无法解析 Claude generate_timeline JSON: {}", err);
                let raw_json = serde_json::to_string_pretty(&json_value)
                    .unwrap_or_else(|_| json_value.to_string());
                info!("Claude generate_timeline 原始 JSON: {}", raw_json);
                info!("Claude generate_timeline 原始文本: {}", response);
                return Err(err.into());
            }
        };

        if cards.is_empty() {
            return Err(anyhow!("Claude 未返回任何时间线卡片"));
        }

        if cards.len() > 1 {
            info!(
                "Claude Timeline 返回 {} 张卡片，合并为单一卡片",
                cards.len()
            );
            let merged = Self::collapse_timeline_cards(cards);
            cards = vec![merged];
        }

        info!("Claude Timeline 生成完成: {} 个卡片", cards.len());
        Ok(cards)
    }

    async fn generate_day_summary(&self, date: &str, sessions: &[SessionBrief]) -> Result<String> {
        let total_minutes: i64 = sessions
            .iter()
            .map(|s| (s.end_time - s.start_time).num_minutes())
            .sum();

        let mut sessions_text = String::new();
        for session in sessions.iter().take(50) {
            use chrono::TimeZone;
            let start_local = chrono::Local.from_utc_datetime(&session.start_time.naive_utc());
            let end_local = chrono::Local.from_utc_datetime(&session.end_time.naive_utc());
            sessions_text.push_str(&format!(
                "\n- {} - {}: {}\n  {}",
                start_local.format("%H:%M"),
                end_local.format("%H:%M"),
                session.title,
                session.summary
            ));
        }

        let prompt = format!(
            r#"基于以下今日屏幕活动记录，生成一份工作总结：

日期: {}
会话数: {}
总时长: {} 分钟

今日活动时间线:
{}

要求：
1. 使用中文，语气自然、专业
2. 重点总结真正在做什么工作/活动
3. 按时间顺序或主题归纳今天的主要工作内容
4. 字数控制在 150-200 字以内

请直接返回总结文本（纯文本，不要 JSON，不要 markdown）。"#,
            date,
            sessions.len(),
            total_minutes,
            sessions_text
        );

        let user_content = vec![json!({
            "type": "text",
            "text": prompt
        })];

        let system_prompt = "You are creating a daily work summary.".to_string();

        let response = self
            .call_claude_api_with_retry(system_prompt, user_content, "generate_day_summary")
            .await?;

        Ok(response.trim().to_string())
    }

    fn name(&self) -> &str {
        "Claude"
    }

    fn configure(&mut self, config: serde_json::Value) -> Result<()> {
        if let Some(api_key) = config.get("api_key").and_then(|v| v.as_str()) {
            if api_key.is_empty() {
                self.api_key = None;
            } else {
                self.api_key = Some(api_key.to_string());
            }
        }

        if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
            self.model = model.to_string();
        }

        if self.api_key.is_none() {
            if let Ok(env_key) = std::env::var("CLAUDE_API_KEY") {
                if !env_key.is_empty() {
                    self.api_key = Some(env_key);
                    info!("Claude 使用环境变量 CLAUDE_API_KEY 作为凭据");
                }
            }
        }

        let auth_mode = if self.api_key.is_some() {
            "direct-key"
        } else {
            "cli-session"
        };

        info!(
            "Claude 提供商已配置: model={}, auth_mode={}",
            self.model, auth_mode
        );
        Ok(())
    }

    fn is_configured(&self) -> bool {
        true
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            vision_support: true,
            batch_analysis: true,
            streaming: true,
            max_input_tokens: 200000,
            supported_image_formats: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
                "gif".to_string(),
                "webp".to_string(),
            ],
        }
    }
}
