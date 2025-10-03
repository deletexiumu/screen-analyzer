# LLM 提供商开发指南

本文档介绍如何为 Screen Analyzer 开发自定义 LLM (Large Language Model) 提供商，以支持更多的 AI 服务。

## 目录

- [架构概览](#架构概览)
- [核心接口](#核心接口)
- [数据结构](#数据结构)
- [实现步骤](#实现步骤)
- [示例代码](#示例代码)
- [测试和调试](#测试和调试)
- [最佳实践](#最佳实践)

## 架构概览

Screen Analyzer 的 LLM 模块采用插件式架构，支持多种 AI 提供商：

```
llm/
├── plugin.rs       # 核心接口定义和数据结构
├── qwen.rs         # 阿里通义千问实现（参考示例）
├── mod.rs          # 模块管理和导出
└── README.md       # 本文档
```

### 关键组件

1. **LLMProvider Trait**: 定义提供商必须实现的接口
2. **数据结构**: 统一的输入输出格式（SessionSummary, TimelineCard 等）
3. **LLMManager**: 管理和协调不同提供商

## 核心接口

所有 LLM 提供商必须实现 `LLMProvider` trait：

```rust
#[async_trait]
pub trait LLMProvider: Send + Sync + std::any::Any {
    // 必须实现的方法
    fn as_any(&mut self) -> &mut dyn std::any::Any;
    async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary>;
    fn name(&self) -> &str;
    fn configure(&mut self, config: serde_json::Value) -> Result<()>;
    fn is_configured(&self) -> bool;

    // 可选方法（有默认实现）
    async fn segment_video(&self, frames: Vec<String>, duration: u32) -> Result<Vec<VideoSegment>>;
    async fn generate_timeline(&self, segments: Vec<VideoSegment>, previous_cards: Option<Vec<TimelineCard>>) -> Result<Vec<TimelineCard>>;
    async fn generate_day_summary(&self, date: &str, sessions: &[SessionBrief]) -> Result<String>;
    fn capabilities(&self) -> ProviderCapabilities;
}
```

### 方法说明

#### 1. `as_any` - 类型转换
用于向下转型，支持动态类型转换。

```rust
fn as_any(&mut self) -> &mut dyn std::any::Any {
    self
}
```

#### 2. `analyze_frames` - 分析截图帧（核心方法）
分析一组屏幕截图，生成会话总结。

**输入**:
- `frames: Vec<String>` - 截图文件路径列表

**输出**:
- `SessionSummary` - 包含标题、摘要、标签、关键时刻等

**示例**:
```rust
async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary> {
    // 1. 采样帧（避免发送过多图片）
    let sampled_frames = self.sample_frames(&frames, 30)?;

    // 2. 转换为模型输入格式（base64、URL等）
    let images = self.prepare_images(&sampled_frames).await?;

    // 3. 调用 AI API
    let response = self.call_api(images).await?;

    // 4. 解析响应为 SessionSummary
    let summary = self.parse_response(response)?;

    Ok(summary)
}
```

#### 3. `segment_video` - 视频分段（可选）
将视频按活动内容分段。

**输入**:
- `frames: Vec<String>` - 截图帧路径
- `duration: u32` - 视频时长（分钟）

**输出**:
- `Vec<VideoSegment>` - 视频分段列表

#### 4. `generate_timeline` - 生成时间线卡片（可选）
根据视频分段生成详细的时间线卡片。

**输入**:
- `segments: Vec<VideoSegment>` - 视频分段
- `previous_cards: Option<Vec<TimelineCard>>` - 之前的卡片（用于上下文）

**输出**:
- `Vec<TimelineCard>` - 时间线卡片列表

#### 5. `generate_day_summary` - 生成每日总结（可选）
汇总一天的所有会话，生成总结文本。

**输入**:
- `date: &str` - 日期 (YYYY-MM-DD)
- `sessions: &[SessionBrief]` - 当天所有会话

**输出**:
- `String` - 总结文本

#### 6. `name` - 提供商名称
返回提供商的唯一标识符。

```rust
fn name(&self) -> &str {
    "my_provider"
}
```

#### 7. `configure` - 配置提供商
接收 JSON 配置并应用。

**配置示例**:
```json
{
    "api_key": "sk-xxxxx",
    "model": "gpt-4-vision-preview",
    "base_url": "https://api.openai.com/v1",
    "max_tokens": 4096,
    "temperature": 0.7
}
```

#### 8. `is_configured` - 检查配置状态
返回提供商是否已正确配置。

```rust
fn is_configured(&self) -> bool {
    self.api_key.is_some()
}
```

#### 9. `capabilities` - 声明能力（可选）
声明提供商支持的功能。

```rust
fn capabilities(&self) -> ProviderCapabilities {
    ProviderCapabilities {
        vision_support: true,
        batch_analysis: true,
        streaming: false,
        max_input_tokens: 128000,
        supported_image_formats: vec!["jpg".to_string(), "png".to_string()],
    }
}
```

## 数据结构

### SessionSummary - 会话总结
```rust
pub struct SessionSummary {
    pub title: String,                      // 会话标题（10字以内）
    pub summary: String,                    // 摘要（50-100字）
    pub tags: Vec<ActivityTag>,             // 活动标签
    pub start_time: DateTime<Utc>,         // 开始时间
    pub end_time: DateTime<Utc>,           // 结束时间
    pub key_moments: Vec<KeyMoment>,       // 关键时刻
    pub productivity_score: Option<f32>,   // 生产力评分（0-100）
    pub focus_score: Option<f32>,          // 专注度评分（0-100）
}
```

### ActivityTag - 活动标签
```rust
pub struct ActivityTag {
    pub category: ActivityCategory,  // 类别
    pub confidence: f32,             // 置信度（0-1）
    pub keywords: Vec<String>,       // 关键词
}

pub enum ActivityCategory {
    Work,          // 工作
    Communication, // 沟通
    Learning,      // 学习
    Personal,      // 个人
    Idle,          // 空闲
    Other,         // 其他
}
```

### TimelineCard - 时间线卡片
```rust
pub struct TimelineCard {
    pub start_time: String,              // 开始时间 (HH:MM)
    pub end_time: String,                // 结束时间 (HH:MM)
    pub category: String,                // 类别
    pub subcategory: String,             // 子类别
    pub title: String,                   // 标题
    pub summary: String,                 // 摘要
    pub detailed_summary: String,        // 详细摘要
    pub distractions: Option<Vec<Distraction>>, // 干扰活动
    pub app_sites: AppSites,            // 应用和网站
    pub video_preview_path: Option<String>, // 视频预览
}
```

### VideoSegment - 视频分段
```rust
pub struct VideoSegment {
    pub start_timestamp: String,  // 开始时间戳 (MM:SS)
    pub end_timestamp: String,    // 结束时间戳 (MM:SS)
    pub description: String,      // 描述
}
```

## 实现步骤

### 第一步：创建新文件

在 `src-tauri/src/llm/` 目录下创建新文件，例如 `openai.rs`：

```rust
// OpenAI GPT-4 Vision 提供商实现

use super::plugin::*;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

pub struct OpenAIProvider {
    api_key: Option<String>,
    model: String,
    client: Client,
    base_url: String,
}

impl OpenAIProvider {
    pub fn new(client: Client) -> Self {
        Self {
            api_key: None,
            model: "gpt-4-vision-preview".to_string(),
            client,
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }
}
```

### 第二步：实现 LLMProvider Trait

```rust
#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary> {
        // 实现核心分析逻辑
        todo!("实现帧分析")
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn configure(&mut self, config: serde_json::Value) -> Result<()> {
        if let Some(api_key) = config.get("api_key").and_then(|v| v.as_str()) {
            self.api_key = Some(api_key.to_string());
        }
        if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
            self.model = model.to_string();
        }
        if let Some(base_url) = config.get("base_url").and_then(|v| v.as_str()) {
            self.base_url = base_url.to_string();
        }
        Ok(())
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }
}
```

### 第三步：实现核心分析逻辑

```rust
impl OpenAIProvider {
    /// 采样帧（每N秒取一帧）
    fn sample_frames(&self, frames: &[String], interval_seconds: usize) -> Vec<String> {
        frames
            .iter()
            .step_by(interval_seconds)
            .take(30) // 最多30帧
            .cloned()
            .collect()
    }

    /// 将图片转换为 base64
    async fn encode_image(&self, path: &str) -> Result<String> {
        let bytes = tokio::fs::read(path).await?;
        Ok(base64::encode(&bytes))
    }

    /// 调用 OpenAI API
    async fn call_api(&self, images: Vec<String>) -> Result<serde_json::Value> {
        let api_key = self.api_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("API key未配置"))?;

        // 构建消息内容
        let mut content = vec![json!({
            "type": "text",
            "text": "分析这些屏幕截图，总结用户的活动..."
        })];

        for image_base64 in images {
            content.push(json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:image/jpeg;base64,{}", image_base64)
                }
            }));
        }

        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&json!({
                "model": self.model,
                "messages": [{
                    "role": "user",
                    "content": content
                }],
                "max_tokens": 4096
            }))
            .send()
            .await?;

        Ok(response.json().await?)
    }

    /// 解析 API 响应
    fn parse_response(&self, response: serde_json::Value) -> Result<SessionSummary> {
        // 提取文本内容
        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("无法提取响应内容"))?;

        // 解析 JSON（假设返回格式化的 JSON）
        let summary: SessionSummary = serde_json::from_str(content)?;
        Ok(summary)
    }
}
```

### 第四步：在 mod.rs 中注册

```rust
// src-tauri/src/llm/mod.rs

pub mod plugin;
pub mod qwen;
pub mod openai;  // 新增

pub use plugin::*;
pub use qwen::QwenProvider;
pub use openai::OpenAIProvider;  // 导出
```

### 第五步：更新 LLMManager

在 `mod.rs` 中添加对新提供商的支持：

```rust
pub enum LLMProviderType {
    Qwen,
    OpenAI,
    // 添加其他提供商
}

impl LLMManager {
    pub fn create_provider(
        provider_type: LLMProviderType,
        client: Client
    ) -> Box<dyn LLMProvider> {
        match provider_type {
            LLMProviderType::Qwen => Box::new(QwenProvider::new(client)),
            LLMProviderType::OpenAI => Box::new(OpenAIProvider::new(client)),
        }
    }
}
```

## 示例代码

### 完整的 OpenAI 提供商示例

```rust
// openai.rs - 完整示例

use super::plugin::*;
use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use reqwest::Client;
use serde_json::json;
use tracing::{debug, error, info};

pub struct OpenAIProvider {
    api_key: Option<String>,
    model: String,
    client: Client,
    base_url: String,
    max_tokens: u32,
    temperature: f32,
}

impl OpenAIProvider {
    pub fn new(client: Client) -> Self {
        Self {
            api_key: None,
            model: "gpt-4-vision-preview".to_string(),
            client,
            base_url: "https://api.openai.com/v1".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }

    fn sample_frames(&self, frames: &[String], interval: usize) -> Vec<String> {
        frames.iter()
            .step_by(interval)
            .take(30)
            .cloned()
            .collect()
    }

    async fn encode_image(&self, path: &str) -> Result<String> {
        let bytes = tokio::fs::read(path).await?;
        Ok(general_purpose::STANDARD.encode(&bytes))
    }

    fn build_prompt(&self) -> String {
        r#"请分析这些屏幕截图，生成JSON格式的总结：
{
  "title": "简短标题（10字以内）",
  "summary": "活动摘要（50-100字）",
  "tags": [
    {
      "category": "work",  // work/communication/learning/personal/idle/other
      "confidence": 0.95,
      "keywords": ["编程", "IDE"]
    }
  ],
  "key_moments": [
    {
      "time": "00:05:30",
      "description": "开始编写代码",
      "importance": 4
    }
  ],
  "productivity_score": 85,
  "focus_score": 90
}
"#.to_string()
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary> {
        info!("开始分析 {} 帧图片", frames.len());

        // 1. 采样
        let sampled = self.sample_frames(&frames, 30);
        debug!("采样后剩余 {} 帧", sampled.len());

        // 2. 编码图片
        let mut images = Vec::new();
        for path in &sampled {
            match self.encode_image(path).await {
                Ok(encoded) => images.push(encoded),
                Err(e) => error!("编码图片失败: {}, 路径: {}", e, path),
            }
        }

        // 3. 构建请求
        let api_key = self.api_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("API key未配置"))?;

        let mut content = vec![json!({
            "type": "text",
            "text": self.build_prompt()
        })];

        for img_base64 in images {
            content.push(json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:image/jpeg;base64,{}", img_base64)
                }
            }));
        }

        // 4. 调用 API
        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&json!({
                "model": self.model,
                "messages": [{
                    "role": "user",
                    "content": content
                }],
                "max_tokens": self.max_tokens,
                "temperature": self.temperature
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("API调用失败: {}", error_text));
        }

        let resp_json: serde_json::Value = response.json().await?;

        // 5. 解析响应
        let content_str = resp_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("无法提取响应内容"))?;

        // 尝试提取 JSON（可能包含在 markdown 代码块中）
        let json_str = if content_str.contains("```json") {
            content_str
                .split("```json")
                .nth(1)
                .and_then(|s| s.split("```").next())
                .unwrap_or(content_str)
        } else {
            content_str
        };

        let mut summary: SessionSummary = serde_json::from_str(json_str.trim())?;

        // 6. 设置时间
        let now = Utc::now();
        summary.start_time = now - chrono::Duration::minutes(15);
        summary.end_time = now;

        info!("分析完成: {}", summary.title);
        Ok(summary)
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn configure(&mut self, config: serde_json::Value) -> Result<()> {
        if let Some(api_key) = config.get("api_key").and_then(|v| v.as_str()) {
            self.api_key = Some(api_key.to_string());
        }
        if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
            self.model = model.to_string();
        }
        if let Some(base_url) = config.get("base_url").and_then(|v| v.as_str()) {
            self.base_url = base_url.to_string();
        }
        if let Some(max_tokens) = config.get("max_tokens").and_then(|v| v.as_u64()) {
            self.max_tokens = max_tokens as u32;
        }
        if let Some(temp) = config.get("temperature").and_then(|v| v.as_f64()) {
            self.temperature = temp as f32;
        }
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
            max_input_tokens: 128000,
            supported_image_formats: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
            ],
        }
    }
}
```

## 测试和调试

### 1. 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_configure() {
        let client = Client::new();
        let mut provider = OpenAIProvider::new(client);

        let config = json!({
            "api_key": "test-key",
            "model": "gpt-4-vision-preview"
        });

        provider.configure(config).unwrap();
        assert!(provider.is_configured());
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_sample_frames() {
        let client = Client::new();
        let provider = OpenAIProvider::new(client);

        let frames: Vec<String> = (0..100)
            .map(|i| format!("frame_{}.jpg", i))
            .collect();

        let sampled = provider.sample_frames(&frames, 30);
        assert!(sampled.len() <= 30);
    }
}
```

### 2. 集成测试

创建测试视频和帧：

```rust
#[tokio::test]
async fn test_analyze_frames_integration() {
    let client = Client::new();
    let mut provider = OpenAIProvider::new(client);

    provider.configure(json!({
        "api_key": std::env::var("OPENAI_API_KEY").unwrap()
    })).unwrap();

    let test_frames = vec![
        "/path/to/frame1.jpg".to_string(),
        "/path/to/frame2.jpg".to_string(),
    ];

    let result = provider.analyze_frames(test_frames).await;
    assert!(result.is_ok());

    let summary = result.unwrap();
    assert!(!summary.title.is_empty());
    assert!(!summary.summary.is_empty());
}
```

### 3. 调试技巧

使用 `tracing` 库输出调试日志：

```rust
use tracing::{debug, info, warn, error};

async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary> {
    info!("开始分析，帧数: {}", frames.len());

    let sampled = self.sample_frames(&frames, 30);
    debug!("采样后帧数: {}", sampled.len());

    match self.call_api(sampled).await {
        Ok(response) => {
            debug!("API响应: {:?}", response);
            // ...
        }
        Err(e) => {
            error!("API调用失败: {}", e);
            return Err(e);
        }
    }

    // ...
}
```

## 最佳实践

### 1. 错误处理

使用 `anyhow::Result` 并提供详细的错误信息：

```rust
async fn call_api(&self, images: Vec<String>) -> Result<serde_json::Value> {
    let response = self.client
        .post(&self.base_url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP请求失败: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "无法读取错误信息".to_string());
        return Err(anyhow::anyhow!("API返回错误 {}: {}", status, error_text));
    }

    response.json().await
        .map_err(|e| anyhow::anyhow!("解析响应失败: {}", e))
}
```

### 2. 帧采样策略

避免发送过多图片，节省成本和时间：

```rust
fn sample_frames(&self, frames: &[String], interval_seconds: usize) -> Vec<String> {
    let max_frames = 30; // 根据 API 限制调整

    if frames.len() <= max_frames {
        return frames.to_vec();
    }

    // 均匀采样
    let step = frames.len() / max_frames;
    frames.iter()
        .step_by(step.max(1))
        .take(max_frames)
        .cloned()
        .collect()
}
```

### 3. 缓存和重试

实现智能重试和响应缓存：

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn call_api_with_retry(&self, images: Vec<String>) -> Result<serde_json::Value> {
    let max_retries = 3;
    let mut last_error = None;

    for attempt in 0..max_retries {
        match self.call_api(images.clone()).await {
            Ok(response) => return Ok(response),
            Err(e) => {
                warn!("API调用失败 (尝试 {}/{}): {}", attempt + 1, max_retries, e);
                last_error = Some(e);

                if attempt < max_retries - 1 {
                    let delay = Duration::from_secs(2_u64.pow(attempt as u32));
                    sleep(delay).await;
                }
            }
        }
    }

    Err(last_error.unwrap())
}
```

### 4. Prompt 工程

设计清晰的提示词以获得更好的结果：

```rust
fn build_analysis_prompt(&self, context: &str) -> String {
    format!(r#"
请分析这些屏幕截图，识别用户的活动并生成结构化总结。

背景信息：
- 时间段：{}
- 采样间隔：30秒/帧

分析要求：
1. 识别主要活动类型（工作/沟通/学习/个人）
2. 提取关键应用程序和网站
3. 识别活动切换和干扰
4. 评估生产力和专注度

返回JSON格式：
{{
  "title": "简短标题（10字以内）",
  "summary": "详细摘要（50-100字）",
  "tags": [...],
  "key_moments": [...],
  "productivity_score": 0-100,
  "focus_score": 0-100
}}
"#, context)
}
```

### 5. 性能优化

并发处理图片编码：

```rust
use tokio::task::JoinSet;

async fn encode_images_parallel(&self, paths: &[String]) -> Result<Vec<String>> {
    let mut tasks = JoinSet::new();

    for path in paths {
        let path = path.clone();
        tasks.spawn(async move {
            tokio::fs::read(&path).await
                .map(|bytes| general_purpose::STANDARD.encode(&bytes))
        });
    }

    let mut results = Vec::new();
    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok(encoded)) => results.push(encoded),
            Ok(Err(e)) => warn!("编码图片失败: {}", e),
            Err(e) => error!("任务失败: {}", e),
        }
    }

    Ok(results)
}
```

### 6. 配置验证

验证配置的完整性和正确性：

```rust
fn configure(&mut self, config: serde_json::Value) -> Result<()> {
    // 验证必需字段
    let api_key = config.get("api_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("缺少api_key字段"))?;

    if api_key.is_empty() {
        return Err(anyhow::anyhow!("api_key不能为空"));
    }

    self.api_key = Some(api_key.to_string());

    // 可选字段
    if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
        self.model = model.to_string();
    }

    // 验证配置
    self.validate_config()?;

    Ok(())
}

