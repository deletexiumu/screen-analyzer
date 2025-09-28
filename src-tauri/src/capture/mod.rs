// 截屏模块 - 负责定时捕获屏幕截图

use anyhow::Result;
use chrono::{DateTime, Utc};
use image::{DynamicImage, ImageFormat};
use mouse_position::mouse_position::Mouse;
use screenshots::Screen;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, trace, warn};

pub mod scheduler;

/// 截屏帧数据结构
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScreenFrame {
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 文件路径
    pub file_path: String,
    /// 屏幕ID
    pub screen_id: usize,
}

/// 截屏管理器
pub struct ScreenCapture {
    /// 可用屏幕列表
    screens: Vec<Screen>,
    /// 输出目录
    output_dir: PathBuf,
    /// 当前会话的帧数据
    current_session: Arc<Mutex<Vec<ScreenFrame>>>,
}

impl ScreenCapture {
    /// 创建新的截屏管理器
    pub fn new(output_dir: PathBuf) -> Result<Self> {
        // 确保输出目录存在
        if !output_dir.exists() {
            std::fs::create_dir_all(&output_dir)?;
        }

        let screens = Screen::all()?;
        info!("检测到 {} 个屏幕", screens.len());

        // 打印每个屏幕的详细信息
        for (index, screen) in screens.iter().enumerate() {
            let display_info = screen.display_info;
            info!(
                "屏幕 #{}: {}x{} @ ({}, {})",
                index,
                display_info.width,
                display_info.height,
                display_info.x,
                display_info.y
            );
        }

        Ok(Self {
            screens,
            output_dir,
            current_session: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 检测系统是否处于锁屏状态
    /// 在 macOS 上通过检查屏幕保护程序状态和系统锁定状态
    pub fn is_screen_locked() -> bool {
        #[cfg(target_os = "macos")]
        {
            // 方法1：使用 Quartz 检查 CGSessionCopyCurrentDictionary
            // 当屏幕锁定时，这个命令会返回空
            if let Ok(output) = Command::new("python3")
                .arg("-c")
                .arg("import Quartz; print('locked' if Quartz.CGSessionCopyCurrentDictionary() is None else 'unlocked')")
                .output()
            {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    if text.trim() == "locked" {
                        info!("检测到屏幕锁定（CGSessionCopyCurrentDictionary 为空）");
                        return true;
                    }
                }
            }

            // 方法2：检查屏幕保护程序是否运行
            if let Ok(output) = Command::new("pgrep")
                .arg("-x")
                .arg("ScreenSaverEngine")
                .output()
            {
                if output.status.success() && !output.stdout.is_empty() {
                    info!("检测到屏幕保护程序正在运行");
                    return true;
                }
            }

            // 方法3：检查显示器睡眠状态
            // 使用 ioreg 检查显示器是否处于睡眠状态
            if let Ok(output) = Command::new("sh")
                .arg("-c")
                .arg("ioreg -n IODisplayWrangler | grep -i 'IOPowerManagement' -A 10 | grep 'CurrentPowerState'")
                .output()
            {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    // CurrentPowerState = 4 表示显示器开启
                    // 其他值（0-3）表示显示器关闭或睡眠
                    if text.contains("CurrentPowerState") {
                        // 提取数字
                        if let Some(pos) = text.find("=") {
                            let state_str = text[pos+1..].trim();
                            if let Ok(state) = state_str.parse::<i32>() {
                                if state < 4 {
                                    info!("检测到显示器睡眠或关闭（CurrentPowerState = {}）", state);
                                    return true;
                                }
                            }
                        }
                    }
                }
            }

            // 方法4：检查系统是否处于登录窗口（备用）
            if let Ok(output) = Command::new("sh")
                .arg("-c")
                .arg("w | grep -c console")
                .output()
            {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    // 如果没有用户登录到控制台，可能处于锁屏状态
                    if text.trim() == "0" {
                        trace!("可能处于锁屏状态（无控制台用户）");
                        // 这个方法不太可靠，所以只作为辅助判断
                    }
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            // 其他平台暂不实现锁屏检测，始终返回 false
            // Windows 等平台在锁屏时系统会自动禁止截屏权限，无需额外检测
            use std::sync::Once;
            static WARN_ONCE: Once = Once::new();
            WARN_ONCE.call_once(|| {
                debug!("当前平台不支持锁屏检测，依赖系统权限管理");
            });
        }

        false
    }

    /// 捕获单个帧
    pub async fn capture_frame(&self) -> Result<ScreenFrame> {
        let timestamp = Utc::now();

        // 获取要截图的屏幕
        let (target_screen, screen_id) = self.get_target_screen()?;

        // 截屏
        let image = target_screen.capture()?;
        let (_width, _height) = (image.width(), image.height());

        // 转换为DynamicImage
        let dynamic_image = DynamicImage::ImageRgba8(image);

        // 调整分辨率到1920x1080
        let resized = self.resize_image(dynamic_image, 1920, 1080)?;

        // 生成文件名
        let file_name = format!("{}.jpg", timestamp.timestamp_millis());
        let file_path = self.output_dir.join(&file_name);

        // 保存为JPEG格式
        resized.save_with_format(&file_path, ImageFormat::Jpeg)?;

        let frame = ScreenFrame {
            timestamp,
            file_path: file_path.to_string_lossy().to_string(),
            screen_id,
        };

        // 添加到当前会话
        self.current_session.lock().await.push(frame.clone());

        trace!("截屏保存成功: {} (屏幕 #{})", frame.file_path, screen_id);
        Ok(frame)
    }

    /// 获取要截图的目标屏幕
    /// 优先级：1. 鼠标所在屏幕 2. 主屏幕（第一个屏幕）
    fn get_target_screen(&self) -> Result<(&Screen, usize)> {
        // 如果只有一个屏幕，直接返回
        if self.screens.len() == 1 {
            return self.screens.first()
                .map(|s| (s, 0))
                .ok_or_else(|| anyhow::anyhow!("未找到可用屏幕"));
        }

        // 尝试获取鼠标位置
        let mouse_pos = match Mouse::get_mouse_position() {
            Mouse::Position { x, y } => {
                trace!("鼠标位置: ({}, {})", x, y);
                Some((x, y))
            }
            Mouse::Error => {
                warn!("无法获取鼠标位置，将使用主屏幕");
                None
            }
        };

        // 如果获取到鼠标位置，找到对应屏幕
        if let Some((mouse_x, mouse_y)) = mouse_pos {
            for (index, screen) in self.screens.iter().enumerate() {
                let display_info = screen.display_info;

                // 检查鼠标是否在当前屏幕范围内
                if mouse_x >= display_info.x as i32
                    && mouse_x < (display_info.x + display_info.width as i32)
                    && mouse_y >= display_info.y as i32
                    && mouse_y < (display_info.y + display_info.height as i32) {
                    trace!("鼠标在屏幕 #{} 上 ({}x{} at {},{}))",
                        index, display_info.width, display_info.height, display_info.x, display_info.y);
                    return Ok((screen, index));
                }
            }

            // 鼠标不在任何屏幕范围内（可能在屏幕之间），使用主屏幕
            warn!("鼠标位置 ({}, {}) 不在任何屏幕范围内，使用主屏幕", mouse_x, mouse_y);
        }

        // 默认返回主屏幕（第一个屏幕）
        self.screens.first()
            .map(|s| (s, 0))
            .ok_or_else(|| anyhow::anyhow!("未找到可用屏幕"))
    }

    /// 调整图像尺寸
    fn resize_image(&self, img: DynamicImage, width: u32, height: u32) -> Result<DynamicImage> {
        Ok(img.resize_exact(width, height, image::imageops::FilterType::Lanczos3))
    }

    /// 获取当前会话的所有帧
    pub async fn get_current_session_frames(&self) -> Vec<ScreenFrame> {
        self.current_session.lock().await.clone()
    }

    /// 清空当前会话
    pub async fn clear_current_session(&self) {
        self.current_session.lock().await.clear();
        info!("当前会话已清空");
    }

    /// 清理当前会话中早于指定时间的帧
    pub async fn prune_session_before(&self, cutoff: DateTime<Utc>) {
        let mut session = self.current_session.lock().await;
        session.retain(|frame| frame.timestamp >= cutoff);
    }

    /// 获取帧保存目录
    pub fn frames_dir(&self) -> PathBuf {
        self.output_dir.clone()
    }

    /// 获取最近的帧（用于采样）
    pub async fn get_recent_frames(&self, count: usize) -> Vec<ScreenFrame> {
        let frames = self.current_session.lock().await;
        let len = frames.len();
        if len <= count {
            frames.clone()
        } else {
            frames[len - count..].to_vec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_screen_capture_creation() {
        let temp_dir = tempdir().unwrap();
        let capture = ScreenCapture::new(temp_dir.path().to_path_buf());
        assert!(capture.is_ok());
    }
}
