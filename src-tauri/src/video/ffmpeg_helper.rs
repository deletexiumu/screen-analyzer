// FFmpeg辅助模块 - 管理内置的FFmpeg可执行文件

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// 获取FFmpeg可执行文件的路径
pub fn get_ffmpeg_path() -> Result<PathBuf> {
    let mut common_paths: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        common_paths.extend(
            [
                "/opt/homebrew/bin/ffmpeg",
                "/usr/local/bin/ffmpeg",
                "/opt/local/bin/ffmpeg",
                "/usr/bin/ffmpeg",
            ]
            .into_iter()
            .map(PathBuf::from),
        );
    }

    #[cfg(target_os = "linux")]
    {
        common_paths.extend(
            [
                "/usr/bin/ffmpeg",
                "/usr/local/bin/ffmpeg",
                "/snap/bin/ffmpeg",
            ]
            .into_iter()
            .map(PathBuf::from),
        );
    }

    #[cfg(target_os = "windows")]
    {
        let direct_candidates = [
            r"C:\ffmpeg\bin\ffmpeg.exe",
            r"C:\Program Files\ffmpeg.exe",
            r"C:\Program Files\ffmpeg\ffmpeg.exe",
            r"C:\Program Files\ffmpeg\bin\ffmpeg.exe",
            r"C:\Program Files\FFmpeg\ffmpeg.exe",
            r"C:\Program Files\FFmpeg\bin\ffmpeg.exe",
            r"C:\Program Files (x86)\FFmpeg\bin\ffmpeg.exe",
            r"C:\Program Files (x86)\ffmpeg\bin\ffmpeg.exe",
        ];

        common_paths.extend(direct_candidates.into_iter().map(PathBuf::from));

        if let Ok(program_files) = std::env::var("PROGRAMFILES") {
            let base = PathBuf::from(&program_files);
            common_paths.push(base.join("ffmpeg.exe"));
            common_paths.push(base.join("ffmpeg").join("ffmpeg.exe"));
            common_paths.push(base.join("ffmpeg").join("bin").join("ffmpeg.exe"));
            common_paths.push(base.join("FFmpeg").join("bin").join("ffmpeg.exe"));
        }

        if let Ok(program_files_x86) = std::env::var("PROGRAMFILES(X86)") {
            let base = PathBuf::from(&program_files_x86);
            common_paths.push(base.join("ffmpeg").join("bin").join("ffmpeg.exe"));
            common_paths.push(base.join("FFmpeg").join("bin").join("ffmpeg.exe"));
        }

        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            let base = PathBuf::from(&local_app);
            common_paths.push(base.join("Programs").join("ffmpeg").join("ffmpeg.exe"));
            common_paths.push(
                base.join("Programs")
                    .join("ffmpeg")
                    .join("bin")
                    .join("ffmpeg.exe"),
            );
        }
    }

    for path in &common_paths {
        if path.exists() {
            let mut command = std::process::Command::new(path);
            command.arg("-version");

            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x08000000;
                command.creation_flags(CREATE_NO_WINDOW);
            }

            if let Ok(output) = command.output() {
                if output.status.success() {
                    info!("使用系统FFmpeg: {:?}", path);
                    return Ok(path.clone());
                }
            }
        }
    }

    let mut command = std::process::Command::new("ffmpeg");
    command.arg("-version");

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

    if let Ok(bundled_path) = get_bundled_ffmpeg_path() {
        if bundled_path.exists() {
            info!("使用内置FFmpeg: {:?}", bundled_path);
            return Ok(bundled_path);
        }
    }

    Err(anyhow!(
        "未找到FFmpeg。请将 ffmpeg.exe 添加到 PATH，或放入 src-tauri/resources/ffmpeg/windows/ 目录，或按官方指引安装 FFmpeg。"
    ))
}

/// 获取内置FFmpeg的路径
fn get_bundled_ffmpeg_path() -> Result<PathBuf> {
    // 获取资源目录
    let resource_dir = get_resource_dir()?;

    // 根据操作系统选择对应的FFmpeg，使用 PathBuf::join 确保跨平台兼容
    let ffmpeg_path = if cfg!(target_os = "windows") {
        resource_dir
            .join("ffmpeg")
            .join("windows")
            .join("ffmpeg.exe")
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
                #[allow(unused_imports)]
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
