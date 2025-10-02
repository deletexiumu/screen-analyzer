<!-- 活动列表视图组件 - 按类型分组显示活动 -->

<template>
  <div class="activity-list-container">
    <!-- 头部：设备选择和视图切换 -->
    <div class="activity-header">
      <div class="device-selector">
        <el-dropdown @command="handleDeviceChange" trigger="click">
          <el-button class="device-button">
            <el-icon><Monitor /></el-icon>
            {{ selectedDeviceName }}
            <span class="device-count">{{ deviceCount }}</span>
            <el-icon class="arrow"><ArrowDown /></el-icon>
          </el-button>
          <template #dropdown>
            <el-dropdown-menu class="device-menu">
              <el-dropdown-item command="all">
                <el-icon><Monitor /></el-icon>
                All Devices
              </el-dropdown-item>
              <el-dropdown-item
                v-for="device in devices"
                :key="device.name"
                :command="device.name"
              >
                <OSIcons
                  :type="getDeviceIcon(device.type)"
                  :size="14"
                  :style="{ color: getDeviceColor(device.name) }"
                />
                {{ device.name }}
              </el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
      </div>

      <div class="view-toggle">
        <el-button
          :class="{ active: currentView === 'stats' }"
          @click="currentView = 'stats'"
          class="view-button"
          circle
        >
          <el-icon><DataAnalysis /></el-icon>
        </el-button>
        <el-button
          :class="{ active: currentView === 'timeline' }"
          @click="currentView = 'timeline'"
          class="view-button"
          circle
        >
          <el-icon><Calendar /></el-icon>
        </el-button>
      </div>
    </div>

    <!-- 统计视图 -->
    <div v-if="currentView === 'stats'" class="stats-view">
      <!-- Today's Timeline 横条图 -->
      <div class="timeline-bars">
        <h3>TODAY'S TIMELINE</h3>
        <div class="time-range">
          <span>0h</span>
          <span>6h</span>
          <span>12h</span>
          <span>18h</span>
          <span>24h</span>
        </div>
        <div class="device-bars">
          <div
            v-for="device in deviceTimelines"
            :key="device.name"
            class="device-bar-row"
          >
            <div class="device-label">
              <OSIcons
                :type="getDeviceIcon(device.type)"
                :size="14"
                :style="{ color: getDeviceColor(device.name) }"
              />
              {{ device.shortName }}
            </div>
            <div class="time-bar">
              <div
                v-for="(segment, index) in device.segments"
                :key="index"
                class="time-segment"
                :style="getSegmentStyle(segment)"
              />
            </div>
            <div class="total-time">{{ device.totalTime }}</div>
          </div>
        </div>
        <div class="total-tracked">
          <div>{{ totalTrackedTime }} total tracked today</div>
          <div class="today-tracked">{{ todayTrackedTime }} tracked today</div>
        </div>
      </div>

      <!-- 活动类型列表 -->
      <div class="activity-categories">
        <div
          v-for="category in activityCategories"
          :key="category.name"
          class="category-section"
        >
          <div class="category-header">
            <div class="category-info">
              <h3>{{ category.name }}</h3>
              <div class="category-duration">{{ category.duration }} across devices</div>
            </div>
            <div class="category-percentage">{{ category.percentage }}%</div>
          </div>

          <!-- 设备分组 -->
          <div class="device-groups">
            <div
              v-for="deviceGroup in category.devices"
              :key="deviceGroup.name"
              class="device-group"
            >
              <div class="device-group-header">
                <OSIcons
                  :type="getDeviceIcon(deviceGroup.type)"
                  :size="14"
                  :style="{ color: getDeviceColor(deviceGroup.name) }"
                />
                {{ deviceGroup.name }}
              </div>

              <!-- 时间段列表 -->
              <div class="time-slots">
                <div
                  v-for="(slot, index) in deviceGroup.slots"
                  :key="index"
                  class="time-slot"
                >
                  <span class="time-range-text">{{ slot.timeRange }}</span>
                  <span class="screenshot-count">{{ slot.screenshots }}</span>
                </div>
              </div>
            </div>
          </div>

          <!-- 标签 -->
          <div v-if="category.tags.length > 0" class="category-tags">
            <el-tag
              v-for="tag in category.tags"
              :key="tag"
              :type="getTagType(tag)"
              size="small"
              class="activity-tag"
            >
              {{ tag }}
            </el-tag>
          </div>
        </div>
      </div>
    </div>

    <!-- 时间线视图 -->
    <div v-else class="timeline-view">
      <TimelineView
        :date="date"
        :selected-device="selectedDevice"
        @session-click="handleSessionClick"
      />
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue'
import { Monitor, ArrowDown, DataAnalysis, Calendar } from '@element-plus/icons-vue'
import OSIcons from './icons/OSIcons.vue'
import TimelineView from './TimelineView.vue'
import { useActivityStore } from '../stores/activity'
import dayjs from 'dayjs'

