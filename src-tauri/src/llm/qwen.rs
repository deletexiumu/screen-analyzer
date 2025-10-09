// 阿里通义千问提供商实现 - 支持视频直接上传分析

use super::plugin::*;
use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use chrono::{Local, TimeZone, Timelike};
use reqwest::{multipart, Client};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, warn};

/// Qwen提供商（阿里通义千问）
pub struct QwenProvider {
    api_key: Option<String>,
    model: String,
    client: Client,
    base_url: String,
    upload_url: String, // 新增：文件上传API地址
    db: Option<Arc<crate::storage::Database>>,
    current_session_id: Option<i64>,
    /// 是否使用视频模式（将多张图片作为视频处理）
    use_video_mode: bool,
    /// 当前会话的视频路径
    session_video_path: Option<String>,
    /// 最近一次LLM调用记录ID
    last_call_ids: Mutex<HashMap<String, i64>>,
    /// 视频速率乘数（用于提示词）
    video_speed_multiplier: f32,
}

impl QwenProvider {
    /// 创建新的Qwen提供商（接受共享的HTTP客户端以复用连接池）
    pub fn new(client: Client) -> Self {
        Self {
            api_key: None,
            model: "qwen-vl-max-latest".to_string(), // 默认使用最新的视觉语言模型
            client,
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
                .to_string(),
            upload_url: "https://dashscope.aliyuncs.com/api/v1/uploads".to_string(), // 新增上传URL
            db: None,
            current_session_id: None,
            use_video_mode: true, // 默认使用视频模式
            session_video_path: None,
            last_call_ids: Mutex::new(HashMap::new()),
            video_speed_multiplier: 8.0, // 默认8倍速
        }
    }

    /// 设置数据库连接
    pub fn set_database(&mut self, db: Arc<crate::storage::Database>) {
        self.db = Some(db);
    }

    /// 设置当前会话ID
    pub fn set_session_id(&mut self, session_id: i64) {
        self.current_session_id = Some(session_id);
    }

    /// 设置API密钥
    pub fn set_api_key(&mut self, api_key: String) {
        self.api_key = Some(api_key);
    }

    /// 设置模型
    pub fn set_model(&mut self, model: String) {
        self.model = model;
    }

    /// 设置是否使用视频模式
    pub fn set_video_mode(&mut self, use_video: bool) {
        self.use_video_mode = use_video;
    }

    /// 设置会话视频路径
    pub fn set_video_path(&mut self, video_path: Option<String>) {
        self.session_video_path = video_path;
    }
    /// 设置视频速率乘数
    pub fn set_video_speed(&mut self, speed_multiplier: f32) {
        self.video_speed_multiplier = speed_multiplier;
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

    /// 获取文件上传凭证
    async fn get_upload_policy(&self) -> Result<UploadPolicy> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("API key未配置"))?;

