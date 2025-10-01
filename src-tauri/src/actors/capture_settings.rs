// Capture Settings Actor - 使用Actor模式管理截屏设置
//
// 用消息传递替代Arc<Mutex<CaptureSettings>>，消除锁竞争

use tokio::sync::{mpsc, oneshot};
use crate::models::CaptureSettings;

/// 截屏设置命令
pub enum CaptureSettingsCommand {
    /// 更新设置
    Update {
        settings: CaptureSettings,
    },

    /// 获取设置
    Get {
        reply: oneshot::Sender<CaptureSettings>,
    },
}

/// 截屏设置Actor
pub struct CaptureSettingsActor {
    receiver: mpsc::Receiver<CaptureSettingsCommand>,
    settings: CaptureSettings,  // 无需Mutex
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
        let _ = self.sender.send(CaptureSettingsCommand::Update { settings }).await;
    }

    /// 获取设置
    pub async fn get(&self) -> CaptureSettings {
        let (reply, rx) = oneshot::channel();
        self.sender.send(CaptureSettingsCommand::Get { reply }).await.ok();
        rx.await.unwrap_or_default()
    }
}