const props = defineProps({
  date: {
    type: String,
    required: true
  }
})

const emit = defineEmits(['session-click'])

const store = useActivityStore()

const selectedDevice = ref('all')
const currentView = ref('stats')

// 设备列表
const devices = computed(() => {
  const deviceMap = new Map()
  store.daySessions.forEach(session => {
    if (session.device_name) {
      deviceMap.set(session.device_name, {
        name: session.device_name,
        type: session.device_type || 'unknown'
      })
    }
  })
  return Array.from(deviceMap.values())
})

const deviceCount = computed(() => devices.value.length)

const selectedDeviceName = computed(() => {
  if (selectedDevice.value === 'all') {
    return 'All Devices'
  }
  return selectedDevice.value
})

const handleDeviceChange = (device) => {
  selectedDevice.value = device
}

// 过滤后的会话（根据选中的设备）
const filteredSessions = computed(() => {
  if (selectedDevice.value === 'all') {
    return store.daySessions
  }
  return store.daySessions.filter(session =>
    session.device_name === selectedDevice.value
  )
})

// 设备时间线数据（横条图）
const deviceTimelines = computed(() => {
  const timelines = []
  const deviceMap = new Map()

  // 按设备分组会话
  filteredSessions.value.forEach(session => {
    const deviceName = session.device_name || 'Unknown'
    if (!deviceMap.has(deviceName)) {
      deviceMap.set(deviceName, {
        name: deviceName,
        shortName: deviceName.length > 10 ? deviceName.substring(0, 10) : deviceName,
        type: session.device_type || 'unknown',
        sessions: [],
        totalMinutes: 0
      })
    }
    deviceMap.get(deviceName).sessions.push(session)
  })

  // 生成时间段
  deviceMap.forEach(device => {
    const segments = []
    device.sessions.forEach(session => {
      const start = dayjs(session.start_time)
      const end = dayjs(session.end_time)
      const duration = end.diff(start, 'minute')
      device.totalMinutes += duration

      // 计算在时间轴上的位置（0:00-24:00，共24小时）
      const startHour = start.hour() + start.minute() / 60
      const endHour = end.hour() + end.minute() / 60
      const startPercent = (startHour / 24) * 100
      const widthPercent = ((endHour - startHour) / 24) * 100

      // 获取分类颜色
      const category = getSessionCategory(session)
      const color = getCategoryColor(category)

      segments.push({
        left: Math.max(0, Math.min(100, startPercent)),
        width: Math.max(0, Math.min(100 - startPercent, widthPercent)),
        color,
        category
      })
    })

    timelines.push({
      ...device,
      segments,
      totalTime: formatDuration(device.totalMinutes)
    })
  })

  return timelines
})

// 总跟踪时间
const totalTrackedTime = computed(() => {
  const total = deviceTimelines.value.reduce((sum, device) => sum + device.totalMinutes, 0)
  return formatDuration(total)
})

const todayTrackedTime = computed(() => {
  // 这里可以单独计算今天的时间，暂时和总时间一样
  return totalTrackedTime.value
})

