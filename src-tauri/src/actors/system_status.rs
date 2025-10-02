// System Status Actor - 使用Actor模式管理系统状态
//
// 用消息传递替代Arc<RwLock<SystemStatus>>，消除锁竞争

use tokio::sync::{mpsc, oneshot};
use crate::models::SystemStatus;
use chrono::{DateTime, Utc};

/// 系统状态命令
pub enum SystemStatusCommand {
    /// 更新截屏状态
    UpdateCapturing {
        is_capturing: bool,
    },

    /// 更新处理状态
    UpdateProcessing {
        is_processing: bool,
    },

    /// 更新最后截屏时间
    UpdateLastCaptureTime {
        time: DateTime<Utc>,
    },

    /// 更新最后处理时间
    UpdateLastProcessTime {
        time: DateTime<Utc>,
    },

    /// 更新当前会话帧数
    UpdateSessionFrames {
        count: usize,
    },

    /// 设置错误信息
    SetError {
        error: Option<String>,
    },

    /// 更新系统资源占用
    UpdateSystemResources {
        cpu_usage: f32,
        memory_usage: f32,
    },

    /// 获取状态
    Get {
        reply: oneshot::Sender<SystemStatus>,
    },

    /// 健康检查（Ping）
    HealthCheck {
        reply: oneshot::Sender<()>,
    },
}

/// 系统状态Actor
pub struct SystemStatusActor {
    receiver: mpsc::Receiver<SystemStatusCommand>,
    status: SystemStatus,  // 无需RwLock
}

impl SystemStatusActor {
    /// 创建新的Actor
    pub fn new() -> (Self, SystemStatusHandle) {
        let (sender, receiver) = mpsc::channel(50);
        let actor = Self {
            receiver,
            status: SystemStatus::default(),
        };
        let handle = SystemStatusHandle { sender };
        (actor, handle)
    }

    /// 运行Actor
    pub async fn run(mut self) {
        tracing::info!("System Status Actor 已启动");

        while let Some(cmd) = self.receiver.recv().await {
            match cmd {
                SystemStatusCommand::UpdateCapturing { is_capturing } => {
                    self.status.is_capturing = is_capturing;
                    if is_capturing {
                        self.status.last_capture_time = Some(crate::storage::local_now());
                    }
                }

                SystemStatusCommand::UpdateProcessing { is_processing } => {
                    self.status.is_processing = is_processing;
                    if is_processing {
                        self.status.last_process_time = Some(crate::storage::local_now());
                    }
                }

                SystemStatusCommand::UpdateLastCaptureTime { time } => {
                    self.status.last_capture_time = Some(time);
                }

                SystemStatusCommand::UpdateLastProcessTime { time } => {
                    self.status.last_process_time = Some(time);
                }

                SystemStatusCommand::UpdateSessionFrames { count } => {
                    self.status.current_session_frames = count;
                }

                SystemStatusCommand::SetError { error } => {
                    self.status.last_error = error;
                }

                SystemStatusCommand::UpdateSystemResources { cpu_usage, memory_usage } => {
                    self.status.cpu_usage = cpu_usage;
                    self.status.memory_usage = memory_usage;
                }

                SystemStatusCommand::Get { reply } => {
                    let _ = reply.send(self.status.clone());
                }

                SystemStatusCommand::HealthCheck { reply } => {
                    // 立即响应，表明Actor正常运行
                    let _ = reply.send(());
                }
            }
        }

        tracing::info!("System Status Actor 已停止");
    }
}

/// 系统状态Handle
#[derive(Clone)]
pub struct SystemStatusHandle {
    sender: mpsc::Sender<SystemStatusCommand>,
}

impl SystemStatusHandle {
    /// 设置截屏状态
    pub async fn set_capturing(&self, is_capturing: bool) {
        let _ = self.sender.send(SystemStatusCommand::UpdateCapturing { is_capturing }).await;
    }

    /// 设置处理状态
    pub async fn set_processing(&self, is_processing: bool) {
        let _ = self.sender.send(SystemStatusCommand::UpdateProcessing { is_processing }).await;
    }

    /// 更新最后截屏时间
    pub async fn update_last_capture_time(&self, time: DateTime<Utc>) {
        let _ = self.sender.send(SystemStatusCommand::UpdateLastCaptureTime { time }).await;
    }

    /// 更新最后处理时间
    pub async fn update_last_process_time(&self, time: DateTime<Utc>) {
        let _ = self.sender.send(SystemStatusCommand::UpdateLastProcessTime { time }).await;
    }

    /// 更新会话帧数
    pub async fn update_session_frames(&self, count: usize) {
        let _ = self.sender.send(SystemStatusCommand::UpdateSessionFrames { count }).await;
    }

    /// 设置错误信息
    pub async fn set_error(&self, error: Option<String>) {
        let _ = self.sender.send(SystemStatusCommand::SetError { error }).await;
    }

    /// 更新系统资源占用
    pub async fn update_system_resources(&self, cpu_usage: f32, memory_usage: f32) {
        let _ = self.sender.send(SystemStatusCommand::UpdateSystemResources {
            cpu_usage,
            memory_usage
        }).await;
    }

    /// 获取系统状态
    pub async fn get(&self) -> SystemStatus {
        let (reply, rx) = oneshot::channel();
        self.sender.send(SystemStatusCommand::Get { reply }).await.ok();
        rx.await.unwrap_or_default()
    }

    /// 健康检查 - 测试Actor是否响应
    ///
    /// 返回true表示Actor正常运行，false表示Actor无响应或已停止
    /// 超时时间为5秒
    pub async fn health_check(&self) -> bool {
        let (reply, rx) = oneshot::channel();

        // 尝试发送健康检查命令
        if self.sender
            .send(SystemStatusCommand::HealthCheck { reply })
            .await
            .is_err()
        {
            tracing::warn!("System Status Actor 健康检查失败: 通道已关闭");
            return false;
        }

        // 等待响应，超时5秒
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            rx
        ).await {
            Ok(Ok(())) => {
                tracing::debug!("System Status Actor 健康检查成功");
                true
            }
            Ok(Err(_)) => {
                tracing::warn!("System Status Actor 健康检查失败: Actor已停止");
                false
            }
            Err(_) => {
                tracing::warn!("System Status Actor 健康检查失败: 超时(5秒)");
                false
            }
        }
    }
}
