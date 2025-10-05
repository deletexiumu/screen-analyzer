// Notion API 客户端模块
// 负责与 Notion API 交互，实现数据同步功能

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::path::Path;
use tokio::fs;
use tracing::{error, info, warn};

use crate::models::{NotionConfig, Session};

const NOTION_API_VERSION: &str = "2022-06-28";
const NOTION_API_BASE: &str = "https://api.notion.com/v1";

/// 获取系统时区
fn get_system_timezone() -> String {
    // 尝试获取系统时区
    #[cfg(target_os = "macos")]
    {
        // macOS 使用 Asia/Shanghai 或其他 IANA 时区
        use std::process::Command;
        if let Ok(output) = Command::new("readlink").arg("/etc/localtime").output() {
            if let Ok(path) = String::from_utf8(output.stdout) {
                // 从 /var/db/timezone/zoneinfo/Asia/Shanghai 提取 Asia/Shanghai
                if let Some(tz) = path.trim().split("/zoneinfo/").nth(1) {
                    return tz.to_string();
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::fs;
        if let Ok(tz) = fs::read_to_string("/etc/timezone") {
            return tz.trim().to_string();
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows 时区映射较复杂，默认使用 Asia/Shanghai
        return "Asia/Shanghai".to_string();
    }

    // 默认返回 Asia/Shanghai
    "Asia/Shanghai".to_string()
}

/// 英文关键词到中文的映射
fn translate_keyword(keyword: &str) -> String {
    match keyword.to_lowercase().as_str() {
        "development" | "coding" | "programming" => "开发".to_string(),
        "design" | "ui" | "ux" => "设计".to_string(),
        "meeting" | "conference" => "会议".to_string(),
        "email" | "mail" => "邮件".to_string(),
        "chat" | "messaging" => "聊天".to_string(),
        "reading" | "research" => "研究".to_string(),
        "writing" | "documentation" => "写作".to_string(),
        "testing" | "debug" => "测试".to_string(),
        "planning" | "management" => "规划".to_string(),
        "social" | "social media" => "社交".to_string(),
        "entertainment" | "video" => "娱乐".to_string(),
        "shopping" => "购物".to_string(),
        "learning" | "study" => "学习".to_string(),
        "break" | "rest" => "休息".to_string(),
        _ => keyword.to_string(), // 未匹配的保持原样
    }
}

/// Notion 页面/数据库信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotionPage {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub page_type: String, // "database" 或 "page"
    pub icon: Option<String>, // emoji 或 URL
}

/// Notion API 客户端
#[derive(Clone)]
pub struct NotionClient {
    config: NotionConfig,
    client: Client,
}

impl NotionClient {
    /// 创建新的 Notion 客户端
    pub fn new(config: NotionConfig) -> Result<Self> {
        if config.api_token.is_empty() {
            return Err(anyhow!("Notion API Token 不能为空"));
        }
        // database_id 可以为空（用于测试连接或搜索页面）

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { config, client })
    }

    /// 获取配置信息（用于调试）
    pub fn get_config(&self) -> &NotionConfig {
        &self.config
    }

    /// 测试 Notion API 连接
    pub async fn test_connection(&self) -> Result<String> {
        let url = format!("{}/databases/{}", NOTION_API_BASE, self.config.database_id);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_token))
            .header("Notion-Version", NOTION_API_VERSION)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Notion API 连接失败: {}", error_text));
        }

        let database: Value = response.json().await?;
        let title = database["title"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|t| t["plain_text"].as_str())
            .unwrap_or("未知");

        Ok(format!("连接成功！数据库名称: {}", title))
    }

    /// 同步会话到 Notion
    pub async fn sync_session(&self, session: &Session) -> Result<String> {
        if !self.config.enabled {
            return Ok("Notion 同步已禁用".to_string());
        }

        if !self.config.sync_options.sync_sessions {
            return Ok("会话同步已禁用".to_string());
        }

        info!("开始同步会话 {:?} 到 Notion", session.id);

        // 构建 Notion 页面属性
        let properties = self.build_session_properties(session)?;

        // 创建 Notion 页面
        let url = format!("{}/pages", NOTION_API_BASE);
        let payload = json!({
            "parent": { "database_id": self.config.database_id },
            "properties": properties,
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("同步会话到 Notion 失败: {}", error_text);
            return Err(anyhow!("同步失败: {}", error_text));
        }

        let page: Value = response.json().await?;
        let page_id = page["id"].as_str().unwrap_or("unknown").to_string();

        info!(
            "会话 {:?} 成功同步到 Notion，页面 ID: {}",
            session.id, page_id
        );

        // 如果有视频且启用了视频同步，添加视频到页面内容
        if self.config.sync_options.sync_videos {
            if let Some(video_path) = &session.video_path {
                if !video_path.is_empty() {
                    match self.add_video_to_page(&page_id, video_path).await {
                        Ok(msg) => info!("视频添加成功: {}", msg),
                        Err(e) => warn!("视频添加失败: {}", e),
                    }
                }
            }
        }

        Ok(page_id)
    }

    /// 构建会话的 Notion 属性
    fn build_session_properties(&self, session: &Session) -> Result<Value> {
        use chrono::Local;

        // 问题分析：
        // 1. 数据库存储的是本地时间（虽然类型标记为 DateTime<Utc>）
        // 2. Notion API 将不带时区的 ISO 8601 时间理解为 UTC
        // 3. Notion 数据库时区设置（如 Asia/Shanghai）会将 UTC 时间转换显示
        //
        // 示例（中国时区 UTC+8）：
        // - 数据库存储：18:00（本地时间）
        // - 发送 "18:00" → Notion 理解为 UTC 18:00 → 显示为 Asia/Shanghai 02:00（次日）❌
        // - 发送 "10:00" → Notion 理解为 UTC 10:00 → 显示为 Asia/Shanghai 18:00（正确）✓
        //
        // 解决方案：减去本地时区偏移量，得到对应的 UTC 时间

        // 获取本地时区偏移量（秒）
        let offset_seconds = Local::now().offset().local_minus_utc();
        let offset_duration = chrono::Duration::seconds(offset_seconds as i64);

        // 减去时区偏移得到 UTC 时间
        let start_utc = session.start_time - offset_duration;
        let end_utc = session.end_time - offset_duration;

        // 格式化为不带时区的 ISO 8601
        let start_time_str = start_utc.format("%Y-%m-%dT%H:%M:%S").to_string();
        let end_time_str = end_utc.format("%Y-%m-%dT%H:%M:%S").to_string();

        let mut properties = json!({
            // 标题（必需）
            "标题": {
                "title": [{
                    "text": { "content": session.title.clone() }
                }]
            },
            // 会话时间（使用本地时间格式）
            "日期": {
                "date": {
                    "start": start_time_str,
                    "end": end_time_str
                }
            },
            // 总结
            "总结": {
                "rich_text": [{
                    "text": { "content": session.summary.clone() }
                }]
            },
            // 设备信息
            "设备": {
                "select": {
                    "name": format!("{} ({})",
                        session.device_name.as_deref().unwrap_or("未知"),
                        session.device_type.as_deref().unwrap_or("未知"))
                }
            },
            // 本地ID（用于去重）
            "本地ID": {
                "rich_text": [{
                    "text": { "content": session.id.map(|id| id.to_string()).unwrap_or_else(|| "未知".to_string()) }
                }]
            },
        });

        // 添加类别和关键词（解析 JSON 格式的标签）
        if !session.tags.is_empty() {
            // 尝试解析 JSON 格式的标签
            if let Ok(activity_tags) =
                serde_json::from_str::<Vec<crate::models::ActivityTag>>(&session.tags)
            {
                // 获取主要类别（第一个标签的类别）
                if let Some(first_tag) = activity_tags.first() {
                    let category_name = match first_tag.category {
                        crate::models::ActivityCategory::Work => "工作",
                        crate::models::ActivityCategory::Communication => "沟通",
                        crate::models::ActivityCategory::Learning => "学习",
                        crate::models::ActivityCategory::Personal => "个人",
                        crate::models::ActivityCategory::Idle => "空闲",
                        crate::models::ActivityCategory::Other => "其他",
                    };

                    properties["类别"] = json!({
                        "select": {
                            "name": category_name
                        }
                    });

                    // 收集所有关键词（翻译为中文）
                    let mut keywords = Vec::new();
                    for tag in activity_tags {
                        for keyword in tag.keywords {
                            if !keyword.trim().is_empty() {
                                // 英文关键词映射为中文
                                let cn_keyword = translate_keyword(&keyword);
                                keywords.push(json!({ "name": cn_keyword }));
                            }
                        }
                    }

                    if !keywords.is_empty() {
                        properties["关键词"] = json!({
                            "multi_select": keywords
                        });
                    }
                }
            }
        }

        // 计算时长（分钟）
        let duration_secs = (session.end_time - session.start_time).num_seconds();
        let duration_minutes = duration_secs / 60;

        properties["时长"] = json!({
            "number": duration_minutes
        });

        // 设置类型为"会话记录"
        properties["类型"] = json!({
            "select": {
                "name": "会话记录"
            }
        });

        Ok(properties)
    }

    /// 添加视频到页面内容
    async fn add_video_to_page(&self, page_id: &str, video_path: &str) -> Result<String> {
        info!("上传视频到 Notion 页面: {}", page_id);

        let path = Path::new(video_path);
        if !path.exists() {
            return Err(anyhow!("视频文件不存在: {}", video_path));
        }

        // 获取文件信息
        let metadata = fs::metadata(path).await?;
        let size_mb = metadata.len() / (1024 * 1024);
        let file_size = metadata.len();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("video.mp4");

        // 检查文件大小限制
        if size_mb > self.config.sync_options.video_size_limit_mb as u64 {
            warn!(
                "视频文件 {} 大小 {}MB 超过限制 {}MB，跳过上传",
                file_name, size_mb, self.config.sync_options.video_size_limit_mb
            );
            return Ok(format!("文件过大（{}MB），已跳过", size_mb));
        }

        // 确定是否需要使用 multi-part 模式
        const MULTI_PART_THRESHOLD: u64 = 20 * 1024 * 1024; // 20 MB
        const PART_SIZE: u64 = 10 * 1024 * 1024; // 10 MB 每个分块
        let use_multi_part = file_size > MULTI_PART_THRESHOLD;

        // 步骤1: 创建 FileUpload 对象
        info!("步骤1: 创建 FileUpload 对象");
        let create_url = format!("{}/file_uploads", NOTION_API_BASE);

        let create_payload = if use_multi_part {
            let number_of_parts = ((file_size + PART_SIZE - 1) / PART_SIZE) as i32;
            info!(
                "文件大小 {} MB 超过 20 MB，使用 multi-part 模式（{} 个块）",
                size_mb, number_of_parts
            );

            json!({
                "mode": "multi_part",
                "number_of_parts": number_of_parts,
                "filename": file_name,
                "content_type": "video/mp4"
            })
        } else {
            json!({
                "filename": file_name,
                "content_type": "video/mp4"
            })
        };

        let response = self
            .client
            .post(&create_url)
            .header("Authorization", format!("Bearer {}", self.config.api_token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&create_payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("创建 FileUpload 失败: {}", error_text);
            return Err(anyhow!("创建 FileUpload 失败: {}", error_text));
        }

        let file_upload: Value = response.json().await?;
        let upload_url = file_upload["upload_url"]
            .as_str()
            .ok_or_else(|| anyhow!("未获取到 upload_url"))?;
        let file_upload_id = file_upload["id"]
            .as_str()
            .ok_or_else(|| anyhow!("未获取到 file upload id"))?;

        info!("FileUpload 创建成功，ID: {}", file_upload_id);

        // 步骤2: 上传文件内容
        info!("步骤2: 上传文件内容 (文件大小: {} MB)", size_mb);
        let file_bytes = fs::read(path).await?;

        // 创建一个有更长超时时间的客户端（文件上传可能需要更长时间）
        let upload_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300)) // 5分钟超时
            .build()?;

        if use_multi_part {
            // Multi-part 模式：分块上传
            let mut offset: u64 = 0;
            let mut part_number = 1;
            const MAX_RETRIES: u32 = 3;
            const RETRY_DELAY_MS: u64 = 2000;

            while offset < file_size {
                let chunk_size = std::cmp::min(PART_SIZE, file_size - offset);
                let chunk = &file_bytes[offset as usize..(offset + chunk_size) as usize];

                info!(
                    "上传第 {} 块 ({} MB)...",
                    part_number,
                    chunk_size / (1024 * 1024)
                );

                // 带重试的上传逻辑
                let mut retry_count = 0;
                let upload_success = loop {
                    let form = reqwest::multipart::Form::new()
                        .text("part_number", part_number.to_string())
                        .part(
                            "file",
                            reqwest::multipart::Part::bytes(chunk.to_vec())
                                .file_name(file_name.to_string())
                                .mime_str("video/mp4")?,
                        );

                    let upload_response = upload_client
                        .post(upload_url)
                        .header("Authorization", format!("Bearer {}", self.config.api_token))
                        .header("Notion-Version", NOTION_API_VERSION)
                        .multipart(form)
                        .send()
                        .await;

                    match upload_response {
                        Ok(resp) if resp.status().is_success() => {
                            info!("第 {} 块上传成功", part_number);
                            break true;
                        }
                        Ok(resp) => {
                            let status = resp.status();
                            let error_text = resp
                                .text()
                                .await
                                .unwrap_or_else(|_| "无法读取错误信息".to_string());

                            retry_count += 1;
                            if retry_count >= MAX_RETRIES {
                                error!(
                                    "上传第 {} 块失败: error code: {}, {}",
                                    part_number,
                                    status.as_u16(),
                                    error_text
                                );
                                return Err(anyhow!(
                                    "上传第 {} 块失败: error code: {}",
                                    part_number,
                                    status.as_u16()
                                ));
                            }

                            warn!("上传第 {} 块失败 (尝试 {}/{}): error code: {}, 等待 {}ms 后重试...",
                                part_number, retry_count, MAX_RETRIES, status.as_u16(), RETRY_DELAY_MS);
                            tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_DELAY_MS))
                                .await;
                        }
                        Err(e) => {
                            retry_count += 1;
                            if retry_count >= MAX_RETRIES {
                                error!("上传第 {} 块网络错误: {}", part_number, e);
                                return Err(anyhow!("上传第 {} 块网络错误: {}", part_number, e));
                            }

                            warn!(
                                "上传第 {} 块网络错误 (尝试 {}/{}): {}, 等待 {}ms 后重试...",
                                part_number, retry_count, MAX_RETRIES, e, RETRY_DELAY_MS
                            );
                            tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_DELAY_MS))
                                .await;
                        }
                    }
                };

                if !upload_success {
                    return Err(anyhow!("上传第 {} 块失败，已达到最大重试次数", part_number));
                }

                offset += chunk_size;
                part_number += 1;
            }

            // 完成 multi-part 上传
            info!("完成 multi-part 上传");
            let complete_url = format!(
                "{}/file_uploads/{}/complete",
                NOTION_API_BASE, file_upload_id
            );

            let complete_response = upload_client
                .post(&complete_url)
                .header("Authorization", format!("Bearer {}", self.config.api_token))
                .header("Notion-Version", NOTION_API_VERSION)
                .send()
                .await?;

            if !complete_response.status().is_success() {
                let error_text = complete_response.text().await?;
                error!("完成上传失败: {}", error_text);
                return Err(anyhow!("完成上传失败: {}", error_text));
            }

            info!("Multi-part 上传完成");
        } else {
            // Single-part 模式：一次性上传
            let form = reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(file_bytes)
                    .file_name(file_name.to_string())
                    .mime_str("video/mp4")?,
            );

            let upload_response = upload_client
                .post(upload_url)
                .header("Authorization", format!("Bearer {}", self.config.api_token))
                .header("Notion-Version", NOTION_API_VERSION)
                .multipart(form)
                .send()
                .await?;

            if !upload_response.status().is_success() {
                let error_text = upload_response.text().await?;
                error!("上传文件内容失败: {}", error_text);
                return Err(anyhow!("上传文件内容失败: {}", error_text));
            }

            info!("文件内容上传成功");
        }

        // 步骤3: 先用 file_upload 创建视频块
        info!("步骤3: 创建视频块（使用 file_upload）");
        let blocks_url = format!("{}/blocks/{}/children", NOTION_API_BASE, page_id);

        let blocks = json!({
            // "parent": { "page_id": page_id },
            "children": [
                {
                    "object": "block",
                    "type": "heading_2",
                    "heading_2": {
                        "rich_text": [{
                            "type": "text",
                            "text": { "content": "📹 会话视频" }
                        }]
                    }
                },
                {
                    "object": "block",
                    "type": "video",
                    "video": {
                        "type": "file_upload",
                        "file_upload": {
                            "id": file_upload_id
                        }
                    }
                }
            ]
        });

        let blocks_response = self
            .client
            .patch(&blocks_url)
            .header("Authorization", format!("Bearer {}", self.config.api_token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&blocks)
            .send()
            .await?;

        if !blocks_response.status().is_success() {
            let error_text = blocks_response.text().await?;
            warn!("添加视频块失败: {}", error_text);
            return Err(anyhow!("添加视频块失败: {}", error_text));
        }

        let blocks_result: Value = blocks_response.json().await?;

        // 尝试从响应中提取视频 URL
        if let Some(results) = blocks_result["results"].as_array() {
            for block in results {
                if block["type"].as_str() == Some("video") {
                    // 检查是否有 file 类型的 URL
                    if let Some(url) = block["video"]["file"]["url"].as_str() {
                        info!("视频块创建成功，Notion URL: {}", url);
                    }
                }
            }
        }

        info!("视频上传并添加到页面成功");
        Ok(format!("视频已上传: {} ({} MB)", file_name, size_mb))
    }

    /// 上传视频到 Notion（如果启用且文件大小合适）
    pub async fn upload_video(&self, _session_id: i64, video_path: &str) -> Result<String> {
        if !self.config.sync_options.sync_videos {
            return Ok("视频同步已禁用".to_string());
        }

        let path = Path::new(video_path);
        if !path.exists() {
            return Err(anyhow!("视频文件不存在: {}", video_path));
        }

        // 检查文件大小
        let metadata = fs::metadata(path).await?;
        let size_mb = metadata.len() / (1024 * 1024);

        if size_mb > self.config.sync_options.video_size_limit_mb as u64 {
            warn!(
                "视频文件 {} 大小 {}MB 超过限制 {}MB，跳过上传",
                video_path, size_mb, self.config.sync_options.video_size_limit_mb
            );
            return Ok(format!("文件过大（{}MB），已跳过", size_mb));
        }

        // TODO: 实现视频上传逻辑
        // Notion API 的文件上传比较复杂，需要先上传到外部存储，然后添加链接
        // 这里先返回占位信息
        warn!("视频上传功能待实现");
        Ok("视频上传功能待实现".to_string())
    }

    /// 同步每日总结到 Notion
    pub async fn sync_daily_summary(&self, date: &str, summary: &str) -> Result<String> {
        if !self.config.sync_options.sync_daily_summary {
            return Ok("每日总结同步已禁用".to_string());
        }

        info!("开始同步每日总结 {} 到 Notion", date);

        let properties = json!({
            "标题": {
                "title": [{
                    "text": { "content": format!("每日总结 - {}", date) }
                }]
            },
            "日期": {
                "date": {
                    "start": date
                }
            },
            "总结": {
                "rich_text": [{
                    "text": { "content": summary }
                }]
            },
            "类型": {
                "select": {
                    "name": "每日总结"
                }
            }
        });

        let url = format!("{}/pages", NOTION_API_BASE);
        let payload = json!({
            "parent": { "database_id": self.config.database_id },
            "properties": properties,
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("同步每日总结到 Notion 失败: {}", error_text);
            return Err(anyhow!("同步失败: {}", error_text));
        }

        let page: Value = response.json().await?;
        let page_id = page["id"].as_str().unwrap_or("unknown");

        info!("每日总结 {} 成功同步到 Notion，页面 ID: {}", date, page_id);
        Ok(page_id.to_string())
    }

    /// 搜索可用的页面和数据库
    pub async fn search_pages(&self) -> Result<Vec<NotionPage>> {
        let url = format!("{}/search", NOTION_API_BASE);

        let payload = json!({
            "filter": {
                "property": "object",
                "value": "page"
            },
            "page_size": 50
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("搜索页面失败: {}", error_text));
        }

        let result: Value = response.json().await?;
        let mut pages = Vec::new();

        if let Some(results) = result["results"].as_array() {
            for item in results {
                let id = item["id"].as_str().unwrap_or("").replace("-", ""); // 移除 ID 中的连字符

                let page_type = item["object"].as_str().unwrap_or("page");

                // 获取标题
                let title = if page_type == "database" {
                    // 数据库标题
                    item["title"]
                        .as_array()
                        .and_then(|arr| arr.first())
                        .and_then(|t| t["plain_text"].as_str())
                        .unwrap_or("未命名数据库")
                        .to_string()
                } else {
                    // 页面标题
                    item["properties"]["title"]["title"]
                        .as_array()
                        .and_then(|arr| arr.first())
                        .and_then(|t| t["plain_text"].as_str())
                        .unwrap_or("未命名页面")
                        .to_string()
                };

                // 获取图标
                let icon = item["icon"]["emoji"].as_str().map(|s| s.to_string());

                pages.push(NotionPage {
                    id,
                    title,
                    page_type: page_type.to_string(),
                    icon,
                });
            }
        }

        Ok(pages)
    }

    /// 在指定页面下创建数据库
    pub async fn create_database(
        &self,
        parent_page_id: &str,
        database_name: &str,
    ) -> Result<String> {
        let url = format!("{}/databases", NOTION_API_BASE);

        // 获取系统时区
        let system_timezone = get_system_timezone();

        let payload = json!({
            "parent": {
                "type": "page_id",
                "page_id": parent_page_id
            },
            "title": [{
                "type": "text",
                "text": { "content": database_name }
            }],
            "properties": {
                "标题": {
                    "title": {}
                },
                "日期": {
                    "date": {
                        "time_zone": system_timezone
                    }
                },
                "总结": {
                    "rich_text": {}
                },
                "设备": {
                    "select": {}
                },
                "本地ID": {
                    "rich_text": {}
                },
                "类别": {
                    "select": {}
                },
                "关键词": {
                    "multi_select": {}
                },
                "时长": {
                    "number": {
                        "format": "number"
                    }
                },
                "类型": {
                    "select": {}
                }
            }
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("创建数据库失败: {}", error_text);
            return Err(anyhow!("创建数据库失败: {}", error_text));
        }

        let database: Value = response.json().await?;
        let database_id = database["id"].as_str().unwrap_or("").replace("-", ""); // 移除 ID 中的连字符

        info!("成功创建数据库，ID: {}", database_id);
        Ok(database_id)
    }

    /// 检查会话是否已经同步（通过本地ID）
    pub async fn is_session_synced(&self, session_id: i64) -> Result<bool> {
        let url = format!(
            "{}/databases/{}/query",
            NOTION_API_BASE, self.config.database_id
        );

        let filter = json!({
            "filter": {
                "property": "本地ID",
                "rich_text": {
                    "equals": session_id.to_string()
                }
            }
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&filter)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("查询失败: {}", error_text));
        }

        let result: Value = response.json().await?;
        let empty_vec = vec![];
        let results = result["results"].as_array().unwrap_or(&empty_vec);

        Ok(!results.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notion_client_creation() {
        let config = NotionConfig {
            enabled: true,
            api_token: "test_token".to_string(),
            database_id: "test_db_id".to_string(),
            ..Default::default()
        };

        let client = NotionClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_notion_client_empty_token() {
        let config = NotionConfig {
            enabled: true,
            api_token: "".to_string(),
            database_id: "test_db_id".to_string(),
            ..Default::default()
        };

        let client = NotionClient::new(config);
        assert!(client.is_err());
    }
}
