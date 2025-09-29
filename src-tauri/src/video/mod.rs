// 视频处理模块 - 负责将截图序列生成视频

pub mod ffmpeg_helper;
pub mod processor;

pub use processor::{VideoConfig, VideoFormat, VideoProcessor};

use anyhow::Result;
use std::path::{Path, PathBuf};

/// 视频生成任务
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VideoTask {
    /// 任务ID
    pub id: String,
    /// 输入帧文件路径列表
    pub frame_paths: Vec<String>,
    /// 输出视频路径
    pub output_path: String,
    /// 视频配置
    pub config: VideoConfig,
    /// 任务状态
    pub status: VideoTaskStatus,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 完成时间
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 错误信息
    pub error: Option<String>,
}

/// 视频任务状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoTaskStatus {
    Pending,    // 待处理
    Processing, // 处理中
    Completed,  // 已完成
    Failed,     // 失败
    Cancelled,  // 已取消
}

/// 视频生成结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VideoResult {
    /// 输出文件路径
    pub file_path: String,
    /// 视频时长（秒）
    pub duration: f32,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 视频分辨率
    pub resolution: (u32, u32),
    /// 帧率
    pub fps: f32,
    /// 生成耗时（毫秒）
    pub processing_time_ms: u64,
}

/// 视频工具函数
pub struct VideoUtils;

impl VideoUtils {
    /// 验证FFmpeg是否可用
    pub async fn check_ffmpeg() -> Result<bool> {
        use crate::video::ffmpeg_helper;
        Ok(ffmpeg_helper::check_ffmpeg_available().await)
    }

    /// 获取视频文件信息
    pub fn get_video_info(video_path: &Path) -> Result<VideoInfo> {
        // 获取FFmpeg路径
        let ffmpeg_path = crate::video::ffmpeg_helper::get_ffmpeg_path()?;
        let ffprobe_path = if ffmpeg_path == std::path::PathBuf::from("ffmpeg") {
            std::path::PathBuf::from("ffprobe")
        } else {
            // 如果使用内置FFmpeg，ffprobe应该在同目录
            ffmpeg_path.with_file_name(if cfg!(target_os = "windows") {
                "ffprobe.exe"
            } else {
                "ffprobe"
            })
        };

        let output = std::process::Command::new(&ffprobe_path)
            .args(&[
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                video_path.to_str().unwrap(),
            ])
            .output()?;

        let json_str = String::from_utf8(output.stdout)?;
        let info: serde_json::Value = serde_json::from_str(&json_str)?;

        // 解析视频流信息
        let streams = info["streams"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("无法解析视频流信息"))?;

        let video_stream = streams
            .iter()
            .find(|s| s["codec_type"] == "video")
            .ok_or_else(|| anyhow::anyhow!("未找到视频流"))?;

        let width = video_stream["width"].as_u64().unwrap_or(0) as u32;
        let height = video_stream["height"].as_u64().unwrap_or(0) as u32;
        let fps = parse_frame_rate(&video_stream["r_frame_rate"].as_str().unwrap_or("0/1"));

        // 解析格式信息
        let format = &info["format"];
        let duration = format["duration"]
            .as_str()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0);
        let file_size = format["size"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        Ok(VideoInfo {
            duration,
            file_size,
            resolution: (width, height),
            fps,
            codec: video_stream["codec_name"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            format: format["format_name"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
        })
    }

    /// 生成缩略图
    pub async fn generate_thumbnail(
        video_path: &Path,
        output_path: &Path,
        time_offset: f32,
    ) -> Result<()> {
        let ffmpeg_path = crate::video::ffmpeg_helper::ensure_ffmpeg_extracted().await?;
        let status = tokio::process::Command::new(&ffmpeg_path)
            .args(&[
                "-ss",
                &time_offset.to_string(),
                "-i",
                video_path.to_str().unwrap(),
                "-vframes",
                "1",
                "-vf",
                "scale=320:-1",
                "-y",
                output_path.to_str().unwrap(),
            ])
            .status()
            .await?;

        if !status.success() {
            return Err(anyhow::anyhow!("缩略图生成失败"));
        }

        Ok(())
    }

    /// 合并多个视频片段
    pub async fn concatenate_videos(video_paths: Vec<PathBuf>, output_path: &Path) -> Result<()> {
        // 创建临时文件列表
        let list_file = output_path.with_extension("txt");
        let list_content: String = video_paths
            .iter()
            .map(|p| format!("file '{}'\n", p.to_string_lossy()))
            .collect();

        tokio::fs::write(&list_file, list_content).await?;

        let ffmpeg_path = crate::video::ffmpeg_helper::ensure_ffmpeg_extracted().await?;
        let status = tokio::process::Command::new(&ffmpeg_path)
            .args(&[
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                list_file.to_str().unwrap(),
                "-c",
                "copy",
                "-y",
                output_path.to_str().unwrap(),
            ])
            .status()
            .await?;

        // 删除临时文件
        tokio::fs::remove_file(list_file).await.ok();

        if !status.success() {
            return Err(anyhow::anyhow!("视频合并失败"));
        }

        Ok(())
    }
}

/// 视频信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VideoInfo {
    pub duration: f32,
    pub file_size: u64,
    pub resolution: (u32, u32),
    pub fps: f32,
    pub codec: String,
    pub format: String,
}

/// 解析帧率字符串（如 "30/1" -> 30.0）
fn parse_frame_rate(rate_str: &str) -> f32 {
    let parts: Vec<&str> = rate_str.split('/').collect();
    if parts.len() == 2 {
        let numerator = parts[0].parse::<f32>().unwrap_or(0.0);
        let denominator = parts[1].parse::<f32>().unwrap_or(1.0);
        if denominator != 0.0 {
            return numerator / denominator;
        }
    }
    0.0
}
