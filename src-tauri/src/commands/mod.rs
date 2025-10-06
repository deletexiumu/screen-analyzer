//! Tauri 命令模块
//!
//! 提供前端调用的所有 Tauri 命令接口，按功能分组：
//! - query: 数据查询命令
//! - config: 配置管理命令
//! - control: 系统控制命令
//! - video: 视频处理命令
//! - storage: 存储管理命令
//! - notion: Notion 集成命令
//! - test: 测试工具命令

pub mod config;
pub mod control;
pub mod notion;
pub mod query;
pub mod storage;
pub mod test;
pub mod video;

// 重新导出所有命令
pub use config::*;
pub use control::*;
pub use notion::*;
pub use query::*;
pub use storage::*;
pub use test::*;
pub use video::*;
