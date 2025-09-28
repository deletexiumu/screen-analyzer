# AI 开发执行手册

## 环境准备
- 安装 Node.js 18+、Rust 1.75+、FFmpeg，并执行 `npm install` 与 `cargo fetch`，验证 `npm run tauri dev` 能正常启动。
- 为选定的 LLM 提供商准备 API Key，通过 `.env` 或安全密钥服务注入，避免硬编码在仓库中。
- 确认工作目录 `src-tauri/` 可写入 `frames/`、数据库与视频输出目录，必要时在启动流程中创建。

## 核心开发任务
### Rust 后端
- 按《技术路线说明》在 `src-tauri/src/` 下补全 `capture/`, `video/`, `llm/`, `storage/`, `models/` 模块，所有公共接口集中在 `lib.rs` 并通过 Tauri 命令暴露。
- 实现 `ScreenCapture` 支持多屏枚举、1920×1080 JPG 输出、毫秒时间戳命名；`CaptureScheduler` 使用 Tokio 定时任务调度截屏、触发 LLM 分析并写入数据库。
- 设计 `LLMProvider` trait 及 `OpenAIProvider`、`AnthropicProvider`，支持 30 秒帧采样、Vision API 调用、JSON 解析为 `SessionSummary`，允许动态注册与切换。
- 在 `video::VideoProcessor` 中调用 FFmpeg 生成 20 倍速回放；临时帧清单需安全创建并在任务完成后清理。
- `storage::Database` 使用 SQLx 创建 `sessions`、`frames` 表，提供按日期查询、详情、标签增补；`StorageCleaner` 每小时清理超过保留期的数据并验证 `retention_days` 合法。

### 前端实现
- 使用 Vue 3 + Element Plus 构建 `CalendarView`、`TimelineView`、`SessionDetail`、`SettingsDialog` 等组件，数据交互通过 `@tauri-apps/api` 调用后端命令。
- Calendar 展示每日摘要与标签预览；Timeline 呈现会话列表及缩略图；SessionDetail 展示标题、摘要、关键时刻，支持调用 `add_manual_tag` 编辑标签并播放回放视频。
- SettingsDialog 调整 `retentionDays`、`llmProvider`、截屏/总结间隔，通过 `update_config` 同步并更新本地状态；必要时引入 Pinia 管理全局配置。
- 样式保持两空格缩进，按布局、交互、主题顺序组织 CSS，静态资源放在 `src/assets`。

### Tauri 集成
- 在 `tauri.conf.json` 维护 `beforeDevCommand`、窗口尺寸、系统托盘与权限白名单，遵循最小权限原则。
- `main.rs` 中初始化 Tokio runtime、`ScreenCapture`、`LLMManager`、`StorageCleaner`，注册 `get_activities`、`get_day_sessions`、`get_session_detail`、`update_config`、`add_manual_tag` 等命令，并确保后台任务启动。

## 开发规范与流程
- Rust 代码启用 `#![warn(clippy::all, clippy::pedantic)]`，保持 snake_case 文件命名、CamelCase 类型；提交前运行 `cargo fmt`、`cargo clippy --all-targets --all-features`。
- 前端保持两空格缩进，组件使用 PascalCase，组合式 API；如引入 Prettier 或 ESLint，需在 `package.json` 增加脚本并在提交前执行。
- Git 提交遵循 Conventional Commits（如 `feat: 添加截图调度器`、`fix: 修复帧采样错误`），单个提交聚焦单一主题。
- PR 描述需包含变更背景、主要改动、验证步骤、截图/录屏；涉及权限或文件写入的改动需额外说明安全考量。
- Code Review 关注异步安全、错误处理、资源释放、UI 状态边界；提出改进建议后等待修改再通过。

## 测试与验证
- Rust：为 `capture`、`storage`、`llm` 编写单元或集成测试（`*_test.rs`），并运行 `cargo test`；关键异步逻辑使用 `#[tokio::test]`。
- 前端：引入 Vitest/Cypress 后提供 `npm run test`，覆盖组件渲染、事件交互、API 调用 mock；必要时补充端到端脚本。
- 端到端联调：运行 `npm run tauri dev` 验证截屏→存储→分析→展示闭环；检查 Windows/macOS 打包命令与产物。
- 性能与资源：长时间运行确认截图与清理任务稳定，确保临时文件删除、LLM 失败时给出提示并不中断主流程。

## 自我检查清单
- 依赖、FFmpeg、API Key 配置完成，开发与打包命令全部无错误输出。
- 开发完成后，自我 code review 无误。
- Rust/前端代码通过格式化、静态检查与测试，日志覆盖关键错误路径。
- 前端 UI 功能完整：日历、时间线、详情、设置交互正常，状态更新及时。
- 数据链路可靠：会话与帧记录与文件一致，保留策略与清理任务经过验证。
- LLM 分析具备容错：网络失败时提供错误提示并允许重试或切换提供商。
