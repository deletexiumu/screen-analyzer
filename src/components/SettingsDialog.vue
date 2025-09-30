<!-- 设置对话框组件 -->

<template>
  <el-dialog
    v-model="dialogVisible"
    title="应用设置"
    width="700px"
    @close="handleClose"
    destroy-on-close
  >
    <el-tabs v-model="activeTab">
      <!-- 基础设置 -->
      <el-tab-pane label="基础设置" name="basic">
        <el-form :model="settings" label-width="140px">
          <el-form-item label="数据保留天数">
            <el-input-number
              v-model="settings.retention_days"
              :min="1"
              :max="30"
              :step="1"
            />
            <span class="form-tip">自动清理超过指定天数的数据</span>
          </el-form-item>

          <el-form-item label="截屏间隔">
            <el-input-number
              v-model="settings.capture_interval"
              :min="1"
              :max="60"
              :step="1"
            />
            <span class="form-tip">秒</span>
          </el-form-item>

          <el-form-item label="总结间隔">
            <el-input-number
              v-model="settings.summary_interval"
              :min="5"
              :max="60"
              :step="5"
            />
            <span class="form-tip">分钟</span>
          </el-form-item>

          <el-form-item label="截屏分辨率">
            <el-select v-model="settings.capture_settings.resolution" style="width: 200px">
              <el-option value="1080p" label="1080P (1920×1080)" />
              <el-option value="2k" label="2K (2560×1440)" />
              <el-option value="4k" label="4K (3840×2160)" />
              <el-option value="original" label="原始分辨率" />
            </el-select>
            <span class="form-tip">更高分辨率占用更多存储</span>
          </el-form-item>

          <el-form-item label="图片质量">
            <el-slider
              v-model="settings.capture_settings.image_quality"
              :min="50"
              :max="100"
              :step="5"
              show-input
              style="width: 300px"
            />
            <span class="form-tip">值越高质量越好，文件越大</span>
          </el-form-item>

          <el-form-item label="黑屏检测">
            <el-switch v-model="settings.capture_settings.detect_black_screen" />
            <span class="form-tip">自动跳过锁屏或黑屏时的截图</span>
          </el-form-item>
        </el-form>
      </el-tab-pane>

      <!-- LLM设置 -->
      <el-tab-pane label="AI设置" name="llm">
        <el-form :model="settings" label-width="140px">
          <div class="llm-header">
            <h4 style="margin: 0 0 20px 0; color: #409EFF;">通义千问 (Qwen)</h4>
          </div>

          <el-form-item label="API Key">
            <el-input
              v-model="llmConfig.openai.api_key"
              type="password"
              placeholder="sk-..."
              show-password
            />
            <el-button
              type="primary"
              size="small"
              @click="testLLMAPI"
              :loading="testingAPI"
              style="margin-left: 10px"
            >
              测试连接
            </el-button>
          </el-form-item>

          <el-form-item label="模型">
            <el-select v-model="llmConfig.openai.model">
              <el-option value="qwen-vl-max-latest" label="Qwen VL Max (最新版)" />
              <el-option value="qwen-vl-plus" label="Qwen VL Plus" />
              <el-option value="qwen-vl-max" label="Qwen VL Max" />
            </el-select>
          </el-form-item>

          <el-form-item label="API地址">
            <el-input
              v-model="llmConfig.openai.base_url"
              placeholder="https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
            />
            <span class="form-tip">通常不需要修改</span>
          </el-form-item>
        </el-form>
      </el-tab-pane>

      <!-- 视频设置 -->
      <el-tab-pane label="视频设置" name="video">
        <el-form :model="settings.video_config" label-width="140px">
          <el-form-item label="自动生成视频">
            <el-switch v-model="settings.video_config.auto_generate" />
            <span class="form-tip">自动任务会在每小时的 00、15、30、45 分尝试生成。</span>
          </el-form-item>

          <el-form-item label="测试自动生成">
            <el-button
              type="primary"
              @click="testAutoGenerate"
              :loading="testingVideo"
            >
              <el-icon><VideoCamera /></el-icon>
              生成测试视频
            </el-button>
            <span class="form-tip">优先使用当前选中的会话，若未选择则尝试最近会话。</span>
          </el-form-item>

          <el-form-item label="播放速度倍数">
            <el-slider
              v-model="settings.video_config.speed_multiplier"
              :min="1"
              :max="50"
              :step="1"
              show-input
            />
          </el-form-item>

          <el-form-item label="视频质量">
            <el-slider
              v-model="settings.video_config.quality"
              :min="0"
              :max="51"
              :step="1"
              :format-tooltip="formatQuality"
            />
            <span class="form-tip">值越小质量越好，文件越大</span>
          </el-form-item>

          <el-form-item label="添加时间戳">
            <el-switch v-model="settings.video_config.add_timestamp" />
          </el-form-item>
        </el-form>
      </el-tab-pane>

      <!-- 标签管理 -->
      <el-tab-pane label="标签管理" name="tags">
        <TagManager />
      </el-tab-pane>

      <!-- 存储管理 -->
      <el-tab-pane label="存储管理" name="storage">
        <div class="storage-info">
          <h4>存储使用情况</h4>
          <el-descriptions :column="2" border>
            <el-descriptions-item label="数据库大小">
              {{ store.formattedStorageUsage.database }}
            </el-descriptions-item>
            <el-descriptions-item label="截图文件">
              {{ store.formattedStorageUsage.frames }}
            </el-descriptions-item>
            <el-descriptions-item label="视频文件">
              {{ store.formattedStorageUsage.videos }}
            </el-descriptions-item>
            <el-descriptions-item label="总计">
              {{ store.formattedStorageUsage.total }}
            </el-descriptions-item>
            <el-descriptions-item label="会话数量">
              {{ store.systemStatus.storage_usage.session_count }}
            </el-descriptions-item>
            <el-descriptions-item label="帧数量">
              {{ store.systemStatus.storage_usage.frame_count }}
            </el-descriptions-item>
          </el-descriptions>

          <div class="storage-actions">
            <el-button
              type="warning"
              @click="cleanupStorage"
              :loading="cleaningUp"
            >
              <el-icon><Delete /></el-icon>
              清理过期数据
            </el-button>
            <el-button
              @click="refreshStorageStats"
              :loading="refreshing"
            >
              <el-icon><Refresh /></el-icon>
              刷新统计
            </el-button>
            <el-button
              @click="openStorageFolder('frames')"
            >
              <el-icon><Folder /></el-icon>
              打开截图文件夹
            </el-button>
            <el-button
              @click="openStorageFolder('videos')"
            >
              <el-icon><VideoCamera /></el-icon>
              打开视频文件夹
            </el-button>
            <el-button
              @click="openLogFolder"
              type="info"
            >
              <el-icon><Document /></el-icon>
              打开日志文件夹
            </el-button>
          </div>
        </div>
      </el-tab-pane>

      <!-- 关于 -->
      <el-tab-pane label="关于" name="about">
        <div class="about-content">
          <h3>屏幕活动分析器</h3>
          <p>版本：1.0.0</p>
          <p>基于 Tauri + Vue 3 + Rust 构建</p>

          <h4>功能特性</h4>
          <ul>
            <li>自动屏幕截图（1 FPS）</li>
            <li>AI智能分析活动模式</li>
            <li>生成时间线视频回顾</li>
            <li>活动标签和分类</li>
            <li>自动数据清理</li>
          </ul>

          <h4>技术栈</h4>
          <ul>
            <li>前端：Vue 3 + Element Plus + Vite</li>
            <li>后端：Rust + Tauri 2.x</li>
            <li>数据库：SQLite</li>
            <li>AI：OpenAI / Anthropic Vision API</li>
          </ul>
        </div>
      </el-tab-pane>
    </el-tabs>

    <template #footer>
      <span class="dialog-footer">
        <el-button @click="handleClose">取消</el-button>
        <el-button type="primary" @click="saveSettings" :loading="saving">
          保存设置
        </el-button>
      </span>
    </template>
  </el-dialog>
