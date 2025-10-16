// Codex CLI 提供商实现 - 使用 codex exec 无头模式

use super::plugin::*;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use llm_json::{loads, repair_json, RepairOptions};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::warn;

/// Codex CLI 提供商
pub struct CodexProvider {
    binary_path: PathBuf,
    model: Option<String>,
    profile: Option<String>,
    sandbox_mode: Option<String>,
    approval_policy: Option<String>,
    extra_args: Vec<String>,
    max_images: usize,
    timeout_secs: u64,
    summary_prompt_override: Option<String>,
    segment_prompt_override: Option<String>,
    timeline_prompt_override: Option<String>,
    day_summary_prompt_override: Option<String>,
    db: Option<Arc<crate::storage::Database>>,
    current_session_id: Option<i64>,
    last_call_ids: Mutex<HashMap<String, i64>>,
    session_window_start: Option<DateTime<Utc>>,
    session_window_end: Option<DateTime<Utc>>,
}

impl CodexProvider {
    /// 创建新的 Codex 提供商
    pub fn new() -> Self {
        Self {
            binary_path: PathBuf::from("codex"),
            model: None,
            profile: None,
            sandbox_mode: None,
            approval_policy: None,
            extra_args: Vec::new(),
            max_images: 16,
            timeout_secs: 600,
            summary_prompt_override: None,
            segment_prompt_override: None,
            timeline_prompt_override: None,
            day_summary_prompt_override: None,
            db: None,
            current_session_id: None,
            last_call_ids: Mutex::new(HashMap::new()),
            session_window_start: None,
            session_window_end: None,
        }
    }

    /// 设置数据库连接
    pub fn set_database(&mut self, db: Arc<crate::storage::Database>) {
        self.db = Some(db);
    }

    /// 设置当前会话 ID
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

    fn summary_prompt(&self) -> String {
        if let Some(prompt) = &self.summary_prompt_override {
            return prompt.clone();
        }

        r#"分析这些屏幕截图，总结用户在这个时间段完成了什么。请输出 JSON：
{
  "title": "活动标题（中文）",
  "summary": "详细描述（中文）",
  "tags": [{"category": "work", "confidence": 0.8, "keywords": ["关键字"]}],
  "key_moments": [{"time": "00:00", "description": "描述", "importance": 3}],
  "productivity_score": 75,
  "focus_score": 80
}

标签类别使用 snake_case，候选值：
- work, communication, learning, personal, idle, other
- 允许使用 work/coding/writing/design/planning/data_analysis/study/social_media 等具体子类

所有文字请使用中文，必须返回有效 JSON。"#
            .to_string()
    }

    fn segment_prompt(&self, duration: u32) -> String {
        if let Some(prompt) = &self.segment_prompt_override {
            return prompt.clone();
        }

        format!(
            r#"# 任务：将屏幕录制划分为少量有意义的活动段落

- 视频总时长：约 {duration} 分钟
- 图片按时间顺序采样，间隔约为几十秒
- 仅在主要活动发生明显变化时才切分
- 每个段落使用中文描述 1-3 句，说明用户完成了什么
- 输出 JSON 数组，时间格式使用 MM:SS

示例：
[
  {{
    "startTimestamp": "00:00",
    "endTimestamp": "05:00",
    "description": "描述该阶段的主要活动（中文）"
  }}
]

请覆盖整个时间范围，优先生成 3-6 个高质量、连贯的长段落。"#,
            duration = duration
        )
    }

