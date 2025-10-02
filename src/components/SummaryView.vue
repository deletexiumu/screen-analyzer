<!-- 总结页面组件 - 显示每日活动总结、设备使用情况、并行工作分析等 -->

<template>
  <div class="summary-container">
    <div class="summary-header">
      <h2>All Devices Overview</h2>
      <div class="active-badge">
        <span class="badge-number">{{ activeDeviceCount }}</span> Active
      </div>
    </div>

    <!-- Today's Summary -->
    <section class="summary-section summary-text-section">
      <h3 class="section-title">Today's Summary</h3>
      <div class="summary-content">
        <p v-if="todaySummary" class="summary-text">{{ todaySummary }}</p>
        <p v-else class="empty-text">暂无总结数据</p>
      </div>
    </section>

    <!-- Device Overview Cards -->
    <section class="summary-section device-stats-section" v-if="deviceStats.length > 0">
      <div class="device-cards-grid">
        <div
          v-for="device in deviceStats"
          :key="device.name"
          class="device-stat-card"
          :style="{ borderLeftColor: getDeviceColor(device.name) }"
        >
          <div class="device-card-header">
            <OSIcons
              :type="getDeviceIcon(device.type)"
              :size="16"
              :style="{ color: getDeviceColor(device.name) }"
            />
            <span class="device-label">{{ device.name }}</span>
          </div>
          <div class="device-stat-time">{{ device.totalTime }}</div>
          <div class="device-stat-screenshots">{{ device.screenshots }} screenshots</div>
        </div>
      </div>
    </section>

    <!-- Parallel Work Analysis -->
    <section class="summary-section parallel-section" v-if="parallelWork.length > 0">
      <h3 class="section-title">Parallel Work Analysis</h3>
      <div class="parallel-work-list">
        <div
          v-for="(work, index) in parallelWork"
          :key="index"
          class="parallel-work-card"
        >
          <div class="parallel-time-badge">{{ work.timeRange }}</div>
          <div class="parallel-content">
            <h4 class="parallel-title">{{ work.title }}</h4>
            <p class="parallel-description">
              <span class="device-icon">💻</span>{{ work.description }}
            </p>
          </div>
        </div>
      </div>
    </section>

    <!-- Device Usage Patterns -->
    <section class="summary-section patterns-section">
      <h3 class="section-title">Device Usage Patterns</h3>
      <div class="usage-patterns">
        <div v-if="deviceUsagePatterns.length > 0" class="patterns-list">
          <div
            v-for="(pattern, index) in deviceUsagePatterns"
            :key="index"
            class="pattern-item"
          >
            <div class="pattern-label">{{ pattern.label }}</div>
            <div class="pattern-value">{{ pattern.value }}</div>
          </div>
        </div>
        <p v-else class="empty-text">暂无使用模式数据</p>
      </div>
    </section>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { useActivityStore } from '../stores/activity'
import OSIcons from './icons/OSIcons.vue'
import dayjs from 'dayjs'

const store = useActivityStore()

// 活跃设备数量
const activeDeviceCount = computed(() => {
  const devices = new Set()
  store.daySessions.forEach(session => {
    if (session.device_name) {
      devices.add(session.device_name)
    }
  })
  return devices.size
})

// 今日总结
const todaySummary = computed(() => {
  // 从所有会话中提取总结信息
  const sessions = store.daySessions
  if (sessions.length === 0) return null

  // 计算总时长
  const totalMinutes = sessions.reduce((total, session) => {
    const start = dayjs(session.start_time)
    const end = dayjs(session.end_time)
    return total + end.diff(start, 'minute')
  }, 0)

  const hours = Math.floor(totalMinutes / 60)
  const minutes = totalMinutes % 60

  // 统计主要活动类别
  const categories = {}
  sessions.forEach(session => {
    try {
      const tags = JSON.parse(session.tags || '[]')
      if (tags.length > 0) {
        const category = tags[0].category || 'Other'
        categories[category] = (categories[category] || 0) + 1
      }
    } catch (e) {
      // 忽略解析错误
    }
  })

  const mainCategory = Object.keys(categories).reduce((a, b) =>
    categories[a] > categories[b] ? a : b, 'Work'
  )

  return `High productivity day with ${sessions.length} work sessions across ${activeDeviceCount.value} devices. ${getCategoryName(mainCategory)} dominated the day with ${hours}h ${minutes}m total tracked time.`
})

