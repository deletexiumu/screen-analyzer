// 事件总线 - 用于模块间解耦通信
//
// 实现发布/订阅模式,消除模块间的直接依赖关系
// 使用 tokio::sync::broadcast 实现高效的事件分发

use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tokio::sync::broadcast;
use crate::llm::SessionSummary;

/// 应用事件枚举 - 定义所有可能的系统事件
#[derive(Debug, Clone)]
pub enum AppEvent {
    // --- 捕获事件 ---

    /// 截屏完成事件
    ScreenshotCaptured {
        session_id: i64,
        frame_path: PathBuf,
        timestamp: DateTime<Utc>,
    },

    /// 会话结束事件
    SessionCompleted {
        session_id: i64,
        frame_count: usize,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    },

    // --- 分析事件 ---

    /// 分析开始事件
    AnalysisStarted {
        session_id: i64,
    },

    /// 分析完成事件
    AnalysisCompleted {
        session_id: i64,
        summary: SessionSummary,
    },

    /// 分析失败事件
    AnalysisFailed {
        session_id: i64,
        error: String,
    },

    // --- 视频事件 ---

    /// 视频生成开始事件
    VideoGenerationStarted {
        session_id: i64,
    },

    /// 视频生成完成事件
    VideoGenerated {
        session_id: i64,
        video_path: PathBuf,
    },

    /// 视频生成失败事件
    VideoGenerationFailed {
        session_id: i64,
        error: String,
    },

    // --- 系统事件 ---

    /// 配置更新事件
    ConfigUpdated {
        config_type: String,
    },

    /// 存储清理开始事件
    StorageCleanupStarted,

    /// 存储清理完成事件
    StorageCleaned {
        sessions_deleted: usize,
        space_freed: u64,
    },
}

/// 事件总线 - 用于模块间解耦通信
///
/// 使用 broadcast channel 实现发布/订阅模式
/// 支持多个订阅者同时接收事件
pub struct EventBus {
    sender: broadcast::Sender<AppEvent>,
}

impl EventBus {
    /// 创建新的事件总线
    ///
    /// # 参数
    /// - `capacity`: 事件缓冲区大小,建议 100-1000
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// 发布事件
    ///
    /// 如果没有订阅者,事件会被丢弃(这是正常的)
    pub fn publish(&self, event: AppEvent) {
        match self.sender.send(event) {
            Ok(receiver_count) => {
                tracing::trace!("事件已发布，订阅者数量: {}", receiver_count);
            }
            Err(_) => {
                // 没有订阅者,忽略错误
                tracing::trace!("事件已发布但无订阅者");
            }
        }
    }

    /// 订阅事件
    ///
    /// 返回一个接收器,可以用 `.recv().await` 接收事件
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.sender.subscribe()
    }

    /// 获取当前订阅者数量
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_basic() {
        let bus = EventBus::new(100);

        // 订阅事件
        let mut receiver = bus.subscribe();

        // 发布事件
        bus.publish(AppEvent::AnalysisStarted { session_id: 1 });

        // 接收事件
        match receiver.recv().await {
            Ok(AppEvent::AnalysisStarted { session_id }) => {
                assert_eq!(session_id, 1);
            }
            _ => panic!("未收到预期事件"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(100);

        // 创建多个订阅者
        let mut receiver1 = bus.subscribe();
        let mut receiver2 = bus.subscribe();

        // 发布事件
        bus.publish(AppEvent::SessionCompleted {
            session_id: 1,
            frame_count: 10,
            window_start: crate::storage::local_now(),
            window_end: crate::storage::local_now(),
        });

        // 两个订阅者都应该收到事件
        assert!(receiver1.try_recv().is_ok());
        assert!(receiver2.try_recv().is_ok());
    }
}
