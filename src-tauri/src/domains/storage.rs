// 存储领域管理器
//
// 负责数据库、存储清理和设置管理相关的功能
// 包含 Database、StorageCleaner 和 SettingsManager 三个核心组件

use std::sync::Arc;
use crate::storage::database::Database;
use crate::storage::cleaner::StorageCleaner;
use crate::settings::SettingsManager;

/// 存储领域管理器 - 负责数据库、存储清理和设置
#[derive(Clone)]
pub struct StorageDomain {
    db: Arc<Database>,
    cleaner: Arc<StorageCleaner>,
    settings: Arc<SettingsManager>,
}

impl StorageDomain {
    /// 创建新的存储领域管理器
    pub fn new(
        db: Arc<Database>,
        cleaner: Arc<StorageCleaner>,
        settings: Arc<SettingsManager>,
    ) -> Self {
        Self { db, cleaner, settings }
    }

    /// 获取数据库
    pub fn get_db(&self) -> &Arc<Database> {
        &self.db
    }

    /// 获取存储清理器
    pub fn get_cleaner(&self) -> &Arc<StorageCleaner> {
        &self.cleaner
    }

    /// 获取设置管理器
    pub fn get_settings(&self) -> &Arc<SettingsManager> {
        &self.settings
    }
}
