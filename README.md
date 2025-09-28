# 屏幕活动分析器 (Screen Analyzer)

## 特别鸣谢 🙏

**本项目的灵感来源于开源项目 [Dayflow](https://github.com/JerryZLiu/Dayflow)**，感谢 JerryZLiu 的创意和分享！Dayflow 的设计理念给了我们很大的启发，帮助我们打造了这款更适合本地使用的屏幕活动分析工具。

> 🤖 **本项目采用 Claude Code + Codex 混合编程方式开发，0人工手搓代码，完全由 AI 驱动实现。**

## 项目简介

屏幕活动分析器是一款基于 Tauri + Vue 3 + Rust 开发的跨平台桌面应用，旨在通过智能化的方式自动记录、分析和总结您的日常屏幕活动。

### 核心功能

- 🎬 **自动截屏录制**：每秒自动捕获屏幕，记录您的工作状态
- 🤖 **AI 智能分析**：使用 LLM（支持通义千问等）自动分析活动内容
- 📹 **视频生成**：将截屏序列生成时间线视频，支持快速回顾
- 📊 **活动时间线**：可视化展示一天的工作流程和活动分布
- 🗂️ **自动分类**：智能识别工作、学习、娱乐等不同活动类型
- 🧹 **存储管理**：自动清理过期数据，默认保留7天记录
- 🔒 **隐私保护**：所有数据本地存储，不上传云端

## 系统要求

### 运行环境
- **操作系统**：macOS 10.15+ / Windows 10+ / Linux (Ubuntu 20.04+)
- **内存**：建议 8GB 及以上
- **存储空间**：至少 10GB 可用空间（用于存储截图和视频）
- **FFmpeg**：应用已内置，无需单独安装（如需使用系统FFmpeg，请确保已安装）

### 开发环境
- Node.js 18.0+
- Rust 1.70+
- pnpm/npm/yarn 包管理器

## 快速开始

### 下载安装包

前往 [Releases](https://github.com/yourusername/screen-analyzer/releases) 页面下载对应平台的安装包：

- **macOS**: `Screen-Analyzer_x.x.x_universal.dmg`
- **Windows**: `Screen-Analyzer_x.x.x_x64-setup.exe`
- **Linux**: `screen-analyzer_x.x.x_amd64.AppImage`

### 源码编译

#### 1. 克隆项目
```bash
git clone https://github.com/yourusername/screen-analyzer.git
cd screen-analyzer
```

#### 2. 安装依赖
```bash
# 安装前端依赖
npm install

# 安装 Rust 依赖（如未安装 Rust）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

#### 3. 配置 FFmpeg（可选）
应用已内置 FFmpeg，如需使用内置版本：
1. 下载对应平台的 FFmpeg 二进制文件
2. 放置到 `src-tauri/resources/ffmpeg/[platform]/` 目录
3. 详见 `src-tauri/resources/README.md`

#### 4. 开发模式运行
```bash
npm run tauri dev
```

#### 5. 构建生产版本

**macOS**:
```bash
npm run tauri build -- --target universal-apple-darwin
```

**Windows**:
```bash
npm run tauri build -- --target x86_64-pc-windows-msvc
```

**Linux**:
```bash
npm run tauri build -- --target x86_64-unknown-linux-gnu
```

构建完成后，安装包会生成在 `src-tauri/target/release/bundle/` 目录下。

## 配置说明

### 首次启动配置

1. **授予权限**（macOS）
   - 首次启动需要授予"屏幕录制"权限
   - 系统偏好设置 → 安全性与隐私 → 隐私 → 屏幕录制

2. **配置 LLM API**
   - 打开设置界面
   - 选择 LLM 提供商（默认支持通义千问）
   - 输入 API Key
   - 测试连接

### 配置文件位置

- **macOS**: `~/Library/Application Support/com.cookie.screen-analyzer/`
- **Windows**: `%APPDATA%\com.cookie.screen-analyzer\`
- **Linux**: `~/.config/com.cookie.screen-analyzer/`

配置文件说明：
- `config.json`: 应用配置
- `data.db`: SQLite 数据库
- `frames/`: 截图存储目录
- `videos/`: 生成的视频文件

## 开机启动设置

### macOS

1. 打开"系统偏好设置" → "用户与群组"
2. 选择当前用户，点击"登录项"标签
3. 点击"+"按钮，选择 Screen Analyzer 应用
4. 勾选"隐藏"选项（可选）

### Windows

1. 按 `Win + R`，输入 `shell:startup`
2. 将 Screen Analyzer 快捷方式复制到打开的文件夹
3. 或在应用设置中启用"开机自启动"选项

### Linux

创建 systemd 服务文件：
```bash
# 创建服务文件
sudo nano /etc/systemd/system/screen-analyzer.service

# 添加以下内容
[Unit]
Description=Screen Analyzer
After=graphical.target

[Service]
Type=simple
ExecStart=/usr/local/bin/screen-analyzer
Restart=always
User=YOUR_USERNAME

[Install]
WantedBy=default.target

# 启用服务
sudo systemctl enable screen-analyzer
sudo systemctl start screen-analyzer
```

## 使用指南

### 基本操作

1. **查看活动记录**
   - 在日历视图中选择日期
   - 查看当天的活动时间线
   - 点击活动卡片查看详情

2. **生成回顾视频**
   - 选择会话
   - 点击"生成视频"按钮
   - 设置播放速度（默认4倍速）

3. **手动分析**
   - 点击"分析"按钮触发 AI 分析
   - 支持重新分析特定会话

4. **数据管理**
   - 在设置中调整保留天数（最多30天）
   - 手动触发存储清理

### 快捷键

- `Cmd/Ctrl + ,`: 打开设置
- `Cmd/Ctrl + R`: 刷新数据
- `Cmd/Ctrl + Q`: 退出应用
- `Space`: 暂停/恢复截屏

## 隐私与安全

- ✅ **完全本地化**：所有数据存储在本地，不会上传到任何服务器
- ✅ **数据加密**：敏感配置信息加密存储
- ✅ **自动清理**：过期数据自动删除，防止占用过多空间
- ✅ **权限控制**：仅在用户授权后才能访问屏幕内容

## 常见问题

### Q: 应用占用空间太大怎么办？
A: 可以在设置中减少保留天数，或手动清理历史数据。建议保留3-7天的数据。

### Q: 为什么 AI 分析失败？
A: 请检查：
1. API Key 是否正确配置
2. 网络连接是否正常
3. API 额度是否充足

### Q: macOS 提示没有权限？
A: 需要在系统偏好设置中授予"屏幕录制"权限，授权后需要重启应用。

### Q: 视频生成失败？
A:
1. 应用已内置 FFmpeg，如果仍然失败请检查：
   - 确保 `src-tauri/resources/ffmpeg/` 目录下有对应平台的FFmpeg文件
   - Windows: `ffmpeg.exe`，macOS/Linux: `ffmpeg`
2. 检查截图文件是否存在
3. 确保会话有对应的截图帧

## 技术栈

- **前端**: Vue 3 + Vite + Element Plus + Pinia
- **后端**: Rust + Tauri 2.x
- **数据库**: SQLite (sqlx)
- **核心库**:
  - screenshots（跨平台截屏）
  - image（图像处理）
  - FFmpeg（视频生成）
  - reqwest（HTTP 客户端）
  - tokio（异步运行时）

## 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 提交 Pull Request

## 开源协议

本项目采用 [MIT License](LICENSE) 开源协议。

## 致谢

- [Dayflow](https://github.com/JerryZLiu/Dayflow) - 项目灵感来源
- [Tauri](https://tauri.app/) - 跨平台框架
- [Vue.js](https://vuejs.org/) - 前端框架
- [通义千问](https://tongyi.aliyun.com/) - AI 分析支持

## 联系方式

- 项目主页: [https://github.com/deletexiumu/screen-analyzer](https://github.com/yourusername/screen-analyzer)
- Issue 反馈: [https://github.com/deletexiumu/screen-analyzer/issues](https://github.com/yourusername/screen-analyzer/issues)

---

<div align="center">
  <sub>使用 ❤️ 和 AI 构建</sub>
</div>