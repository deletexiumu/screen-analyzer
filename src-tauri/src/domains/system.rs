// 系统领域管理器
//
// 负责系统状态、日志和基础设施相关的功能
// 包含 SystemStatusHandle、LogBroadcaster 和 HTTP 客户端三个核心组件
// 使用Actor模式管理系统状态，消除锁竞争

use crate::actors::SystemStatusHandle;
use crate::logger::LogBroadcaster;
use std::sync::Arc;

/// 系统领域管理器 - 负责系统状态、日志和基础设施
#[derive(Clone)]
pub struct SystemDomain {
    system_status_handle: SystemStatusHandle,
    log_broadcaster: Arc<LogBroadcaster>,
    http_client: Arc<reqwest::Client>,
}

impl SystemDomain {
    /// 创建新的系统领域管理器
    pub fn new(
        system_status_handle: SystemStatusHandle,
        log_broadcaster: Arc<LogBroadcaster>,
        http_client: Arc<reqwest::Client>,
    ) -> Self {
        Self {
            system_status_handle,
            log_broadcaster,
            http_client,
        }
    }

    /// 获取系统状态Handle
    pub fn get_status_handle(&self) -> &SystemStatusHandle {
        &self.system_status_handle
    }

    /// 获取日志广播器
    pub fn get_logger(&self) -> &Arc<LogBroadcaster> {
        &self.log_broadcaster
    }

    /// 获取 HTTP 客户端
    pub fn get_http_client(&self) -> &Arc<reqwest::Client> {
        &self.http_client
    }
}
