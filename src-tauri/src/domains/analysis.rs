// 分析领域管理器
//
// 负责 LLM 分析和视频处理相关的功能
// 包含 LLMManager 和 VideoProcessor 两个核心组件

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::llm::LLMManager;
use crate::video::processor::VideoProcessor;

/// 分析领域管理器 - 负责 LLM 分析和视频处理
#[derive(Clone)]
pub struct AnalysisDomain {
    llm_manager: Arc<Mutex<LLMManager>>,
    video_processor: Arc<VideoProcessor>,
}

impl AnalysisDomain {
    /// 创建新的分析领域管理器
    pub fn new(llm_manager: Arc<Mutex<LLMManager>>, video_processor: Arc<VideoProcessor>) -> Self {
        Self { llm_manager, video_processor }
    }

    /// 获取 LLM 管理器
    pub fn get_llm_manager(&self) -> &Arc<Mutex<LLMManager>> {
        &self.llm_manager
    }

    /// 获取视频处理器
    pub fn get_video_processor(&self) -> &Arc<VideoProcessor> {
        &self.video_processor
    }
}
