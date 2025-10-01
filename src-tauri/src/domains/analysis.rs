// 分析领域管理器
//
// 负责 LLM 分析和视频处理相关的功能
// 包含 LLMHandle 和 VideoProcessor 两个核心组件
// 使用Actor模式管理LLM状态，消除锁竞争

use std::sync::Arc;
use crate::actors::LLMHandle;
use crate::video::processor::VideoProcessor;

/// 分析领域管理器 - 负责 LLM 分析和视频处理
#[derive(Clone)]
pub struct AnalysisDomain {
    llm_handle: LLMHandle,
    video_processor: Arc<VideoProcessor>,
}

impl AnalysisDomain {
    /// 创建新的分析领域管理器
    pub fn new(llm_handle: LLMHandle, video_processor: Arc<VideoProcessor>) -> Self {
        Self { llm_handle, video_processor }
    }

    /// 获取 LLM Handle
    pub fn get_llm_handle(&self) -> &LLMHandle {
        &self.llm_handle
    }

    /// 获取视频处理器
    pub fn get_video_processor(&self) -> &Arc<VideoProcessor> {
        &self.video_processor
    }
}
