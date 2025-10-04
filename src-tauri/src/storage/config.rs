// 存储配置定义

use serde::{Deserialize, Serialize};

/// 数据库配置类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DatabaseConfig {
    /// SQLite 配置
    #[serde(rename = "sqlite")]
    SQLite {
        /// 数据库文件路径
        db_path: String,
    },
    /// MariaDB 配置
    #[serde(rename = "mariadb")]
    MariaDB {
        /// 主机地址
        host: String,
        /// 端口
        port: u16,
        /// 数据库名
        database: String,
        /// 用户名
        username: String,
        /// 密码
        password: String,
    },
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        DatabaseConfig::SQLite {
            db_path: "data/screen-analyzer.db".to_string(),
        }
    }
}

/// 存储配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// 数据保留天数
    pub retention_days: i64,
    /// 最大保留天数
    pub max_retention_days: i64,
    /// 是否启用自动清理
    pub auto_cleanup_enabled: bool,
    /// 清理检查间隔（小时）
    pub cleanup_interval_hours: u64,
    /// 数据库配置
    #[serde(default)]
    pub database: DatabaseConfig,
}

/// 获取当前设备信息
pub fn get_device_info() -> (String, String) {
    let device_name = whoami::devicename();
    let device_type = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };
    (device_name, device_type.to_string())
}
