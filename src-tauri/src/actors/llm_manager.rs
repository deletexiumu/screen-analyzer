// LLM Manager Actor - 使用Actor模式管理LLM状态
//
// 用消息传递替代锁机制，消除Arc<Mutex<LLMManager>>的锁竞争

use crate::llm::{LLMConfig, LLMManager, QwenConfig, SessionBrief, SessionSummary};
use anyhow::Result;
use tokio::sync::{mpsc, oneshot};

use crate::llm::{TimelineAnalysis, TimelineCard, VideoSegment};
use crate::storage::Database;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// LLM管理器命令
pub enum LLMCommand {
    /// 配置LLM
    Configure {
        config: QwenConfig,
        reply: oneshot::Sender<Result<()>>,
    },

    /// 分析帧
    AnalyzeFrames {
        frames: Vec<String>,
        reply: oneshot::Sender<Result<SessionSummary>>,
    },

    /// 获取配置
    GetConfig { reply: oneshot::Sender<LLMConfig> },

    /// 设置视频路径
    SetVideoPath { video_path: Option<String> },

    /// 设置视频速率
    SetVideoSpeed { speed_multiplier: f32 },

    /// 设置会话时间范围
    SetSessionWindow {
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    },

    /// 设置provider的数据库连接
    SetProviderDatabase {
        db: Arc<Database>,
        session_id: Option<i64>,
    },

    /// 分析视频并生成时间线（两阶段处理）
    SegmentVideoAndGenerateTimeline {
        frames: Vec<String>,
        duration: u32,
        previous_cards: Option<Vec<TimelineCard>>,
        reply: oneshot::Sender<Result<TimelineAnalysis>>,
    },

    /// 获取最后一次LLM调用的ID
    GetLastCallId {
        call_type: String,
        reply: oneshot::Sender<Option<i64>>,
    },

    /// 生成时间线卡片
    GenerateTimeline {
        segments: Vec<VideoSegment>,
        previous_cards: Option<Vec<TimelineCard>>,
        reply: oneshot::Sender<Result<Vec<TimelineCard>>>,
    },

    /// 生成每日总结
    GenerateDaySummary {
        date: String,
        sessions: Vec<SessionBrief>,
        reply: oneshot::Sender<Result<String>>,
    },

    /// 切换 LLM provider
    SwitchProvider {
        provider: String,
        reply: oneshot::Sender<Result<()>>,
    },

    /// 配置 Claude provider
    ConfigureClaude {
        config: serde_json::Value,
        reply: oneshot::Sender<Result<()>>,
    },

    /// 健康检查（Ping）
    HealthCheck { reply: oneshot::Sender<()> },
}

/// LLM Manager Actor（无需外层Mutex）
pub struct LLMManagerActor {
    receiver: mpsc::Receiver<LLMCommand>,
    manager: LLMManager, // 直接持有，无需锁
}

impl LLMManagerActor {
    /// 创建新的Actor
    pub fn new(manager: LLMManager) -> (Self, LLMHandle) {
        let (sender, receiver) = mpsc::channel(200); // 增加容量到200以支持高负载
        let actor = Self { receiver, manager };
        let handle = LLMHandle { sender };
        (actor, handle)
    }

    /// 运行Actor（在单独的任务中运行）
    pub async fn run(mut self) {
        tracing::info!("LLM Manager Actor 已启动");

        while let Some(cmd) = self.receiver.recv().await {
            match cmd {
                LLMCommand::Configure { config, reply } => {
                    let result = self.manager.configure(config).await;
                    let _ = reply.send(result);
                }

                LLMCommand::AnalyzeFrames { frames, reply } => {
                    let result = self.manager.analyze_frames(frames).await;
                    let _ = reply.send(result);
                }

                LLMCommand::GetConfig { reply } => {
                    let config = self.manager.get_config().await;
                    let _ = reply.send(config);
                }

                LLMCommand::SetVideoPath { video_path } => {
                    self.manager.set_video_path(video_path);
                }

                LLMCommand::SetVideoSpeed { speed_multiplier } => {
                    self.manager.set_video_speed(speed_multiplier);
                }

                LLMCommand::SetSessionWindow { start, end } => {
                    self.manager.set_session_window(start, end);
                }

                LLMCommand::SetProviderDatabase { db, session_id } => {
                    self.manager.set_provider_database(db, session_id);
                }

                LLMCommand::SegmentVideoAndGenerateTimeline {
                    frames,
                    duration,
                    previous_cards,
                    reply,
                } => {
                    let result = self
                        .manager
                        .segment_video_and_generate_timeline(frames, duration, previous_cards)
                        .await;
                    let _ = reply.send(result);
                }

                LLMCommand::GetLastCallId { call_type, reply } => {
                    let id = self.manager.get_last_call_id(&call_type);
                    let _ = reply.send(id);
                }

                LLMCommand::GenerateTimeline {
                    segments,
                    previous_cards,
                    reply,
                } => {
                    let result = self
                        .manager
                        .generate_timeline(segments, previous_cards)
                        .await;
                    let _ = reply.send(result);
                }

                LLMCommand::GenerateDaySummary {
                    date,
                    sessions,
                    reply,
                } => {
                    let result = self.manager.generate_day_summary(&date, &sessions).await;
                    let _ = reply.send(result);
                }

                LLMCommand::SwitchProvider { provider, reply } => {
                    let result = self.manager.switch_provider(&provider).await;
                    let _ = reply.send(result);
                }

                LLMCommand::ConfigureClaude { config, reply } => {
                    let result = self.manager.configure_claude(config).await;
                    let _ = reply.send(result);
                }

                LLMCommand::HealthCheck { reply } => {
                    // 立即响应，表明Actor正常运行
                    let _ = reply.send(());
                }
            }
        }

        tracing::info!("LLM Manager Actor 已停止");
    }
}

