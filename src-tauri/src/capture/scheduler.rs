// 截屏调度器 - 负责定时截屏任务的调度
//
// 使用事件驱动架构,通过EventBus发布SessionCompleted事件
// 解耦调度器与业务逻辑处理

use super::ScreenCapture;
use crate::event_bus::{AppEvent, EventBus};
use anyhow::{anyhow, Result};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, trace};

/// 窗口跟踪器 - 用于跟踪已处理的窗口，防止内存泄漏
struct WindowTracker {
    /// 已处理的窗口集合
    processed: HashSet<i64>,
    /// 窗口队列，用于按顺序移除旧窗口
    queue: VecDeque<i64>,
    /// 最大容量
    max_size: usize,
}

impl WindowTracker {
    /// 创建新的窗口跟踪器
    fn new(max_size: usize) -> Self {
        Self {
            processed: HashSet::new(),
            queue: VecDeque::new(),
            max_size,
        }
    }

    /// 检查窗口是否已处理
    fn contains(&self, key: &i64) -> bool {
        self.processed.contains(key)
    }

    /// 插入新窗口
    fn insert(&mut self, key: i64) {
        // 如果已经存在，不需要重复插入
        if self.processed.contains(&key) {
            return;
        }

        // 如果达到最大容量，移除最旧的窗口
        if self.queue.len() >= self.max_size {
            if let Some(old_key) = self.queue.pop_front() {
                self.processed.remove(&old_key);
            }
        }

        self.processed.insert(key);
        self.queue.push_back(key);
    }
}

/// 截屏调度器
pub struct CaptureScheduler {
    /// 截屏管理器
    capture: Arc<ScreenCapture>,
    /// 截屏间隔（秒）
    capture_interval: u64,
    /// 会话时长（分钟）
    session_duration: u64,
}

impl CaptureScheduler {
    /// 创建新的调度器
    pub fn new(capture: Arc<ScreenCapture>) -> Self {
        Self {
            capture,
            capture_interval: 1,  // 默认1秒一次（1 FPS）
            session_duration: 15, // 默认15分钟一个会话
        }
    }

    /// 配置调度参数
    pub fn configure(&mut self, capture_interval: u64, session_duration: u64) {
        self.capture_interval = capture_interval;
        self.session_duration = session_duration;
        info!(
            "调度器配置更新: 截屏间隔={}秒, 会话时长={}分钟",
            capture_interval, session_duration
        );
    }

    /// 启动截屏任务
    pub fn start_capture_task(self: Arc<Self>) {
        let capture = self.capture.clone();
        let interval_secs = self.capture_interval;

        info!("准备启动截屏任务，间隔: {}秒", interval_secs);

        // 直接在当前的异步上下文中生成任务
        tokio::task::spawn(async move {
            info!("截屏任务已启动，间隔: {}秒", interval_secs);
            let mut interval = interval(Duration::from_secs(interval_secs));

            // 立即执行第一次截屏（检查锁屏状态）
            if super::ScreenCapture::is_screen_locked() {
                trace!("系统锁屏中，跳过初始截屏");
            } else {
                match capture.capture_frame().await {
                    Ok(frame) => {
                        trace!("初始截屏成功: {}", frame.timestamp);
                    }
                    Err(e) => {
                        // 黑屏不是真正的错误，只记录trace级别日志
                        if e.to_string().contains("黑屏") {
                            debug!("初始截屏检测到黑屏，已跳过");
                        } else {
                            error!("初始截屏失败: {}", e);
                        }
                    }
                }
            }

            loop {
                interval.tick().await;

                // 检查锁屏状态
                if super::ScreenCapture::is_screen_locked() {
                    info!("系统锁屏中，跳过截屏");
                    continue;
                }

                match capture.capture_frame().await {
                    Ok(frame) => {
                        trace!("自动截屏成功: {}", frame.timestamp);
                    }
                    Err(e) => {
                        // 黑屏不是真正的错误，只记录trace级别日志
                        if e.to_string().contains("黑屏") {
                            trace!("跳过黑屏图像");
                        } else {
                            error!("自动截屏失败: {}", e);
                        }
                    }
                }
            }
        });
    }

    /// 启动会话处理任务(事件驱动版本)
    pub fn start_session_task(self: Arc<Self>, event_bus: Arc<EventBus>) {
        let capture = self.capture.clone();
        let session_mins = self.session_duration;

        tokio::task::spawn(async move {
            // 使用 WindowTracker 限制内存使用，最多保留 1000 个窗口记录
            let mut processed_windows = WindowTracker::new(1000);
            let check_interval = Duration::from_secs(60);

            info!("会话处理任务已启动，每60秒扫描待处理图片（事件驱动模式）");

            loop {
                if let Err(e) = CaptureScheduler::scan_pending_sessions(
                    capture.clone(),
                    event_bus.clone(),
                    session_mins,
                    &mut processed_windows,
                )
                .await
                {
                    error!("扫描待处理图片失败: {}", e);
                }

                tokio::time::sleep(check_interval).await;
            }
        });
    }

