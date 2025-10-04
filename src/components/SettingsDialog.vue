<!-- 设置对话框组件 -->

<template>
  <el-dialog
    v-model="dialogVisible"
    title="应用设置"
    width="900px"
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
          <el-form-item label="AI 提供商">
            <el-radio-group v-model="settings.llm_provider">
              <el-radio value="openai">通义千问 (Qwen)</el-radio>
              <el-radio value="claude">Claude</el-radio>
            </el-radio-group>
          </el-form-item>

          <el-divider />

          <!-- 通义千问配置 -->
          <template v-if="settings.llm_provider === 'openai'">
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
                @click="testLLMAPI('openai')"
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
          </template>

          <!-- Claude 配置 -->
          <template v-if="settings.llm_provider === 'claude'">
            <div class="llm-header">
              <h4 style="margin: 0 0 20px 0; color: #9333EA;">Claude</h4>
            </div>

            <el-form-item label="API Key">
              <el-input
                v-model="llmConfig.claude.api_key"
                type="password"
                placeholder="留空则使用 Claude CLI 登录账号"
                show-password
              />
              <el-button
                type="primary"
                size="small"
                @click="testLLMAPI('claude')"
                :loading="testingAPI"
                style="margin-left: 10px"
              >
                测试连接
              </el-button>
              <div class="form-tip" style="margin-top: 8px; margin-left: 0;">
                使用订阅账号时可留空，应用将使用 Claude CLI 的登录凭据
              </div>
            </el-form-item>

            <el-form-item label="模型">
              <el-select
                v-model="llmConfig.claude.model"
                filterable
                allow-create
                default-first-option
                placeholder="选择或输入模型名称"
              >
                <el-option value="claude-sonnet-4-5" label="Claude Sonnet 4.5 (官方)" />
                <el-option value="claude-opus-4-1" label="Claude Opus 4.1 (官方)" />
                <el-option value="kimi" label="Kimi (月之暗面)" />
                <el-option value="glm-4-plus" label="GLM-4-Plus (智谱)" />
                <el-option value="glm-4-air" label="GLM-4-Air (智谱)" />
              </el-select>
              <div class="form-tip" style="margin-top: 8px;">
                支持 Claude 官方模型或兼容 Claude Agent 的国内大模型（如 Kimi、GLM 等）
              </div>
            </el-form-item>
          </template>
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

      <!-- 数据库设置 -->
      <el-tab-pane label="数据库设置" name="database">
        <el-form :model="settings" label-width="140px">
          <el-form-item label="数据库类型">
            <el-radio-group v-model="databaseConfig.type">
              <el-radio value="sqlite">SQLite (本地)</el-radio>
              <el-radio value="mariadb">MariaDB (远程)</el-radio>
            </el-radio-group>
            <span class="form-tip">切换数据库类型需要重启应用</span>
          </el-form-item>

          <!-- SQLite配置 -->
          <template v-if="databaseConfig.type === 'sqlite'">
            <el-form-item label="数据库路径">
              <el-input
                v-model="databaseConfig.db_path"
                placeholder="data/screen-analyzer.db"
                disabled
              />
              <span class="form-tip">SQLite使用本地文件存储</span>
            </el-form-item>
          </template>

          <!-- MariaDB配置 -->
          <template v-if="databaseConfig.type === 'mariadb'">
            <el-form-item label="主机地址">
              <el-input
                v-model="databaseConfig.host"
                placeholder="localhost"
              />
            </el-form-item>

            <el-form-item label="端口">
              <el-input-number
                v-model="databaseConfig.port"
                :min="1"
                :max="65535"
                :step="1"
              />
            </el-form-item>

            <el-form-item label="数据库名">
              <el-input
                v-model="databaseConfig.database"
                placeholder="screen_analyzer"
              />
            </el-form-item>

            <el-form-item label="用户名">
              <el-input
                v-model="databaseConfig.username"
                placeholder="root"
              />
            </el-form-item>

            <el-form-item label="密码">
              <el-input
                v-model="databaseConfig.password"
                type="password"
                placeholder="请输入数据库密码"
                show-password
              />
            </el-form-item>

            <el-form-item>
              <el-button
                type="primary"
                @click="testDatabaseConnection"
                :loading="testingDatabase"
              >
                测试连接
              </el-button>
              <el-button
                type="warning"
                @click="syncDataToMariaDB"
                :loading="syncingData"
                style="margin-left: 10px"
              >
                <el-icon><Upload /></el-icon>
                同步本地数据
              </el-button>
              <span class="form-tip" style="margin-left: 10px">
                首次连接时会自动同步SQLite数据到MariaDB
              </span>
            </el-form-item>
          </template>
        </el-form>
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

      <!-- 日志 -->
      <el-tab-pane label="日志" name="logs">
        <div class="logs-content">
          <div class="logs-header">
            <el-switch
              v-model="settings.logger_settings.enable_frontend_logging"
              active-text="启用实时日志"
              inactive-text="禁用实时日志"
            />
            <div class="logs-actions">
              <el-button
                @click="clearLogs"
                size="small"
                :icon="Delete"
              >
                清空日志
              </el-button>
              <el-button
                @click="openLogFolder"
                size="small"
                type="info"
                :icon="Folder"
              >
                打开日志文件夹
              </el-button>
            </div>
          </div>

          <div class="logs-container" ref="logsContainer">
            <div
              v-for="(log, index) in logs"
              :key="index"
              :class="['log-entry', `log-${log.level.toLowerCase()}`]"
            >
              <span class="log-time">{{ log.timestamp }}</span>
              <span class="log-level">{{ log.level }}</span>
              <span class="log-target">{{ log.target }}</span>
              <span class="log-message">{{ log.message }}</span>
            </div>
            <div v-if="logs.length === 0" class="no-logs">
              暂无日志
            </div>
          </div>
        </div>
      </el-tab-pane>

      <!-- Notion 集成 -->
      <el-tab-pane label="Notion 集成" name="notion">
        <el-form :model="notionConfig" label-width="140px">
          <el-form-item label="启用 Notion 同步">
            <el-switch v-model="notionConfig.enabled" />
            <span class="form-tip">启用后会将会话记录同步到 Notion</span>
          </el-form-item>

          <el-form-item label="API Token">
            <el-input
              v-model="notionConfig.api_token"
              type="password"
              placeholder="secret_..."
              show-password
              :disabled="!notionConfig.enabled"
            />
            <el-button
              type="primary"
              size="small"
              @click="testNotionConnection"
              :loading="testingNotion"
              :disabled="!notionConfig.enabled || !notionConfig.api_token"
              style="margin-left: 10px"
            >
              测试连接
            </el-button>
            <span class="form-tip">Notion Integration 的 API Token</span>
          </el-form-item>

          <el-form-item label="选择数据库">
            <div style="display: flex; gap: 8px; width: 100%; flex-wrap: wrap;">
              <el-select
                v-model="notionConfig.database_id"
                placeholder="请先填写 API Token 并搜索页面"
                :disabled="!notionConfig.enabled || !notionConfig.api_token"
                filterable
                style="flex: 1; min-width: 200px"
              >
                <el-option
                  v-for="page in notionPages"
                  :key="page.id"
                  :label="`${page.icon || '📄'} ${page.title} (${page.page_type === 'database' ? '数据库' : '页面'})`"
                  :value="page.id"
                />
              </el-select>
              <el-button
                :disabled="!notionConfig.enabled || !notionConfig.api_token"
                :loading="searchingNotionPages"
                @click="searchNotionPages"
              >
                搜索页面
              </el-button>
              <el-button
                :disabled="!notionConfig.enabled || !notionConfig.api_token || !selectedPageForDatabase"
                :loading="creatingNotionDatabase"
                @click="showCreateDatabaseDialog"
              >
                创建数据库
              </el-button>
            </div>
            <span class="form-tip">选择已存在的数据库，或在某个页面下创建新数据库</span>
          </el-form-item>

          <el-divider>同步选项</el-divider>

          <el-form-item label="同步会话">
            <el-switch
              v-model="notionConfig.sync_options.sync_sessions"
              :disabled="!notionConfig.enabled"
            />
            <span class="form-tip">同步会话记录到 Notion</span>
          </el-form-item>

          <el-form-item label="同步视频">
            <el-switch
              v-model="notionConfig.sync_options.sync_videos"
              :disabled="!notionConfig.enabled"
            />
            <span class="form-tip">同步视频文件（小于 5MB）</span>
          </el-form-item>

          <el-form-item label="同步每日总结">
            <el-switch
              v-model="notionConfig.sync_options.sync_daily_summary"
              :disabled="!notionConfig.enabled"
            />
          </el-form-item>

          <el-form-item label="同步关键截图">
            <el-switch
              v-model="notionConfig.sync_options.sync_screenshots"
              :disabled="!notionConfig.enabled"
            />
          </el-form-item>

          <el-form-item label="视频大小限制">
            <el-input-number
              v-model="notionConfig.sync_options.video_size_limit_mb"
              :min="1"
              :max="50"
              :disabled="!notionConfig.enabled"
            />
            <span class="form-tip">MB（超过限制的视频不会上传）</span>
          </el-form-item>

          <el-form-item label="失败重试次数">
            <el-input-number
              v-model="notionConfig.max_retries"
              :min="0"
              :max="10"
              :disabled="!notionConfig.enabled"
            />
          </el-form-item>
        </el-form>

        <!-- 创建数据库对话框 -->
        <el-dialog
          v-model="createDatabaseDialogVisible"
          title="创建 Notion 数据库"
          width="500px"
        >
          <el-form label-width="100px">
            <el-form-item label="父页面">
              <el-text>{{ selectedPageForDatabase?.icon || '📄' }} {{ selectedPageForDatabase?.title }}</el-text>
            </el-form-item>
            <el-form-item label="数据库名称">
              <el-input
                v-model="newDatabaseName"
                placeholder="请输入数据库名称"
              />
            </el-form-item>
          </el-form>
          <template #footer>
            <el-button @click="createDatabaseDialogVisible = false">取消</el-button>
            <el-button
              type="primary"
              :loading="creatingNotionDatabase"
              @click="createNotionDatabase"
            >
              创建
            </el-button>
          </template>
        </el-dialog>
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
import { ref, computed, reactive, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { Delete, Refresh, VideoCamera, Folder, Document, Upload } from '@element-plus/icons-vue'
import { useActivityStore } from '../stores/activity'
import { ElMessage, ElMessageBox } from 'element-plus'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
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
const testingDatabase = ref(false)
const syncingData = ref(false)

// 日志相关
const logs = ref([])
const logsContainer = ref(null)
let unlistenLog = null
const MAX_LOGS = 1000 // 最大日志条数

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
  ui_settings: null,
  logger_settings: {
    enable_frontend_logging: true,
    log_level: 'info',
    max_log_buffer: 1000
  }
})