// 活动类型统计
const activityCategories = computed(() => {
  const categories = new Map()
  let totalMinutes = 0

  // 按类型分组 - 使用 timeline_cards 而不是 session
  filteredSessions.value.forEach(session => {
    const deviceName = session.device_name || 'Unknown'
    const deviceType = session.device_type || 'unknown'

    // 提取标签
    let sessionTags = []
    try {
      sessionTags = JSON.parse(session.tags || '[]')
    } catch (e) {
      // 忽略
    }

    // 尝试从 timeline_cards 获取数据
    let cards = []
    if (session.timeline_cards) {
      try {
        const sessionCards = JSON.parse(session.timeline_cards)
        if (Array.isArray(sessionCards) && sessionCards.length > 0) {
          cards = sessionCards
        }
      } catch (e) {
        console.error('解析时间线卡片失败:', e)
      }
    }

    // 如果没有 timeline_cards，使用 session 本身
    if (cards.length === 0) {
      const sessionCategory = getSessionCategory(session)
      cards = [{
        category: sessionCategory,
        start_time: session.start_time,
        end_time: session.end_time
      }]
    }

    // 处理每个 card
    cards.forEach(card => {
      const category = card.category || 'Other'
      const categoryName = getCategoryDisplayName(category)

      if (!categories.has(categoryName)) {
        categories.set(categoryName, {
          name: categoryName,
          category,
          devices: new Map(),
          totalMinutes: 0,
          tags: new Set()
        })
      }

      const cat = categories.get(categoryName)

      // 添加设备分组
      if (!cat.devices.has(deviceName)) {
        cat.devices.set(deviceName, {
          name: deviceName,
          type: deviceType,
          slots: []
        })
      }

      const start = dayjs(card.start_time)
      const end = dayjs(card.end_time)
      const duration = end.diff(start, 'minute')

      cat.totalMinutes += duration
      totalMinutes += duration

      // 添加时间段
      cat.devices.get(deviceName).slots.push({
        start: start,
        end: end,
        timeRange: `${start.format('HH:mm')} - ${end.format('HH:mm')}`,
        screenshots: Math.floor(duration)
      })

      // 添加标签
      sessionTags.forEach(tag => {
        if (tag.name) {
          cat.tags.add(tag.name)
        }
      })
    })
  })

  // 转换为数组并计算百分比
  const result = Array.from(categories.values()).map(cat => {
    // 合并每个设备的连续时间段
    const mergedDevices = Array.from(cat.devices.values()).map(device => {
      const mergedSlots = mergeTimeSlots(device.slots)
      return {
        ...device,
        slots: mergedSlots
      }
    })

    return {
      name: cat.name,
      category: cat.category,
      duration: formatDuration(cat.totalMinutes),
      percentage: totalMinutes > 0 ? Math.round((cat.totalMinutes / totalMinutes) * 100) : 0,
      devices: mergedDevices,
      tags: Array.from(cat.tags)
    }
  })

  // 按时长排序
  result.sort((a, b) => b.percentage - a.percentage)

  return result
})

// 合并连续的时间段
const mergeTimeSlots = (slots) => {
  if (slots.length === 0) return []

  // 按开始时间排序
  const sorted = [...slots].sort((a, b) => a.start.valueOf() - b.start.valueOf())
  const merged = []
  let current = { ...sorted[0] }

  for (let i = 1; i < sorted.length; i++) {
    const slot = sorted[i]
    // 如果当前段的结束时间等于下一段的开始时间（或差距很小，比如1分钟内），则合并
    const gap = slot.start.diff(current.end, 'minute')
    if (gap <= 1) {
      // 合并时间段
      current.end = slot.end
      current.screenshots += slot.screenshots
      current.timeRange = `${current.start.format('HH:mm')} - ${current.end.format('HH:mm')}`
    } else {
      // 保存当前段，开始新的段
      merged.push({
        timeRange: current.timeRange,
        screenshots: `${current.screenshots}📸`
      })
      current = { ...slot }
    }
  }

  // 添加最后一段
  merged.push({
    timeRange: current.timeRange,
    screenshots: `${current.screenshots}📸`
  })

  return merged
}

// 获取时间段样式
const getSegmentStyle = (segment) => {
  return {
    left: `${segment.left}%`,
    width: `${segment.width}%`,
    backgroundColor: segment.color
  }
}

// 格式化时长
const formatDuration = (minutes) => {
  const hours = Math.floor(minutes / 60)
  const mins = minutes % 60
  return `${hours}h ${mins}m`
}

// 获取会话分类
const getSessionCategory = (session) => {
  try {
    const tags = JSON.parse(session.tags || '[]')
    return tags[0]?.category || 'Other'
  } catch {
    return 'Other'
  }
}

// 类别配置
const categoryDisplayNames = {
  'work': 'Code Development',
  'Work': 'Code Development',
  'Coding': 'Code Development',
  'coding': 'Code Development',
  'communication': 'Meetings',
  'Communication': 'Meetings',
  'Meeting': 'Meetings',
  'meeting': 'Meetings',
  'learning': 'Learning',
  'Learning': 'Learning',
  'Research': 'Learning',
  'research': 'Learning',
  'personal': 'Personal',
  'Personal': 'Personal',
  'Entertainment': 'Entertainment',
  'entertainment': 'Entertainment',
  'Other': 'Other',
  'other': 'Other'
}

const getCategoryDisplayName = (category) => {
  return categoryDisplayNames[category] || category
}

// 类别颜色
const categoryColors = {
  'Code Development': '#409EFF',
  'Meetings': '#E6A23C',
  'Learning': '#67C23A',
  'Personal': '#F56C6C',
  'Entertainment': '#909399',
  'Other': '#6C757D'
}