    /// 启动所有任务(事件驱动版本)
    pub fn start(self: Arc<Self>, event_bus: Arc<EventBus>) {
        info!("启动截屏调度器（事件驱动模式）...");

        // 启动截屏任务
        self.clone().start_capture_task();

        // 启动会话处理任务
        self.start_session_task(event_bus);

        info!("所有调度任务已启动");
    }

    async fn scan_pending_sessions(
        capture: Arc<ScreenCapture>,
        event_bus: Arc<EventBus>,
        session_duration: u64,
        processed_windows: &mut WindowTracker,
    ) -> Result<()> {
        use chrono::{TimeZone, Utc};

        if session_duration == 0 {
            return Err(anyhow!("会话时长必须大于0"));
        }

        let frames_dir = capture.frames_dir();
        if !frames_dir.exists() {
            return Ok(());
        }

        let interval_ms = session_duration as i64 * 60_000;
        let mut grouped: BTreeMap<i64, Vec<super::ScreenFrame>> = BTreeMap::new();
        let mut entries = tokio::fs::read_dir(&frames_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            if !extension.eq_ignore_ascii_case("jpg") {
                continue;
            }

            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };

            let Ok(timestamp_ms) = stem.parse::<i64>() else {
                trace!("无法解析文件名中的时间戳: {}", stem);
                continue;
            };

            let Some(timestamp) = Utc.timestamp_millis_opt(timestamp_ms).single() else {
                trace!("无法构建时间戳: {}", timestamp_ms);
                continue;
            };

            let frame = super::ScreenFrame {
                timestamp,
                file_path: path.to_string_lossy().to_string(),
                screen_id: 0,
            };

            let bucket = (timestamp_ms / interval_ms) * interval_ms;
            grouped.entry(bucket).or_default().push(frame);
        }

        if grouped.is_empty() {
            trace!("未发现待处理图片");
            return Ok(());
        }

        let now_ms = crate::storage::local_now().timestamp_millis();
        let cutoff_ms = now_ms - 30_000; // 留出缓冲，避免处理仍在写入的区间

        for (bucket_start_ms, mut frames) in grouped.into_iter() {
            let bucket_end_ms = bucket_start_ms + interval_ms;
            if bucket_end_ms > cutoff_ms {
                continue;
            }

            if frames.is_empty() {
                continue;
            }

            if processed_windows.contains(&bucket_start_ms) {
                continue;
            }

            frames.sort_by_key(|f| f.timestamp);

            let Some(window_start) = Utc.timestamp_millis_opt(bucket_start_ms).single() else {
                continue;
            };
            let Some(window_end) = Utc.timestamp_millis_opt(bucket_end_ms).single() else {
                continue;
            };

            let window = SessionWindow {
                start: window_start,
                end: window_end,
            };

            let frame_count = frames.len();
            info!(
                "发现待处理会话: {} - {}, 帧数 {}",
                window.start, window.end, frame_count
            );

            // 发布SessionCompleted事件（事件驱动架构）
            // 不再直接调用processor，而是发布事件让订阅者处理
            event_bus.publish(AppEvent::SessionCompleted {
                session_id: bucket_start_ms, // 使用bucket_start_ms作为临时session_id
                frame_count,
                window_start: window.start,
                window_end: window.end,
            });

            // 标记为已处理
            processed_windows.insert(bucket_start_ms);

            info!(
                "会话事件已发布: {} - {} (session_id: {})",
                window.start, window.end, bucket_start_ms
            );

            // 注意：不再在这里清理图片，由事件订阅者（LLMProcessor）处理后决定是否清理
        }

        Ok(())
    }
}

/// 会话时间窗
#[derive(Debug, Clone)]
pub struct SessionWindow {
    /// 会话开始时间
    pub start: chrono::DateTime<chrono::Utc>,
    /// 会话结束时间
    pub end: chrono::DateTime<chrono::Utc>,
}

/// 会话处理器trait
#[async_trait::async_trait]
pub trait SessionProcessor {
    /// 处理一个会话的截图
    async fn process_session(
        &self,
        frames: Vec<super::ScreenFrame>,
        window: SessionWindow,
    ) -> Result<()>;
}

/// 默认会话处理器（用于测试）
pub struct DefaultSessionProcessor;

#[async_trait::async_trait]
impl SessionProcessor for DefaultSessionProcessor {
    async fn process_session(
        &self,
        frames: Vec<super::ScreenFrame>,
        window: SessionWindow,
    ) -> Result<()> {
        info!(
            "处理会话: {} 帧, 时间段 {} - {}",
            frames.len(),
            window.start,
            window.end
        );
        // 这里将由LLMProcessor实现实际的处理逻辑
        Ok(())
    }
}
