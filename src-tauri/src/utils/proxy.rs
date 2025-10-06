//! 系统代理配置工具
//!
//! 自动读取系统代理设置并配置环境变量，支持 macOS 和 Windows

use tracing::info;
#[cfg(target_os = "windows")]
use tracing::warn;

/// 配置系统代理（macOS）
///
/// 通过 scutil 命令读取系统代理设置，并设置相应的环境变量
#[cfg(target_os = "macos")]
pub fn setup_system_proxy_macos() {
    use std::process::Command;

    if let Ok(output) = Command::new("scutil").arg("--proxy").output() {
        if output.status.success() {
            if let Ok(proxy_info) = String::from_utf8(output.stdout) {
                // 解析 HTTP 代理
                if let Some(http_enabled) = proxy_info
                    .lines()
                    .find(|l| l.trim().starts_with("HTTPEnable"))
                    .and_then(|l| l.split(':').nth(1))
                    .and_then(|v| v.trim().parse::<i32>().ok())
                {
                    if http_enabled == 1 {
                        let http_host = proxy_info
                            .lines()
                            .find(|l| l.trim().starts_with("HTTPProxy"))
                            .and_then(|l| l.split(':').nth(1))
                            .map(|s| s.trim().to_string());

                        let http_port = proxy_info
                            .lines()
                            .find(|l| l.trim().starts_with("HTTPPort"))
                            .and_then(|l| l.split(':').nth(1))
                            .map(|s| s.trim().to_string());

                        if let (Some(host), Some(port)) = (http_host, http_port) {
                            let proxy_url = format!("http://{}:{}", host, port);
                            std::env::set_var("HTTP_PROXY", &proxy_url);
                            std::env::set_var("http_proxy", &proxy_url);
                            info!("已设置 HTTP 代理: {}", proxy_url);
                        }
                    }
                }

                // 解析 HTTPS 代理
                if let Some(https_enabled) = proxy_info
                    .lines()
                    .find(|l| l.trim().starts_with("HTTPSEnable"))
                    .and_then(|l| l.split(':').nth(1))
                    .and_then(|v| v.trim().parse::<i32>().ok())
                {
                    if https_enabled == 1 {
                        let https_host = proxy_info
                            .lines()
                            .find(|l| l.trim().starts_with("HTTPSProxy"))
                            .and_then(|l| l.split(':').nth(1))
                            .map(|s| s.trim().to_string());

                        let https_port = proxy_info
                            .lines()
                            .find(|l| l.trim().starts_with("HTTPSPort"))
                            .and_then(|l| l.split(':').nth(1))
                            .map(|s| s.trim().to_string());

                        if let (Some(host), Some(port)) = (https_host, https_port) {
                            let proxy_url = format!("http://{}:{}", host, port);
                            std::env::set_var("HTTPS_PROXY", &proxy_url);
                            std::env::set_var("https_proxy", &proxy_url);
                            info!("已设置 HTTPS 代理: {}", proxy_url);
                        }
                    }
                }
            }
        }
    }
}

/// 配置系统代理（Windows）
///
/// 通过读取注册表获取系统代理设置，并设置相应的环境变量
#[cfg(target_os = "windows")]
pub fn setup_system_proxy_windows() {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(internet_settings) =
        hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
    {
        // 检查代理是否启用
        if let Ok(proxy_enable) = internet_settings.get_value::<u32, _>("ProxyEnable") {
            if proxy_enable == 1 {
                // 读取代理服务器设置
                if let Ok(proxy_server) = internet_settings.get_value::<String, _>("ProxyServer") {
                    info!("Windows 代理服务器配置: {}", proxy_server);

                    // 代理服务器格式可能是：
                    // 1. "host:port" (所有协议使用同一代理)
                    // 2. "http=host:port;https=host:port" (不同协议使用不同代理)

                    if proxy_server.contains('=') {
                        // 格式2：解析不同协议的代理
                        for part in proxy_server.split(';') {
                            if let Some((protocol, addr)) = part.split_once('=') {
                                let protocol = protocol.trim().to_lowercase();
                                let addr = addr.trim();

                                match protocol.as_str() {
                                    "http" => {
                                        let proxy_url = if addr.starts_with("http://") {
                                            addr.to_string()
                                        } else {
                                            format!("http://{}", addr)
                                        };
                                        std::env::set_var("HTTP_PROXY", &proxy_url);
                                        std::env::set_var("http_proxy", &proxy_url);
                                        info!("已设置 HTTP 代理: {}", proxy_url);
                                    }
                                    "https" => {
                                        let proxy_url = if addr.starts_with("http://") {
                                            addr.to_string()
                                        } else {
                                            format!("http://{}", addr)
                                        };
                                        std::env::set_var("HTTPS_PROXY", &proxy_url);
                                        std::env::set_var("https_proxy", &proxy_url);
                                        info!("已设置 HTTPS 代理: {}", proxy_url);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    } else {
                        // 格式1：所有协议使用同一代理
                        let proxy_url = if proxy_server.starts_with("http://") {
                            proxy_server.clone()
                        } else {
                            format!("http://{}", proxy_server)
                        };

                        std::env::set_var("HTTP_PROXY", &proxy_url);
                        std::env::set_var("http_proxy", &proxy_url);
                        std::env::set_var("HTTPS_PROXY", &proxy_url);
                        std::env::set_var("https_proxy", &proxy_url);
                        info!("已设置系统代理: {}", proxy_url);
                    }
                }
            } else {
                info!("Windows 系统代理未启用");
            }
        }
    } else {
        warn!("无法读取 Windows 代理设置");
    }
}

/// 配置系统 PATH 环境变量（macOS）
///
/// 确保 Homebrew 和常用路径在 PATH 中，以便找到 claude 等命令
#[cfg(target_os = "macos")]
pub fn setup_path_env_macos() {
    if let Ok(current_path) = std::env::var("PATH") {
        let homebrew_paths = vec![
            "/opt/homebrew/bin",  // Apple Silicon Homebrew
            "/usr/local/bin",     // Intel Mac Homebrew
            "/usr/bin",
            "/bin",
        ];

        let mut path_parts: Vec<String> =
            current_path.split(':').map(|s| s.to_string()).collect();

        // 添加 Homebrew 路径到开头（如果还没有）
        for homebrew_path in homebrew_paths.iter().rev() {
            if !path_parts.contains(&homebrew_path.to_string()) {
                path_parts.insert(0, homebrew_path.to_string());
            }
        }

        let new_path = path_parts.join(":");
        std::env::set_var("PATH", &new_path);
        info!("已设置 PATH 环境变量（包含 Homebrew 路径）");
    }
}
