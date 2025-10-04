fn main() {
    // 添加 macOS Info.plist 配置
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=10.13");
        // 设置 Info.plist 键值
        std::env::set_var("TAURI_BUNDLE_NSScreenCaptureUsageDescription", "本应用需要屏幕录制权限来自动捕获屏幕截图，用于分析您的工作活动。所有数据仅存储在本地，不会上传到任何服务器。");
        std::env::set_var(
            "TAURI_BUNDLE_NSAppleEventsUsageDescription",
            "本应用需要自动化权限来正常运行。",
        );
    }

    tauri_build::build()
}
