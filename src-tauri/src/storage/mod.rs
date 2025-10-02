// 存储模块 - 统一的数据库抽象层

// 子模块
pub mod cache;
pub mod cleaner;
pub mod config;
pub mod database;
pub mod models;
pub mod repository;

// 重新导出主要类型
pub use cache::CachedRepository;
pub use cleaner::StorageCleaner;
pub use config::{get_device_info, DatabaseConfig, StorageConfig};
pub use database::Database;
pub use models::*;
pub use repository::DatabaseRepository;

// 重新导出具体实现（可选，用于高级用法）
pub use repository::mariadb::MariaDbRepository;
pub use repository::sqlite::SqliteRepository;
