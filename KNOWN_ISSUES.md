# 已知问题

## Windows Release 版本 Claude 调用黑框

### 问题描述

在 Windows 系统的 Release 版本中，使用 Claude CLI 订阅账号时，会出现临时的黑色控制台窗口闪烁。这个窗口是 `claude-agent-sdk` 库创建 Claude 子进程时产生的。

### 影响范围

- **仅影响**: Windows Release 版本 + 使用订阅账号（无 API Key）
- **不影响**:
  - Dev 开发环境
  - macOS 系统
  - 使用 API Key 的配置

### 原因

`claude-agent-sdk` 的 `SubprocessTransport` 在 Windows 下创建子进程时，未设置 `CREATE_NO_WINDOW` 标志，导致会显示控制台窗口。这是外部依赖库的限制。

### 解决方案

#### 方案1: 使用 API Key（推荐）

如果有 Anthropic API Key，可以在设置中配置：

1. 打开应用设置
2. 进入 "AI设置" → "Claude"
3. 填写您的 API Key
4. 保存设置

使用 API Key 后，将通过 HTTP API 调用，不会出现黑框。

#### 方案2: 接受此限制

如果使用订阅账号，黑框会在调用 Claude 时短暂出现，但不影响功能。窗口会在 API 调用完成后自动关闭。

### 技术细节

相关代码位置：
- `src-tauri/src/lib.rs:test_claude_sdk_api()` - 测试连接
- `src-tauri/src/llm/claude.rs:call_claude_api()` - 实际调用

我们已经尝试的解决方案：
- ✅ 在应用启动时设置全局 `ANTHROPIC_API_KEY` 环境变量
- ❌ 无法直接控制 `claude-agent-sdk` 创建进程的方式（外部库）
- ⏳ 已提交 Issue 到上游项目

### 未来改进

我们正在关注以下可能的改进：
1. 等待 `claude-agent-sdk` 添加 Windows CREATE_NO_WINDOW 支持
2. 探索使用 HTTP Transport 替代 Subprocess Transport
3. 考虑 fork 并修改 `claude-agent-sdk`

---

## 环境变量继承问题（已修复）

### 问题描述 ✅

~~Release 版本无法继承系统环境变量中的 Claude CLI 会话令牌。~~

### 修复方案

已在应用启动时显式读取 Claude CLI 配置文件并设置全局环境变量。

相关代码：
- `src-tauri/src/lib.rs:read_claude_cli_session_token()` - 读取配置
- `src-tauri/src/lib.rs:run()` - 启动时设置环境变量

---

如有其他问题，请访问 [GitHub Issues](https://github.com/your-repo/issues)
