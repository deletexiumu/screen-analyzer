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

      <!-- 帧预览 -->
      <div class="frames-section" v-if="session && session.frames?.length > 0">
        <h4>截图预览（采样显示）</h4>
        <div class="frames-gallery">
          <div
            v-for="(frame, index) in sampledFrames"
            :key="index"
            class="frame-item"
            @click="previewFrame(frame)"
          >
            <img
              :src="`file://${frame.file_path}`"
              :alt="`Frame ${index + 1}`"
              @error="handleImageError"
            />
            <div class="frame-time">
              {{ formatTime(frame.timestamp) }}
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
import { ref, computed, watch } from 'vue'
import { VideoPlay, VideoCamera, Refresh } from '@element-plus/icons-vue'
import { useActivityStore } from '../stores/activity'
import dayjs from 'dayjs'
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

const dialogVisible = computed({
  get: () => props.modelValue,
  set: (value) => emit('update:modelValue', value)
})

const session = computed(() => store.selectedSession)

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

// 获取类别颜色
const getCategoryColor = (category) => {
  const colors = {
    'Work': '#409EFF',
    'Personal': '#67C23A',
    'Break': '#E6A23C',
    'Idle': '#909399',
    'Meeting': '#F56C6C',
    'Coding': '#7C4DFF',
    'Research': '#00BCD4',
    'Communication': '#FFC107',
    'Entertainment': '#FF69B4',
    'Other': '#795548'
  }
  return colors[category] || '#909399'
}

// 获取类别名称
const getCategoryName = (category) => {
  const names = {
    'Work': '工作',
    'Personal': '私人',
    'Break': '休息',
    'Idle': '空闲',
    'Meeting': '会议',
    'Coding': '编程',
    'Research': '研究',
    'Communication': '沟通',
    'Entertainment': '娱乐',
    'Other': '其他'
  }
  return names[category] || category
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

// 处理图片加载错误
const handleImageError = (e) => {
  e.target.src = '/placeholder.png'
}

// 预览帧
const previewFrame = (frame) => {
  previewUrl.value = `file://${frame.file_path}`
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
  await store.generateVideo(session.value.session.id)
}

const retryAnalysis = async () => {
  if (!session.value?.session?.id || isProcessing.value) return
  await store.retrySessionAnalysis(session.value.session.id)
}

// 播放视频
const playVideo = () => {
  // TODO: 实现视频播放
  console.log('Play video:', session.value.session.video_path)
}

// 关闭对话框
const handleClose = () => {
  emit('close')
  dialogVisible.value = false
}

// 监听sessionId变化
watch(() => props.sessionId, (newId) => {
  if (newId) {
    store.fetchSessionDetail(newId)
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
  transition: transform 0.3s;
}

.frame-item:hover {
  transform: scale(1.05);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
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