        let response = self
            .client
            .get(&self.upload_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .query(&[("action", "getPolicy"), ("model", &self.model)])
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("获取上传凭证失败: {}", error_text));
        }

        let policy_response: UploadPolicyResponse = response.json().await?;
        Ok(policy_response.data)
    }

    /// 上传文件到阿里云OSS
    async fn upload_file_to_oss(&self, policy: &UploadPolicy, file_path: &str) -> Result<String> {
        let path = Path::new(file_path);
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("无效的文件路径"))?;

        let key = format!("{}/{}", policy.upload_dir, file_name);
        let file_content = tokio::fs::read(file_path).await?;

        // 构建multipart form
        let form = multipart::Form::new()
            .text("OSSAccessKeyId", policy.oss_access_key_id.clone())
            .text("Signature", policy.signature.clone())
            .text("policy", policy.policy.clone())
            .text("x-oss-object-acl", policy.x_oss_object_acl.clone())
            .text(
                "x-oss-forbid-overwrite",
                policy.x_oss_forbid_overwrite.clone(),
            )
            .text("key", key.clone())
            .text("success_action_status", "200")
            .part(
                "file",
                multipart::Part::bytes(file_content).file_name(file_name.to_string()),
            );

        let response = self
            .client
            .post(&policy.upload_host)
            .header("X-DashScope-OssResourceResolve", "enable")
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("文件上传失败: {}", error_text));
        }

        Ok(format!("oss://{}", key))
    }

    /// 上传视频文件到阿里云
    async fn upload_video(&self, video_path: &str) -> Result<String> {
        info!("开始上传视频文件: {}", video_path);

        // 获取上传凭证
        let policy = self.get_upload_policy().await?;

        // 上传文件到OSS
        let oss_url = self.upload_file_to_oss(&policy, video_path).await?;

        info!("视频上传成功: {}", oss_url);
        Ok(oss_url)
    }

    /// 将图片文件转换为base64
    async fn image_to_base64(&self, path: &str) -> Result<String> {
        let image_data = tokio::fs::read(path).await?;
        Ok(general_purpose::STANDARD.encode(&image_data))
    }

    /// 构建视频分段提示词
    fn build_segment_prompt(&self, duration: u32, speed_multiplier: f32) -> String {
        let frame_interval_seconds = 5; // 每5秒抽取一帧
        let frame_display_seconds = 1; // 每帧显示1秒

        format!(
            r#"# Video Analysis Task
Your job is to transcribe someone's computer usage into a small number of meaningful activity segments.

## CRITICAL VIDEO TIME MAPPING:
- This is a {} minute screen recording video
- The video was created from screenshots taken every {} seconds
- Each frame is displayed for {} second in the video
- The video plays at {}x speed (accelerated)
- Video time 00:00 to {:02}:00 represents the actual session time
- When you see something at video time X:XX, the actual activity happened at that relative time in the session

## Golden Rule: Aim for 3-5 segments per 15-minute session (fewer is better than more)

## Core Principles:
1. **Group by purpose, not by platform** - If someone is planning a trip across 5 websites, that's ONE segment
2. **Include interruptions in the description** - Don't create segments for brief distractions
3. **Only split when context changes for 2-3+ minutes** - Quick checks don't count as context switches
4. **Combine related activities** - Multiple videos on the same topic = one segment
5. **Think in terms of "sessions"** - What would you tell a friend you spent time doing?
6. **Idle detection** - if the screen stays exactly the same for 5+ minutes, note that the user was idle

## Output Format (CRITICAL - TIME FORMAT):
[
  {{
    "startTimestamp": "00:00",  // FORMAT: MM:SS (分钟:秒) NOT HH:MM!
    "endTimestamp": "05:00",    // FORMAT: MM:SS (分钟:秒) e.g., "05:00" = 5分钟0秒
    "description": "1-3 sentences describing what the user accomplished during this period"
  }}
]

## CRITICAL - Time Format Requirements:
- ALL timestamps MUST be in MM:SS format (MINUTES:SECONDS)
- "00:00" means 0 minutes 0 seconds (start of video)
- "05:00" means 5 minutes 0 seconds (NOT 5 hours!)
- "15:00" means 15 minutes 0 seconds
- Maximum time is {:02}:00 ({} minutes)
- Do NOT use HH:MM format!

## Important:
- All timestamps must be VIDEO RELATIVE TIME within 00:00 to {:02}:00
- Use Chinese for descriptions
- The timestamps represent minutes and seconds in the video, NOT hours and minutes
- Remember: Group aggressively and only split when they truly change what they're doing for an extended period
- Focus on what the user accomplished during each time period"#,
            duration,
            frame_interval_seconds,
            frame_display_seconds,
            speed_multiplier,
            duration,
            duration,
            duration,
            duration
        )
    }

    /// 构建timeline生成提示词
    fn build_timeline_prompt(&self, previous_cards: &Option<Vec<TimelineCard>>) -> String {
        let previous_cards_json = if let Some(cards) = previous_cards {
            serde_json::to_string_pretty(cards).unwrap_or_else(|_| "[]".to_string())
        } else {
            "[]".to_string()
        };

        format!(
            r#"Based on the video segments, create timeline activity cards.
You are a digital anthropologist, observing a user's raw activity log. Your goal is to synthesize this log into a high-level, human-readable story of their session, presented as a series of timeline cards.

CRITICAL UNDERSTANDING:
- Video segments are SAMPLES/SNAPSHOTS taken during a session, NOT the actual activity duration
- Each segment's timestamps (e.g., 05:45:00 to 05:45:15) represent when that SAMPLE was taken
- You need to create timeline cards that span the ENTIRE session period, not just individual samples
- If you have segments from 05:30 to 06:30, create cards covering this FULL hour
- DO NOT create 15-second cards - that's just the sampling duration!

THE GOLDEN RULE:
Create long, meaningful cards that represent cohesive sessions of activity, ideally 30-60 minutes+.

## Categories（精简为6类，使用英文值）:
- work: 工作（编程、写作、设计、数据分析、会议、规划等专业工作）
- communication: 沟通（聊天、邮件、视频会议、团队协作等）
- learning: 学习（阅读、观看教程、研究、在线课程等）
- personal: 个人（娱乐、购物、社交媒体、财务等个人活动）
- idle: 空闲（无活动或锁屏状态）
- other: 其他（休息、运动等未分类活动）

## Previous Timeline Cards:
{}

## Merging Strategy (重要):
1. 检查 previous cards 的最后一张卡片
2. 如果新 segments 的开始时间与上一张卡片结束时间连续（间隔<5分钟）
3. 且活动类型相似（同 category/subcategory 或相关活动）
4. 则扩展该卡片的时间范围和内容，而不是创建新卡片
5. 返回时需要标注哪些是更新的卡片（使用 "isUpdated": true 标记）

## Current Video Segments:
将在下方提供当前的 video segments 信息
- startTimestamp/endTimestamp 格式是 MM:SS（分钟:秒）
- 例如 "00:00" = 0分0秒，"05:00" = 5分钟0秒，"15:00" = 15分钟0秒
- 这些时间代表视频中的位置，NOT 实际时钟时间（不是HH:MM格式！）
- 你需要基于这些segments创建覆盖整个会话的timeline cards

## Requirements:
1. Timeline cards MUST span from FIRST segment's start to LAST segment's end time
2. Create comprehensive activity cards (ideally 30-60+ minutes each)
3. Group ALL related segments together - 连续的相似活动必须合并成一个长卡片
4. DO NOT create one card per segment - analyze the overall pattern
5. Include any distractions within the main activity
6. Output must be in Chinese
7. 智能合并：多个连续的相似活动segments必须合并成一个长时间卡片
8. 标记更新：如果更新了已有卡片，需要包含完整的更新后卡片信息并标记 "isUpdated": true

## IMPORTANT - Time Format Requirements:
- 如果输入的 video segments 包含 ISO 格式时间戳（如 2025-09-28T05:30:00+00:00），请保持相同格式
- 如果输入的 video segments 使用 MM:SS 格式（如 05:00），返回也必须使用 MM:SS 格式
- startTime 应该接近第一个 segment 的开始时间
- endTime 应该接近最后一个 segment 的结束时间
- 时间格式必须与输入segments的格式完全一致

## 重要：类别选择指导
- 优先选择最具体、最准确的类别，而不是笼统的"other"
- 对于混合活动，根据时间占比选择主要类别
- 常见映射示例：
  * 编程、开发、调试、会议、规划 → work
  * 邮件、聊天、视频会议、团队协作 → communication
  * 教程、文档阅读、研究、学习新技能 → learning
  * 游戏、视频、购物、社交媒体 → personal
  * 锁屏、无活动 → idle
  * 只有真正无法归类的才用 other

## JSON Format:
[
  {{
    "startTime": "根据输入格式返回",  // 如 "00:00" (MM:SS) 或 "2025-09-28T10:00:00+00:00" (ISO)
    "endTime": "根据输入格式返回",    // 如 "30:00" (MM:SS) 表示30分钟，可跨越多个片段
    "category": "work",  // 必须从6个类别中选择最合适的，避免滥用other
    "subcategory": "Development",
    "title": "功能开发",
    "summary": "持续开发新功能模块",
    "detailedSummary": "连续90分钟的开发工作，完成了用户管理模块的增删改查功能，进行了单元测试，修复了若干bug",
    "distractions": [],
    "appSites": {{
      "primary": "vscode",
      "secondary": "github.com"
    }},
    "isUpdated": false  // 是否是对已有卡片的更新
  }}
]"#,
            previous_cards_json
        )
    }

    fn parse_timeline_cards(value: serde_json::Value) -> Result<Vec<TimelineCard>> {
        match value {
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                Ok(serde_json::from_value(value)?)
            }
            serde_json::Value::String(text) => {
                let mut inner_value = serde_json::from_str::<serde_json::Value>(&text)?;
                crate::llm::plugin::normalize_timeline_cards_value(&mut inner_value);
                Ok(serde_json::from_value(inner_value)?)
            }
            other => Err(anyhow::anyhow!("Qwen时间线返回格式不正确: {}", other)),
        }
    }

    fn fallback_timeline_cards(value: &serde_json::Value) -> Option<Vec<TimelineCard>> {
        let entries: Vec<serde_json::Value> = match value {
            serde_json::Value::Array(arr) => arr.clone(),
            serde_json::Value::String(text) => {
                match serde_json::from_str::<serde_json::Value>(text).ok()? {
                    serde_json::Value::Array(arr) => arr,
                    _ => return None,
                }
            }
            _ => return None,
        };

        let mut cards = Vec::new();
        for entry in entries {
            if let Some(card) = Self::build_fallback_card(&entry) {
                cards.push(card);
            }
        }

        if cards.is_empty() {
            None
        } else {
            Some(cards)
        }
    }

    fn build_fallback_card(value: &serde_json::Value) -> Option<TimelineCard> {
        let start = value
            .get("startTimestamp")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("startTime").and_then(|v| v.as_str()))
            .unwrap_or("00:00")
            .to_string();

        let end = value
            .get("endTimestamp")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("endTime").and_then(|v| v.as_str()))
            .unwrap_or(&start)
            .to_string();

        let description = value
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        if description.is_empty() {
            return None;
        }

        let mut title: String = description.chars().take(20).collect();
        if title.is_empty() {
            title = "屏幕活动".to_string();
        }

        Some(TimelineCard {
            start_time: start,
            end_time: end,
            category: "work".to_string(),
            subcategory: "General".to_string(),
            title,
            summary: description.clone(),
            detailed_summary: description,
            distractions: None,
            app_sites: AppSites {
                primary: "unknown".to_string(),
                secondary: None,
            },
            video_preview_path: None,
        })
    }

    /// 调用Qwen API - 支持图片base64模式
    async fn call_qwen_api(
        &self,
        prompt: String,
        images_base64: Vec<String>,
        call_type: &str,
    ) -> Result<String> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Qwen API key未配置"))?;

        let start_time = std::time::Instant::now();

        self.reset_call_id(call_type);

        // 构建消息内容
        let mut content_parts = vec![];

        // Qwen特殊处理：使用video类型来处理多张图片
        if self.use_video_mode && !images_base64.is_empty() {
            let image_urls: Vec<String> = images_base64
                .iter()
                .map(|base64| format!("data:image/jpeg;base64,{}", base64))
                .collect();

            content_parts.push(json!({
                "type": "video",
                "video": image_urls
            }));
        } else if !images_base64.is_empty() {
            // 传统图片模式
            for base64 in images_base64 {
                content_parts.push(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/jpeg;base64,{}", base64)
                    }
                }));
            }
        }

        // 添加文本提示
        content_parts.push(json!({
            "type": "text",
            "text": prompt.clone()
        }));

        let request_body = json!({
            "model": self.model,
            "response_format": {"type": "json_object"},  // 保证结构化输出
            "messages": [
                {
                    "role": "user",
                    "content": content_parts
                }
            ],
            "max_tokens": 8000,  // 增加到8000以支持长视频分析
            "temperature": 0.3
        });

        debug!(
            "调用Qwen API: model={}, base_url={}",
            self.model, self.base_url
        );

        // 记录请求信息
        let mut llm_record = crate::storage::LLMCallRecord {
            id: None,
            session_id: self.current_session_id,
            provider: "qwen".to_string(),
            model: self.model.clone(),
            call_type: call_type.to_string(),
            request_headers: json!({
                "Authorization": "Bearer ***",
                "Content-Type": "application/json",
                "X-DashScope-OssResourceResolve": "enable"
            })
            .to_string(),
            request_body: request_body.to_string(),
            response_headers: None,
            response_body: None,
            status_code: None,
            error_message: None,
            latency_ms: None,
            token_usage: None,
            created_at: crate::storage::local_now(),
        };

        let endpoint = self.base_url.clone();
        let response = self
            .client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("X-DashScope-OssResourceResolve", "enable") // 重要：解析OSS资源
            .json(&request_body)
            .send()
            .await?;

        let status_code = response.status().as_u16() as i32;
        llm_record.status_code = Some(status_code);

        // 记录响应头
        let response_headers = response.headers().clone();
        let headers_json = json!({
            "content-type": response_headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or(""),
            "x-request-id": response_headers.get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or(""),
        });
        llm_record.response_headers = Some(headers_json.to_string());

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Qwen API错误: {}", error_text);
            llm_record.error_message = Some(error_text.clone());
            llm_record.latency_ms = Some(start_time.elapsed().as_millis() as i64);

            // 保存错误记录
            if let Some(ref db) = self.db {
                if let Err(e) = db.insert_llm_call(&llm_record).await {
                    error!("保存LLM调用记录失败: {}", e);
                }
            }

            return Err(anyhow::anyhow!("Qwen API调用失败: {}", error_text));
        }

        let response_text = response.text().await?;

        // 解析响应
        let response_data: QwenResponse = serde_json::from_str(&response_text)?;

        // 检查 finish_reason，如果是 "length" 说明达到 token 限制
        if let Some(finish_reason) = response_data
            .choices
            .get(0)
            .and_then(|c| c.finish_reason.as_ref())
        {
            if finish_reason == "length" {
                warn!("LLM 响应因达到 token 限制而被截断 (finish_reason=length)");
            }
        }

        // 记录成功的响应
        llm_record.response_body = Some(response_text.clone());
        llm_record.latency_ms = Some(start_time.elapsed().as_millis() as i64);

        // 提取token使用信息
        if let Ok(response_json) = serde_json::from_str::<serde_json::Value>(&response_text) {
            if let Some(usage) = response_json.get("usage") {
                llm_record.token_usage = Some(usage.to_string());
            }
        }

        // 保存成功记录
        if let Some(ref db) = self.db {
            match db.insert_llm_call(&llm_record).await {
                Ok(id) => self.record_call_id(call_type, id),
                Err(e) => error!("保存LLM调用记录失败: {}", e),
            }
        }

        let content = response_data.choices[0].message.content.clone();

        // 如果响应被截断，返回错误而不是不完整的 JSON
        if response_data
            .choices
            .get(0)
            .and_then(|c| c.finish_reason.as_ref())
            == Some(&"length".to_string())
        {
            return Err(anyhow::anyhow!(
                "LLM 响应被截断（达到 max_tokens 限制）。内容长度: {} 字符。请尝试缩短视频时长或联系管理员。",
                content.len()
            ));
        }

        Ok(content)
    }

    /// 调用Qwen API - 支持视频URL模式
    async fn call_qwen_api_with_video(
        &self,
        prompt: String,
        video_url: String,
        call_type: &str,
    ) -> Result<String> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Qwen API key未配置"))?;

        let start_time = std::time::Instant::now();

        self.reset_call_id(call_type);

        // 构建系统消息
        let system_content = vec![json!({
            "type": "text",
            "text": "You are a helpful assistant that transcribes computer screen recordings."
        })];

        // 构建用户消息内容
        let user_content = vec![
            json!({
                "type": "video_url",
                "video_url": {
                    "url": video_url.clone()
                }
            }),
            json!({
                "type": "text",
                "text": prompt.clone()
            }),
        ];

        let request_body = json!({
            "model": self.model,
            "response_format": {"type": "json_object"},  // 保证结构化输出
            "messages": [
                {
                    "role": "system",
                    "content": system_content
                },
                {
                    "role": "user",
                    "content": user_content
                }
            ],
            "max_tokens": 8000,  // 增加到8000以支持长视频分析
            "temperature": 0.3
        });

        debug!(
            "调用Qwen API with video: model={}, video_url={}",
            self.model, video_url
        );

        // 记录请求信息
        let mut llm_record = crate::storage::LLMCallRecord {
            id: None,
            session_id: self.current_session_id,
            provider: "qwen".to_string(),
            model: self.model.clone(),
            call_type: call_type.to_string(),
            request_headers: json!({
                "Authorization": "Bearer ***",
                "Content-Type": "application/json",
                "X-DashScope-OssResourceResolve": "enable"
            })
            .to_string(),
            request_body: request_body.to_string(),
            response_headers: None,
            response_body: None,
            status_code: None,
            error_message: None,
            latency_ms: None,
            token_usage: None,
            created_at: crate::storage::local_now(),
        };

        let endpoint = self.base_url.clone();
        let response = self
            .client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("X-DashScope-OssResourceResolve", "enable") // 重要：解析OSS资源
            .json(&request_body)
            .send()
            .await?;

        let status_code = response.status().as_u16() as i32;
        llm_record.status_code = Some(status_code);

        // 记录响应头
        let response_headers = response.headers().clone();
        let headers_json = json!({
            "content-type": response_headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or(""),
            "x-request-id": response_headers.get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or(""),
        });
        llm_record.response_headers = Some(headers_json.to_string());

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Qwen API错误: {}", error_text);
            llm_record.error_message = Some(error_text.clone());
            llm_record.latency_ms = Some(start_time.elapsed().as_millis() as i64);

            // 保存错误记录
            if let Some(ref db) = self.db {
                if let Err(e) = db.insert_llm_call(&llm_record).await {
                    error!("保存LLM调用记录失败: {}", e);
                }
            }

            // 检测是否是视频过短错误
            if error_text.contains("video file is too short")
                || error_text.contains("The video file is too short")
            {
                return Err(anyhow::anyhow!("VIDEO_TOO_SHORT: {}", error_text));
            }

            return Err(anyhow::anyhow!("Qwen API调用失败: {}", error_text));
        }

        let response_text = response.text().await?;

        // 解析响应
        let response_data: QwenResponse = serde_json::from_str(&response_text)?;

        // 检查 finish_reason，如果是 "length" 说明达到 token 限制
        if let Some(finish_reason) = response_data
            .choices
            .get(0)
            .and_then(|c| c.finish_reason.as_ref())
        {
            if finish_reason == "length" {
                warn!("LLM 响应因达到 token 限制而被截断 (finish_reason=length, 视频模式)");
            }
        }

        // 记录成功的响应
        llm_record.response_body = Some(response_text.clone());
        llm_record.latency_ms = Some(start_time.elapsed().as_millis() as i64);

        // 提取token使用信息
        if let Ok(response_json) = serde_json::from_str::<serde_json::Value>(&response_text) {
            if let Some(usage) = response_json.get("usage") {
                llm_record.token_usage = Some(usage.to_string());
            }
        }

        // 保存成功记录
        if let Some(ref db) = self.db {
            match db.insert_llm_call(&llm_record).await {
                Ok(id) => self.record_call_id(call_type, id),
                Err(e) => error!("保存LLM调用记录失败: {}", e),
            }
        }

        let content = response_data.choices[0].message.content.clone();

        // 如果响应被截断，返回错误而不是不完整的 JSON
        if response_data
            .choices
            .get(0)
            .and_then(|c| c.finish_reason.as_ref())
            == Some(&"length".to_string())
        {
            return Err(anyhow::anyhow!(
                "LLM 响应被截断（达到 max_tokens 限制，视频模式）。内容长度: {} 字符。请尝试缩短视频时长或联系管理员。",
                content.len()
            ));
        }

        Ok(content)
    }

    /// 智能合并timeline cards
    /// 解析时间字符串（支持多种格式）
    fn parse_time_str(time_str: &str) -> Option<chrono::NaiveTime> {
        // 尝试解析 "HH:MM AM/PM" 格式
        if time_str.contains("AM") || time_str.contains("PM") {
            let cleaned = time_str
                .replace("AM", "")
                .replace("PM", "")
                .trim()
                .to_string();
            if let Ok(mut time) = chrono::NaiveTime::parse_from_str(&cleaned, "%I:%M") {
                if time_str.contains("PM") && time.hour() < 12 {
                    time = time + chrono::Duration::hours(12);
                } else if time_str.contains("AM") && time.hour() == 12 {
                    time = time - chrono::Duration::hours(12);
                }
                return Some(time);
            }
        }

        // 尝试解析 "HH:mm" 格式
        if let Ok(time) = chrono::NaiveTime::parse_from_str(time_str, "%H:%M") {
            return Some(time);
        }

        // 尝试解析 ISO 格式的时间部分
        if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(time_str) {
            return Some(datetime.time());
        }

        None
    }

    /// 检查两个时间是否连续（间隔小于5分钟）
    fn is_time_continuous(end_time_str: &str, start_time_str: &str) -> bool {
        if let (Some(end_time), Some(start_time)) = (
            Self::parse_time_str(end_time_str),
            Self::parse_time_str(start_time_str),
        ) {
            let diff = if start_time >= end_time {
                start_time - end_time
            } else {
                // 跨天的情况
                chrono::Duration::days(1) - (end_time - start_time)
            };
            diff.num_minutes() <= 5
        } else {
            false
        }
    }

    /// 合并相邻的相似活动卡片
    fn merge_adjacent_cards(cards: Vec<TimelineCard>) -> Vec<TimelineCard> {
        if cards.is_empty() {
            return cards;
        }

        let card_count = cards.len();
        let mut merged = Vec::new();
        let mut current = cards[0].clone();

        for next_card in cards.into_iter().skip(1) {
            // 检查是否可以合并：时间连续且类别相同
            if Self::is_time_continuous(&current.end_time, &next_card.start_time)
                && current.category == next_card.category
                && current.subcategory == next_card.subcategory
            {
                // 合并卡片
                current.end_time = next_card.end_time;
                current.detailed_summary = format!(
                    "{}；{}",
                    current.detailed_summary, next_card.detailed_summary
                );
                // 保持简洁的摘要
                if !current.summary.contains(&next_card.summary) {
                    current.summary = format!("持续{}: {}", current.category, current.title);
                }
            } else {
                // 不能合并，保存当前卡片并开始新的
                merged.push(current);
                current = next_card;
            }
        }

        // 添加最后一个卡片
        merged.push(current);

        info!(
            "卡片合并: 原始 {} 张，合并后 {} 张",
            card_count,
            merged.len()
        );
        merged
    }

    fn merge_timeline_cards(
        previous_cards: &Option<Vec<TimelineCard>>,
        new_cards: Vec<TimelineCard>,
    ) -> Vec<TimelineCard> {
        // 首先合并新卡片中相邻的相似活动
        let merged_new = Self::merge_adjacent_cards(new_cards);

        // 如果没有历史卡片，直接返回合并后的新卡片
        if previous_cards.is_none() || previous_cards.as_ref().unwrap().is_empty() {
            return merged_new;
        }

        let prev_cards = previous_cards.as_ref().unwrap();

        // 如果有历史卡片，尝试将第一张新卡片与最后一张历史卡片合并
        let mut result = prev_cards[..prev_cards.len() - 1].to_vec();
        let last_prev = &prev_cards[prev_cards.len() - 1];

        if !merged_new.is_empty() {
            let first_new = &merged_new[0];

            // 检查是否可以合并最后一张历史卡片和第一张新卡片
            if Self::is_time_continuous(&last_prev.end_time, &first_new.start_time)
                && last_prev.category == first_new.category
                && last_prev.subcategory == first_new.subcategory
            {
                // 合并
                let mut merged_card = last_prev.clone();
                merged_card.end_time = first_new.end_time.clone();
                merged_card.detailed_summary = format!(
                    "{}；{}",
                    last_prev.detailed_summary, first_new.detailed_summary
                );
                merged_card.summary =
                    format!("持续{}: {}", merged_card.category, merged_card.title);
                result.push(merged_card);

                // 添加剩余的新卡片
                result.extend_from_slice(&merged_new[1..]);
            } else {
                // 不能合并，保留历史卡片并添加所有新卡片
                result.push(last_prev.clone());
                result.extend(merged_new);
            }
        } else {
            // 没有新卡片，只保留历史卡片
            result.push(last_prev.clone());
        }

        result
    }
}