// 设备统计
const deviceStats = computed(() => {
  const stats = new Map()

  store.daySessions.forEach(session => {
    const deviceName = session.device_name || 'Unknown Device'
    const deviceType = session.device_type || 'unknown'

    if (!stats.has(deviceName)) {
      stats.set(deviceName, {
        name: deviceName,
        type: deviceType,
        totalMinutes: 0,
        screenshots: 0
      })
    }

    const device = stats.get(deviceName)
    const start = dayjs(session.start_time)
    const end = dayjs(session.end_time)
    device.totalMinutes += end.diff(start, 'minute')
    // 假设每个会话的截图数量（实际应该从数据库获取）
    device.screenshots += Math.floor(end.diff(start, 'minute'))
  })

  return Array.from(stats.values()).map(device => ({
    ...device,
    totalTime: formatDuration(device.totalMinutes)
  }))
})

// 并行工作分析
const parallelWork = computed(() => {
  const sessions = store.daySessions
  if (sessions.length < 2) return []

  const overlaps = []

  // 检测时间重叠的会话（表示同时使用多个设备）
  for (let i = 0; i < sessions.length; i++) {
    for (let j = i + 1; j < sessions.length; j++) {
      const s1 = sessions[i]
      const s2 = sessions[j]

      // 检查是否是不同设备
      if (s1.device_name === s2.device_name) continue

      const start1 = dayjs(s1.start_time)
      const end1 = dayjs(s1.end_time)
      const start2 = dayjs(s2.start_time)
      const end2 = dayjs(s2.end_time)

      // 检查时间重叠
      const overlapStart = start1.isAfter(start2) ? start1 : start2
      const overlapEnd = end1.isBefore(end2) ? end1 : end2

      if (overlapStart.isBefore(overlapEnd)) {
        const duration = overlapEnd.diff(overlapStart, 'minute')
        if (duration >= 5) { // 至少5分钟的重叠才算
          overlaps.push({
            timeRange: `${overlapStart.format('HH:mm')}-${overlapEnd.format('HH:mm')}`,
            title: `${getCategoryName(getSessionCategory(s1))} + ${getCategoryName(getSessionCategory(s2))}`,
            description: `${getActivityName(s1)} on ${s1.device_name} while ${getActivityName(s2)} on ${s2.device_name}`,
            duration
          })
        }
      }
    }
  }

  // 按时间排序并去重
  return overlaps
    .sort((a, b) => a.timeRange.localeCompare(b.timeRange))
    .slice(0, 5) // 只显示前5个
})

// 格式化时长
const formatDuration = (minutes) => {
  const hours = Math.floor(minutes / 60)
  const mins = minutes % 60
  if (hours > 0) {
    return `${hours}h ${mins}m`
  }
  return `${mins}m`
}

// 获取设备图标类型
const getDeviceIcon = (deviceType) => {
  if (!deviceType) return 'unknown'
  const type = deviceType.toLowerCase()
  if (type === 'windows') return 'windows'
  if (type === 'macos') return 'macos'
  if (type === 'linux') return 'linux'
  return 'unknown'
}

// 获取设备颜色
const getDeviceColor = (deviceName) => {
  if (!deviceName) return '#909399'

  let hash = 0
  for (let i = 0; i < deviceName.length; i++) {
    hash = deviceName.charCodeAt(i) + ((hash << 5) - hash)
  }

  const colors = [
    '#409EFF',
    '#67C23A',
    '#E6A23C',
    '#F56C6C',
    '#909399',
    '#9C27B0',
    '#00BCD4',
    '#FF9800',
  ]

  return colors[Math.abs(hash) % colors.length]
}