/// LLM Handle（用于与Actor通信，可克隆）
#[derive(Clone)]
pub struct LLMHandle {
    sender: mpsc::Sender<LLMCommand>,
}

impl LLMHandle {
    /// 配置LLM
    pub async fn configure(&self, config: QwenConfig) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(LLMCommand::Configure { config, reply })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Actor已停止"))?
    }

    /// 分析帧
    pub async fn analyze_frames(&self, frames: Vec<String>) -> Result<SessionSummary> {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(LLMCommand::AnalyzeFrames { frames, reply })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Actor已停止"))?
    }

    /// 获取配置
    pub async fn get_config(&self) -> Result<LLMConfig> {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(LLMCommand::GetConfig { reply })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        Ok(rx.await.map_err(|_| anyhow::anyhow!("Actor已停止"))?)
    }

    /// 设置视频路径
    pub async fn set_video_path(&self, video_path: Option<String>) -> Result<()> {
        self.sender
            .send(LLMCommand::SetVideoPath { video_path })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        Ok(())
    }

    /// 设置视频速率
    pub async fn set_video_speed(&self, speed_multiplier: f32) -> Result<()> {
        self.sender
            .send(LLMCommand::SetVideoSpeed { speed_multiplier })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        Ok(())
    }

    /// 设置会话时间范围
    pub async fn set_session_window(
        &self,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<()> {
        self.sender
            .send(LLMCommand::SetSessionWindow { start, end })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        Ok(())
    }

    /// 设置provider的数据库连接
    pub async fn set_provider_database(
        &self,
        db: Arc<Database>,
        session_id: Option<i64>,
    ) -> Result<()> {
        self.sender
            .send(LLMCommand::SetProviderDatabase { db, session_id })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        Ok(())
    }

    /// 分析视频并生成时间线（两阶段处理）
    pub async fn segment_video_and_generate_timeline(
        &self,
        frames: Vec<String>,
        duration: u32,
        previous_cards: Option<Vec<TimelineCard>>,
    ) -> Result<TimelineAnalysis> {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(LLMCommand::SegmentVideoAndGenerateTimeline {
                frames,
                duration,
                previous_cards,
                reply,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Actor已停止"))?
    }

    /// 获取最后一次LLM调用的ID
    pub async fn get_last_call_id(&self, call_type: &str) -> Option<i64> {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(LLMCommand::GetLastCallId {
                call_type: call_type.to_string(),
                reply,
            })
            .await
            .ok()?;
        rx.await.ok().flatten()
    }

    /// 生成时间线卡片
    pub async fn generate_timeline(
        &self,
        segments: Vec<VideoSegment>,
        previous_cards: Option<Vec<TimelineCard>>,
    ) -> Result<Vec<TimelineCard>> {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(LLMCommand::GenerateTimeline {
                segments,
                previous_cards,
                reply,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Actor已停止"))?
    }

    /// 健康检查 - 测试Actor是否响应
    ///
    /// 生成每日总结
    pub async fn generate_day_summary(
        &self,
        date: &str,
        sessions: &[SessionBrief],
    ) -> Result<String> {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(LLMCommand::GenerateDaySummary {
                date: date.to_string(),
                sessions: sessions.to_vec(),
                reply,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Actor已停止"))?
    }

    /// 切换 LLM provider
    pub async fn switch_provider(&self, provider: &str) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(LLMCommand::SwitchProvider {
                provider: provider.to_string(),
                reply,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Actor已停止"))?
    }

    /// 配置 Claude provider
    pub async fn configure_claude(&self, config: serde_json::Value) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(LLMCommand::ConfigureClaude { config, reply })
            .await
            .map_err(|_| anyhow::anyhow!("Actor通道已关闭"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Actor已停止"))?
    }

    /// 健康检查
    /// 返回true表示Actor正常运行，false表示Actor无响应或已停止
    /// 超时时间为5秒

    pub async fn health_check(&self) -> bool {
        let (reply, rx) = oneshot::channel();

        // 尝试发送健康检查命令
        if self
            .sender
            .send(LLMCommand::HealthCheck { reply })
            .await
            .is_err()
        {
            tracing::warn!("LLM Manager Actor 健康检查失败: 通道已关闭");
            return false;
        }

        // 等待响应，超时5秒
        match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
            Ok(Ok(())) => {
                tracing::debug!("LLM Manager Actor 健康检查成功");
                true
            }
            Ok(Err(_)) => {
                tracing::warn!("LLM Manager Actor 健康检查失败: Actor已停止");
                false
            }
            Err(_) => {
                tracing::warn!("LLM Manager Actor 健康检查失败: 超时(5秒)");
                false
            }
        }
    }
}
