<!-- 活动摘要组件 - 在日历中显示每日活动概览 -->

<template>
  <div class="activity-summary">
    <div v-if="activity" class="activity-content">
      <div class="activity-indicator" :style="{ background: activityColor }"></div>
      <div class="activity-info">
        <div class="activity-count">{{ activity.session_count || 0 }} 会话</div>
        <div class="activity-time">{{ formatDuration(activity.total_duration_minutes) }}</div>
      </div>
    </div>
    <div v-else class="no-activity">
      <span class="no-data-text">无数据</span>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'

const props = defineProps({
  activity: {
    type: Object,
    default: null
  }
})

// 计算活动颜色（根据活动强度）
const activityColor = computed(() => {
  if (!props.activity) return '#f0f0f0'
  const count = props.activity.session_count || 0
  if (count === 0) return '#f0f0f0'
  if (count <= 2) return '#c6e48b'
  if (count <= 5) return '#7bc96f'
  if (count <= 10) return '#239a3b'
  return '#196127'
})

// 格式化时长
const formatDuration = (minutes) => {
  if (!minutes || minutes === 0) return '0分钟'
  if (minutes < 60) return `${minutes}分钟`
  const hours = Math.floor(minutes / 60)
  const mins = minutes % 60
  return mins > 0 ? `${hours}小时${mins}分钟` : `${hours}小时`
}
</script>

<style scoped>
.activity-summary {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}

.activity-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}

.activity-indicator {
  width: 40px;
  height: 40px;
  border-radius: 4px;
  opacity: 0.8;
  transition: opacity 0.3s;
}

.activity-indicator:hover {
  opacity: 1;
}

.activity-info {
  text-align: center;
  font-size: 11px;
  color: #606266;
  line-height: 1.2;
}

.activity-count {
  font-weight: bold;
  color: #303133;
}

.activity-time {
  color: #909399;
}

.no-activity {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
}

.no-data-text {
  font-size: 11px;
  color: #c0c4cc;
}
</style>