// 获取会话类别
const getSessionCategory = (session) => {
  try {
    const tags = JSON.parse(session.tags || '[]')
    return tags[0]?.category || 'Other'
  } catch {
    return 'Other'
  }
}

// 获取活动名称
const getActivityName = (session) => {
  if (session.title && session.title !== 'null') {
    return session.title.length > 30 ? session.title.substring(0, 30) + '...' : session.title
  }
  return getCategoryName(getSessionCategory(session))
}

// 类别映射
const categoryConfig = {
  'work': { name: 'Code Development', emoji: '💼' },
  'communication': { name: 'Meetings', emoji: '💬' },
  'learning': { name: 'Learning', emoji: '📚' },
  'personal': { name: 'Personal', emoji: '🏠' },
  'idle': { name: 'Idle', emoji: '⏸️' },
  'other': { name: 'Other', emoji: '📌' },
  'Work': { name: 'Code Development', emoji: '💼' },
  'Coding': { name: 'Code Development', emoji: '💼' },
  'coding': { name: 'Code Development', emoji: '💼' },
  'Meeting': { name: 'Meetings', emoji: '💬' },
  'meeting': { name: 'Meetings', emoji: '💬' },
  'Communication': { name: 'Meetings', emoji: '💬' },
  'Personal': { name: 'Personal', emoji: '🏠' },
  'Idle': { name: 'Idle', emoji: '⏸️' },
  'Other': { name: 'Other', emoji: '📌' }
}

// 获取类别名称
const getCategoryName = (category) => {
  if (!category) return 'Other'
  const config = categoryConfig[category] || categoryConfig[category.toLowerCase()]
  return config?.name || category
}

// 设备使用模式
const deviceUsagePatterns = computed(() => {
  const sessions = store.daySessions
  if (sessions.length === 0) return []

  const patterns = []

  // 计算最活跃的时间段
  const hourActivity = new Array(24).fill(0)
  sessions.forEach(session => {
    const start = dayjs(session.start_time)
    const end = dayjs(session.end_time)
    for (let hour = start.hour(); hour <= end.hour(); hour++) {
      hourActivity[hour]++
    }
  })

  const peakHour = hourActivity.indexOf(Math.max(...hourActivity))
  if (peakHour >= 0) {
    patterns.push({
      label: '最活跃时段',
      value: `${peakHour.toString().padStart(2, '0')}:00 - ${(peakHour + 1).toString().padStart(2, '0')}:00`
    })
  }

  // 计算平均会话时长
  const avgDuration = sessions.reduce((sum, session) => {
    const start = dayjs(session.start_time)
    const end = dayjs(session.end_time)
    return sum + end.diff(start, 'minute')
  }, 0) / sessions.length

  patterns.push({
    label: '平均会话时长',
    value: `${Math.round(avgDuration)} 分钟`
  })

  // 计算设备切换次数
  let deviceSwitches = 0
  let lastDevice = null
  sessions.forEach(session => {
    if (lastDevice && lastDevice !== session.device_name) {
      deviceSwitches++
    }
    lastDevice = session.device_name
  })

  if (activeDeviceCount.value > 1) {
    patterns.push({
      label: '设备切换次数',
      value: `${deviceSwitches} 次`
    })
  }

  return patterns
})
</script>

<style scoped>
.summary-container {
  height: 100%;
  overflow-y: auto;
  padding: 32px;
  background: #0f0f0f;
  color: #e0e0e0;
}

.summary-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 32px;
  padding-bottom: 16px;
}

.summary-header h2 {
  margin: 0;
  font-size: 26px;
  font-weight: 700;
  color: #ffffff;
  letter-spacing: -0.5px;
}