    fn timeline_prompt(&self, previous_cards: &Option<Vec<TimelineCard>>) -> String {
        if let Some(prompt) = &self.timeline_prompt_override {
            return prompt.clone();
        }

        let previous_json = previous_cards
            .as_ref()
            .map(|cards| serde_json::to_string_pretty(cards).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or_else(|| "[]".to_string());

        format!(
            r#"# 任务：根据视频分段生成时间线卡片（中文）

要求：
1. 卡片应覆盖整个会话，优先使用 30-60 分钟的长卡片
2. 合并相邻、主题一致的活动
3. 如有轻微分心，请在 card 内描述，不单独拆分
4. category 使用 snake_case：work / communication / learning / personal / idle / other
5. 字段：startTime、endTime、category、subcategory、title、summary、detailedSummary、distractions、appSites
6. 仅返回 JSON 数组

历史卡片（可选，便于合并）：
{}
"#,
            previous_json
        )
    }

    fn day_summary_prompt(&self, date: &str, sessions: &[SessionBrief]) -> String {
        if let Some(prompt) = &self.day_summary_prompt_override {
            return prompt.clone();
        }

        let mut lines = String::new();
        for session in sessions {
            let start = session.start_time.format("%H:%M");
            let end = session.end_time.format("%H:%M");
            let minutes = (session.end_time - session.start_time).num_minutes();
            lines.push_str(&format!(
                "- {} - {} ({} 分钟): {} —— {}\n",
                start, end, minutes, session.title, session.summary
            ));
        }

        let total_minutes: i64 = sessions
            .iter()
            .map(|s| (s.end_time - s.start_time).num_minutes())
            .sum();

        format!(
            r#"基于以下 {count} 个会话，概括 {date} 的主要工作（150 字以内，中文）：
总时长：{total} 分钟
{lines}
要求重点描述完成的事情、重要节点及未完成事项，语气专业自然。"#,
            count = sessions.len(),
            date = date,
            total = total_minutes,
            lines = lines
        )
    }

    fn sample_frames(&self, frames: &[String]) -> Vec<String> {
        if frames.len() <= self.max_images {
            return frames.to_vec();
        }

        let total = frames.len() as f32;
        let step = total / self.max_images as f32;
        let mut sampled = Vec::with_capacity(self.max_images);
        for i in 0..self.max_images {
            let index = ((i as f32 + 0.5) * step).floor() as usize;
            sampled.push(frames[index.min(frames.len() - 1)].clone());
        }
        sampled
    }

    fn canonicalize_paths(&self, frames: &[String]) -> Vec<String> {
        frames
            .iter()
            .filter_map(|path| match std::fs::canonicalize(path) {
                Ok(p) => Some(p.to_string_lossy().to_string()),
                Err(e) => {
                    warn!("无法解析图片路径 {}: {}", path, e);
                    None
                }
            })
            .collect()
    }

    async fn run_codex_exec(
        &self,
        prompt: &str,
        images: &[String],
        call_type: &str,
    ) -> Result<String> {
        self.reset_call_id(call_type);

        let mut command = Command::new(&self.binary_path);
        command.arg("exec");

        if let Some(model) = &self.model {
            if !model.trim().is_empty() {
                command.arg("--model").arg(model);
            }
        }

        if let Some(profile) = &self.profile {
            if !profile.trim().is_empty() {
                command.arg("--profile").arg(profile);
            }
        }

        if let Some(sandbox) = &self.sandbox_mode {
            if !sandbox.trim().is_empty() {
                command.arg("--sandbox").arg(sandbox);
            }
        }

        if let Some(policy) = &self.approval_policy {
            if !policy.trim().is_empty() {
                command.arg("--ask-for-approval").arg(policy);
            }
        }

        for arg in &self.extra_args {
            if !arg.trim().is_empty() {
                command.arg(arg);
            }
        }

        for image in images {
            command.arg("--image").arg(image);
        }

        command.arg(prompt);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);

        let child = command
            .spawn()
            .map_err(|e| anyhow!("启动 codex CLI 失败: {}", e))?;

        let start = Instant::now();
        let output = match timeout(
            Duration::from_secs(self.timeout_secs),
            child.wait_with_output(),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => {
                return Err(anyhow!(
                    "Codex CLI 超时（>{} 秒），已终止执行",
                    self.timeout_secs
                ));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or_default();
        let latency_ms = start.elapsed().as_millis() as i64;

        let request_body = json!({
            "prompt": prompt,
            "images": images,
            "model": self.model,
            "profile": self.profile,
            "sandbox": self.sandbox_mode,
            "approval_policy": self.approval_policy,
            "extra_args": self.extra_args,
        });

        let mut record = crate::storage::LLMCallRecord {
            id: None,
            session_id: self.current_session_id,
            provider: "codex".to_string(),
            model: self
                .model
                .clone()
                .unwrap_or_else(|| "codex-cli".to_string()),
            call_type: call_type.to_string(),
            request_headers: "{}".to_string(),
            request_body: crate::llm::sanitize_request_body(&request_body),
            response_headers: None,
            response_body: None,
            status_code: Some(exit_code),
            error_message: None,
            latency_ms: Some(latency_ms),
            token_usage: None,
            created_at: crate::storage::local_now(),
        };

        if !stderr.trim().is_empty() {
            record.response_headers =
                Some(json!({ "stderr": truncate_for_log(&stderr, 2000) }).to_string());
        }

        if output.status.success() {
            record.response_body = Some(truncate_for_log(&stdout, 4000));
        } else {
            record.error_message = Some(truncate_for_log(&stderr, 2000));
        }

        if let Some(db) = self.db.clone() {
            if let Ok(id) = db.insert_llm_call(&record).await {
                if output.status.success() {
                    self.record_call_id(call_type, id);
                }
            }
        }

        if !output.status.success() {
            return Err(anyhow!(
                "Codex CLI 执行失败 (exit={}): {}",
                exit_code,
                stderr.trim()
            ));
        }

        Ok(stdout)
    }

    /// 执行纯文本提示词（无图），用于健康检查或脚本化调用
    pub async fn run_text_prompt(&self, prompt: &str, call_type: &str) -> Result<String> {
        self.run_codex_exec(prompt, &[], call_type).await
    }

    fn parse_json<T: DeserializeOwned>(&self, raw: &str) -> Result<T> {
        let cleaned = strip_code_fence(raw.trim());
        if cleaned.is_empty() {
            return Err(anyhow!("Codex CLI 没有返回内容"));
        }

        if let Ok(value) = serde_json::from_str::<T>(&cleaned) {
            return Ok(value);
        }

        let repaired = repair_json(&cleaned, &RepairOptions::default())
            .map_err(|e| anyhow!("无法修复 Codex 返回的 JSON: {}", e))?;
        let value = loads(&repaired, &RepairOptions::default())
            .map_err(|e| anyhow!("解析修复后的 JSON 失败: {}", e))?;
        serde_json::from_value(value).map_err(|e| anyhow!("JSON 结构不符合预期: {}", e))
    }

    fn map_tags(&self, raw: Vec<CodexTagPayload>) -> Vec<ActivityTag> {
        raw.into_iter()
            .filter_map(|tag| {
                if tag.category.trim().is_empty() {
                    return None;
                }

                let keywords = if tag.keywords.is_empty() {
                    vec![tag.category.clone()]
                } else {
                    tag.keywords
                };

                Some(ActivityTag {
                    category: map_activity_category(&tag.category),
                    confidence: tag.confidence.unwrap_or(0.5).clamp(0.0, 1.0),
                    keywords,
                })
            })
            .collect()
    }

    fn map_key_moments(&self, raw: Vec<CodexKeyMomentPayload>) -> Vec<KeyMoment> {
        raw.into_iter()
            .filter_map(|moment| {
                if moment.time.trim().is_empty() || moment.description.trim().is_empty() {
                    return None;
                }

                Some(KeyMoment {
                    time: moment.time,
                    description: moment.description,
                    importance: moment.importance.unwrap_or(3).clamp(1, 5),
                })
            })
            .collect()
    }

    fn fallback_timeline(&self, segments: &[VideoSegment]) -> Vec<TimelineCard> {
        if segments.is_empty() {
            return Vec::new();
        }

        segments
            .iter()
            .map(|segment| TimelineCard {
                start_time: segment.start_timestamp.clone(),
                end_time: segment.end_timestamp.clone(),
                category: "work".to_string(),
                subcategory: "General".to_string(),
                title: "主要活动".to_string(),
                summary: segment.description.clone(),
                detailed_summary: segment.description.clone(),
                distractions: None,
                app_sites: AppSites {
                    primary: "unknown".to_string(),
                    secondary: None,
                },
                video_preview_path: None,
            })
            .collect()
    }
}

#[async_trait]
impl LLMProvider for CodexProvider {
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary> {
        if frames.is_empty() {
            return Err(anyhow!("没有可分析的帧图像"));
        }

        let sampled = self.sample_frames(&frames);
        let images = self.canonicalize_paths(&sampled);
        if images.is_empty() {
            return Err(anyhow!("采样后没有有效图片路径"));
        }

        let response = self
            .run_codex_exec(&self.summary_prompt(), &images, "analyze_frames")
            .await?;

        let payload: CodexSummaryPayload = self.parse_json(&response)?;

        let now = crate::storage::local_now();
        let start = self
            .session_window_start
            .unwrap_or_else(|| now - ChronoDuration::minutes(15));
        let end = self.session_window_end.unwrap_or(now);

        Ok(SessionSummary {
            title: payload.title.unwrap_or_else(|| "未命名会话".to_string()),
            summary: payload.summary.unwrap_or_default(),
            tags: self.map_tags(payload.tags),
            start_time: start,
            end_time: end,
            key_moments: self.map_key_moments(payload.key_moments),
            productivity_score: payload.productivity_score,
            focus_score: payload.focus_score,
        })
    }

    async fn segment_video(&self, frames: Vec<String>, duration: u32) -> Result<Vec<VideoSegment>> {
        if frames.is_empty() {
            return Err(anyhow!("没有可分析的帧图像"));
        }

        let sampled = self.sample_frames(&frames);
        let images = self.canonicalize_paths(&sampled);
        if images.is_empty() {
            return Err(anyhow!("采样后没有有效图片路径"));
        }

        let response = self
            .run_codex_exec(&self.segment_prompt(duration), &images, "segment_video")
            .await?;

        let mut segments: Vec<VideoSegment> = self.parse_json(&response)?;
        if segments.is_empty() {
            warn!("Codex 未返回分段信息，使用兜底结果");
            segments.push(VideoSegment {
                start_timestamp: "00:00".to_string(),
                end_timestamp: format!("{:02}:00", duration),
                description: "基于截图生成的兜底描述".to_string(),
            });
        }

        Ok(segments)
    }

    async fn generate_timeline(
        &self,
        segments: Vec<VideoSegment>,
        previous_cards: Option<Vec<TimelineCard>>,
    ) -> Result<Vec<TimelineCard>> {
        let mut prompt = self.timeline_prompt(&previous_cards);
        prompt.push_str("\n\n当前视频分段：\n");
        prompt.push_str(&serde_json::to_string_pretty(&segments)?);

        let response = self
            .run_codex_exec(&prompt, &[], "generate_timeline")
            .await?;

        let cards: Vec<TimelineCard> = match self.parse_json(&response) {
            Ok(data) => data,
            Err(err) => {
                warn!("解析 Codex 时间线失败，使用回退：{}", err);
                return Ok(self.fallback_timeline(&segments));
            }
        };

        if cards.is_empty() {
            warn!("Codex 时间线为空，使用回退结果");
            return Ok(self.fallback_timeline(&segments));
        }

        Ok(cards)
    }

    fn set_session_window(&mut self, start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) {
        self.session_window_start = start;
        self.session_window_end = end;
    }

    fn name(&self) -> &str {
        "Codex"
    }

    fn configure(&mut self, config: serde_json::Value) -> Result<()> {
        if let Some(path) = config.get("binary_path").and_then(|v| v.as_str()) {
            if !path.trim().is_empty() {
                self.binary_path = PathBuf::from(path);
            }
        }

        if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
            self.model = Some(model.to_string());
        }

        if let Some(profile) = config.get("profile").and_then(|v| v.as_str()) {
            self.profile = Some(profile.to_string());
        }

        if let Some(sandbox) = config.get("sandbox_mode").and_then(|v| v.as_str()) {
            self.sandbox_mode = Some(sandbox.to_string());
        }

        if let Some(policy) = config.get("approval_policy").and_then(|v| v.as_str()) {
            self.approval_policy = Some(policy.to_string());
        }

        if let Some(max_images) = config.get("max_images").and_then(|v| v.as_u64()) {
            if max_images > 0 {
                self.max_images = max_images.min(64) as usize;
            }
        }

        if let Some(timeout) = config.get("timeout_secs").and_then(|v| v.as_u64()) {
            if timeout >= 60 {
                self.timeout_secs = timeout;
            }
        }

        if let Some(extra) = config.get("extra_args") {
            self.extra_args = match extra {
                Value::Array(items) => items
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
                Value::String(text) => text.split_whitespace().map(|s| s.to_string()).collect(),
                _ => Vec::new(),
            };
        }

        if let Some(prompt) = config.get("summary_prompt").and_then(|v| v.as_str()) {
            if !prompt.trim().is_empty() {
                self.summary_prompt_override = Some(prompt.to_string());
            }
        }

        if let Some(prompt) = config.get("segment_prompt").and_then(|v| v.as_str()) {
            if !prompt.trim().is_empty() {
                self.segment_prompt_override = Some(prompt.to_string());
            }
        }

        if let Some(prompt) = config.get("timeline_prompt").and_then(|v| v.as_str()) {
            if !prompt.trim().is_empty() {
                self.timeline_prompt_override = Some(prompt.to_string());
            }
        }

        if let Some(prompt) = config.get("day_summary_prompt").and_then(|v| v.as_str()) {
            if !prompt.trim().is_empty() {
                self.day_summary_prompt_override = Some(prompt.to_string());
            }
        }

        Ok(())
    }

    fn is_configured(&self) -> bool {
        true
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            vision_support: true,
            batch_analysis: true,
            streaming: false,
            max_input_tokens: 128000,
            supported_image_formats: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
                "gif".to_string(),
                "webp".to_string(),
            ],
        }
    }

    fn last_llm_call_id(&self, call_type: &str) -> Option<i64> {
        self.last_call_ids
            .lock()
            .ok()
            .and_then(|map| map.get(call_type).copied())
    }

    async fn generate_day_summary(&self, date: &str, sessions: &[SessionBrief]) -> Result<String> {
        if sessions.is_empty() {
            return Ok(format!("{} 当天没有记录到任何屏幕活动。", date));
        }

        let prompt = self.day_summary_prompt(date, sessions);
        let response = self
            .run_codex_exec(&prompt, &[], "generate_day_summary")
            .await?;
        Ok(response.trim().to_string())
    }
}

