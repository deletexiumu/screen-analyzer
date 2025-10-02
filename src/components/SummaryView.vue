<!-- 总结页面组件 - 显示每日活动总结、设备使用情况、并行工作分析等 -->

<template>
  <div class="summary-container">
    <!-- Loading 状态 -->
    <div v-if="loading" class="loading-container">
      <el-icon class="is-loading" :size="40"><Loading /></el-icon>
      <p>加载中...</p>
    </div>

    <!-- 数据内容 -->
    <div v-else>
      <div class="summary-header">
        <h2>设备概览</h2>
        <div class="header-right">
          <el-button
            @click="refreshSummary"
            :loading="refreshing"
            circle
            class="refresh-button"
            title="重新生成总结"
          >
            <el-icon><Refresh /></el-icon>
          </el-button>
          <div class="active-badge">
            <span class="badge-number">{{ activeDeviceCount }}</span> 活跃设备
          </div>
        </div>
      </div>

      <!-- Today's Summary -->
      <section class="summary-section summary-text-section">
        <h3 class="section-title">今日总结</h3>
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
          <div class="device-stat-screenshots">{{ device.screenshots }} 张截图</div>
        </div>
      </div>
    </section>

    <!-- Parallel Work Analysis -->
    <section class="summary-section parallel-section" v-if="parallelWork.length > 0">
      <h3 class="section-title">并行工作分析</h3>
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
      <h3 class="section-title">设备使用模式</h3>
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
  </div>
</template>

<script setup>
import { ref, computed, watch } from 'vue'
import { useActivityStore } from '../stores/activity'
import OSIcons from './icons/OSIcons.vue'
import { Loading, Refresh } from '@element-plus/icons-vue'
import { invoke } from '@tauri-apps/api/core'
import { ElMessage } from 'element-plus'

const store = useActivityStore()

// 总结数据（从后端获取）
const summaryData = ref(null)
const loading = ref(false)
const refreshing = ref(false)

// 获取总结数据
const fetchSummary = async (forceRefresh = false) => {
  loading.value = true
  try {
    const data = await invoke('get_day_summary', {
      date: store.selectedDate,
      forceRefresh
    })
    summaryData.value = data
  } catch (error) {
    console.error('获取总结数据失败:', error)
    summaryData.value = null
  } finally {
    loading.value = false
  }
}

// 刷新总结（强制重新生成）
const refreshSummary = async () => {
  refreshing.value = true
  try {
    await fetchSummary(true)
    ElMessage.success('总结已重新生成')
  } catch (error) {
    ElMessage.error('刷新失败: ' + error)
  } finally {
    refreshing.value = false
  }
}

// 监听日期变化，重新获取总结
watch(() => store.selectedDate, () => {
  fetchSummary()
}, { immediate: true })

// 活跃设备数量
const activeDeviceCount = computed(() => {
  return summaryData.value?.activeDeviceCount || 0
})

// 今日总结
const todaySummary = computed(() => {
  return summaryData.value?.summaryText || null
})

// 设备统计
const deviceStats = computed(() => {
  return summaryData.value?.deviceStats || []
})

// 并行工作分析
const parallelWork = computed(() => {
  return summaryData.value?.parallelWork || []
})

// 设备使用模式
const deviceUsagePatterns = computed(() => {
  return summaryData.value?.usagePatterns || []
})

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
</script>

<style scoped>
.summary-container {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  padding: 32px;
  background: #1a1a1a;
  border-radius: 8px;
  border: 1px solid #2d2d2d;
  color: #e0e0e0;
}

.loading-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: #909399;
}

.loading-container p {
  margin-top: 16px;
  font-size: 14px;
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

.header-right {
  display: flex;
  align-items: center;
  gap: 12px;
}

.refresh-button {
  background: #2d2d2d;
  border: 1px solid #3d3d3d;
  color: #e0e0e0;
  width: 36px;
  height: 36px;
  padding: 0;
  transition: all 0.3s ease;
}

.refresh-button:hover {
  background: #3d3d3d;
  border-color: #4d4d4d;
  color: #ffffff;
  transform: rotate(180deg);
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
  display: grid;
  grid-template-columns: repeat(3, 1fr); /* 一行三列 */
  gap: 12px;
  margin-top: 16px;
}

.parallel-work-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 16px 18px;
  background: #1a1a1a;
  border-radius: 8px;
  border: 1px solid #2d2d2d;
  transition: all 0.25s ease;
}

.parallel-work-card:hover {
  background: #1f1f1f;
  border-color: #3d3d3d;
  transform: translateY(-2px);
}

.parallel-time-badge {
  background: #8b3838;
  color: white;
  padding: 5px 11px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
  letter-spacing: 0.3px;
  align-self: flex-start;
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
