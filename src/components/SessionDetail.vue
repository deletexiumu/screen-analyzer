<!-- 会话详情组件 - 显示会话的详细信息 -->

<template>
  <el-dialog
    v-model="dialogVisible"
    :title="session?.session?.title || '会话详情'"
    width="80%"
    :before-close="handleClose"
    destroy-on-close
  >
    <div class="session-detail" v-loading="store.loading.sessionDetail">
      <el-descriptions :column="2" border v-if="session">
        <el-descriptions-item label="开始时间">
          {{ formatDateTime(session.session.start_time) }}
        </el-descriptions-item>
        <el-descriptions-item label="结束时间">
          {{ formatDateTime(session.session.end_time) }}
        </el-descriptions-item>
        <el-descriptions-item label="持续时长">
          {{ formatDuration(session.session.start_time, session.session.end_time) }}
        </el-descriptions-item>
        <el-descriptions-item label="帧数">
          {{ session.frames?.length || 0 }} 帧
        </el-descriptions-item>
        <el-descriptions-item label="摘要" :span="2">
          {{ session.session.summary }}
        </el-descriptions-item>
      </el-descriptions>

      <!-- 标签管理 -->
      <div class="tags-section" v-if="session">
        <h4>活动标签</h4>
        <div class="tags-list">
          <el-tag
            v-for="(tag, index) in session.tags"
            :key="index"
            :color="getCategoryColor(tag.category)"
            effect="dark"
            closable
            @close="removeTag(index)"
          >
            {{ getCategoryName(tag.category) }}
            <el-badge :value="`${Math.round(tag.confidence * 100)}%`" />
          </el-tag>
          <el-button
            size="small"
            @click="showAddTag = true"
          >
            + 添加标签
          </el-button>
        </div>

        <!-- 关键词 -->
        <div class="keywords" v-if="allKeywords.length > 0">
          <span class="keyword-label">关键词：</span>
          <el-tag
            v-for="keyword in allKeywords"
            :key="keyword"
            size="small"
            effect="plain"
          >
            {{ keyword }}
          </el-tag>
        </div>
      </div>

      <!-- 关键时刻 -->
      <div class="key-moments-section" v-if="session && keyMoments.length > 0">
        <h4>关键时刻</h4>
        <el-timeline>
          <el-timeline-item
            v-for="(moment, index) in keyMoments"
            :key="index"
            :timestamp="moment.time"
            placement="top"
            :type="getImportanceType(moment.importance)"
          >
            {{ moment.description }}
          </el-timeline-item>
        </el-timeline>
      </div>

      <!-- 评分 -->
      <div class="scores-section" v-if="session && (productivityScore || focusScore)">
        <h4>评分</h4>
        <div class="scores">
          <div class="score-item" v-if="productivityScore">
            <span>生产力评分：</span>
            <el-progress
              :percentage="productivityScore"
              :color="getScoreColor"
            />
          </div>
          <div class="score-item" v-if="focusScore">
            <span>专注度评分：</span>
            <el-progress
              :percentage="focusScore"
              :color="getScoreColor"
            />
          </div>
        </div>
      </div>

      <!-- 视频播放器或帧预览 -->
      <div class="media-section" v-if="session">
        <!-- 如果有视频，显示视频播放器 -->
        <div v-if="session.session.video_path" class="video-section">
          <h4>会话视频</h4>
          <div class="video-container" v-if="isTauriEnv">
            <video
              ref="videoPlayer"
              :src="videoUrl"
              controls
              preload="metadata"
              width="100%"
              style="max-width: 800px; border-radius: 4px; box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);"
              @error="handleVideoError"
              @loadstart="onVideoLoadStart"
              @loadeddata="onVideoLoadedData"
            >
              您的浏览器不支持视频播放
            </video>
          </div>
          <el-alert
            v-else
            title="视频预览不可用"
            type="warning"
            :closable="false"
            show-icon
          >
            视频文件位于：{{ session.session.video_path }}<br>
            请使用 Tauri 应用查看（运行 npm run tauri dev 或使用打包后的应用）
          </el-alert>
        </div>

        <!-- 如果没有视频，显示帧预览 -->
        <div v-else-if="session.frames?.length > 0" class="frames-section">
          <h4>截图预览（采样显示）</h4>
          <div class="frames-gallery">
            <div
              v-for="(frame, index) in sampledFrames"
              :key="index"
              class="frame-item"
              :class="{ 'no-animation': isWindows }"
              @click="previewFrame(frame)"
            >
              <div class="frame-loading" v-if="loadingImages[index]">
                <el-icon class="is-loading"><Loading /></el-icon>
              </div>
              <img
                v-show="!loadingImages[index]"
                :src="getConvertedPath(frame.file_path)"
                :alt="`Frame ${index + 1}`"
                @load="handleImageLoad(index)"
                @error="handleImageError($event, index)"
                loading="lazy"
              />
              <div class="frame-time">
                {{ formatTime(frame.timestamp) }}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <template #footer>
      <span class="dialog-footer">
        <el-button @click="handleClose">关闭</el-button>
        <el-button
          v-if="session?.session?.video_path"
          type="warning"
          :loading="isProcessing"
          @click="retryAnalysis"
        >
          <el-icon><Refresh /></el-icon>
          重新解析
        </el-button>
        <el-button
          v-if="session?.session?.video_path"
          type="primary"
          @click="playVideo"
        >
          <el-icon><VideoPlay /></el-icon>
          播放视频
        </el-button>
        <el-button
          v-else
          type="primary"
          @click="generateVideo"
        >
          <el-icon><VideoCamera /></el-icon>
          生成视频
        </el-button>
      </span>
    </template>

    <!-- 添加标签对话框 -->
    <AddTagDialog
      v-model:visible="showAddTag"
      @confirm="addTag"
    />

    <!-- 图片预览 -->
    <el-image-viewer
      v-if="previewUrl"
      :url-list="[previewUrl]"
      @close="previewUrl = null"
    />
  </el-dialog>