fn truncate_for_log(input: &str, max_len: usize) -> String {
    if input.len() <= max_len {
        input.to_string()
    } else {
        let mut truncated = input[..max_len].to_string();
        truncated.push_str("...<truncated>");
        truncated
    }
}

fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        // 跳过 ```json 或 ``` 开头
        lines.next();
        let mut body = Vec::new();
        for line in lines {
            if line.trim_start().starts_with("```") {
                break;
            }
            body.push(line);
        }
        body.join("\n")
    } else {
        trimmed.to_string()
    }
}

fn map_activity_category(value: &str) -> ActivityCategory {
    match value.to_lowercase().as_str() {
        "work" | "coding" | "writing" | "design" | "planning" | "data_analysis" => {
            ActivityCategory::Work
        }
        "communication" | "meeting" => ActivityCategory::Communication,
        "learning" | "research" => ActivityCategory::Learning,
        "personal" | "entertainment" | "social_media" | "shopping" | "finance" => {
            ActivityCategory::Personal
        }
        "idle" | "break" => ActivityCategory::Idle,
        _ => ActivityCategory::Other,
    }
}

#[derive(Debug, Deserialize)]
struct CodexSummaryPayload {
    title: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    tags: Vec<CodexTagPayload>,
    #[serde(default)]
    key_moments: Vec<CodexKeyMomentPayload>,
    productivity_score: Option<f32>,
    focus_score: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct CodexTagPayload {
    category: String,
    confidence: Option<f32>,
    #[serde(default)]
    keywords: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CodexKeyMomentPayload {
    time: String,
    description: String,
    importance: Option<u8>,
}
