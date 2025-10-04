// Actor模块 - 使用Actor模式管理并发状态
//
// 用Actor模式替代Arc<Mutex<T>>，通过消息传递实现并发控制
// 消除锁竞争，避免死锁风险，提升并发性能

pub mod capture_settings;
pub mod llm_manager;
pub mod system_status;

pub use capture_settings::{CaptureSettingsActor, CaptureSettingsCommand, CaptureSettingsHandle};
pub use llm_manager::{LLMCommand, LLMHandle, LLMManagerActor};
pub use system_status::{SystemStatusActor, SystemStatusCommand, SystemStatusHandle};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CaptureSettings;

    #[tokio::test]
    async fn test_system_status_health_check() {
        // 创建SystemStatusActor
        let (actor, handle) = SystemStatusActor::new();

        // 在后台运行Actor
        tokio::spawn(async move {
            actor.run().await;
        });

        // 执行健康检查
        let is_healthy = handle.health_check().await;
        assert!(is_healthy, "SystemStatusActor应该是健康的");
    }

    #[tokio::test]
    async fn test_capture_settings_health_check() {
        // 创建CaptureSettingsActor
        let settings = CaptureSettings::default();
        let (actor, handle) = CaptureSettingsActor::new(settings);

        // 在后台运行Actor
        tokio::spawn(async move {
            actor.run().await;
        });

        // 执行健康检查
        let is_healthy = handle.health_check().await;
        assert!(is_healthy, "CaptureSettingsActor应该是健康的");
    }

    #[tokio::test]
    async fn test_health_check_timeout() {
        // 创建Actor但不运行，模拟Actor无响应
        let (actor, handle) = SystemStatusActor::new();

        // 不运行Actor，直接drop
        drop(actor);

        // 执行健康检查应该失败
        let is_healthy = handle.health_check().await;
        assert!(!is_healthy, "停止的Actor应该健康检查失败");
    }
}
