// 总结领域 - 负责生成每日活动总结、统计分析等

use crate::actors::LLMHandle;
use crate::storage::{Database, Session};
use chrono::Timelike;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

/// 每日总结
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    /// 日期
    pub date: String,
    /// AI生成或规则生成的总结文本
    pub summary_text: String,
    /// 设备统计
    pub device_stats: Vec<DeviceStat>,
    /// 并行工作分析
    pub parallel_work: Vec<ParallelWork>,
    /// 使用模式
    pub usage_patterns: Vec<UsagePattern>,
    /// 活跃设备数量
    pub active_device_count: usize,
}

/// 设备统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStat {
    /// 设备名称
    pub name: String,
    /// 设备类型
    #[serde(rename = "type")]
    pub device_type: String,
    /// 总使用时长（分钟）
    pub total_minutes: i64,
    /// 格式化的时长字符串 (如 "2h 30m")
    pub total_time: String,
    /// 截图数量
    pub screenshots: i64,
}

/// 并行工作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParallelWork {
    /// 时间范围 (如 "14:00-14:30")
    pub time_range: String,
    /// 标题
    pub title: String,
    /// 描述
    pub description: String,
    /// 持续时长（分钟）
    pub duration: i64,
}

/// 使用模式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagePattern {
    /// 模式标签
    pub label: String,
    /// 模式值
    pub value: String,
}

/// 总结生成器
pub struct SummaryGenerator {
    db: Arc<Database>,
    llm_handle: Option<LLMHandle>,
}

