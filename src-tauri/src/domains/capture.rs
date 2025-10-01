// 捕获领域管理器
//
// 负责屏幕截取和调度相关的功能
// 包含 ScreenCapture 和 CaptureScheduler 两个核心组件

use std::sync::Arc;
use crate::capture::{ScreenCapture, scheduler::CaptureScheduler};

/// 捕获领域管理器 - 负责屏幕截取和调度
#[derive(Clone)]
pub struct CaptureDomain {
    capture: Arc<ScreenCapture>,
    scheduler: Arc<CaptureScheduler>,
}

impl CaptureDomain {
    /// 创建新的捕获领域管理器
    pub fn new(capture: Arc<ScreenCapture>, scheduler: Arc<CaptureScheduler>) -> Self {
        Self { capture, scheduler }
    }

    /// 获取截屏管理器
    pub fn get_capture(&self) -> &Arc<ScreenCapture> {
        &self.capture
    }

    /// 获取调度器
    pub fn get_scheduler(&self) -> &Arc<CaptureScheduler> {
        &self.scheduler
    }
}