.active-badge {
  background: #ffffff;
  color: #000000;
  padding: 6px 14px;
  border-radius: 16px;
  font-size: 13px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 4px;
}

.badge-number {
  font-size: 15px;
  font-weight: 700;
}

/* 通用 section 样式 */
.summary-section {
  margin-bottom: 28px;
}

.section-title {
  margin: 0 0 16px 0;
  font-size: 17px;
  font-weight: 600;
  color: #ffffff;
}

/* Today's Summary 部分 */
.summary-text-section {
  background: transparent;
  padding: 0;
}

.summary-content {
  padding: 0;
}

.summary-text {
  margin: 0;
  line-height: 1.7;
  color: #b0b0b0;
  font-size: 15px;
}

.empty-text {
  color: #666666;
  font-style: italic;
  font-size: 14px;
}

/* Device Stats Cards */
.device-stats-section {
  background: transparent;
  padding: 0;
}

.device-cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 16px;
  margin-top: 16px;
}

.device-stat-card {
  background: #1a1a1a;
  border-radius: 10px;
  padding: 20px;
  border: 1px solid #2d2d2d;
  border-left: 4px solid #409EFF;
  transition: all 0.25s ease;
}

.device-stat-card:hover {
  background: #1f1f1f;
  border-color: #3d3d3d;
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
}

.device-card-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 16px;
}

.device-label {
  font-size: 13px;
  color: #909399;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.device-stat-time {
  font-size: 36px;
  font-weight: 700;
  color: #ffffff;
  margin-bottom: 6px;
  line-height: 1;
}

.device-stat-screenshots {
  font-size: 13px;
  color: #666666;
}

/* Parallel Work Analysis */
.parallel-section {
  background: transparent;
  padding: 0;
}

.parallel-work-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 16px;
}

.parallel-work-card {
  display: flex;
  gap: 14px;
  align-items: flex-start;
  padding: 16px 18px;
  background: #1a1a1a;
  border-radius: 8px;
  border: 1px solid #2d2d2d;
  transition: all 0.25s ease;
}

.parallel-work-card:hover {
  background: #1f1f1f;
  border-color: #3d3d3d;
  transform: translateX(2px);
}

.parallel-time-badge {
  background: #8b3838;
  color: white;
  padding: 5px 11px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
  flex-shrink: 0;
  letter-spacing: 0.3px;
}

.parallel-content {
  flex: 1;
  min-width: 0;
}

.parallel-title {
  margin: 0 0 6px 0;
  font-size: 14px;
  font-weight: 600;
  color: #ffffff;
}

.parallel-description {
  margin: 0;
  font-size: 13px;
  color: #909399;
  line-height: 1.5;
  display: flex;
  align-items: flex-start;
  gap: 6px;
}

.device-icon {
  flex-shrink: 0;
  font-size: 14px;
}

/* Device Usage Patterns */
.patterns-section {
  background: transparent;
  padding: 0;
}

.usage-patterns {
  margin-top: 16px;
}

.patterns-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.pattern-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 14px 18px;
  background: #1a1a1a;
  border-radius: 8px;
  border: 1px solid #2d2d2d;
  transition: all 0.25s ease;
}

.pattern-item:hover {
  background: #1f1f1f;
  border-color: #3d3d3d;
}

.pattern-label {
  font-size: 13px;
  color: #909399;
  font-weight: 500;
}

.pattern-value {
  font-size: 14px;
  color: #ffffff;
  font-weight: 600;
}

/* 滚动条样式 */
.summary-container::-webkit-scrollbar {
  width: 8px;
}

.summary-container::-webkit-scrollbar-track {
  background: #0f0f0f;
}

.summary-container::-webkit-scrollbar-thumb {
  background: #2d2d2d;
  border-radius: 4px;
}

.summary-container::-webkit-scrollbar-thumb:hover {
  background: #3d3d3d;
}
</style>
