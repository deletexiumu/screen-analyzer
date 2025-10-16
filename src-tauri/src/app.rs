//! 应用程序初始化和启动
//!
//! 负责 Tauri 应用的完整启动流程，包括：
//! - 日志系统初始化
//! - 环境变量配置（PATH、代理）
//! - 各领域模块初始化
//! - Actor 系统启动
//! - Tauri Builder 配置
//! - 命令注册

use std::path::Path;
use std::sync::Arc;
use tauri::Manager;
use tracing::{error, info, warn};

// 导入 crate 内部模块
use crate::actors;
use crate::capture::{scheduler::CaptureScheduler, ScreenCapture};
use crate::commands::*;
use crate::domains::{AnalysisDomain, CaptureDomain, StorageDomain, SystemDomain};
use crate::event_bus::EventBus;
use crate::llm::{self, LLMManager};
use crate::logger;
use crate::settings::SettingsManager;
use crate::storage::{self, Database, StorageCleaner};
use crate::video::VideoProcessor;
use crate::AppState;

/// 应用程序入口点
///
/// 初始化并启动 Tauri 应用，包含以下步骤：
/// 1. 日志系统初始化
/// 2. 环境变量配置（macOS/Windows 平台特定）
/// 3. 应用数据目录创建
/// 4. 领域模块初始化
/// 5. Actor 系统启动
/// 6. 后台任务启动
/// 7. Tauri 命令注册
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 创建日志广播器
    let log_broadcaster = Arc::new(logger::LogBroadcaster::new());

    // 初始化日志系统（带前端推送功能）
    logger::init_with_broadcaster(log_broadcaster.clone()).expect("Failed to initialize logger");

    tauri::Builder::default()
        .setup(move |app| {
            info!("初始化屏幕活动分析器...");

            // 设置 PATH 环境变量，确保能找到 claude 等命令
            // macOS 应用运行时不会继承 shell 的 PATH，需要手动添加常见路径
            #[cfg(target_os = "macos")]
            {
                if let Ok(current_path) = std::env::var("PATH") {
                    let homebrew_paths = vec![
                        "/opt/homebrew/bin", // Apple Silicon Homebrew
                        "/usr/local/bin",    // Intel Mac Homebrew
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

                // 设置系统代理
                #[cfg(target_os = "macos")]
                {
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

                // Windows 系统代理设置
                #[cfg(target_os = "windows")]
                {
                    use winreg::enums::*;
                    use winreg::RegKey;

                    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                    if let Ok(internet_settings) = hkcu.open_subkey(
                        "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
                    ) {
                        // 检查代理是否启用
                        if let Ok(proxy_enable) =
                            internet_settings.get_value::<u32, _>("ProxyEnable")
                        {
                            if proxy_enable == 1 {
                                // 读取代理服务器设置
                                if let Ok(proxy_server) =
                                    internet_settings.get_value::<String, _>("ProxyServer")
                                {
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
                                                        let proxy_url =
                                                            if addr.starts_with("http://") {
                                                                addr.to_string()
                                                            } else {
                                                                format!("http://{}", addr)
                                                            };
                                                        std::env::set_var("HTTP_PROXY", &proxy_url);
                                                        std::env::set_var("http_proxy", &proxy_url);
                                                        info!("已设置 HTTP 代理: {}", proxy_url);
                                                    }
                                                    "https" => {
                                                        let proxy_url =
                                                            if addr.starts_with("http://") {
                                                                addr.to_string()
                                                            } else {
                                                                format!("http://{}", addr)
                                                            };
                                                        std::env::set_var(
                                                            "HTTPS_PROXY",
                                                            &proxy_url,
                                                        );
                                                        std::env::set_var(
                                                            "https_proxy",
                                                            &proxy_url,
                                                        );
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
            }

            // 设置日志广播器的 app handle
            log_broadcaster.set_app_handle(app.handle().clone());

            let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;

            // 创建必要的目录
            let frames_dir = app_dir.join("frames");
            let videos_dir = app_dir.join("videos");
            let temp_dir = app_dir.join("temp");

            std::fs::create_dir_all(&frames_dir).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&videos_dir).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

            // 初始化运行时（仅用于初始化，不用于运行 Actor）
            let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;

            let (
                state,
                llm_actor,
                status_actor,
                llm_provider_name,
                llm_config_to_load,
                db_config_to_load,
                frames_dir_clone,
                videos_dir_clone,
            ) = runtime.block_on(async {
                // 先初始化设置管理器，以便读取数据库配置
                let settings = Arc::new(
                    SettingsManager::new(app_dir.join("config.json"))
                        .await
                        .expect("设置管理器初始化失败"),
                );

                // 读取初始配置
                let initial_config = settings.get().await;

                // 准备数据库配置（延迟初始化）
                let db_config_to_load =
                    if let Some(db_config) = initial_config.database_config.clone() {
                        info!("将使用配置的数据库: {:?}", db_config);
                        Some(db_config)
                    } else {
                        info!("将使用默认 SQLite 数据库");
                        None
                    };

                // 初始化截屏管理器
                let capture =
                    Arc::new(ScreenCapture::new(frames_dir.clone()).expect("截屏管理器初始化失败"));

                // 创建共享的 HTTP 客户端（用于 LLM API 调用，复用连接池提升性能）
                let http_client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(300))
                    .pool_max_idle_per_host(10)
                    .build()
                    .expect("无法创建 HTTP 客户端");

                // 初始化LLM管理器（使用Actor模式，无需外层锁）
                // 注意：Actor 不在此处启动，而是在后台任务的运行时中启动
                let llm_manager = LLMManager::new(http_client.clone());
                let (llm_actor, llm_handle) = actors::LLMManagerActor::new(llm_manager);

                // 从配置加载截屏设置
                if let Some(capture_settings) = initial_config.capture_settings.clone() {
                    capture.update_settings(capture_settings.clone()).await;
                    info!("已加载截屏配置: {:?}", capture_settings);
                }

                // 保存 LLM 配置（在 Actor 启动后再配置）
                let llm_provider_name = initial_config.llm_provider.clone();
                let llm_config_to_load = initial_config.llm_config.clone();

                // 初始化视频处理器
                let video_processor = Arc::new(
                    VideoProcessor::new(videos_dir.clone(), temp_dir)
                        .expect("视频处理器初始化失败"),
                );

                // 初始化调度器
                let mut scheduler_inner = CaptureScheduler::new(capture.clone());
                scheduler_inner.configure(
                    initial_config.capture_interval,
                    initial_config.summary_interval,
                );
                let scheduler = Arc::new(scheduler_inner);

                // 初始化系统状态（使用Actor模式，无需锁）
                // 注意：Actor 不在此处启动，而是在后台任务的运行时中启动
                let (status_actor, status_handle) = actors::SystemStatusActor::new();

                // 从配置中读取日志设置并应用
                let initial_logger_settings = initial_config.logger_settings.unwrap_or_default();
                log_broadcaster.set_enabled(initial_logger_settings.enable_frontend_logging);
                info!(
                    "日志推送已设置: {}",
                    initial_logger_settings.enable_frontend_logging
                );

                // 将 HTTP 客户端包装为 Arc 以便在 AppState 中共享
                let http_client = Arc::new(http_client);

                // ==================== 组装领域管理器 ====================

                // 创建捕获领域
                let capture_domain =
                    Arc::new(CaptureDomain::new(capture.clone(), scheduler.clone()));

                // 创建分析领域（使用LLM Handle）
                let analysis_domain = Arc::new(AnalysisDomain::new(
                    llm_handle.clone(),
                    video_processor.clone(),
                ));

                // 创建存储领域（数据库未初始化）
                let storage_domain = Arc::new(StorageDomain::new_pending(settings.clone()));

                // 创建系统领域（使用SystemStatus Handle）
                let system_domain = Arc::new(SystemDomain::new(
                    status_handle.clone(),
                    log_broadcaster.clone(),
                    http_client,
                ));

                // 创建事件总线（容量1000,足够缓冲）
                let event_bus = Arc::new(EventBus::new(1000));

                info!("领域管理器已初始化完成");

                let app_state = AppState {
                    capture_domain,
                    analysis_domain,
                    storage_domain,
                    system_domain,
                    event_bus,
                };

                // 返回 AppState、两个 Actor、LLM provider、LLM 配置、数据库配置和目录路径
                (
                    app_state,
                    llm_actor,
                    status_actor,
                    llm_provider_name,
                    llm_config_to_load,
                    db_config_to_load,
                    frames_dir.clone(),
                    videos_dir.clone(),
                )
            });

            // 启动后台任务
            {
                let state_clone = state.clone();
                let app_dir_clone = app_dir.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new()
                        .expect("无法创建 Tokio 运行时，程序无法继续运行");
                    rt.block_on(async {
                        info!("启动后台任务...");

                        // ========== 异步初始化数据库 ==========
                        info!("开始异步初始化数据库...");
                        let db_result = if let Some(mut db_config) = db_config_to_load {
                            // 如果是 SQLite，检查路径是否为相对路径，如果是则转换为应用数据目录下的绝对路径
                            if let crate::storage::config::DatabaseConfig::SQLite {
                                ref mut db_path,
                            } = db_config
                            {
                                let path = std::path::Path::new(db_path.as_str());
                                if path.is_relative() {
                                    let absolute_path = app_dir_clone.join(path);
                                    info!(
                                        "将相对数据库路径 '{}' 转换为绝对路径: {:?}",
                                        db_path, absolute_path
                                    );
                                    *db_path = absolute_path.to_string_lossy().to_string();
                                }
                            }
                            info!("使用配置的数据库: {:?}", db_config);
                            Database::from_config(&db_config).await
                        } else {
                            info!("使用默认 SQLite 数据库");
                            Database::new(&app_dir_clone.join("data.db").to_string_lossy()).await
                        };

                        match db_result {
                            Ok(db) => {
                                let db = Arc::new(db);
                                info!("数据库初始化成功，类型: {}", db.db_type());

                                // 设置数据库到 StorageDomain
                                state_clone.storage_domain.set_database(db.clone()).await;

                                // 初始化存储清理器
                                let cleaner = Arc::new(StorageCleaner::new(
                                    db.clone(),
                                    frames_dir_clone.clone(),
                                    videos_dir_clone.clone(),
                                ));

                                // 从配置读取保留天数
                                let retention_days = state_clone
                                    .storage_domain
                                    .get_settings()
                                    .get()
                                    .await
                                    .retention_days;
                                if let Err(e) = cleaner.set_retention_days(retention_days).await {
                                    error!("设置保留天数失败: {}", e);
                                }

                                // 设置清理器到 StorageDomain
                                state_clone.storage_domain.set_cleaner(cleaner).await;

                                info!("数据库和存储清理器已就绪");
                            }
                            Err(e) => {
                                let error_msg = format!("数据库初始化失败: {}", e);
                                error!("{}", error_msg);
                                state_clone
                                    .storage_domain
                                    .set_database_error(error_msg)
                                    .await;
                                // 继续运行，但数据库相关功能将不可用
                            }
                        }

                        // 启动 Actor（在这个长期运行的运行时中）
                        info!("启动 LLM Manager Actor 和 System Status Actor...");
                        tokio::spawn(llm_actor.run());
                        tokio::spawn(status_actor.run());
                        info!("Actors 已启动");

                        // 配置 LLM（Actor 启动后才能配置）
                        // 1. 根据配置切换 provider
                        let provider = llm_provider_name.as_str();
                        info!("配置 LLM provider: {}", provider);

                        if let Err(e) = state_clone
                            .analysis_domain
                            .get_llm_handle()
                            .switch_provider(provider)
                            .await
                        {
                            error!("切换 LLM provider 失败: {}", e);
                        }

                        // 2. 加载 provider 配置
                        if let Some(llm_config) = llm_config_to_load {
                            match provider {
                                "openai" => {
                                    // Qwen 配置
                                    let qwen_config = llm::QwenConfig {
                                        api_key: llm_config.api_key,
                                        model: llm_config.model,
                                        base_url: llm_config.base_url,
                                        use_video_mode: llm_config.use_video_mode,
                                        video_path: None,
                                    };

                                    if let Err(e) = state_clone
                                        .analysis_domain
                                        .get_llm_handle()
                                        .configure(qwen_config)
                                        .await
                                    {
                                        error!("加载 Qwen 配置失败: {}", e);
                                    } else {
                                        info!("已从配置文件加载 Qwen 设置");
                                    }
                                }
                                "claude" => {
                                    // Claude 配置
                                    let claude_config = serde_json::json!({
                                        "model": llm_config.model,
                                        "auth_token": llm_config.auth_token,
                                        "base_url": llm_config.base_url
                                    });

                                    if let Err(e) = state_clone
                                        .analysis_domain
                                        .get_llm_handle()
                                        .configure_claude(claude_config)
                                        .await
                                    {
                                        error!("加载 Claude 配置失败: {}", e);
                                    } else {
                                        info!("已从配置文件加载 Claude 设置");
                                    }
                                }
                                _ => {
                                    warn!("未知的 LLM provider: {}", provider);
                                }
                            }
                        }

                        // 初始化 Notion 集成
                        let config = state_clone.storage_domain.get_settings().get().await;
                        if let Some(notion_config) = config.notion_config {
                            if notion_config.enabled {
                                if let Err(e) = state_clone
                                    .storage_domain
                                    .get_notion_manager()
                                    .initialize(notion_config)
                                    .await
                                {
                                    error!("Notion 初始化失败: {}", e);
                                } else {
                                    info!("Notion 集成已初始化");
                                }
                            }
                        }

                        // 仅在数据库就绪时启动依赖数据库的组件
                        if let Some(db) = state_clone.storage_domain.try_get_db().await {
                            // 创建LLMProcessor并启动事件监听器（包含 Notion 支持）
                            let llm_processor = Arc::new(llm::LLMProcessor::with_video_and_notion(
                                state_clone.analysis_domain.get_llm_handle().clone(),
                                db.clone(),
                                state_clone.analysis_domain.get_video_processor().clone(),
                                state_clone.storage_domain.get_settings().clone(),
                                state_clone.storage_domain.get_notion_manager().clone(),
                            ));

                            // 启动LLM处理器事件监听器
                            llm_processor
                                .start_event_listener(
                                    state_clone.event_bus.clone(),
                                    state_clone.capture_domain.get_capture().clone(),
                                )
                                .await;

                            info!("LLM处理器事件监听器已启动");

                            // 启动调度器（事件驱动模式）
                            state_clone
                                .capture_domain
                                .get_scheduler()
                                .clone()
                                .start(state_clone.event_bus.clone());

                            // 启动存储清理任务
                            if let Ok(cleaner) = state_clone.storage_domain.get_cleaner().await {
                                cleaner.start_cleanup_task().await;
                                info!("存储清理任务已启动");
                            } else {
                                error!("存储清理器未就绪");
                            }
                        } else {
                            error!("数据库未就绪，跳过数据库相关组件的启动");
                        }

                        // 周期性扫描视频目录，处理未分析的视频
                        {
                            let video_state = state_clone.clone();
                            tokio::spawn(async move {
                                loop {
                                    // 直接执行分析，无需 analysis_lock（已移除临时方案）
                                    match analyze_unprocessed_videos(&video_state, None, false)
                                        .await
                                    {
                                        Ok(report) => {
                                            if report.processed > 0 || report.failed > 0 {
                                                info!(
                                                    "自动视频分析完成: 处理 {} 个, 失败 {} 个",
                                                    report.processed, report.failed
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            error!("自动视频分析失败: {}", e);
                                        }
                                    }
                                    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                                }
                            });
                        }

                        // 更新系统状态
                        state_clone
                            .system_domain
                            .get_status_handle()
                            .set_capturing(true)
                            .await;

                        // 启动系统资源监控任务（每5秒更新一次CPU和内存占用率）
                        {
                            let system_state = state_clone.clone();
                            tokio::spawn(async move {
                                use sysinfo::{Pid, ProcessesToUpdate, System};

                                let mut sys = System::new_all();
                                let current_pid = Pid::from_u32(std::process::id());

                                loop {
                                    // 刷新指定进程信息
                                    sys.refresh_processes(ProcessesToUpdate::Some(&[current_pid]));

                                    // 等待一小段时间让CPU统计稳定
                                    tokio::time::sleep(tokio::time::Duration::from_millis(200))
                                        .await;

                                    // 再次刷新获取准确的CPU使用率
                                    sys.refresh_processes(ProcessesToUpdate::Some(&[current_pid]));

                                    let (cpu_usage, memory_mb) =
                                        if let Some(process) = sys.process(current_pid) {
                                            // 获取当前进程的CPU使用率（单核百分比）
                                            let cpu_single_core = process.cpu_usage();

                                            // 获取CPU核心数
                                            let cpu_count = sys.cpus().len() as f32;

                                            // 计算总CPU占用率（所有核心的平均占用率）
                                            let cpu_total = if cpu_count > 0.0 {
                                                cpu_single_core / cpu_count
                                            } else {
                                                cpu_single_core
                                            };

                                            // 获取当前进程的内存使用（字节）转换为MB
                                            let process_memory = process.memory();
                                            let mem_mb = process_memory as f32 / (1024.0 * 1024.0);

                                            (cpu_total, mem_mb)
                                        } else {
                                            (0.0, 0.0)
                                        };

                                    // 更新系统状态
                                    system_state
                                        .system_domain
                                        .get_status_handle()
                                        .update_system_resources(cpu_usage, memory_mb)
                                        .await;

                                    // 每5秒更新一次
                                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                }
                            });
                        }

                        info!("所有后台任务已启动");

                        // 在独立的后台任务中处理历史图片（不阻塞启动）
                        {
                            let history_state = state_clone.clone();
                            tokio::spawn(async move {
                                info!("开始处理历史图片，生成视频...");
                                if let Err(e) = process_historical_frames(&history_state).await {
                                    error!("处理历史图片失败: {}", e);
                                }
                            });
                        }

                        // 保持运行时活跃
                        loop {
                            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                        }
                    });
                });
            }

            app.manage(state);
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_database_status,
            get_activities,
            get_day_sessions,
            get_day_summary,
            get_session_detail,
            get_app_config,
            update_config,
            add_manual_tag,
            remove_tag,
            get_system_status,
            toggle_capture,
            trigger_analysis,
            generate_video,
            get_video_url,
            get_video_data,
            test_generate_videos,
            cleanup_storage,
            get_storage_stats,
            migrate_timezone_to_local,
            refresh_device_info,
            sync_data_to_mariadb,
            configure_qwen,
            configure_llm_provider,
            test_capture,
            test_llm_api,
            retry_session_analysis,
            regenerate_timeline,
            delete_session,
            open_storage_folder,
            get_log_dir,
            open_log_folder,
            test_notion_connection,
            update_notion_config,
            search_notion_pages,
            create_notion_database,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ==================== 辅助函数 ====================

/// 处理历史图片，为没有视频的会话生成视频
///
/// # 参数
/// - `state`: 应用状态
///
/// # 返回
/// - `Ok(())`: 处理成功
/// - `Err(String)`: 处理失败
async fn process_historical_frames(state: &AppState) -> Result<(), String> {
    // 仅在 SQLite 模式下处理历史图片
    let db = state.storage_domain.get_db().await?;
    if !db.is_sqlite() {
        info!("跳过历史图片处理（仅 SQLite 模式支持）");
        return Ok(());
    }

    info!("开始处理历史图片");

    // 查询所有会话，筛选出未生成视频的
    let all_sessions = db.get_all_sessions().await.map_err(|e| e.to_string())?;

    let sessions_without_video: Vec<_> = all_sessions
        .into_iter()
        .filter(|s| s.video_path.is_none() || s.video_path.as_ref().map_or(false, |p| p.is_empty()))
        .take(10)
        .collect();

    info!("找到 {} 个未生成视频的会话", sessions_without_video.len());

    for session in sessions_without_video {
        let session_id = match session.id {
            Some(id) => id,
            None => continue,
        };

        info!(
            "处理会话 {}: {} - {}",
            session_id, session.start_time, session.end_time
        );

        // 获取该会话的所有帧
        let frames = match db.get_frames_by_session(session_id).await {
            Ok(frames) => frames,
            Err(e) => {
                error!("获取会话 {} 的帧失败: {}", session_id, e);
                continue;
            }
        };

        if !frames.is_empty() {
            let frame_paths: Vec<String> = frames.into_iter().map(|f| f.file_path).collect();

            if !frame_paths.is_empty() {
                info!(
                    "为会话 {} 生成视频，共 {} 帧",
                    session_id,
                    frame_paths.len()
                );

                // 生成视频
                let video_config = crate::video::VideoConfig::default();
                let video_filename = format!(
                    "{}-{}.mp4",
                    session.start_time.format("%Y%m%d%H%M"),
                    session.end_time.format("%Y%m%d%H%M")
                );

                let video_path_buf = state
                    .analysis_domain
                    .get_video_processor()
                    .output_dir
                    .join(&video_filename);
                match state
                    .analysis_domain
                    .get_video_processor()
                    .create_summary_video(frame_paths.clone(), &video_path_buf, &video_config)
                    .await
                {
                    Ok(video_result) => {
                        info!(
                            "视频生成成功: {} ({}字节, {}ms)",
                            video_path_buf.display(),
                            video_result.file_size,
                            video_result.processing_time_ms
                        );

                        let video_path_str = video_path_buf.to_string_lossy();
                        // 更新数据库中的视频路径
                        if let Err(e) = db
                            .update_session_video_path(session_id, &video_path_str)
                            .await
                        {
                            error!("更新会话 {} 视频路径失败: {}", session_id, e);
                        }

                        // 删除已合并到视频的图片文件（使用异步 I/O）
                        let mut deleted_count = 0;
                        for frame_path in &frame_paths {
                            if std::path::Path::new(frame_path).exists() {
                                if let Err(e) = tokio::fs::remove_file(frame_path).await {
                                    error!("删除图片失败 {}: {}", frame_path, e);
                                } else {
                                    deleted_count += 1;
                                }
                            }
                        }
                        info!("已删除 {} 个图片文件", deleted_count);
                    }
                    Err(e) => {
                        error!("为会话 {} 生成视频失败: {}", session_id, e);
                    }
                }
            }
        }
    }

    info!("历史图片处理完成");
    Ok(())
}

/// 视频分析报告
#[derive(Default)]
struct VideoAnalysisReport {
    #[allow(dead_code)]
    total_candidates: usize,
    processed: usize,
    failed: usize,
    messages: Vec<String>,
}

/// 视频分析结果
struct VideoAnalysisOutcome {
    #[allow(dead_code)]
    _session_id: i64, // 保留用于未来可能的扩展
    segments_count: usize,
    timeline_count: usize,
    #[allow(dead_code)]
    summary: llm::SessionSummary,
}

/// 分析单个视频
///
/// # 参数
/// - `state`: 应用状态
/// - `video_path`: 视频路径
/// - `session_start`: 会话开始时间
/// - `session_end`: 会话结束时间
/// - `duration_minutes`: 会话时长（分钟）
/// - `reuse_session`: 复用已有会话ID（可选）
///
/// # 返回
/// - `Ok(VideoAnalysisOutcome)`: 分析成功
/// - `Err(String)`: 分析失败
async fn analyze_video_once(
    state: &AppState,
    video_path: &Path,
    session_start: chrono::DateTime<chrono::Utc>,
    session_end: chrono::DateTime<chrono::Utc>,
    duration_minutes: u32,
    reuse_session: Option<i64>,
) -> Result<VideoAnalysisOutcome, String> {
    let video_path_str = video_path.to_string_lossy().to_string();
    let file_stem = video_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("视频");

    let persisted_config = state.storage_domain.get_settings().get().await;
    let llm_handle = state.analysis_domain.get_llm_handle();

    // 根据当前 provider 配置 LLM
    let current_provider = persisted_config.llm_provider.as_str();

    match current_provider {
        "openai" => {
            // 注意：启动时已经根据配置切换到了正确的 provider，这里无需再次切换

            // Qwen provider 需要 API key
            let qwen_config = if let Some(llm_config) = persisted_config.llm_config {
                llm::QwenConfig {
                    api_key: llm_config.api_key,
                    model: llm_config.model,
                    base_url: llm_config.base_url,
                    use_video_mode: llm_config.use_video_mode,
                    video_path: Some(video_path_str.clone()),
                }
            } else {
                let config = llm_handle.get_config().await.map_err(|e| e.to_string())?;
                llm::QwenConfig {
                    api_key: config.qwen.api_key.clone(),
                    model: config.qwen.model.clone(),
                    base_url: config.qwen.base_url.clone(),
                    use_video_mode: true,
                    video_path: Some(video_path_str.clone()),
                }
            };

            if qwen_config.api_key.is_empty() {
                return Err("请先在设置中配置 Qwen API Key".to_string());
            }

            if let Err(e) = llm_handle.configure(qwen_config).await {
                return Err(e.to_string());
            }
        }
        "claude" => {
            // Claude provider 无需额外配置
            // 注意：启动时已经根据配置切换到了正确的 provider，这里无需再次切换
            info!("使用 Claude provider 进行视频分析（API key 可选）");
        }
        _ => {
            return Err(format!("不支持的 LLM provider: {}", current_provider));
        }
    }

    // 设置视频路径
    if let Err(e) = llm_handle
        .set_video_path(Some(video_path_str.clone()))
        .await
    {
        return Err(e.to_string());
    }

    let now = storage::local_now();

    // 准备会话
    let db = state.storage_domain.get_db().await?;
    let session_id = if let Some(existing_id) = reuse_session {
        if let Err(e) = db.delete_video_segments_by_session(existing_id).await {
            let _ = llm_handle.set_video_path(None).await;
            return Err(format!("清理历史视频分段失败: {}", e));
        }

        if let Err(e) = db.delete_timeline_cards_by_session(existing_id).await {
            let _ = llm_handle.set_video_path(None).await;
            return Err(format!("清理历史时间线卡片失败: {}", e));
        }

        existing_id
    } else {
        let (device_name, device_type) = storage::get_device_info();
        let temp_session = storage::Session {
            id: None,
            start_time: session_start,
            end_time: session_end,
            title: format!("视频分析中: {}", file_stem),
            summary: "正在分析...".to_string(),
            video_path: Some(video_path_str.clone()),
            tags: "[]".to_string(),
            created_at: Some(now),
            device_name: Some(device_name),
            device_type: Some(device_type),
        };

        match state
            .storage_domain
            .get_db()
            .await?
            .insert_session(&temp_session)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                let _ = llm_handle.set_video_path(None).await;
                return Err(e.to_string());
            }
        }
    };

    llm_handle
        .set_provider_database(
            state.storage_domain.get_db().await?.clone(),
            Some(session_id),
        )
        .await
        .map_err(|e| e.to_string())?;

    llm_handle
        .set_session_window(Some(session_start), Some(session_end))
        .await
        .map_err(|e| e.to_string())?;

    // 设置视频速率乘数（从配置获取）
    let speed_multiplier = persisted_config.video_config.speed_multiplier;
    llm_handle
        .set_video_speed(speed_multiplier)
        .await
        .map_err(|e| e.to_string())?;

    let analysis = match llm_handle
        .segment_video_and_generate_timeline(vec![], duration_minutes, None)
        .await
    {
        Ok(res) => res,
        Err(e) => {
            let _ = llm_handle.set_video_path(None).await;
            let error_msg = e.to_string();
            // 检测是否是视频过短的错误
            if error_msg.contains("The video file is too short") {
                return Err(format!("VIDEO_TOO_SHORT:{}", error_msg));
            }
            return Err(error_msg);
        }
    };

    let _ = llm_handle.set_video_path(None).await;
    let _ = llm_handle.set_session_window(None, None).await;

    let mut segments = analysis.segments;
    for segment in &mut segments {
        let start_abs =
            llm::relative_to_absolute(session_start, session_end, &segment.start_timestamp);
        let end_abs = llm::relative_to_absolute(session_start, session_end, &segment.end_timestamp);
        segment.start_timestamp = start_abs.to_rfc3339();
        segment.end_timestamp = end_abs.to_rfc3339();
    }

    let mut timeline_cards = analysis.timeline_cards;
    for card in &mut timeline_cards {
        let start_abs = llm::relative_to_absolute(session_start, session_end, &card.start_time);
        let end_abs = llm::relative_to_absolute(session_start, session_end, &card.end_time);
        card.start_time = start_abs.to_rfc3339();
        card.end_time = end_abs.to_rfc3339();

        if let Some(distractions) = card.distractions.as_mut() {
            for distraction in distractions {
                let d_start =
                    llm::relative_to_absolute(session_start, session_end, &distraction.start_time);
                let d_end =
                    llm::relative_to_absolute(session_start, session_end, &distraction.end_time);
                distraction.start_time = d_start.to_rfc3339();
                distraction.end_time = d_end.to_rfc3339();
            }
        }
    }

    if !segments.is_empty() {
        let segment_records: Vec<storage::VideoSegmentRecord> = segments
            .iter()
            .map(|seg| storage::VideoSegmentRecord {
                id: None,
                session_id,
                llm_call_id: analysis.segment_call_id,
                start_timestamp: seg.start_timestamp.clone(),
                end_timestamp: seg.end_timestamp.clone(),
                description: seg.description.clone(),
                created_at: now,
            })
            .collect();

        if let Err(e) = state
            .storage_domain
            .get_db()
            .await?
            .insert_video_segments(&segment_records)
            .await
        {
            return Err(format!("保存视频分段失败: {}", e));
        }
    }

    if !timeline_cards.is_empty() {
        let card_records: Vec<storage::TimelineCardRecord> = timeline_cards
            .iter()
            .map(|card| storage::TimelineCardRecord {
                id: None,
                session_id,
                llm_call_id: analysis.timeline_call_id,
                start_time: card.start_time.clone(),
                end_time: card.end_time.clone(),
                category: card.category.clone(),
                subcategory: card.subcategory.clone(),
                title: card.title.clone(),
                summary: card.summary.clone(),
                detailed_summary: card.detailed_summary.clone(),
                distractions: card
                    .distractions
                    .as_ref()
                    .map(|d| serde_json::to_string(d).unwrap_or_else(|_| "[]".to_string())),
                video_preview_path: Some(video_path_str.clone()),
                app_sites: serde_json::to_string(&card.app_sites)
                    .unwrap_or_else(|_| "{}".to_string()),
                created_at: now,
            })
            .collect();

        if let Err(e) = state
            .storage_domain
            .get_db()
            .await?
            .insert_timeline_cards(&card_records)
            .await
        {
            return Err(format!("保存时间线卡片失败: {}", e));
        }
    }

    let summary =
        llm::build_session_summary(session_start, session_end, &segments, &timeline_cards);

    let tags_json = serde_json::to_string(&summary.tags).unwrap_or_else(|_| "[]".to_string());
    if let Err(e) = db
        .update_session(
            session_id,
            &summary.title,
            &summary.summary,
            Some(&video_path_str),
            &tags_json,
        )
        .await
    {
        return Err(format!("更新会话信息失败: {}", e));
    }

    Ok(VideoAnalysisOutcome {
        _session_id: session_id,
        segments_count: segments.len(),
        timeline_count: timeline_cards.len(),
        summary,
    })
}

/// 分析未处理的视频
///
/// # 参数
/// - `state`: 应用状态
/// - `limit`: 处理数量限制（可选）
/// - `mark_status`: 是否更新系统状态
///
/// # 返回
/// - `Ok(VideoAnalysisReport)`: 分析报告
/// - `Err(String)`: 分析失败
async fn analyze_unprocessed_videos(
    state: &AppState,
    limit: Option<usize>,
    mark_status: bool,
) -> Result<VideoAnalysisReport, String> {
    use std::collections::HashSet;

    let videos_dir = state
        .analysis_domain
        .get_video_processor()
        .output_dir
        .clone();

    // 使用异步 I/O 读取目录
    let mut video_files = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&videos_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("mp4") {
                video_files.push(path);
            }
        }
    }

    video_files.sort();

    if video_files.is_empty() {
        return Ok(VideoAnalysisReport::default());
    }

    // 使用新的抽象方法获取已分析的视频路径（支持 SQLite 和 MariaDB）
    let analyzed_video_paths = state
        .storage_domain
        .get_db()
        .await?
        .get_analyzed_video_paths()
        .await
        .map_err(|e| e.to_string())?;

    let analyzed_paths: HashSet<String> = analyzed_video_paths.into_iter().collect();

    let mut unanalyzed_videos: Vec<std::path::PathBuf> = video_files
        .into_iter()
        .filter(|path| {
            let path_str = path.to_string_lossy().to_string();
            !analyzed_paths.contains(&path_str)
        })
        .collect();

    unanalyzed_videos.sort();

    let total_candidates = unanalyzed_videos.len();
    if total_candidates == 0 {
        return Ok(VideoAnalysisReport::default());
    }

    let total_to_process = limit
        .map(|l| l.min(total_candidates))
        .unwrap_or(total_candidates);

    if total_to_process == 0 {
        return Ok(VideoAnalysisReport {
            total_candidates,
            ..Default::default()
        });
    }

    // 使用单一的原子操作更新状态
    if mark_status {
        state
            .system_domain
            .get_status_handle()
            .set_processing(true)
            .await;
        state
            .system_domain
            .get_status_handle()
            .set_error(None)
            .await;
    }

    let mut report = VideoAnalysisReport {
        total_candidates,
        processed: 0,
        failed: 0,
        messages: Vec::new(),
    };

    let mut processing_error: Option<String> = None;

    for (index, video_path) in unanalyzed_videos.iter().enumerate() {
        if index >= total_to_process {
            break;
        }

        info!("开始分析视频: {:?}", video_path);

        let video_filename = video_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let (session_start, session_end) = parse_video_window_from_stem(video_filename)
            .unwrap_or_else(|| {
                let end = storage::local_now();
                (end - chrono::Duration::minutes(15), end)
            });

        let diff = session_end.signed_duration_since(session_start);
        let duration_minutes = if diff.num_seconds() > 0 {
            ((diff.num_seconds() as f64) / 60.0).ceil() as u32
        } else {
            1
        };

        match analyze_video_once(
            state,
            video_path,
            session_start,
            session_end,
            duration_minutes,
            None,
        )
        .await
        {
            Ok(outcome) => {
                info!(
                    "视频分析成功: {} 个片段, {} 个卡片",
                    outcome.segments_count, outcome.timeline_count
                );
                report.processed += 1;
                report.messages.push(format!(
                    "✅ {}: {} 片段, {} 卡片",
                    video_filename, outcome.segments_count, outcome.timeline_count
                ));
            }
            Err(err) => {
                error!("视频分析失败: {}", err);

                // 如果是视频过短错误，删除视频文件避免反复尝试
                if err.contains("VIDEO_TOO_SHORT") {
                    info!("检测到视频过短错误，删除视频文件: {:?}", video_path);
                    if let Err(e) = tokio::fs::remove_file(video_path).await {
                        error!("删除视频文件失败: {}", e);
                    } else {
                        info!("已删除过短的视频文件: {:?}", video_path);
                    }
                }

                report.failed += 1;
                report
                    .messages
                    .push(format!("❌ {}: 分析失败 - {}", video_filename, err));
                processing_error = Some(err);
                break;
            }
        }

        if total_to_process > 1 && (index + 1) < total_to_process {
            info!("等待2秒后继续分析下一个视频...");
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }

    // 使用单一的原子操作更新所有状态字段
    if mark_status {
        state
            .system_domain
            .get_status_handle()
            .set_processing(false)
            .await;
    }
    state
        .system_domain
        .get_status_handle()
        .update_last_process_time(storage::local_now())
        .await;
    state
        .system_domain
        .get_status_handle()
        .set_error(processing_error.clone())
        .await;

    if let Some(err) = processing_error {
        return Err(err);
    }

    Ok(report)
}

/// 从视频文件名解析时间窗口
///
/// # 参数
/// - `stem`: 文件名（不含扩展名）
///
/// # 返回
/// - `Some((开始时间, 结束时间))`: 解析成功
/// - `None`: 解析失败
fn parse_video_window_from_stem(
    stem: &str,
) -> Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
    use chrono::{Local, NaiveDateTime, TimeZone};

    // 处理 segment_YYYYMMDDHHMMSS_YYYYMMDDHHMMSS 格式
    if stem.starts_with("segment_") {
        let parts: Vec<&str> = stem
            .strip_prefix("segment_")?
            .split('_')
            .filter(|p| !p.is_empty())
            .collect();

        if parts.len() != 2 {
            return None;
        }

        let start = parts[0];
        let end = parts[1];

        if start.len() != 12 || end.len() != 12 {
            return None;
        }

        let start_naive = NaiveDateTime::parse_from_str(start, "%Y%m%d%H%M").ok()?;
        let end_naive = NaiveDateTime::parse_from_str(end, "%Y%m%d%H%M").ok()?;

        // 视频文件名中的时间是本地时间，需要先转为本地时区，再转为 UTC
        return Some((
            Local
                .from_local_datetime(&start_naive)
                .single()?
                .with_timezone(&chrono::Utc),
            Local
                .from_local_datetime(&end_naive)
                .single()?
                .with_timezone(&chrono::Utc),
        ));
    }

    // 处理带连字符的旧格式 YYYYMMDDHHMMSS-YYYYMMDDHHMMSS
    let cleaned: String = stem
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .collect();

    let mut parts = cleaned.split('-').filter(|p| !p.is_empty());
    let start = parts.next()?;
    let end = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    if start.len() != 12 || end.len() != 12 {
        return None;
    }

    let start_naive = NaiveDateTime::parse_from_str(start, "%Y%m%d%H%M").ok()?;
    let end_naive = NaiveDateTime::parse_from_str(end, "%Y%m%d%H%M").ok()?;

    // 视频文件名中的时间是本地时间，需要先转为本地时区，再转为 UTC
    Some((
        Local
            .from_local_datetime(&start_naive)
            .single()?
            .with_timezone(&chrono::Utc),
        Local
            .from_local_datetime(&end_naive)
            .single()?
            .with_timezone(&chrono::Utc),
    ))
}