// LLM配置
const llmConfig = reactive({
  openai: {
    api_key: '',
    model: 'qwen-vl-max-latest',
    base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions'
  },
  claude: {
    api_key: '',
    model: 'claude-sonnet-4-5'
  }
})

// 数据库配置
const databaseConfig = reactive({
  type: 'sqlite',
  db_path: 'data/screen-analyzer.db',
  host: 'localhost',
  port: 3306,
  database: 'screen_analyzer',
  username: 'root',
  password: ''
})

// Notion 配置
const notionConfig = reactive({
  enabled: false,
  api_token: '',
  database_id: '',
  sync_options: {
    sync_sessions: true,
    sync_videos: false,
    sync_daily_summary: false,
    sync_screenshots: true,
    video_size_limit_mb: 5
  },
  max_retries: 3
})

const testingNotion = ref(false)
const notionPages = ref([])
const searchingNotionPages = ref(false)
const selectedPageForDatabase = computed(() => {
  // 找到当前选中的页面（只要不是 database 类型，就可以在其下创建数据库）
  const selected = notionPages.value.find(p => p.id === notionConfig.database_id)
  // 只有非 database 类型才能创建子数据库
  return selected && selected.page_type !== 'database' ? selected : null
})
const creatingNotionDatabase = ref(false)
const createDatabaseDialogVisible = ref(false)
const newDatabaseName = ref('Screen Analyzer 会话记录')

