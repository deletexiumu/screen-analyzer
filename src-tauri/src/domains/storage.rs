// 存储领域管理器
//
// 负责数据库、存储清理和设置管理相关的功能
// 包含 Database、StorageCleaner 和 SettingsManager 三个核心组件

use crate::notion::NotionManager;
use crate::settings::SettingsManager;
use crate::storage::cleaner::StorageCleaner;
use crate::storage::database::Database;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 数据库初始化状态
#[derive(Clone, serde::Serialize)]
pub enum DatabaseStatus {
    /// 初始化中
    Initializing,
    /// 已就绪
    Ready,
    /// 初始化失败
    Failed(String),
}

/// 存储领域管理器 - 负责数据库、存储清理和设置
#[derive(Clone)]
pub struct StorageDomain {
    /// 数据库实例（异步初始化）
    db: Arc<RwLock<Option<Arc<Database>>>>,
    /// 数据库初始化状态
    db_status: Arc<RwLock<DatabaseStatus>>,
    /// 存储清理器（需要等数据库就绪）
    cleaner: Arc<RwLock<Option<Arc<StorageCleaner>>>>,
    /// 设置管理器
    settings: Arc<SettingsManager>,
    /// Notion 同步管理器
    notion_manager: Arc<NotionManager>,
}

impl StorageDomain {
    /// 创建新的存储领域管理器（数据库未初始化）
    pub fn new_pending(settings: Arc<SettingsManager>) -> Self {
        Self {
            db: Arc::new(RwLock::new(None)),
            db_status: Arc::new(RwLock::new(DatabaseStatus::Initializing)),
            cleaner: Arc::new(RwLock::new(None)),
            settings,
            notion_manager: Arc::new(NotionManager::new()),
        }
    }

    /// 设置数据库实例（异步初始化完成后调用）
    pub async fn set_database(&self, db: Arc<Database>) {
        let mut db_lock = self.db.write().await;
        *db_lock = Some(db);

        let mut status_lock = self.db_status.write().await;
        *status_lock = DatabaseStatus::Ready;
    }

    /// 设置数据库初始化失败
    pub async fn set_database_error(&self, error: String) {
        let mut status_lock = self.db_status.write().await;
        *status_lock = DatabaseStatus::Failed(error);
    }

    /// 设置存储清理器（数据库就绪后调用）
    pub async fn set_cleaner(&self, cleaner: Arc<StorageCleaner>) {
        let mut cleaner_lock = self.cleaner.write().await;
        *cleaner_lock = Some(cleaner);
    }

    /// 获取数据库（如果未就绪返回 None）
    pub async fn try_get_db(&self) -> Option<Arc<Database>> {
        let db_lock = self.db.read().await;
        db_lock.clone()
    }

    /// 获取数据库（等待直到就绪或超时）
    ///
    /// 最多等待 30 秒，避免永久阻塞
    pub async fn get_db(&self) -> Result<Arc<Database>, String> {
        use tokio::time::{timeout, Duration};

        // 快速路径：如果已就绪，直接返回
        {
            let db_lock = self.db.read().await;
            if let Some(db) = db_lock.as_ref() {
                return Ok(db.clone());
            }
        }

        // 等待数据库就绪（最多 30 秒）
        let wait_result = timeout(Duration::from_secs(30), async {
            loop {
                {
                    let db_lock = self.db.read().await;
                    if let Some(db) = db_lock.as_ref() {
                        return Ok(db.clone());
                    }
                }

                // 检查是否失败
                {
                    let status = self.db_status.read().await;
                    if let DatabaseStatus::Failed(err) = &*status {
                        return Err(format!("数据库初始化失败: {}", err));
                    }
                }

                // 短暂等待后重试
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;

        match wait_result {
            Ok(result) => result,
            Err(_) => Err("数据库初始化超时（30秒）".to_string()),
        }
    }

    /// 获取数据库状态
    pub async fn get_db_status(&self) -> DatabaseStatus {
        let status = self.db_status.read().await;
        status.clone()
    }

    /// 检查数据库是否就绪
    pub async fn is_db_ready(&self) -> bool {
        let db_lock = self.db.read().await;
        db_lock.is_some()
    }

    /// 获取存储清理器（等待直到就绪或超时）
    pub async fn get_cleaner(&self) -> Result<Arc<StorageCleaner>, String> {
        use tokio::time::{timeout, Duration};

        // 快速路径：如果已就绪，直接返回
        {
            let cleaner_lock = self.cleaner.read().await;
            if let Some(cleaner) = cleaner_lock.as_ref() {
                return Ok(cleaner.clone());
            }
        }

        // 等待清理器就绪（最多 30 秒）
        let wait_result = timeout(Duration::from_secs(30), async {
            loop {
                {
                    let cleaner_lock = self.cleaner.read().await;
                    if let Some(cleaner) = cleaner_lock.as_ref() {
                        return Ok(cleaner.clone());
                    }
                }

                // 短暂等待后重试
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;

        match wait_result {
            Ok(result) => result,
            Err(_) => Err("存储清理器初始化超时（30秒）".to_string()),
        }
    }

    /// 获取设置管理器
    pub fn get_settings(&self) -> &Arc<SettingsManager> {
        &self.settings
    }

    /// 获取 Notion 管理器
    pub fn get_notion_manager(&self) -> &Arc<NotionManager> {
        &self.notion_manager
    }
}
