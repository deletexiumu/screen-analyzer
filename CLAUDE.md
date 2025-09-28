# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 基本规则
- 全程使用中文
- 代码中需要一定量的中文注释，以便于后续代码理解。在每个代码文件开头需要注释该文件主要用途。

## Commands

### Development
```bash
npm run dev          # 启动前端开发服务器 (端口 1420)
npm run tauri dev    # 启动完整的 Tauri 应用开发模式
```

### Build
```bash
npm run build        # 构建前端资源
npm run tauri build  # 构建完整的桌面应用
npm run preview      # 预览构建结果
```

## Architecture

这是一个基于 Tauri + Vue 3 + Rust 的跨平台屏幕分析桌面应用。

### 技术栈
- **前端**: Vue 3 + Vite + Element Plus + Pinia
- **后端**: Rust + Tauri 2.x
- **数据库**: SQLite (通过 sqlx)
- **关键库**: screenshots (截屏), image (图像处理), reqwest (HTTP), tokio (异步)

### 核心模块设计（根据技术方案）
1. **截屏模块**: 使用 `screenshots` crate 实现 1 FPS 自动截屏
2. **LLM 分析**: 插件式架构支持 OpenAI/Anthropic 等多种提供商
3. **视频处理**: FFmpeg 生成时间线视频
4. **数据管理**: SQLite 存储，支持自动清理（默认7天）

### 项目结构
- `src/`: Vue 3 前端应用
- `src-tauri/`: Rust 后端和 Tauri 配置
  - `tauri.conf.json`: Tauri 应用配置（标识: com.cookie.screen-analyzer）
  - `Cargo.toml`: Rust 依赖定义

### 开发注意事项
- 应用需要屏幕录制权限（macOS）
- Vite 开发服务器端口: 1420, HMR 端口: 1421
- 当前处于初始模板状态，核心功能待实现
- 详细技术方案见 `技术路线说明.md`