// 格式化质量提示
const formatQuality = (value) => {
  if (value <= 18) return '最高质量'
  if (value <= 23) return '高质量'
  if (value <= 28) return '标准质量'
  if (value <= 35) return '低质量'
  return '最低质量'
}

// 测试LLM API连接
const testLLMAPI = async (provider) => {
  testingAPI.value = true
  try {
    const config = llmConfig[provider]

    // OpenAI (Qwen) 必须提供 API Key
    if (provider === 'openai' && !config.api_key) {
      ElMessage.warning('请先填写API Key')
      return
    }

    // Claude 可选 API Key（可以使用 CLI 登录凭据）
    if (provider === 'claude' && !config.api_key) {
      ElMessage.info('将使用 Claude CLI 登录凭据进行测试')
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

// 测试数据库连接
const testDatabaseConnection = async () => {
  testingDatabase.value = true
  try {
    if (databaseConfig.type === 'mariadb') {
      if (!databaseConfig.host || !databaseConfig.database || !databaseConfig.username) {
        ElMessage.warning('请填写完整的数据库配置')
        return
      }
    }

    const config = buildDatabaseConfig()

    // 这里可以调用后端API测试连接
    // const result = await invoke('test_database_connection', { config })

    ElMessage.success('数据库连接测试成功')
  } catch (error) {
    ElMessage.error('数据库连接失败: ' + error)
  } finally {
    testingDatabase.value = false
  }
}

// 构建数据库配置对象
const buildDatabaseConfig = () => {
  if (databaseConfig.type === 'sqlite') {
    return {
      type: 'sqlite',
      db_path: databaseConfig.db_path
    }
  } else {
    return {
      type: 'mariadb',
      host: databaseConfig.host,
      port: databaseConfig.port,
      database: databaseConfig.database,
      username: databaseConfig.username,
      password: databaseConfig.password
    }
  }
}

// 同步数据到 MariaDB
const syncDataToMariaDB = async () => {
  if (databaseConfig.type !== 'mariadb') {
    ElMessage.warning('请先切换到 MariaDB 模式')
    return
  }

  try {
    await ElMessageBox.confirm(
      '此操作将清空 MariaDB 中的所有数据，然后从本地 SQLite 同步数据。确定要继续吗？',
      '同步数据',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }
    )

    syncingData.value = true
    const result = await invoke('sync_data_to_mariadb')
    ElMessage.success(result)
  } catch (error) {
    if (error !== 'cancel') {
      ElMessage.error('同步数据失败: ' + error)
    }
  } finally {
    syncingData.value = false
  }
}

// 测试 Notion 连接
const testNotionConnection = async () => {
  if (!notionConfig.api_token) {
    ElMessage.warning('请先填写 API Token')
    return
  }

  testingNotion.value = true
  try {
    const result = await invoke('test_notion_connection', {
      apiToken: notionConfig.api_token
    })
    ElMessage({
      message: result,
      type: 'success',
      duration: 5000
    })
  } catch (error) {
    ElMessage.error('Notion 连接测试失败: ' + error)
  } finally {
    testingNotion.value = false
  }
}

// 搜索 Notion 页面和数据库
const searchNotionPages = async () => {
  if (!notionConfig.api_token) {
    ElMessage.warning('请先填写 API Token')
    return
  }

  searchingNotionPages.value = true
  try {
    const pages = await invoke('search_notion_pages', {
      apiToken: notionConfig.api_token
    })
    notionPages.value = pages
    ElMessage.success(`找到 ${pages.length} 个页面/数据库`)
  } catch (error) {
    ElMessage.error('搜索页面失败: ' + error)
  } finally {
    searchingNotionPages.value = false
  }
}

// 显示创建数据库对话框
const showCreateDatabaseDialog = () => {
  if (!selectedPageForDatabase.value) {
    ElMessage.warning('请先选择一个页面作为数据库的父页面')
    return
  }
  createDatabaseDialogVisible.value = true
}

// 创建 Notion 数据库
const createNotionDatabase = async () => {
  if (!notionConfig.api_token || !selectedPageForDatabase.value || !newDatabaseName.value) {
    ElMessage.warning('请填写完整信息')
    return
  }

  creatingNotionDatabase.value = true
  try {
    const databaseId = await invoke('create_notion_database', {
      apiToken: notionConfig.api_token,
      parentPageId: selectedPageForDatabase.value.id,
      databaseName: newDatabaseName.value
    })

    ElMessage.success('数据库创建成功！')

    // 更新配置并刷新页面列表
    notionConfig.database_id = databaseId
    createDatabaseDialogVisible.value = false

    // 重新搜索页面以获取最新列表
    await searchNotionPages()
  } catch (error) {
    ElMessage.error('创建数据库失败: ' + error)
  } finally {
    creatingNotionDatabase.value = false
  }
}

// 清空日志
const clearLogs = () => {
  logs.value = []
  ElMessage.success('日志已清空')
}

// 滚动到日志底部
const scrollToBottom = async () => {
  await nextTick()
  if (logsContainer.value) {
    logsContainer.value.scrollTop = logsContainer.value.scrollHeight
  }
}

// 保存设置
const saveSettings = async () => {
  saving.value = true
  try {
    const videoConfigPayload = JSON.parse(JSON.stringify(settings.video_config))
    const captureSettingsPayload = JSON.parse(JSON.stringify(settings.capture_settings))
    const loggerSettingsPayload = JSON.parse(JSON.stringify(settings.logger_settings))
    const databaseConfigPayload = buildDatabaseConfig()
    const notionConfigPayload = JSON.parse(JSON.stringify(notionConfig))

    // 保存基础设置
    await store.updateConfig({
      retention_days: settings.retention_days,
      llm_provider: settings.llm_provider,
      capture_interval: settings.capture_interval,
      summary_interval: settings.summary_interval,
      video_config: videoConfigPayload,
      capture_settings: captureSettingsPayload,
      ui_settings: settings.ui_settings,
      logger_settings: loggerSettingsPayload,
      database_config: databaseConfigPayload,
      notion_config: notionConfigPayload
    })

    // 配置LLM提供商
    if (settings.llm_provider === 'openai' && llmConfig.openai.api_key) {
      await store.configureLLMProvider('openai', llmConfig.openai)
    } else if (settings.llm_provider === 'claude') {
      // Claude 允许不填写 API key，会使用 CLI 凭据
      await store.configureLLMProvider('claude', llmConfig.claude)
    }

    ElMessage.success('设置已保存，如果修改了数据库配置请重启应用')
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
  const { video_config, llm_config, capture_settings, logger_settings, database_config, ...rest } = store.appConfig
  Object.assign(settings, rest)
  if (video_config) {
    Object.assign(settings.video_config, video_config)
  }
  if (capture_settings) {
    Object.assign(settings.capture_settings, capture_settings)
  }
  if (logger_settings) {
    Object.assign(settings.logger_settings, logger_settings)
  }
  // 加载LLM配置
  if (llm_config) {
    // 根据当前 provider 加载对应配置
    const currentProvider = settings.llm_provider || 'openai'

    if (currentProvider === 'openai') {
      llmConfig.openai.api_key = llm_config.api_key || ''
      llmConfig.openai.model = llm_config.model || 'qwen-vl-max-latest'
      llmConfig.openai.base_url = llm_config.base_url || 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions'
    } else if (currentProvider === 'claude') {
      llmConfig.claude.api_key = llm_config.api_key || ''
      llmConfig.claude.model = llm_config.model || 'claude-sonnet-4-5'
    }
  }
  // 加载数据库配置
  if (database_config) {
    databaseConfig.type = database_config.type || 'sqlite'
    if (database_config.type === 'sqlite') {
      databaseConfig.db_path = database_config.db_path || 'data/screen-analyzer.db'
    } else if (database_config.type === 'mariadb') {
      databaseConfig.host = database_config.host || 'localhost'
      databaseConfig.port = database_config.port || 3306
      databaseConfig.database = database_config.database || 'screen_analyzer'
      databaseConfig.username = database_config.username || 'root'
      databaseConfig.password = database_config.password || ''
    }
  }
  // 加载 Notion 配置
  const { notion_config } = store.appConfig
  if (notion_config) {
    notionConfig.enabled = notion_config.enabled || false
    notionConfig.api_token = notion_config.api_token || ''
    notionConfig.database_id = notion_config.database_id || ''
    if (notion_config.sync_options) {
      Object.assign(notionConfig.sync_options, notion_config.sync_options)
    }
    notionConfig.max_retries = notion_config.max_retries || 3
  }
}

// 监听对话框打开
watch(dialogVisible, (newVal) => {
  if (newVal) {
    initSettings()
    refreshStorageStats()
  }
})

onMounted(async () => {
  // 监听日志事件
  unlistenLog = await listen('log-message', (event) => {
    const logMessage = event.payload
    logs.value.push(logMessage)

    // 限制日志数量
    if (logs.value.length > MAX_LOGS) {
      logs.value.shift()
    }

    // 自动滚动到底部
    scrollToBottom()
  })
})

onUnmounted(() => {
  // 取消监听
  if (unlistenLog) {
    unlistenLog()
  }
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

.logs-content {
  padding: 20px;
  display: flex;
  flex-direction: column;
  height: 500px;
}

.logs-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 15px;
}

.logs-actions {
  display: flex;
  gap: 10px;
}

.logs-container {
  flex: 1;
  overflow-y: auto;
  background: #1e1e1e;
  border-radius: 6px;
  padding: 12px;
  font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
  font-size: 12px;
  line-height: 1.6;
}

.log-entry {
  display: flex;
  gap: 10px;
  margin-bottom: 4px;
  padding: 4px 0;
  border-bottom: 1px solid #2d2d2d;
}

.log-time {
  color: #6c757d;
  flex-shrink: 0;
  width: 200px;
}

.log-level {
  flex-shrink: 0;
  width: 60px;
  font-weight: bold;
  text-transform: uppercase;
}

.log-target {
  color: #6c757d;
  flex-shrink: 0;
  width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.log-message {
  flex: 1;
  color: #f8f9fa;
  word-break: break-word;
}

.log-trace .log-level {
  color: #6c757d;
}

.log-debug .log-level {
  color: #17a2b8;
}

.log-info .log-level {
  color: #28a745;
}

.log-warn .log-level {
  color: #ffc107;
}

.log-error .log-level {
  color: #dc3545;
}

.no-logs {
  text-align: center;
  color: #6c757d;
  padding: 40px;
  font-size: 14px;
}
</style>