</template>

<script setup>
import { ref, computed, reactive, watch, onMounted } from 'vue'
import { Delete, Refresh, VideoCamera, Folder, Document } from '@element-plus/icons-vue'
import { useActivityStore } from '../stores/activity'
import { ElMessage, ElMessageBox } from 'element-plus'
import { invoke } from '@tauri-apps/api/core'
import TagManager from './TagManager.vue'

const props = defineProps({
  modelValue: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(['update:modelValue'])

const store = useActivityStore()
const activeTab = ref('basic')
const saving = ref(false)
const cleaningUp = ref(false)
const refreshing = ref(false)
const testingAPI = ref(false)
const testingVideo = ref(false)

const dialogVisible = computed({
  get: () => props.modelValue,
  set: (value) => emit('update:modelValue', value)
})

// 设置数据
const settings = reactive({
  retention_days: 7,
  llm_provider: 'openai',
  capture_interval: 1,
  summary_interval: 15,
  video_config: {
    auto_generate: true,
    speed_multiplier: 4,
    quality: 23,
    add_timestamp: true
  },
  capture_settings: {
    resolution: '1080p',
    image_quality: 85,
    detect_black_screen: true,
    black_screen_threshold: 5
  },
  ui_settings: null
})

// LLM配置
const llmConfig = reactive({
  openai: {
    api_key: '',
    model: 'qwen-vl-max-latest',
    base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions'
  }
})

// 格式化质量提示
const formatQuality = (value) => {
  if (value <= 18) return '最高质量'
  if (value <= 23) return '高质量'
  if (value <= 28) return '标准质量'
  if (value <= 35) return '低质量'
  return '最低质量'
}

// 测试LLM API连接
const testLLMAPI = async () => {
  testingAPI.value = true
  try {
    const provider = 'openai'  // 固定使用openai接口（通义千问兼容）
    const config = llmConfig.openai

    if (!config.api_key) {
      ElMessage.warning('请先填写API Key')
      return
    }

    const result = await invoke('test_llm_api', {
      provider,
      config
    })

    ElMessage({
      message: result,
      type: 'success',
      duration: 5000
    })
  } catch (error) {
    ElMessage.error('API测试失败: ' + error)
  } finally {
    testingAPI.value = false
  }
}

// 保存设置
const saveSettings = async () => {
  saving.value = true
  try {
    const videoConfigPayload = JSON.parse(JSON.stringify(settings.video_config))
    const captureSettingsPayload = JSON.parse(JSON.stringify(settings.capture_settings))
    // 保存基础设置
    await store.updateConfig({
      retention_days: settings.retention_days,
      llm_provider: 'openai',  // 固定使用openai接口
      capture_interval: settings.capture_interval,
      summary_interval: settings.summary_interval,
      video_config: videoConfigPayload,
      capture_settings: captureSettingsPayload,
      ui_settings: settings.ui_settings
    })

    // 配置LLM提供商
    if (settings.llm_provider === 'openai' && llmConfig.openai.api_key) {
      await store.configureLLMProvider('openai', llmConfig.openai)
    } else if (settings.llm_provider === 'anthropic' && llmConfig.anthropic.api_key) {
      await store.configureLLMProvider('anthropic', llmConfig.anthropic)
    }

    ElMessage.success('设置已保存')
    handleClose()
  } catch (error) {
    ElMessage.error('保存设置失败: ' + error)
  } finally {
    saving.value = false
  }
}

// 测试自动生成视频
const testAutoGenerate = async () => {
  testingVideo.value = true
  try {
    const payload = JSON.parse(JSON.stringify(settings.video_config))

    const result = await invoke('test_generate_videos', { settings: payload })
    const generated = Array.isArray(result) ? result.length : 0

    if (generated === 0) {
      ElMessage.info('没有检测到需要生成的视频段')
    } else {
      ElMessage.success(`成功生成 ${generated} 段视频`)
    }

    await Promise.all([
      store.fetchStorageStats(),
      store.fetchDaySessions(store.selectedDate)
    ])
  } catch (error) {
    ElMessage.error('测试生成视频失败: ' + error)
    console.error('Failed to test auto video generation:', error)
  } finally {
    testingVideo.value = false
  }
}

// 清理存储
const cleanupStorage = async () => {
  try {
    await ElMessageBox.confirm(
      '确定要清理过期数据吗？这将删除超过保留期限的所有数据。',
      '清理存储',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }
    )

    cleaningUp.value = true
    await store.cleanupStorage()
    await refreshStorageStats()
    ElMessage.success('清理完成')
  } catch (error) {
    if (error !== 'cancel') {
      console.error('Failed to cleanup storage:', error)
    }
  } finally {
    cleaningUp.value = false
  }
}

// 刷新存储统计
const refreshStorageStats = async () => {
  refreshing.value = true
  try {
    await store.fetchStorageStats()
  } finally {
    refreshing.value = false
  }
}

// 打开存储文件夹
const openStorageFolder = async (folderType) => {
  try {
    await invoke('open_storage_folder', { folderType })
  } catch (error) {
    ElMessage.error('打开文件夹失败: ' + error)
  }
}

// 打开日志文件夹
const openLogFolder = async () => {
  try {
    await invoke('open_log_folder')
    ElMessage.success('已打开日志文件夹')
  } catch (error) {
    ElMessage.error('打开日志文件夹失败: ' + error)
  }
}

// 关闭对话框
const handleClose = () => {
  dialogVisible.value = false
}

// 初始化设置
const initSettings = () => {
  const { video_config, llm_config, capture_settings, ...rest } = store.appConfig
  Object.assign(settings, rest)
  if (video_config) {
    Object.assign(settings.video_config, video_config)
  }
  if (capture_settings) {
    Object.assign(settings.capture_settings, capture_settings)
  }
  // 加载LLM配置
  if (llm_config) {
    llmConfig.openai.api_key = llm_config.api_key || ''
    llmConfig.openai.model = llm_config.model || 'qwen-vl-max-latest'
    llmConfig.openai.base_url = llm_config.base_url || 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions'
  }
}

// 监听对话框打开
watch(dialogVisible, (newVal) => {
  if (newVal) {
    initSettings()
    refreshStorageStats()
  }
})

onMounted(() => {
  // 不再需要获取提供商列表
  // store.fetchLLMProviders()
})
</script>

<style scoped>
.form-tip {
  margin-left: 10px;
  color: #909399;
  font-size: 12px;
}

.storage-info {
  padding: 20px;
}

.storage-info h4 {
  margin-bottom: 20px;
  color: #303133;
}

.storage-actions {
  margin-top: 20px;
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}

.about-content {
  padding: 20px;
  line-height: 1.8;
}

.about-content h3 {
  color: #303133;
  margin-bottom: 10px;
}

.about-content h4 {
  color: #606266;
  margin-top: 20px;
  margin-bottom: 10px;
}

.about-content ul {
  list-style-position: inside;
  color: #909399;
}

.about-content li {
  margin-bottom: 5px;
}

:deep(.el-tabs__content) {
  min-height: 400px;
}
</style>
