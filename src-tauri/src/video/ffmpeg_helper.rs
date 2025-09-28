// FFmpeg辅助模块 - 管理内置的FFmpeg可执行文件

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// 获取FFmpeg可执行文件的路径
pub fn get_ffmpeg_path() -> Result<PathBuf> {
    // 首先尝试使用内置的FFmpeg
    if let Ok(bundled_path) = get_bundled_ffmpeg_path() {
        if bundled_path.exists() {
            info!("使用内置FFmpeg: {:?}", bundled_path);
            return Ok(bundled_path);
        }
    }

    // 如果没有内置的，尝试系统PATH中的FFmpeg
    if let Ok(output) = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
    {
        if output.status.success() {
            info!("使用系统FFmpeg");
            return Ok(PathBuf::from("ffmpeg"));
        }
    }

    // 都没找到
    Err(anyhow!("未找到FFmpeg。请确保FFmpeg已安装或使用内置版本。"))
}

/// 获取内置FFmpeg的路径
fn get_bundled_ffmpeg_path() -> Result<PathBuf> {
    // 获取资源目录
    let resource_dir = get_resource_dir()?;

    // 根据操作系统选择对应的FFmpeg
    let ffmpeg_name = if cfg!(target_os = "windows") {
        "ffmpeg/windows/ffmpeg.exe"
    } else if cfg!(target_os = "macos") {
        "ffmpeg/macos/ffmpeg"
    } else if cfg!(target_os = "linux") {
        "ffmpeg/linux/ffmpeg"
    } else {
        return Err(anyhow!("不支持的操作系统"));
    };

    let ffmpeg_path = resource_dir.join(ffmpeg_name);

    // 在Unix系统上，确保可执行权限
    #[cfg(unix)]
    {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        if ffmpeg_path.exists() {
            let metadata = fs::metadata(&ffmpeg_path)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&ffmpeg_path, permissions)?;
        }
    }

    Ok(ffmpeg_path)
}

/// 获取资源目录路径
fn get_resource_dir() -> Result<PathBuf> {
    // 在开发模式下，资源在 src-tauri/resources
    if cfg!(debug_assertions) {
        let mut path = std::env::current_dir()?;
        // 如果当前目录是src-tauri，直接使用
        if path.file_name() == Some(std::ffi::OsStr::new("src-tauri")) {
            path.push("resources");
        } else {
            // 否则假设在项目根目录
            path.push("src-tauri");
            path.push("resources");
        }

        if path.exists() {
            return Ok(path);
        }
    }

    // 在生产模式下，使用Tauri的资源解析器
    #[cfg(not(debug_assertions))]
    {
        // 获取可执行文件所在目录
        let exe_dir = std::env::current_exe()?
            .parent()
            .ok_or_else(|| anyhow!("无法获取可执行文件目录"))?
            .to_path_buf();

        // Windows: resources在exe同目录
        #[cfg(target_os = "windows")]
        {
            let resource_dir = exe_dir.join("resources");
            if resource_dir.exists() {
                return Ok(resource_dir);
            }
        }

        // macOS: resources在 .app/Contents/Resources
        #[cfg(target_os = "macos")]
        {
            let resource_dir = exe_dir
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("Resources"))
                .unwrap_or_else(|| exe_dir.join("resources"));

            if resource_dir.exists() {
                return Ok(resource_dir);
            }
        }

        // Linux: resources在exe同目录
        #[cfg(target_os = "linux")]
        {
            let resource_dir = exe_dir.join("resources");
            if resource_dir.exists() {
                return Ok(resource_dir);
            }
        }
    }

    Err(anyhow!("未找到资源目录"))
}

/// 检查FFmpeg是否可用
pub async fn check_ffmpeg_available() -> bool {
    match get_ffmpeg_path() {
        Ok(path) => {
            let result = tokio::process::Command::new(&path)
                .arg("-version")
                .output()
                .await;

            match result {
                Ok(output) => {
                    if output.status.success() {
                        debug!("FFmpeg可用: {:?}", path);
                        true
                    } else {
                        warn!("FFmpeg执行失败: {:?}", path);
                        false
                    }
                }
                Err(e) => {
                    warn!("无法执行FFmpeg: {}", e);
                    false
                }
            }
        }
        Err(e) => {
            warn!("FFmpeg不可用: {}", e);
            false
        }
    }
}

/// 提取FFmpeg到临时目录（如果需要）
pub async fn ensure_ffmpeg_extracted() -> Result<PathBuf> {
    let ffmpeg_path = get_ffmpeg_path()?;

    // 如果是系统FFmpeg，直接返回
    if ffmpeg_path == PathBuf::from("ffmpeg") {
        return Ok(ffmpeg_path);
    }

    // 确保文件存在
    if !ffmpeg_path.exists() {
        // 尝试从备用位置提取
        extract_bundled_ffmpeg().await?;
    }

    Ok(ffmpeg_path)
}

/// 从应用包中提取FFmpeg（备用方案）
async fn extract_bundled_ffmpeg() -> Result<()> {
    // 这个函数可以在需要时实现
    // 比如从压缩包中提取FFmpeg到临时目录
    warn!("需要提取内置FFmpeg，但功能尚未实现");
    Err(anyhow!("FFmpeg提取功能尚未实现"))
}