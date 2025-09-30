// 自定义日志层 - 支持将日志实时推送到前端

use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Emitter};
use tracing::subscriber::SetGlobalDefaultError;
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::Layer;

/// 日志消息
#[derive(Clone, Debug, serde::Serialize)]
pub struct LogMessage {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// 日志推送器 - 将日志发送到前端
pub struct LogBroadcaster {
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    enabled: Arc<RwLock<bool>>,
}

impl LogBroadcaster {
    pub fn new() -> Self {
        Self {
            app_handle: Arc::new(RwLock::new(None)),
            enabled: Arc::new(RwLock::new(true)),
        }
    }

    /// 设置 Tauri App Handle
    pub fn set_app_handle(&self, handle: AppHandle) {
        if let Ok(mut app) = self.app_handle.write() {
            *app = Some(handle);
        }
    }

    /// 设置日志推送开关
    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut e) = self.enabled.write() {
            *e = enabled;
        }
    }

    /// 获取日志推送状态
    pub fn is_enabled(&self) -> bool {
        self.enabled.read().map(|e| *e).unwrap_or(false)
    }

    /// 发送日志到前端
    fn emit_log(&self, log: LogMessage) {
        // 检查是否启用
        if !self.is_enabled() {
            return;
        }

        // 获取 app handle 并发送事件
        if let Ok(app_guard) = self.app_handle.read() {
            if let Some(app) = app_guard.as_ref() {
                let _ = app.emit("log-message", &log);
            }
        }
    }
}

/// 自定义日志层
pub struct TauriLogLayer {
    broadcaster: Arc<LogBroadcaster>,
}

impl TauriLogLayer {
    pub fn new(broadcaster: Arc<LogBroadcaster>) -> Self {
        Self { broadcaster }
    }
}

impl<S: Subscriber> Layer<S> for TauriLogLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // 提取日志级别
        let metadata = event.metadata();
        let level = metadata.level().to_string();
        let target = metadata.target().to_string();

        // 使用 visitor 提取消息
        struct MessageVisitor {
            message: String,
        }

        impl tracing::field::Visit for MessageVisitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.message = format!("{:?}", value);
                    // 移除首尾引号
                    if self.message.starts_with('"') && self.message.ends_with('"') {
                        self.message = self.message[1..self.message.len() - 1].to_string();
                    }
                }
            }
        }

        let mut visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);

        // 生成时间戳
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

        // 创建日志消息
        let log = LogMessage {
            timestamp,
            level,
            target,
            message: visitor.message,
        };

        // 发送到前端
        self.broadcaster.emit_log(log);
    }
}

/// 初始化日志系统（带 Tauri 推送功能）
pub fn init_with_broadcaster(
    broadcaster: Arc<LogBroadcaster>,
) -> Result<(), SetGlobalDefaultError> {
    use std::path::PathBuf;
    use tracing_subscriber::fmt::time::LocalTime;
    use tracing_subscriber::fmt::writer::MakeWriterExt;

    // 获取日志目录
    let log_dir = if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("Library/Logs/screen-analyzer")
    } else if cfg!(target_os = "windows") {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata).join("screen-analyzer").join("logs")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join(".local/share/screen-analyzer/logs")
    };

    // 创建日志目录
    std::fs::create_dir_all(&log_dir).ok();

    // 配置日志输出到文件（每天轮转）
    let file_appender = tracing_appender::rolling::daily(log_dir.clone(), "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // 保持 guard 在整个程序生命周期
    std::mem::forget(_guard);

    // 使用 MultiWriter 同时输出到控制台和文件
    let writer = std::io::stdout.and(non_blocking);

    // 使用本地时区
    let timer = LocalTime::new(
        time::format_description::parse(
            "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]",
        )
        .unwrap(),
    );

    // 创建自定义日志层
    let tauri_layer = TauriLogLayer::new(broadcaster);

    // 组合所有层
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(writer)
        .with_timer(timer)
        .with_ansi(cfg!(debug_assertions)) // release 版本不使用颜色代码
        .finish()
        .with(tauri_layer);

    tracing::subscriber::set_global_default(subscriber)?;

    eprintln!("日志文件位置: {:?}", log_dir);
    Ok(())
}