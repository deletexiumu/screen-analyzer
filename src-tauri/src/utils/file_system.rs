//! 文件系统操作工具
//!
//! 提供跨平台的文件夹打开、日志目录访问等功能

use std::path::{Path, PathBuf};
use tracing::info;

/// 在系统文件管理器中打开文件夹
///
/// 根据不同操作系统使用对应的命令：
/// - Windows: explorer
/// - macOS: open
/// - Linux: xdg-open
///
/// # 参数
/// - `path`: 要打开的文件夹路径
///
/// # 返回
/// - `Ok(())`: 成功打开
/// - `Err(String)`: 错误信息
pub fn open_folder_in_explorer(path: &Path) -> Result<(), String> {
    // 确保目录存在
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 根据操作系统打开文件夹
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }

    Ok(())
}

/// 获取日志目录路径（跨平台）
///
/// - macOS: ~/Library/Logs/screen-analyzer
/// - Windows: %APPDATA%/screen-analyzer/logs
/// - Linux: ~/.local/share/screen-analyzer/logs
pub fn get_log_dir() -> Result<PathBuf, String> {
    let log_dir = if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("Library/Logs/screen-analyzer")
    } else if cfg!(target_os = "windows") {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata).join("screen-analyzer").join("logs")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".local/share/screen-analyzer/logs")
    };

    Ok(log_dir)
}

/// 打开日志文件夹
pub fn open_log_folder_impl() -> Result<(), String> {
    let log_dir = get_log_dir()?;
    info!("打开日志文件夹: {:?}", log_dir);
    open_folder_in_explorer(&log_dir)
}
