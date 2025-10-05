//! 视频分析模块
//!
//! 负责核心的视频分析业务逻辑，包括：
//! - 单个视频的LLM分析
//! - 批量视频处理
//! - 历史会话数据处理

pub mod session_processor;
pub mod video_analyzer;

// 重新导出常用结构体和函数
pub use session_processor::*;
pub use video_analyzer::*;
