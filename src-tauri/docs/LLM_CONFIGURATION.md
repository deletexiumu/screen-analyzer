# LLM配置指南

## 系统架构

系统使用阿里通义千问（Qwen）作为AI分析服务，提供屏幕活动分析、视频分段和时间线生成功能。

## Qwen配置

### 基本配置

```json
{
  "llm": {
    "qwen": {
      "api_key": "your-dashscope-api-key"
    }
  }
}
```

### 完整配置选项

```json
{
  "llm": {
    "qwen": {
      "api_key": "your-dashscope-api-key",
      "model": "qwen-vl-max-latest",  // 默认值
      "base_url": "https://dashscope.aliyuncs.com/compatible-mode/v1",  // 默认值
      "use_video_mode": true  // 默认值，将图片序列作为视频处理
    },
    "analysis_params": {
      "frame_sampling_interval": 30,  // 帧采样间隔（秒）
      "max_frames_per_analysis": 15,  // 最大分析帧数
      "include_detailed_description": true,
      "confidence_threshold": 0.5
    }
  }
}
```

### 配置说明

#### Qwen参数
- `api_key`: 必需，从阿里云DashScope获取
- `model`: 可选，默认使用最新的视觉语言模型
- `base_url`: 可选，一般不需要修改
- `use_video_mode`: 可选，启用后将多张图片作为连续视频帧处理，提供更好的时序理解

#### 分析参数
- `frame_sampling_interval`: 控制分析的采样密度
- `max_frames_per_analysis`: 限制每次分析的最大帧数，控制成本

## API密钥获取

1. 访问 [阿里云DashScope控制台](https://dashscope.console.aliyun.com/)
2. 创建API Key
3. 将密钥配置到系统中

## 使用方法

### 通过Tauri命令配置

```javascript
// 在前端调用
await invoke('configure_qwen', {
  config: {
    api_key: 'sk-xxxxx',
    model: 'qwen-vl-max-latest',
    use_video_mode: true
  }
});
```

### 功能特性

1. **视频分段分析**
   - 将15分钟的屏幕录制分成3-5个有意义的活动段
   - 每个段落包含时间戳和活动描述

2. **时间线生成**
   - 基于视频分段生成详细的活动时间线
   - 包含活动分类、摘要、使用的应用等信息

3. **智能分析**
   - 自动识别工作、休息、会议等不同活动类型
   - 检测并记录干扰和上下文切换

## 数据流程

```
截屏采集 (1FPS)
    ↓
视频生成 (可选)
    ↓
Qwen分析
    ├── 视频分段
    └── 时间线生成
    ↓
数据存储
    ├── LLM调用记录
    ├── 视频分段
    └── 时间线卡片
```

## 性能优化

- **采样策略**: 智能采样关键帧，减少API调用
- **并行处理**: 分段和时间线生成可并行执行
- **缓存机制**: 相似内容避免重复分析
- **批量处理**: 支持批量分析多个会话

## 错误处理

所有LLM调用都会记录到数据库，包括：
- 请求和响应内容
- 调用延迟
- Token使用量
- 错误信息

便于问题排查和成本分析。

## 注意事项

1. **成本控制**: 合理设置采样间隔和最大帧数
2. **隐私安全**: API密钥不要提交到代码仓库
3. **网络要求**: 需要稳定的网络连接到阿里云服务