</template>

<script setup>
import { ref, computed, watch, onMounted, onUnmounted, reactive } from 'vue'
import { VideoPlay, VideoCamera, Refresh, Loading } from '@element-plus/icons-vue'
import { useActivityStore } from '../stores/activity'
import dayjs from 'dayjs'
import { ElMessage } from 'element-plus'
import { convertFileSrc } from '@tauri-apps/api/core'
import { invoke } from '@tauri-apps/api/core'
import AddTagDialog from './AddTagDialog.vue'

const props = defineProps({
  modelValue: {
    type: Boolean,
    default: false
  },
  sessionId: {
    type: Number,
    default: null
  }
})

const emit = defineEmits(['update:modelValue', 'close'])

const store = useActivityStore()
const showAddTag = ref(false)
const previewUrl = ref(null)
const isProcessing = computed(() => store.systemStatus.is_processing)
const videoPlayer = ref(null)
const loadingImages = reactive({})
const isWindows = ref(false)
const videoUrl = ref(null)
const isTauriEnv = ref(false)

const dialogVisible = computed({
  get: () => props.modelValue,
  set: (value) => emit('update:modelValue', value)
})

const session = computed(() => store.selectedSession)

// 加载视频URL（使用Tauri的文件协议）
const loadVideoUrl = () => {
  if (!session.value?.session?.video_path) return

  try {
    // 检查是否在 Tauri 环境中
    if (!window.__TAURI__) {
      console.warn('不在 Tauri 环境中，无法加载本地视频文件')
      ElMessage.warning('请在 Tauri 应用中查看视频（需要运行 npm run tauri dev）')
      return
    }

    let videoPath = session.value.session.video_path

    // Windows 路径处理：确保使用正斜杠
    if (isWindows.value) {
      // 将反斜杠替换为正斜杠
      videoPath = videoPath.replace(/\\/g, '/')
    }

    // 使用 convertFileSrc 转换路径
    const convertedUrl = convertFileSrc(videoPath)

    // 对于 Windows，可能需要特殊处理
    if (isWindows.value && convertedUrl.includes('asset.localhost')) {
      // Windows 下的特殊处理：确保路径编码正确
      console.log('原始路径:', videoPath)
      console.log('转换后URL:', convertedUrl)

      // 如果是绝对路径，尝试使用 file:// 协议作为备选
      if (videoPath.match(/^[A-Za-z]:\//)) {
        // 先尝试使用 convertFileSrc 的结果
        videoUrl.value = convertedUrl

        // 如果加载失败，可以在 handleVideoError 中尝试 file:// 协议
        videoUrl.value._fallbackPath = 'file:///' + videoPath
      } else {
        videoUrl.value = convertedUrl
      }
    } else {
      videoUrl.value = convertedUrl
    }

    console.log('最终视频URL:', videoUrl.value)
  } catch (error) {
    console.error('转换视频路径失败:', error)
    ElMessage.error('视频路径转换失败：' + error)
  }
}

// 转换文件路径
const getConvertedPath = (filePath) => {
  if (!filePath) return '/placeholder.png'

  // 检查是否在 Tauri 环境中
  if (!window.__TAURI__) {
    // 在纯前端开发模式下，返回占位图
    return '/placeholder.png'
  }

  try {
    return convertFileSrc(filePath)
  } catch (error) {
    console.error('转换文件路径失败:', error)
    return '/placeholder.png'
  }
}

// 解析的关键时刻
const keyMoments = computed(() => {
  try {
    const tags = session.value?.tags || []
    return tags.flatMap(tag => tag.key_moments || [])
  } catch {
    return []
  }
})

// 生产力评分
const productivityScore = computed(() => {
  const tags = session.value?.tags || []
  const scores = tags.map(t => t.productivity_score).filter(Boolean)
  return scores.length > 0 ? Math.round(scores.reduce((a, b) => a + b) / scores.length) : null
})

// 专注度评分
const focusScore = computed(() => {
  const tags = session.value?.tags || []
  const scores = tags.map(t => t.focus_score).filter(Boolean)
  return scores.length > 0 ? Math.round(scores.reduce((a, b) => a + b) / scores.length) : null
})

// 所有关键词
const allKeywords = computed(() => {
  const tags = session.value?.tags || []
  const keywords = new Set()
  tags.forEach(tag => {
    (tag.keywords || []).forEach(kw => keywords.add(kw))
  })
  return Array.from(keywords)
})

// 采样的帧（最多显示10帧）
const sampledFrames = computed(() => {
  const frames = session.value?.frames || []
  if (frames.length <= 10) return frames

  const step = Math.floor(frames.length / 10)
  return frames.filter((_, index) => index % step === 0).slice(0, 10)
})

// 格式化日期时间
const formatDateTime = (timestamp) => {
  return dayjs(timestamp).format('YYYY-MM-DD HH:mm:ss')
}

// 格式化时间
const formatTime = (timestamp) => {
  return dayjs(timestamp).format('HH:mm:ss')
}

// 格式化时长
const formatDuration = (startTime, endTime) => {
  const start = dayjs(startTime)
  const end = dayjs(endTime)
  const minutes = end.diff(start, 'minute')

  if (minutes >= 60) {
    const hours = Math.floor(minutes / 60)
    const mins = minutes % 60
    return `${hours}小时${mins > 0 ? mins + '分钟' : ''}`
  }
  return `${minutes}分钟`
}

// 类别映射表（兼容旧数据）
const categoryMapping = {
  // 工作类
  'work': 'work',
  'Work': 'work',
  'coding': 'work',
  'Coding': 'work',
  'writing': 'work',
  'Writing': 'work',
  'design': 'work',
  'Design': 'work',
  'planning': 'work',
  'Planning': 'work',
  'data_analysis': 'work',
  'DataAnalysis': 'work',
  // 沟通类
  'communication': 'communication',
  'Communication': 'communication',
  'meeting': 'communication',
  'Meeting': 'communication',
  // 学习类
  'learning': 'learning',
  'Learning': 'learning',
  'research': 'learning',
  'Research': 'learning',
  // 个人类
  'personal': 'personal',
  'Personal': 'personal',
  'entertainment': 'personal',
  'Entertainment': 'personal',
  'social_media': 'personal',
  'SocialMedia': 'personal',
  'shopping': 'personal',
  'Shopping': 'personal',
  'finance': 'personal',
  'Finance': 'personal',
  // 空闲类
  'idle': 'idle',
  'Idle': 'idle',
  // 其他类
  'other': 'other',
  'Other': 'other',
  'break': 'other',
  'Break': 'other',
  'exercise': 'other',
  'Exercise': 'other'
}

// 新的6类标签配置（分离emoji和名称）
const categoryConfig = {
  'work': { name: '工作', emoji: '💼', color: '#409EFF' },
  'communication': { name: '沟通', emoji: '💬', color: '#FFC107' },
  'learning': { name: '学习', emoji: '📚', color: '#67C23A' },
  'personal': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'idle': { name: '空闲', emoji: '⏸️', color: '#909399' },
  'other': { name: '其他', emoji: '📌', color: '#6C757D' }
}

// 获取类别颜色
const getCategoryColor = (category) => {
  const mapped = categoryMapping[category] || 'other'
  return categoryConfig[mapped]?.color || '#909399'
}

// 获取类别名称（不含emoji）
const getCategoryName = (category) => {
  const mapped = categoryMapping[category] || 'other'
  return categoryConfig[mapped]?.name || category
}

// 获取类别emoji
const getCategoryEmoji = (category) => {
  const mapped = categoryMapping[category] || 'other'
  return categoryConfig[mapped]?.emoji || '📌'
}

// 获取重要性类型
const getImportanceType = (importance) => {
  if (importance >= 4) return 'danger'
  if (importance >= 3) return 'warning'
  if (importance >= 2) return 'primary'
  return 'info'
}

// 获取评分颜色
const getScoreColor = (percentage) => {
  if (percentage < 30) return '#F56C6C'
  if (percentage < 60) return '#E6A23C'
  if (percentage < 80) return '#409EFF'
  return '#67C23A'
}

// 处理图片加载成功
const handleImageLoad = (index) => {
  loadingImages[index] = false
}

// 处理图片加载错误
const handleImageError = (e, index) => {
  e.target.src = '/placeholder.png'
  if (index !== undefined) {
    loadingImages[index] = false
  }
}

// 预览帧
const previewFrame = (frame) => {
  if (!window.__TAURI__) {
    ElMessage.warning('请在 Tauri 应用中查看完整图片')
    return
  }
  previewUrl.value = convertFileSrc(frame.file_path)
}

// 移除标签
const removeTag = (index) => {
  const newTags = [...session.value.tags]
  newTags.splice(index, 1)
  // TODO: 更新到后端
}

// 添加标签
const addTag = async (tag) => {
  await store.addManualTag(session.value.session.id, tag)
}

// 生成视频
const generateVideo = async () => {
  try {
    await store.generateVideo(session.value.session.id)
    // 重新获取会话详情以更新video_path
    await store.fetchSessionDetail(session.value.session.id)
    ElMessage.success('视频已生成并可播放')
  } catch (error) {
    console.error('生成视频失败:', error)
  }
}

const retryAnalysis = async () => {
  if (!session.value?.session?.id || isProcessing.value) return
  await store.retrySessionAnalysis(session.value.session.id)
}

// 处理视频加载开始
const onVideoLoadStart = () => {
  console.log('视频开始加载...')
}

// 处理视频加载完成
const onVideoLoadedData = () => {
  console.log('视频数据已加载')
}

// 处理视频加载错误
const handleVideoError = (e) => {
  console.error('视频加载失败:', e)
  console.log('视频路径:', session.value?.session?.video_path)
  console.log('当前视频URL:', videoUrl.value)

  // 在 Windows 下尝试备用方案
  if (isWindows.value && videoUrl.value?._fallbackPath) {
    console.log('尝试备用路径:', videoUrl.value._fallbackPath)

    // 尝试直接使用 file:// 协议
    const fallbackUrl = videoUrl.value._fallbackPath
    videoUrl.value = fallbackUrl

    // 给第二次加载一个机会
    setTimeout(() => {
      if (videoPlayer.value) {
        videoPlayer.value.load()
      }
    }, 100)
  } else if (window.__TAURI__) {
    // 其他情况下显示错误消息
    ElMessage.error('视频加载失败，请尝试重新生成')
  }
}

// 播放视频
const playVideo = () => {
  if (videoPlayer.value) {
    videoPlayer.value.play()
  }
}

// 关闭对话框
const handleClose = () => {
  emit('close')
  dialogVisible.value = false
}

// 监听sessionId变化
watch(() => props.sessionId, async (newId) => {
  if (newId) {
    await store.fetchSessionDetail(newId)
    // 如果有视频，加载视频
    if (store.selectedSession?.session?.video_path) {
      loadVideoUrl()
    }
  }
})

// 监听会话视频路径变化
watch(() => session.value?.session?.video_path, async (newPath) => {
  if (newPath) {
    loadVideoUrl()
  }
})

// 监听采样帧变化，初始化加载状态
watch(sampledFrames, (frames) => {
  frames.forEach((_, index) => {
    loadingImages[index] = true
  })
}, { immediate: true })

// 检测是否为Windows系统和Tauri环境
onMounted(() => {
  isWindows.value = navigator.platform.toLowerCase().includes('win')
  isTauriEnv.value = !!window.__TAURI__

  if (!isTauriEnv.value) {
    console.warn('当前不在 Tauri 环境中，部分功能可能受限')
  }
})

// 组件销毁时清理
onUnmounted(() => {
  // 清理视频URL
  if (videoUrl.value) {
    videoUrl.value = null
  }
})
</script>

<style scoped>
.session-detail {
  padding: 20px;
}

.tags-section,
.key-moments-section,
.scores-section,
.frames-section {
  margin-top: 30px;
}

.tags-section h4,
.key-moments-section h4,
.scores-section h4,
.frames-section h4 {
  margin-bottom: 15px;
  color: #303133;
}

.tags-list {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: center;
}

.keywords {
  margin-top: 15px;
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.keyword-label {
  font-size: 14px;
  color: #606266;
  margin-right: 8px;
}

.scores {
  display: flex;
  flex-direction: column;
  gap: 15px;
}

.score-item {
  display: flex;
  align-items: center;
  gap: 10px;
}

.score-item span {
  width: 100px;
  color: #606266;
}

.media-section {
  margin-top: 30px;
}

.video-section h4,
.frames-section h4 {
  margin-bottom: 15px;
  color: #303133;
}

.video-container {
  display: flex;
  justify-content: center;
  margin-top: 15px;
}

.frames-gallery {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 15px;
  margin-top: 15px;
}

.frame-item {
  position: relative;
  cursor: pointer;
  border-radius: 4px;
  overflow: hidden;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  transition: transform 0.3s, box-shadow 0.3s;
  background: #f5f5f5;
  min-height: 100px;
}

/* Windows 系统禁用动画以防止闪烁 */
.frame-item.no-animation {
  transition: none;
}

.frame-item:hover {
  transform: scale(1.05);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

.frame-item.no-animation:hover {
  transform: none;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.frame-loading {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  font-size: 24px;
  color: #409EFF;
}

.frame-item img {
  width: 100%;
  height: 100px;
  object-fit: cover;
}

.frame-time {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  background: rgba(0, 0, 0, 0.6);
  color: white;
  padding: 2px 5px;
  font-size: 12px;
  text-align: center;
}

.dialog-footer {
  display: flex;
  gap: 10px;
}
</style>