impl SummaryGenerator {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            llm_handle: None,
        }
    }

    pub fn with_llm(db: Arc<Database>, llm_handle: LLMHandle) -> Self {
        Self {
            db,
            llm_handle: Some(llm_handle),
        }
    }

    /// 生成每日总结
    ///
    /// # 参数
    /// * `date` - 日期 (YYYY-MM-DD)
    /// * `force_refresh` - 是否强制重新生成（跳过缓存）
    pub async fn generate_day_summary(&self, date: &str, force_refresh: bool) -> Result<DaySummary, String> {
        info!("生成每日总结: {} (force_refresh={})", date, force_refresh);

        // 如果不是强制刷新，先尝试从数据库读取缓存
        if !force_refresh {
            match self.db.get_day_summary(date).await {
                Ok(Some(cached)) => {
                    info!("使用缓存的每日总结: {}", date);

                    // 反序列化 JSON 字段
                    let device_stats = serde_json::from_str(&cached.device_stats)
                        .unwrap_or_default();
                    let parallel_work = serde_json::from_str(&cached.parallel_work)
                        .unwrap_or_default();
                    let usage_patterns = serde_json::from_str(&cached.usage_patterns)
                        .unwrap_or_default();

                    return Ok(DaySummary {
                        date: cached.date.format("%Y-%m-%d").to_string(),
                        summary_text: cached.summary_text,
                        device_stats,
                        parallel_work,
                        usage_patterns,
                        active_device_count: cached.active_device_count as usize,
                    });
                }
                Ok(None) => {
                    info!("数据库中无缓存，重新生成总结");
                }
                Err(e) => {
                    warn!("读取缓存失败: {}, 将重新生成", e);
                }
            }
        }

        // 获取当天的所有会话
        let sessions = self
            .db
            .get_sessions_by_date(date)
            .await
            .map_err(|e| format!("获取会话失败: {}", e))?;

        if sessions.is_empty() {
            return Ok(DaySummary {
                date: date.to_string(),
                summary_text: "今天没有活动记录".to_string(),
                device_stats: vec![],
                parallel_work: vec![],
                usage_patterns: vec![],
                active_device_count: 0,
            });
        }

        // 统计设备数量
        let active_devices: std::collections::HashSet<String> = sessions
            .iter()
            .filter_map(|s| s.device_name.clone())
            .collect();
        let active_device_count = active_devices.len();

        // 计算设备统计
        let device_stats = self.calculate_device_stats(&sessions).await?;

        // 检测并行工作
        let parallel_work = self.detect_parallel_work(&sessions);

        // 分析使用模式
        let usage_patterns = self.analyze_usage_patterns(&sessions, active_device_count);

        // 生成总结文本
        let summary_text = self
            .generate_summary_text(date, &sessions, &device_stats, &parallel_work, &usage_patterns, active_device_count)
            .await;

        let summary = DaySummary {
            date: date.to_string(),
            summary_text: summary_text.clone(),
            device_stats: device_stats.clone(),
            parallel_work: parallel_work.clone(),
            usage_patterns: usage_patterns.clone(),
            active_device_count,
        };

        // 保存到数据库
        // 解析日期字符串
        let naive_date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|e| format!("日期格式错误: {}", e))?;

        let record = crate::storage::DaySummaryRecord {
            id: None,
            date: naive_date,
            summary_text,
            device_stats: serde_json::to_string(&device_stats).unwrap_or_default(),
            parallel_work: serde_json::to_string(&parallel_work).unwrap_or_default(),
            usage_patterns: serde_json::to_string(&usage_patterns).unwrap_or_default(),
            active_device_count: active_device_count as i32,
            llm_call_id: None, // TODO: 关联 LLM 调用记录
            created_at: crate::storage::local_now(),
            updated_at: crate::storage::local_now(),
        };

        if let Err(e) = self.db.save_day_summary(date, &record).await {
            warn!("保存每日总结失败: {}", e);
            // 不影响返回结果，只是记录警告
        } else {
            info!("每日总结已保存到数据库: {}", date);
        }

        Ok(summary)
    }

    /// 计算设备统计
    async fn calculate_device_stats(&self, sessions: &[Session]) -> Result<Vec<DeviceStat>, String> {
        let mut device_map: HashMap<String, (String, i64, i64)> = HashMap::new();

        for session in sessions {
            let device_name = session
                .device_name
                .clone()
                .unwrap_or_else(|| "Unknown Device".to_string());
            let device_type = session
                .device_type
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            let duration = (session.end_time - session.start_time).num_minutes();

            // 获取截图数量（从数据库查询）
            let screenshot_count = if let Some(session_id) = session.id {
                match self.db.get_frames_by_session(session_id).await {
                    Ok(frames) => frames.len() as i64,
                    Err(_) => {
                        // 如果无法获取帧数，估算为每分钟1帧
                        duration
                    }
                }
            } else {
                duration
            };

            let entry = device_map.entry(device_name.clone()).or_insert((
                device_type.clone(),
                0,
                0,
            ));
            entry.1 += duration;
            entry.2 += screenshot_count;
        }

        let mut stats: Vec<DeviceStat> = device_map
            .into_iter()
            .map(|(name, (device_type, minutes, screenshots))| {
                let total_time = format_duration(minutes);
                DeviceStat {
                    name,
                    device_type,
                    total_minutes: minutes,
                    total_time,
                    screenshots,
                }
            })
            .collect();

        // 按使用时长排序
        stats.sort_by(|a, b| b.total_minutes.cmp(&a.total_minutes));

        Ok(stats)
    }

    /// 检测并行工作（不同设备同时使用的时间段）
    fn detect_parallel_work(&self, sessions: &[Session]) -> Vec<ParallelWork> {
        let mut parallel_works = Vec::new();

        if sessions.len() < 2 {
            return parallel_works;
        }

        // 检测时间重叠的会话（表示同时使用多个设备）
        for i in 0..sessions.len() {
            for j in (i + 1)..sessions.len() {
                let s1 = &sessions[i];
                let s2 = &sessions[j];

                // 检查是否是不同设备
                if s1.device_name == s2.device_name {
                    continue;
                }

                let start1 = s1.start_time;
                let end1 = s1.end_time;
                let start2 = s2.start_time;
                let end2 = s2.end_time;

                // 检查时间重叠
                let overlap_start = if start1 > start2 { start1 } else { start2 };
                let overlap_end = if end1 < end2 { end1 } else { end2 };

                if overlap_start < overlap_end {
                    let duration = (overlap_end - overlap_start).num_minutes();

                    // 至少5分钟的重叠才算并行工作
                    if duration >= 5 {
                        let device1 = s1.device_name.as_deref().unwrap_or("Unknown");
                        let device2 = s2.device_name.as_deref().unwrap_or("Unknown");

                        let cat1 = get_category_display_name(&extract_category_from_session(s1));
                        let cat2 = get_category_display_name(&extract_category_from_session(s2));

                        let title = format!("{} + {}", cat1, cat2);

                        let description = format!(
                            "在 {} 上{}，同时在 {} 上{}",
                            device1,
                            extract_activity_name(s1),
                            device2,
                            extract_activity_name(s2)
                        );

                        parallel_works.push(ParallelWork {
                            time_range: format!(
                                "{}-{}",
                                overlap_start.format("%H:%M"),
                                overlap_end.format("%H:%M")
                            ),
                            title,
                            description,
                            duration,
                        });
                    }
                }
            }
        }

        // 按时间排序
        parallel_works.sort_by(|a, b| a.time_range.cmp(&b.time_range));

        // 去重：合并相同时间范围的并行工作
        let mut deduplicated = Vec::new();
        for work in parallel_works {
            if let Some(last) = deduplicated.last_mut() {
                let last_work: &mut ParallelWork = last;
                // 如果时间范围相同，合并描述
                if last_work.time_range == work.time_range {
                    // 跳过重复的，只保留第一个
                    continue;
                }
            }
            deduplicated.push(work);
        }

        // 只保留前5个
        deduplicated.truncate(5);

        deduplicated
    }

    /// 分析使用模式
    fn analyze_usage_patterns(
        &self,
        sessions: &[Session],
        active_device_count: usize,
    ) -> Vec<UsagePattern> {
        let mut patterns = Vec::new();

        if sessions.is_empty() {
            return patterns;
        }

        // 1. 计算最活跃的时间段
        let mut hour_activity = vec![0; 24];
        for session in sessions {
            let start_hour = session.start_time.hour() as usize;
            let end_hour = session.end_time.hour() as usize;

            for hour in start_hour..=end_hour.min(23) {
                hour_activity[hour] += 1;
            }
        }

        if let Some((peak_hour, _)) = hour_activity
            .iter()
            .enumerate()
            .max_by_key(|(_, &count)| count)
        {
            patterns.push(UsagePattern {
                label: "最活跃时段".to_string(),
                value: format!(
                    "{:02}:00 - {:02}:00",
                    peak_hour,
                    (peak_hour + 1) % 24
                ),
            });
        }

        // 2. 计算平均会话时长
        let total_minutes: i64 = sessions
            .iter()
            .map(|s| (s.end_time - s.start_time).num_minutes())
            .sum();
        let avg_duration = total_minutes / sessions.len() as i64;

        patterns.push(UsagePattern {
            label: "平均会话时长".to_string(),
            value: format!("{} 分钟", avg_duration),
        });

        // 3. 计算设备切换次数（如果有多个设备）
        if active_device_count > 1 {
            let mut device_switches = 0;
            let mut last_device: Option<String> = None;

            for session in sessions {
                if let Some(current_device) = &session.device_name {
                    if let Some(last) = &last_device {
                        if last != current_device {
                            device_switches += 1;
                        }
                    }
                    last_device = Some(current_device.clone());
                }
            }

            patterns.push(UsagePattern {
                label: "设备切换次数".to_string(),
                value: format!("{} 次", device_switches),
            });
        }

        // 4. 总活动时长
        patterns.push(UsagePattern {
            label: "总活动时长".to_string(),
            value: format_duration(total_minutes),
        });

        // 5. 会话数量
        patterns.push(UsagePattern {
            label: "会话数量".to_string(),
            value: format!("{} 个", sessions.len()),
        });

        patterns
    }

    /// 生成总结文本（优先使用LLM，fallback到规则）
    async fn generate_summary_text(
        &self,
        date: &str,
        sessions: &[Session],
        device_stats: &[DeviceStat],
        parallel_work: &[ParallelWork],
        usage_patterns: &[UsagePattern],
        active_device_count: usize,
    ) -> String {
        // 计算总时长
        let total_minutes: i64 = sessions
            .iter()
            .map(|s| (s.end_time - s.start_time).num_minutes())
            .sum();

        // 如果有 LLM handle，尝试使用 LLM 生成
        if let Some(llm_handle) = &self.llm_handle {
            match self
                .generate_summary_with_llm(
                    llm_handle,
                    date,
                    device_stats,
                    parallel_work,
                    usage_patterns,
                    sessions.len(),
                    total_minutes,
                )
                .await
            {
                Ok(summary) => {
                    info!("使用 LLM 生成总结成功");
                    return summary;
                }
                Err(e) => {
                    warn!("LLM 生成总结失败，使用规则生成: {}", e);
                    // fallback 到规则生成
                }
            }
        }

        // 规则生成（fallback）
        self.generate_summary_with_rules(sessions, total_minutes, active_device_count)
    }

    /// 使用 LLM 生成总结
    async fn generate_summary_with_llm(
        &self,
        llm_handle: &LLMHandle,
        date: &str,
        device_stats: &[DeviceStat],
        parallel_work: &[ParallelWork],
        usage_patterns: &[UsagePattern],
        session_count: usize,
        total_minutes: i64,
    ) -> Result<String, String> {
        // 将数据序列化为 JSON 字符串
        let device_stats_json =
            serde_json::to_string_pretty(device_stats).unwrap_or_else(|_| "[]".to_string());
        let parallel_work_json =
            serde_json::to_string_pretty(parallel_work).unwrap_or_else(|_| "[]".to_string());
        let usage_patterns_json =
            serde_json::to_string_pretty(usage_patterns).unwrap_or_else(|_| "[]".to_string());

        // 调用 LLM
        llm_handle
            .generate_day_summary(
                date,
                &device_stats_json,
                &parallel_work_json,
                &usage_patterns_json,
                session_count,
                total_minutes,
            )
            .await
            .map_err(|e| e.to_string())
    }

    /// 使用规则生成总结（fallback）
    fn generate_summary_with_rules(
        &self,
        sessions: &[Session],
        total_minutes: i64,
        active_device_count: usize,
    ) -> String {
        // 统计主要活动类别
        let mut category_counts: HashMap<String, usize> = HashMap::new();
        for session in sessions {
            let category = extract_category_from_session(session);
            *category_counts.entry(category).or_insert(0) += 1;
        }

        let main_category = category_counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(cat, _)| cat.clone())
            .unwrap_or_else(|| "Work".to_string());

        // 生成总结文本
        format!(
            "High productivity day with {} work sessions across {} devices. {} dominated the day with {} total tracked time.",
            sessions.len(),
            active_device_count,
            get_category_display_name(&main_category),
            format_duration(total_minutes)
        )
    }
}

