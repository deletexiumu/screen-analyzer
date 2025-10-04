// Capture Settings Actor - 使用Actor模式管理截屏设置
//
// 用消息传递替代Arc<Mutex<CaptureSettings>>，消除锁竞争

use crate::models::CaptureSettings;
use tokio::sync::{mpsc, oneshot};

/// 截屏设置命令
pub enum CaptureSettingsCommand {
    /// 更新设置
    Update { settings: CaptureSettings },

    /// 获取设置
    Get {
        reply: oneshot::Sender<CaptureSettings>,
    },

    /// 健康检查（Ping）
    HealthCheck { reply: oneshot::Sender<()> },
}

/// 截屏设置Actor
pub struct CaptureSettingsActor {
    receiver: mpsc::Receiver<CaptureSettingsCommand>,
    settings: CaptureSettings, // 无需Mutex
}

impl CaptureSettingsActor {
    /// 创建新的Actor
    pub fn new(settings: CaptureSettings) -> (Self, CaptureSettingsHandle) {
        let (sender, receiver) = mpsc::channel(10);
        let actor = Self { receiver, settings };
        let handle = CaptureSettingsHandle { sender };
        (actor, handle)
    }

    /// 运行Actor
    pub async fn run(mut self) {
        tracing::info!("Capture Settings Actor 已启动");

        while let Some(cmd) = self.receiver.recv().await {
            match cmd {
                CaptureSettingsCommand::Update { settings } => {
                    self.settings = settings;
                    tracing::info!("截屏配置已更新: {:?}", self.settings);
                }

                CaptureSettingsCommand::Get { reply } => {
                    let _ = reply.send(self.settings.clone());
                }

                CaptureSettingsCommand::HealthCheck { reply } => {
                    // 立即响应，表明Actor正常运行
                    let _ = reply.send(());
                }
            }
        }

        tracing::info!("Capture Settings Actor 已停止");
    }
}

/// 截屏设置Handle
#[derive(Clone)]
pub struct CaptureSettingsHandle {
    sender: mpsc::Sender<CaptureSettingsCommand>,
}

impl CaptureSettingsHandle {
    /// 更新设置
    pub async fn update(&self, settings: CaptureSettings) {
        let _ = self
            .sender
            .send(CaptureSettingsCommand::Update { settings })
            .await;
    }

    /// 获取设置
    pub async fn get(&self) -> CaptureSettings {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(CaptureSettingsCommand::Get { reply })
            .await
            .ok();
        rx.await.unwrap_or_default()
    }

    /// 健康检查 - 测试Actor是否响应
    ///
    /// 返回true表示Actor正常运行，false表示Actor无响应或已停止
    /// 超时时间为5秒
    pub async fn health_check(&self) -> bool {
        let (reply, rx) = oneshot::channel();

        // 尝试发送健康检查命令
        if self
            .sender
            .send(CaptureSettingsCommand::HealthCheck { reply })
            .await
            .is_err()
        {
            tracing::warn!("Capture Settings Actor 健康检查失败: 通道已关闭");
            return false;
        }

        // 等待响应，超时5秒
        match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
            Ok(Ok(())) => {
                tracing::debug!("Capture Settings Actor 健康检查成功");
                true
            }
            Ok(Err(_)) => {
                tracing::warn!("Capture Settings Actor 健康检查失败: Actor已停止");
                false
            }
            Err(_) => {
                tracing::warn!("Capture Settings Actor 健康检查失败: 超时(5秒)");
                false
            }
        }
    }
}
