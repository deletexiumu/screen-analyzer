// FFmpeg辅助模块 - 管理内置的FFmpeg可执行文件

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// 获取FFmpeg可执行文件的路径
pub fn get_ffmpeg_path() -> Result<PathBuf> {
    // macOS 常见的 ffmpeg 安装路径
    let common_paths = vec![
        "/opt/homebrew/bin/ffmpeg",      // Apple Silicon Homebrew
        "/usr/local/bin/ffmpeg",         // Intel Homebrew
        "/opt/local/bin/ffmpeg",         // MacPorts
        "/usr/bin/ffmpeg",               // 系统自带（少见）
    ];

    // 先尝试常见路径
    for path_str in &common_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            // 验证可执行
            let mut command = std::process::Command::new(&path);
            command.arg("-version");

            // Windows下隐藏控制台窗口
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x08000000;
                command.creation_flags(CREATE_NO_WINDOW);
            }

            if let Ok(output) = command.output() {
                if output.status.success() {
                    info!("使用系统FFmpeg: {:?}", path);
                    return Ok(path);
                }
            }
        }
    }

    // 尝试 PATH 环境变量中的 ffmpeg
    let mut command = std::process::Command::new("ffmpeg");
    command.arg("-version");

    // Windows下隐藏控制台窗口
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    if let Ok(output) = command.output() {
        if output.status.success() {
            info!("使用PATH中的FFmpeg");
            return Ok(PathBuf::from("ffmpeg"));
        }
    }

    // 最后尝试内置的FFmpeg（仅作为备用）
    if let Ok(bundled_path) = get_bundled_ffmpeg_path() {
        if bundled_path.exists() {
            info!("使用内置FFmpeg: {:?}", bundled_path);
            return Ok(bundled_path);
        }
    }

    // 都没找到，返回友好的错误提示
    Err(anyhow!("未找到FFmpeg。请通过 Homebrew 安装: brew install ffmpeg"))
}

/// 获取内置FFmpeg的路径
fn get_bundled_ffmpeg_path() -> Result<PathBuf> {
    // 获取资源目录
    let resource_dir = get_resource_dir()?;

    // 根据操作系统选择对应的FFmpeg，使用 PathBuf::join 确保跨平台兼容
    let ffmpeg_path = if cfg!(target_os = "windows") {
        resource_dir.join("ffmpeg").join("windows").join("ffmpeg.exe")
    } else if cfg!(target_os = "macos") {
        resource_dir.join("ffmpeg").join("macos").join("ffmpeg")
    } else if cfg!(target_os = "linux") {
        resource_dir.join("ffmpeg").join("linux").join("ffmpeg")
    } else {
        return Err(anyhow!("不支持的操作系统"));
    };

    // 在Unix系统上，确保可执行权限（限制为所有者和组，避免过于宽松）
    #[cfg(unix)]
    {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        if ffmpeg_path.exists() {
            let metadata = fs::metadata(&ffmpeg_path)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o750); // 仅所有者和组可执行，其他用户无权限
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
            let mut command = tokio::process::Command::new(&path);
            command.arg("-version");

            // Windows下隐藏控制台窗口
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x08000000;
                command.creation_flags(CREATE_NO_WINDOW);
            }

            let result = command.output().await;

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