const getCategoryColor = (category) => {
  const displayName = getCategoryDisplayName(category)
  return categoryColors[displayName] || '#909399'
}

// 获取设备图标
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

// 获取标签类型
const getTagType = (tag) => {
  const types = {
    'Coding': '',
    'Python': 'success',
    'React': 'primary',
    'Meeting': 'warning',
    'Design': 'danger'
  }
  return types[tag] || 'info'
}

const handleSessionClick = (session) => {
  emit('session-click', session)
}
</script>

<style scoped>
.activity-list-container {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: #1a1a1a;
  border-radius: 8px;
  padding: 20px;
  border: 1px solid #2d2d2d;
  overflow: hidden;
}

.activity-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
}

.device-button {
  background: transparent;
  border: 1px solid #3d3d3d;
  color: #ffffff;
  padding: 8px 16px;
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
}

.device-button:hover {
  background: #2d2d2d;
  border-color: #4d4d4d;
}

.device-count {
  background: #3d3d3d;
  color: #ffffff;
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 12px;
}

.arrow {
  margin-left: 4px;
}

.view-toggle {
  display: flex;
  gap: 8px;
}

.view-button {
  background: transparent;
  border: 1px solid #3d3d3d;
  color: #909399;
  width: 36px;
  height: 36px;
  padding: 0;
}

.view-button:hover {
  background: #2d2d2d;
  border-color: #4d4d4d;
  color: #ffffff;
}

.view-button.active {
  background: #409EFF;
  border-color: #409EFF;
  color: #ffffff;
}

.stats-view {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
}

/* 时间线横条图 */
.timeline-bars {
  margin-bottom: 24px;
  padding: 20px;
  background: #0f0f0f;
  border-radius: 8px;
}

.timeline-bars h3 {
  margin: 0 0 12px 0;
  font-size: 13px;
  color: #666666;
  font-weight: 600;
  letter-spacing: 1px;
}

.time-range {
  display: flex;
  justify-content: space-between;
  margin-bottom: 8px;
  font-size: 12px;
  color: #666666;
  padding: 0 60px 0 80px;
}

.device-bars {
  margin-bottom: 12px;
}

.device-bar-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 8px;
}

.device-label {
  width: 70px;
  font-size: 12px;
  color: #909399;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.time-bar {
  flex: 1;
  height: 24px;
  background: #1a1a1a;
  border-radius: 4px;
  position: relative;
  overflow: hidden;
}

.time-segment {
  position: absolute;
  height: 100%;
  border-radius: 3px;
  opacity: 0.8;
}

.total-time {
  width: 60px;
  text-align: right;
  font-size: 12px;
  color: #e0e0e0;
  font-weight: 600;
  flex-shrink: 0;
}

.total-tracked {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid #2d2d2d;
  font-size: 13px;
  color: #e0e0e0;
  text-align: center;
}

.today-tracked {
  font-size: 12px;
  color: #666666;
  margin-top: 4px;
}

/* 活动类型列表 */
.activity-categories {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.category-section {
  background: #0f0f0f;
  border-radius: 8px;
  padding: 16px;
}

.category-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.category-info h3 {
  margin: 0;
  font-size: 16px;
  color: #ffffff;
  font-weight: 600;
}

.category-duration {
  font-size: 12px;
  color: #666666;
  margin-top: 4px;
}

.category-percentage {
  font-size: 20px;
  font-weight: 700;
  color: #ffffff;
  background: #e6a23c;
  padding: 4px 12px;
  border-radius: 12px;
  min-width: 50px;
  text-align: center;
}

.device-groups {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-bottom: 12px;
}

.device-group-header {
  font-size: 13px;
  color: #909399;
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
  font-weight: 500;
}

.time-slots {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 20px;
}

.time-slot {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 12px;
  color: #b0b0b0;
  padding: 4px 8px;
  background: #1a1a1a;
  border-radius: 4px;
}

.time-range-text {
  flex: 1;
}

.screenshot-count {
  color: #666666;
}

.category-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid #2d2d2d;
}

.activity-tag {
  font-size: 11px;
}

.timeline-view {
  flex: 1;
  overflow: hidden;
}

/* 滚动条样式 */
.stats-view::-webkit-scrollbar {
  width: 6px;
}

.stats-view::-webkit-scrollbar-track {
  background: #1a1a1a;
}

.stats-view::-webkit-scrollbar-thumb {
  background: #3d3d3d;
  border-radius: 3px;
}

.stats-view::-webkit-scrollbar-thumb:hover {
  background: #4d4d4d;
}
</style>
