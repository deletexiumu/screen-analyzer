// 系统领域管理器
//
// 负责系统状态、日志和基础设施相关的功能
// 包含 SystemStatus、LogBroadcaster 和 HTTP 客户端三个核心组件

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::models::SystemStatus;
use crate::logger::LogBroadcaster;

/// 系统领域管理器 - 负责系统状态、日志和基础设施
#[derive(Clone)]
pub struct SystemDomain {
    system_status: Arc<RwLock<SystemStatus>>,
    log_broadcaster: Arc<LogBroadcaster>,
    http_client: Arc<reqwest::Client>,
}

impl SystemDomain {
    /// 创建新的系统领域管理器
    pub fn new(
        system_status: Arc<RwLock<SystemStatus>>,
        log_broadcaster: Arc<LogBroadcaster>,
        http_client: Arc<reqwest::Client>,
    ) -> Self {
        Self { system_status, log_broadcaster, http_client }
    }

    /// 获取系统状态
    pub fn get_status(&self) -> &Arc<RwLock<SystemStatus>> {
        &self.system_status
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
