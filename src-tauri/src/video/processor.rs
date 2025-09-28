// 视频处理器 - 负责将截图序列转换为视频

use super::{VideoResult, VideoTask, VideoTaskStatus};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, error, info};

/// 视频处理器
pub struct VideoProcessor {
    /// 输出目录
    pub output_dir: PathBuf,
    /// 临时文件目录
    pub temp_dir: PathBuf,
    /// FFmpeg路径（可选自定义路径）
    pub ffmpeg_path: String,
}

/// 视频配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VideoConfig {
    /// 播放速度倍数
    pub speed_multiplier: f32,
    /// 输出帧率
    pub fps: u32,
    /// 输出分辨率
    pub resolution: (u32, u32),
    /// 视频质量（CRF值，0-51，越小质量越好）
    pub quality: u8,
    /// 编码预设（ultrafast, fast, medium, slow, veryslow）
    pub preset: String,
    /// 视频格式
    pub format: VideoFormat,
    /// 是否添加时间戳水印
    pub add_timestamp: bool,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            speed_multiplier: 20.0, // 20倍速
            fps: 30,
            resolution: (1920, 1080),
            quality: 23,
            preset: "fast".to_string(),
            format: VideoFormat::Mp4,
            add_timestamp: true,
        }
    }
}

/// 视频格式
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoFormat {
    Mp4,
    Webm,
    Avi,
    Mkv,
}

impl VideoFormat {
    pub fn extension(&self) -> &str {
        match self {
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
            Self::Avi => "avi",
            Self::Mkv => "mkv",
        }
    }

    fn codec(&self) -> &str {
        match self {
            Self::Mp4 => "libx264",
            Self::Webm => "libvpx-vp9",
            Self::Avi => "libx264",
            Self::Mkv => "libx264",
        }
    }
}

impl VideoProcessor {
    /// 创建新的视频处理器
    pub fn new(output_dir: PathBuf, temp_dir: PathBuf) -> Result<Self> {
        // 确保目录存在
        std::fs::create_dir_all(&output_dir)?;
        std::fs::create_dir_all(&temp_dir)?;

        Ok(Self {
            output_dir,
            temp_dir,
            ffmpeg_path: "ffmpeg".to_string(),
        })
    }

    /// 设置自定义FFmpeg路径
    pub fn set_ffmpeg_path(&mut self, path: String) {
        self.ffmpeg_path = path;
    }

    /// 创建会话回顾视频
    pub async fn create_summary_video(
        &self,
        frames: Vec<String>,
        output_path: &Path,
        config: &VideoConfig,
    ) -> Result<VideoResult> {
        let start_time = Instant::now();

        info!(
            "开始生成视频: {} 帧, 速度 {}x",
            frames.len(),
            config.speed_multiplier
        );

        // 验证输入
        if frames.is_empty() {
            return Err(anyhow::anyhow!("没有可用的帧"));
        }

        // 生成帧列表文件
        let frame_list_path = self.create_frame_list(&frames).await?;

        // 构建FFmpeg命令
        let mut command = tokio::process::Command::new(&self.ffmpeg_path);

        // 基础参数
        command
            .arg("-f")
            .arg("concat")
            .arg("-safe")
            .arg("0")
            .arg("-i")
            .arg(&frame_list_path);

        // 视频滤镜
        let mut filters = vec![];

        // 缩放
        filters.push(format!(
            "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
            config.resolution.0, config.resolution.1, config.resolution.0, config.resolution.1
        ));

        // 速度调整
        if config.speed_multiplier != 1.0 {
            filters.push(format!("setpts=PTS/{}", config.speed_multiplier));
        }

        // 添加时间戳水印
        if config.add_timestamp {
            filters.push(
                "drawtext=text='%{localtime}':x=10:y=10:fontcolor=white:fontsize=24:box=1:boxcolor=black@0.5".to_string()
            );
        }

        if !filters.is_empty() {
            command.arg("-vf").arg(filters.join(","));
        }

        // 编码参数
        command
            .arg("-c:v")
            .arg(config.format.codec())
            .arg("-crf")
            .arg(config.quality.to_string())
            .arg("-preset")
            .arg(&config.preset)
            .arg("-r")
            .arg(config.fps.to_string())
            .arg("-pix_fmt")
            .arg("yuv420p") // 兼容性
            .arg("-movflags")
            .arg("+faststart") // 优化流媒体播放
            .arg("-y") // 覆盖输出文件
            .arg(output_path);

        debug!("FFmpeg命令: {:?}", command);

        // 执行命令
        let output = command.output().await?;

        // 清理临时文件
        tokio::fs::remove_file(frame_list_path).await.ok();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("FFmpeg错误: {}", stderr);
            return Err(anyhow::anyhow!("视频生成失败: {}", stderr));
        }