// ==================== 辅助函数 ====================

/// 格式化时长
fn format_duration(minutes: i64) -> String {
    if minutes < 60 {
        format!("{}m", minutes)
    } else {
        let hours = minutes / 60;
        let mins = minutes % 60;
        if mins == 0 {
            format!("{}h", hours)
        } else {
            format!("{}h {}m", hours, mins)
        }
    }
}

/// 从会话中提取类别
fn extract_category_from_session(session: &Session) -> String {
    // 尝试从 tags JSON 中提取类别
    if let Ok(tags) = serde_json::from_str::<Vec<serde_json::Value>>(&session.tags) {
        if let Some(tag) = tags.first() {
            if let Some(category) = tag.get("category").and_then(|v| v.as_str()) {
                return category.to_string();
            }
        }
    }
    "Other".to_string()
}

/// 提取活动名称
fn extract_activity_name(session: &Session) -> String {
    if !session.title.is_empty() && session.title != "null" {
        // 使用 chars() 正确处理 UTF-8 字符边界
        let char_count = session.title.chars().count();
        if char_count > 30 {
            let truncated: String = session.title.chars().take(30).collect();
            format!("{}...", truncated)
        } else {
            session.title.clone()
        }
    } else {
        get_category_display_name(&extract_category_from_session(session))
    }
}

/// 获取类别显示名称
fn get_category_display_name(category: &str) -> String {
    match category.to_lowercase().as_str() {
        "work" | "coding" => "工作".to_string(),
        "communication" | "meeting" => "会议".to_string(),
        "learning" => "学习".to_string(),
        "personal" => "个人".to_string(),
        "idle" => "空闲".to_string(),
        _ => "其他".to_string(),
    }
}