#[async_trait]
impl LLMProvider for QwenProvider {
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// 分析屏幕截图帧
    async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary> {
        if !self.is_configured() {
            error!("Qwen API key未配置！api_key = {:?}", self.api_key);
            error!("请检查配置文件和配置加载流程");
            return Err(anyhow::anyhow!("Qwen API key未配置，请先配置 API key"));
        }

        info!("Qwen开始分析 {} 帧图像", frames.len());

        // 采样帧（最多10张以控制成本）
        let sampled_frames = if frames.len() > 10 {
            frames
                .iter()
                .step_by(frames.len() / 10)
                .take(10)
                .cloned()
                .collect()
        } else {
            frames
        };

        // 转换为base64
        let mut images_base64 = Vec::new();
        for frame_path in sampled_frames {
            match self.image_to_base64(&frame_path).await {
                Ok(base64) => images_base64.push(base64),
                Err(e) => error!("图片转换失败 {}: {}", frame_path, e),
            }
        }

        if images_base64.is_empty() {
            return Err(anyhow::anyhow!("没有有效的图片可以分析"));
        }

        // 调用API分析
        let prompt = r#"分析这些屏幕截图，总结用户的活动。返回JSON格式：
{
  "title": "活动标题",
  "summary": "详细描述",
  "tags": [{"category": "类别", "confidence": 0.9, "keywords": ["关键词"]}],
  "key_moments": [{"time": "00:00", "description": "描述", "importance": 3}],
  "productivity_score": 75,
  "focus_score": 80
}

标签类别必须是以下之一（使用snake_case格式）：
- work: 工作（专注工作、编程、文档等生产力活动）
- meeting: 会议（视频会议、在线讨论、团队协作）
- coding: 编程（代码编写、调试、开发环境配置）
- research: 研究（资料查找、文献阅读、信息收集）
- learning: 学习（在线课程、教程学习、技能提升）
- writing: 写作（文档撰写、博客创作、笔记整理）
- design: 设计（界面设计、图形处理、创意工作）
- communication: 沟通（邮件、即时消息、社交互动）
- planning: 规划（任务规划、日程安排、项目管理）
- data_analysis: 数据分析（数据处理、报表制作、统计分析）
- entertainment: 娱乐（游戏、视频、音乐、休闲浏览）
- social_media: 社交媒体（社交平台浏览、内容创作与互动）
- shopping: 购物（网上购物、商品浏览、价格比较）
- finance: 财务（理财、账单管理、投资交易）
- break: 休息（短暂休息、放松时间）
- exercise: 运动（运动健身、健康管理）
- personal: 个人事务（个人生活相关事务处理）
- idle: 空闲（无具体活动、等待状态）
- other: 其他（未分类的其他活动）"#
            .to_string();

        let response = self
            .call_qwen_api(prompt, images_base64, "analyze_frames")
            .await?;

        // 解析响应
        let json_str = if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                &response[start..]
            }
        } else {
            &response
        };

        let parsed: serde_json::Value = serde_json::from_str(json_str)?;

        // 转换为SessionSummary
        let now = crate::storage::local_now();
        Ok(SessionSummary {
            title: parsed["title"].as_str().unwrap_or("未命名会话").to_string(),
            summary: parsed["summary"].as_str().unwrap_or("").to_string(),
            tags: vec![], // 简化处理
            start_time: now - chrono::Duration::minutes(15),
            end_time: now,
            key_moments: vec![],
            productivity_score: parsed["productivity_score"].as_f64().map(|v| v as f32),
            focus_score: parsed["focus_score"].as_f64().map(|v| v as f32),
        })
    }

    /// 分析视频并分段
    async fn segment_video(&self, frames: Vec<String>, duration: u32) -> Result<Vec<VideoSegment>> {
        if !self.is_configured() {
            return Err(anyhow::anyhow!("Qwen API key未配置，请先配置 API key"));
        }

        info!(
            "Qwen开始分析视频segments: {} 帧, 时长 {} 分钟",
            frames.len(),
            duration
        );

        // 检查是否设置了视频路径
        if let Some(ref video_file) = self.session_video_path {
            // 如果设置了视频路径，尝试上传并使用视频URL模式
            match self.upload_video(&video_file).await {
                Ok(video_url) => {
                    info!("使用视频URL模式分析: {}", video_url);
                    let prompt = self.build_segment_prompt(duration, self.video_speed_multiplier);
                    let response = self
                        .call_qwen_api_with_video(prompt, video_url, "segment_video")
                        .await?;

                    // 解析响应 - 由于使用了response_format: json_object，直接解析
                    let segments: Vec<VideoSegment> = serde_json::from_str(&response)?;
                    info!("Qwen视频分段完成（视频模式）: {} 个segment", segments.len());
                    return Ok(segments);
                }
                Err(e) => {
                    warn!("视频上传失败，回退到图片序列模式: {}", e);
                }
            }
        }

        // 回退到原来的图片序列模式
        // 采样帧
        let sampled_frames = if frames.len() > 15 {
            frames
                .iter()
                .step_by(frames.len() / 15)
                .take(15)
                .cloned()
                .collect()
        } else {
            frames
        };

        // 转换为base64
        let mut images_base64 = Vec::new();
        for frame_path in sampled_frames {
            match self.image_to_base64(&frame_path).await {
                Ok(base64) => images_base64.push(base64),
                Err(e) => error!("图片转换失败 {}: {}", frame_path, e),
            }
        }

        if images_base64.is_empty() {
            return Err(anyhow::anyhow!("没有有效的图片可以分析"));
        }

        // 调用API进行分段
        let prompt = self.build_segment_prompt(duration, self.video_speed_multiplier);
        let response = self
            .call_qwen_api(prompt, images_base64, "segment_video")
            .await?;

        // 解析响应
        let json_str = if let Some(start) = response.find('[') {
            if let Some(end) = response.rfind(']') {
                &response[start..=end]
            } else {
                &response[start..]
            }
        } else {
            &response
        };

        let segments: Vec<VideoSegment> = serde_json::from_str(json_str)?;
        info!("Qwen视频分段完成（图片模式）: {} 个segment", segments.len());
        Ok(segments)
    }

    /// 生成时间线卡片
    async fn generate_timeline(
        &self,
        segments: Vec<VideoSegment>,
        previous_cards: Option<Vec<TimelineCard>>,
    ) -> Result<Vec<TimelineCard>> {
        if !self.is_configured() {
            return Err(anyhow::anyhow!("Qwen API key未配置，请先配置 API key"));
        }

        info!("Qwen开始生成timeline: {} 个segments", segments.len());

        // 构建prompt
        let mut prompt = self.build_timeline_prompt(&previous_cards);
        prompt.push_str("\n\n当前视频分段:\n");
        prompt.push_str(&serde_json::to_string_pretty(&segments)?);

        // 调用API（不需要图片）
        let response = self
            .call_qwen_api(prompt, vec![], "generate_timeline")
            .await?;

        // 解析响应
        let json_str = if let Some(start) = response.find('[') {
            if let Some(end) = response.rfind(']') {
                &response[start..=end]
            } else {
                &response[start..]
            }
        } else {
            &response
        };

        let mut timeline_value = serde_json::from_str::<serde_json::Value>(json_str)?;
        crate::llm::plugin::normalize_timeline_cards_value(&mut timeline_value);

        let raw_cards = match Self::parse_timeline_cards(timeline_value.clone()) {
            Ok(cards) => cards,
            Err(err) => {
                if let Some(fallback) = Self::fallback_timeline_cards(&timeline_value) {
                    info!(
                        "Timeline解析失败，使用回退结果生成 {} 张卡片: {}",
                        fallback.len(),
                        err
                    );
                    fallback
                } else {
                    return Err(err);
                }
            }
        };

        // 应用智能合并逻辑
        let merged_cards = Self::merge_timeline_cards(&previous_cards, raw_cards);

        info!(
            "Qwen Timeline生成完成: {} 个原始卡片，合并后 {} 个卡片",
            segments.len(),
            merged_cards.len()
        );
        Ok(merged_cards)
    }

    fn name(&self) -> &str {
        "Qwen"
    }

    fn configure(&mut self, config: serde_json::Value) -> Result<()> {
        info!("Qwen configure 被调用，配置内容: {:?}", config);

        // 配置 API key（只接受非空字符串）
        if let Some(api_key) = config.get("api_key").and_then(|v| v.as_str()) {
            if !api_key.trim().is_empty() {
                self.api_key = Some(api_key.to_string());
                info!("✓ Qwen API key 已设置 (长度: {} 字符)", api_key.len());
            } else {
                warn!("✗ 收到空的 API key，忽略");
            }
        } else {
            warn!("✗ 配置中没有 api_key 字段");
        }

        if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
            self.model = model.to_string();
            info!("✓ Qwen model 已设置: {}", model);
        }

        if let Some(use_video) = config.get("use_video_mode").and_then(|v| v.as_bool()) {
            self.use_video_mode = use_video;
            info!("✓ Qwen video_mode 已设置: {}", use_video);
        }

        if let Some(video_path) = config.get("video_path").and_then(|v| v.as_str()) {
            self.session_video_path = Some(video_path.to_string());
            info!("✓ Qwen video_path 已设置: {}", video_path);
        }

        info!(
            "✓ Qwen提供商配置完成: model={}, video_mode={}, api_key_configured={}",
            self.model,
            self.use_video_mode,
            self.api_key.is_some()
        );
        Ok(())
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            vision_support: true,
            batch_analysis: true,
            streaming: false,
            max_input_tokens: 32000, // Qwen-VL-Max支持更多token
            supported_image_formats: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
                "gif".to_string(),
                "webp".to_string(),
            ],
        }
    }

    /// 生成每日总结文本（使用LLM）
    async fn generate_day_summary(
        &self,
        date: &str,
        sessions: &[crate::llm::SessionBrief],
    ) -> Result<String> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Qwen API Key未配置"))?;

        // 构建会话时间线文本
        // 如果会话数量过多，需要智能合并或筛选以控制 token 数量
        let max_sessions_display = 200; // 最多显示50个会话
        let sessions_to_process: Vec<&crate::llm::SessionBrief> =
            if sessions.len() > max_sessions_display {
                info!("会话数量 {} 超过限制，将筛选重要会话", sessions.len());
                // 按时长排序，选择较长的会话
                let mut sorted_sessions: Vec<_> = sessions.iter().collect();
                sorted_sessions
                    .sort_by_key(|s| std::cmp::Reverse((s.end_time - s.start_time).num_minutes()));
                sorted_sessions
                    .into_iter()
                    .take(max_sessions_display)
                    .collect()
            } else {
                sessions.iter().collect()
            };

        let mut sessions_text = String::new();
        for session in sessions_to_process {
            let start_local = Local.from_utc_datetime(&session.start_time.naive_utc());
            let end_local = Local.from_utc_datetime(&session.end_time.naive_utc());
            let start_time = start_local.format("%H:%M");
            let end_time = end_local.format("%H:%M");
            let duration = (session.end_time - session.start_time).num_minutes();

            // 如果 summary 过长，进行截断（安全截断，考虑 UTF-8 字符边界）
            let summary_display = if session.summary.chars().count() > 100 {
                let truncated: String = session.summary.chars().take(100).collect();
                format!("{}...", truncated)
            } else {
                session.summary.clone()
            };

            sessions_text.push_str(&format!(
                "\n- {} - {} ({} 分钟): {}\n  {}",
                start_time, end_time, duration, session.title, summary_display
            ));
        }

        // 计算总时长
        let total_minutes: i64 = sessions
            .iter()
            .map(|s| (s.end_time - s.start_time).num_minutes())
            .sum();

        // 构建提示词
        let prompt = format!(
            r#"基于以下今日屏幕活动记录，生成一份工作总结：

日期: {}
会话数: {}
总时长: {} 分钟

今日活动时间线:
{}

要求：
1. 使用中文，语气自然、专业
2. 重点总结真正在做什么工作/活动，而不是简单罗列统计数据
3. 按时间顺序或主题归纳今天的主要工作内容
4. 可以提及关键时间段的重要活动
5. 字数控制在 150-200 字以内
6. 输出格式要清晰易读，可以使用适当的分段

请直接返回总结文本（只要中文总结，不要标题、不要其他说明）。"#,
            date,
            sessions.len(),
            total_minutes,
            sessions_text
        );

        info!("使用Qwen生成每日总结: {}", date);

        // 调用API
        let request_body = json!({
            "model": self.model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "temperature": 0.7,
            "max_tokens": 10000  // 支持 200 字的中文输出（约 400-500 tokens）
        });

        let response = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("Qwen API 错误: {} - {}", status, error_text);
            return Err(anyhow::anyhow!("Qwen API 请求失败: {}", error_text));
        }

        let result: QwenResponse = response.json().await?;

        if let Some(choice) = result.choices.first() {
            let summary = choice.message.content.trim().to_string();
            info!("生成的每日总结: {}", summary);
            Ok(summary)
        } else {
            Err(anyhow::anyhow!("Qwen API 返回空结果"))
        }
    }
}

/// 上传凭证响应结构
#[derive(Debug, Deserialize)]
struct UploadPolicyResponse {
    data: UploadPolicy,
}

/// 上传凭证数据
#[derive(Debug, Deserialize)]
struct UploadPolicy {
    upload_host: String,
    upload_dir: String,
    signature: String,
    policy: String,
    oss_access_key_id: String,
    x_oss_object_acl: String,
    x_oss_forbid_overwrite: String,
}

/// Qwen API响应结构
#[derive(Debug, Deserialize)]
struct QwenResponse {
    choices: Vec<QwenChoice>,
}

#[derive(Debug, Deserialize)]
struct QwenChoice {
    message: QwenMessage,
    finish_reason: Option<String>, // 完成原因：stop, length, etc
}

#[derive(Debug, Deserialize)]
struct QwenMessage {
    content: String,
}
