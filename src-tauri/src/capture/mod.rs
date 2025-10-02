// 截屏模块 - 负责定时捕获屏幕截图

use anyhow::Result;
use chrono::{DateTime, Utc};
use image::DynamicImage;
#[cfg(not(target_os = "macos"))]
use image::imageops;
#[cfg(not(target_os = "macos"))]
use screenshots::display_info::DisplayInfo;
use screenshots::Screen;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, trace, warn};
use crate::models::CaptureSettings;

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(not(target_os = "macos"))]
use tracing::debug;

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
    /// 截屏配置
    capture_settings: Arc<Mutex<CaptureSettings>>,
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
                index, display_info.width, display_info.height, display_info.x, display_info.y
            );
        }

        Ok(Self {
            screens,
            output_dir,
            current_session: Arc::new(Mutex::new(Vec::new())),
            capture_settings: Arc::new(Mutex::new(CaptureSettings::default())),
        })
    }

    /// 更新截屏配置
    pub async fn update_settings(&self, settings: CaptureSettings) {
        let mut current = self.capture_settings.lock().await;
        *current = settings;
        info!("截屏配置已更新: {:?}", *current);
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

    /// 检测图像是否为黑屏
    async fn is_black_screen(&self, img: &DynamicImage) -> bool {
        let settings = self.capture_settings.lock().await;

        if !settings.detect_black_screen {
            return false;
        }

        // 获取图像尺寸
        let (width, height) = (img.width(), img.height());

        // 采样检测：使用更大的步长提高性能，对于高分辨率图像足以准确检测
        let sample_step = 100;
        let mut total_brightness = 0u64;
        let mut sample_count = 0u64;

        // 转换为RGB8格式进行处理
        let rgb_img = img.to_rgb8();

        for y in (0..height).step_by(sample_step) {
            for x in (0..width).step_by(sample_step) {
                let pixel = rgb_img.get_pixel(x, y);
                // 计算亮度（简单平均）
                let brightness = (pixel[0] as u64 + pixel[1] as u64 + pixel[2] as u64) / 3;
                total_brightness += brightness;
                sample_count += 1;
            }
        }

        if sample_count == 0 {
            return true; // 无效图像视为黑屏
        }

        let avg_brightness = total_brightness / sample_count;

        // 使用配置的阈值判断是否为黑屏
        let is_black = avg_brightness < settings.black_screen_threshold as u64;

        if is_black {
            info!("检测到黑屏图像（平均亮度: {}, 阈值: {}）", avg_brightness, settings.black_screen_threshold);
        }

        is_black
    }

    /// 捕获单个帧
    pub async fn capture_frame(&self) -> Result<ScreenFrame> {
        let timestamp = Utc::now();

        if self.screens.is_empty() {
            return Err(anyhow::anyhow!("未找到可用屏幕"));
        }

        // 在 macOS 上使用系统原生截图命令，避免截取到应用窗口
        #[cfg(target_os = "macos")]
        let combined = {
            use std::process::Command;
            use tempfile::Builder;

            // 创建临时文件用于保存截图（使用 persist 避免自动删除）
            let temp_file = Builder::new()
                .prefix("screenshot_")
                .suffix(".png")
                .tempfile()
                .map_err(|e| anyhow::anyhow!("创建临时文件失败: {}", e))?;

            let (file, temp_path) = temp_file.keep()
                .map_err(|e| anyhow::anyhow!("保留临时文件失败: {}", e.error))?;

            drop(file); // 关闭文件句柄，让 screencapture 能写入

            // 使用 screencapture 命令截取主屏幕
            // -x: 不播放快门声音
            // -C: 不包含光标
            // -t png: 输出格式为 PNG（保证质量）
            let output = Command::new("screencapture")
                .arg("-x")
                .arg("-C")
                .arg("-t")
                .arg("png")
                .arg(&temp_path)
                .output()
                .map_err(|e| anyhow::anyhow!("执行 screencapture 命令失败: {}", e))?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                // 清理临时文件
                let _ = std::fs::remove_file(&temp_path);
                return Err(anyhow::anyhow!("screencapture 命令执行失败: {}", error));
            }

            // 读取截图文件
            let img = image::open(&temp_path)
                .map_err(|e| {
                    // 清理临时文件
                    let _ = std::fs::remove_file(&temp_path);
                    anyhow::anyhow!("读取截图文件失败: {}", e)
                })?;

            // 读取成功后删除临时文件
            if let Err(e) = std::fs::remove_file(&temp_path) {
                warn!("删除临时截图文件失败: {}", e);
            }

            trace!("使用 screencapture 命令截屏成功");
            img
        };

        // 在其他平台使用 screenshots crate
        #[cfg(not(target_os = "macos"))]
        let combined = {
            let mut captures = Vec::new();

            for (index, screen) in self.screens.iter().enumerate() {
                match screen.capture() {
                    Ok(image) => {
                        let info = screen.display_info;
                        captures.push((info, DynamicImage::ImageRgba8(image)));
                        trace!("截取屏幕 #{} 成功", index);
                    }
                    Err(err) => {
                        warn!("截取屏幕 #{} 失败: {}", index, err);
                    }
                }
            }

            if captures.is_empty() {
                return Err(anyhow::anyhow!("未能获取到任何屏幕截图"));
            }

            self.combine_screens(captures)?
        };

        // 根据配置调整分辨率
        let settings = self.capture_settings.lock().await.clone();
        let resized = if let Some((width, height)) = settings.resolution.dimensions() {
            self.resize_image(combined, width, height)?
        } else {
            // 原始分辨率，不调整
            combined
        };

        // 检测是否为黑屏
        if self.is_black_screen(&resized).await {
            info!("检测到黑屏，跳过保存");
            return Err(anyhow::anyhow!("黑屏图像，已跳过"));
        }

        // 生成文件名
        let file_name = format!("{}.jpg", timestamp.timestamp_millis());
        let file_path = self.output_dir.join(&file_name);

        // 保存为JPEG格式，使用配置的质量
        // 使用 JpegEncoder 来指定质量参数
        use std::fs::File;
        use std::io::BufWriter;
        use image::codecs::jpeg::JpegEncoder;

        let output_file = File::create(&file_path)
            .map_err(|e| anyhow::anyhow!("创建文件失败: {}", e))?;
        let writer = BufWriter::new(output_file);
        let mut encoder = JpegEncoder::new_with_quality(writer, settings.image_quality);

        encoder.encode(
            resized.as_bytes(),
            resized.width(),
            resized.height(),
            resized.color(),
        )?;

        let frame = ScreenFrame {
            timestamp,
            file_path: file_path.to_string_lossy().to_string(),
            screen_id: 0,
        };

        // 添加到当前会话
        self.current_session.lock().await.push(frame.clone());

        trace!("截屏保存成功: {}", frame.file_path);
        Ok(frame)
    }

    #[cfg(not(target_os = "macos"))]
    fn combine_screens(&self, captures: Vec<(DisplayInfo, DynamicImage)>) -> Result<DynamicImage> {
        if captures.is_empty() {
            return Err(anyhow::anyhow!("没有可合成的屏幕图像"));
        }

        struct Region {
            x: i64,
            y: i64,
            width: u32,
            height: u32,
            image: DynamicImage,
        }

        let mut regions: Vec<Region> = Vec::with_capacity(captures.len());

        for (info, image) in captures {
            let (img_w, img_h) = (image.width(), image.height());

            let scale_x = if info.width > 0 {
                img_w as f32 / info.width as f32
            } else {
                info.scale_factor.max(1.0)
            };
            let scale_y = if info.height > 0 {
                img_h as f32 / info.height as f32
            } else {
                info.scale_factor.max(1.0)
            };

            let mut scale = if scale_x.is_finite() && scale_x > 0.0 {
                scale_x
            } else if scale_y.is_finite() && scale_y > 0.0 {
                scale_y
            } else {
                info.scale_factor.max(1.0)
            };

            if !scale.is_finite() || scale <= 0.0 {
                scale = 1.0;
            }

            let pixel_x = ((info.x as f32) * scale).round() as i64;
            let pixel_y = ((info.y as f32) * scale).round() as i64;

            regions.push(Region {
                x: pixel_x,
                y: pixel_y,
                width: img_w,
                height: img_h,
                image,
            });
        }

        let min_x = regions.iter().map(|region| region.x).min().unwrap_or(0);
        let min_y = regions.iter().map(|region| region.y).min().unwrap_or(0);
        let max_x = regions
            .iter()
            .map(|region| region.x + region.width as i64)
            .max()
            .unwrap_or(min_x);
        let max_y = regions
            .iter()
            .map(|region| region.y + region.height as i64)
            .max()
            .unwrap_or(min_y);

        let canvas_width = (max_x - min_x).max(0) as u32;
        let canvas_height = (max_y - min_y).max(0) as u32;

        if canvas_width == 0 || canvas_height == 0 {
            return Err(anyhow::anyhow!("屏幕尺寸无效"));
        }

        let mut canvas = DynamicImage::new_rgba8(canvas_width, canvas_height);

        for region in regions {
            let offset_x = (region.x - min_x) as i64;
            let offset_y = (region.y - min_y) as i64;
            imageops::overlay(&mut canvas, &region.image, offset_x, offset_y);
        }

        Ok(canvas)
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