        // 获取文件信息
        let metadata = tokio::fs::metadata(output_path).await?;
        let file_size = metadata.len();

        let processing_time_ms = start_time.elapsed().as_millis() as u64;

        // 计算视频时长
        let duration = frames.len() as f32 / config.fps as f32 / config.speed_multiplier;

        let result = VideoResult {
            file_path: output_path.to_string_lossy().to_string(),
            duration,
            file_size,
            resolution: config.resolution,
            fps: config.fps as f32,
            processing_time_ms,
        };

        info!("视频生成成功: {:?}", result);
        Ok(result)
    }

    /// 创建帧列表文件
    async fn create_frame_list(&self, frames: &[String]) -> Result<PathBuf> {
        let list_path = self
            .temp_dir
            .join(format!("frames_{}.txt", uuid::Uuid::new_v4()));

        let mut content = String::new();
        for frame in frames {
            content.push_str(&format!("file '{}'\n", frame));
            content.push_str("duration 1\n"); // 每张图片展示1秒
        }

        // 最后一帧需要特殊处理
        if !frames.is_empty() {
            content.push_str(&format!("file '{}'\n", frames.last().unwrap()));
        }

        tokio::fs::write(&list_path, content).await?;
        Ok(list_path)
    }

    /// 生成延时摄影视频（极速版本）
    pub async fn create_timelapse_video(
        &self,
        frames: Vec<String>,
        output_path: &Path,
        target_duration_seconds: f32,
    ) -> Result<VideoResult> {
        // 计算需要的速度倍数
        let total_frames = frames.len() as f32;
        let base_fps = 1.0; // 原始1fps
        let target_fps = 30.0;
        let speed_multiplier = total_frames / (target_duration_seconds * target_fps);

        let config = VideoConfig {
            speed_multiplier,
            fps: target_fps as u32,
            ..Default::default()
        };

        self.create_summary_video(frames, output_path, &config)
            .await
    }

    /// 批量处理视频任务
    pub async fn process_batch(&self, tasks: Vec<VideoTask>) -> Vec<Result<VideoResult>> {
        let mut results = Vec::new();

        for mut task in tasks {
            task.status = VideoTaskStatus::Processing;

            let output_path = Path::new(&task.output_path);
            let result = self
                .create_summary_video(task.frame_paths.clone(), output_path, &task.config)
                .await;

            match result {
                Ok(video_result) => {
                    task.status = VideoTaskStatus::Completed;
                    task.completed_at = Some(chrono::Utc::now());
                    results.push(Ok(video_result));
                }
                Err(e) => {
                    task.status = VideoTaskStatus::Failed;
                    task.error = Some(e.to_string());
                    results.push(Err(e));
                }
            }
        }

        results
    }

    /// 清理临时文件
    pub async fn cleanup_temp_files(&self) -> Result<()> {
        let mut entries = tokio::fs::read_dir(&self.temp_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("txt") {
                // 删除超过1小时的临时文件
                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(modified) = metadata.modified() {
                        let age = std::time::SystemTime::now()
                            .duration_since(modified)
                            .unwrap_or_default();

                        if age.as_secs() > 3600 {
                            tokio::fs::remove_file(path).await.ok();
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

// 添加uuid依赖用于生成唯一文件名
use uuid;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_video_processor_creation() {
        let temp_dir = tempdir().unwrap();
        let output_dir = temp_dir.path().join("output");
        let temp_files_dir = temp_dir.path().join("temp");

        let processor = VideoProcessor::new(output_dir, temp_files_dir);
        assert!(processor.is_ok());
    }

    #[test]
    fn test_video_format_extension() {
        assert_eq!(VideoFormat::Mp4.extension(), "mp4");
        assert_eq!(VideoFormat::Webm.extension(), "webm");
        assert_eq!(VideoFormat::Avi.extension(), "avi");
        assert_eq!(VideoFormat::Mkv.extension(), "mkv");
    }

    #[test]
    fn test_video_config_default() {
        let config = VideoConfig::default();
        assert_eq!(config.speed_multiplier, 20.0);
        assert_eq!(config.fps, 30);
        assert_eq!(config.resolution, (1920, 1080));
        assert_eq!(config.quality, 23);
    }
}
