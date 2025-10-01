// Actor模块 - 使用Actor模式管理并发状态
//
// 用Actor模式替代Arc<Mutex<T>>，通过消息传递实现并发控制
// 消除锁竞争，避免死锁风险，提升并发性能

pub mod llm_manager;
pub mod system_status;
pub mod capture_settings;

pub use llm_manager::{LLMManagerActor, LLMHandle, LLMCommand};
pub use system_status::{SystemStatusActor, SystemStatusHandle, SystemStatusCommand};
pub use capture_settings::{CaptureSettingsActor, CaptureSettingsHandle, CaptureSettingsCommand};