fn validate_config(&self) -> Result<()> {
    if self.api_key.is_none() {
        return Err(anyhow::anyhow!("API key未配置"));
    }

    if self.model.is_empty() {
        return Err(anyhow::anyhow!("模型名称不能为空"));
    }

    Ok(())
}
```

## 参考资料

### 现有实现

- **QwenProvider** (`qwen.rs`): 完整的阿里通义千问实现，包含视频上传和多模式分析
- **Plugin** (`plugin.rs`): 核心接口定义和数据结构

### 相关文档

- [Tauri 异步调用](https://tauri.app/v1/guides/features/command)
- [async-trait 文档](https://docs.rs/async-trait)
- [reqwest HTTP 客户端](https://docs.rs/reqwest)
- [serde JSON 处理](https://docs.rs/serde_json)

### API 文档

常见 LLM 提供商 API：

- [OpenAI Vision API](https://platform.openai.com/docs/guides/vision)
- [Anthropic Claude Vision](https://docs.anthropic.com/claude/docs/vision)
- [Google Gemini Vision](https://ai.google.dev/gemini-api/docs/vision)
- [阿里通义千问](https://help.aliyun.com/zh/dashscope/developer-reference/qwen-vl-api)

## 贡献

欢迎贡献新的 LLM 提供商实现！提交 PR 时请确保：

1. ✅ 完整实现 `LLMProvider` trait
2. ✅ 包含单元测试和集成测试
3. ✅ 提供配置示例和使用文档
4. ✅ 遵循现有代码风格和最佳实践
5. ✅ 更新 `mod.rs` 中的导出和注册

## 常见问题

### Q: 如何处理 API 速率限制？
A: 实现退避重试策略，并在 `capabilities()` 中声明限制。

### Q: 如何支持流式响应？
A: 在 `capabilities()` 中设置 `streaming: true`，并实现自定义的流式处理方法。

### Q: 如何处理大尺寸图片？
A: 在发送前压缩图片，或使用 URL 而非 base64 编码。

### Q: 如何添加自定义提示词？
A: 在 `configure()` 方法中接收 `custom_prompt` 参数。

---

**开发愉快！** 如有问题，请查看 [Issues](https://github.com/deletexiumu/screen-analyzer/issues) 或提交新的问题。
