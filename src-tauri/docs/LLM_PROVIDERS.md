# LLM提供商配置指南

## 架构说明

系统支持多种LLM提供商，采用两层架构设计：

### 1. 协议层
- **OpenAI协议**: 兼容OpenAI API格式的基础协议实现
- **Anthropic协议**: Claude系列模型的协议实现

### 2. 提供商层
具体的LLM服务提供商，基于协议层实现：

## 支持的提供商

### 阿里通义千问 (Qwen)
- **协议**: OpenAI兼容协议
- **特性**:
  - 支持视频模式（将多张图片作为视频分析）
  - 支持qwen-vl-max-latest等视觉语言模型
  - 更高的token限制（32K）

**配置示例**:
```json
{
  "provider": "qwen",
  "config": {
    "api_key": "your-dashscope-api-key",
    "model": "qwen-vl-max-latest",
    "use_video_mode": true
  }
}
```

### OpenAI
- **协议**: 原生OpenAI协议
- **特性**:
  - 支持GPT-4-Vision等模型
  - 标准的OpenAI API功能

**配置示例**:
```json
{
  "provider": "openai",
  "config": {
    "api_key": "your-openai-api-key",
    "model": "gpt-4-vision-preview",
    "base_url": "https://api.openai.com/v1"
  }
}
```

### Anthropic Claude
- **协议**: Anthropic原生协议
- **特性**:
  - 支持Claude 3系列模型
  - 更大的上下文窗口（200K tokens）

**配置示例**:
```json
{
  "provider": "anthropic",
  "config": {
    "api_key": "your-anthropic-api-key",
    "model": "claude-3-haiku-20240307",
    "base_url": "https://api.anthropic.com"
  }
}
```

## 使用方法

### 1. 切换提供商

```rust
// 在代码中切换
manager.set_active_provider("qwen").await?;
```

### 2. 配置提供商

```rust
// 配置Qwen
manager.configure_provider(
    "qwen",
    json!({
        "api_key": "your-key",
        "model": "qwen-vl-max-latest",
        "use_video_mode": true
    })
).await?;
```

### 3. 数据流程

1. **截屏收集**: 系统以1FPS采集屏幕截图
2. **视频生成**: 将截图序列合成为视频文件
3. **分段分析**:
   - 调用LLM将15分钟视频分成3-5个有意义的segment
   - 每个segment描述一个连贯的活动
4. **Timeline生成**:
   - 基于segments生成timeline活动卡片
   - 包含类别、摘要、干扰等详细信息
5. **数据存储**:
   - LLM调用记录（请求/响应/延迟等）
   - 视频分段记录
   - Timeline卡片（含视频预览路径）

## 扩展新的提供商

要添加新的LLM提供商（如Google Gemini），需要：

1. **创建Provider文件**
```rust
// src/llm/gemini.rs
pub struct GeminiProvider {
    // 实现细节
}

impl LLMProvider for GeminiProvider {
    // 实现trait方法
}
```

2. **注册到Manager**
```rust
// 在LLMManager::new()中注册
providers.insert(
    "gemini".to_string(),
    Box::new(GeminiProvider::new()) as Box<dyn LLMProvider>,
);
```

3. **实现关键方法**
- `analyze_frames()`: 分析截图帧
- `segment_video()`: 视频分段
- `generate_timeline()`: 生成timeline
- `configure()`: 配置provider

## 特殊功能

### Qwen视频模式
Qwen支持将多张图片作为视频处理，这种模式下：
- 图片序列被作为连续的视频帧
- LLM能更好地理解时序关系
- 适合分析连续的屏幕活动

### 视频上传（待实现）
某些提供商支持直接上传视频文件：
- 减少API调用次数
- 更好的时序理解
- 降低传输成本

## 注意事项

1. **API密钥安全**: 确保API密钥不被提交到代码仓库
2. **成本控制**:
   - 采样帧数可配置（默认10-15帧）
   - Token使用量会被记录
3. **错误处理**: 所有API调用都有完整的错误记录和重试机制
4. **性能优化**:
   - 并行处理多个segment
   - 缓存常用的分析结果