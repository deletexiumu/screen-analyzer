<!-- 主应用组件 -->

<template>
  <div id="app">
    <el-container>
      <!-- 顶部栏 -->
      <el-header class="app-header">
        <div class="header-left">
          <h1>Screen Summary</h1>
          <div class="date-selector">
            <el-date-picker
              v-model="selectedDateObj"
              type="date"
              placeholder="选择日期"
              :clearable="false"
              format="YYYY/MM/DD"
              value-format="YYYY-MM-DD"
              @change="handleDateChange"
              class="dark-date-picker"
            />
          </div>
          <div class="status-info">
            <el-tag
              :type="store.isCapturing ? 'success' : 'danger'"
              effect="dark"
            >
              <el-icon :class="{ 'is-blinking': store.isCapturing }">
                <VideoCameraFilled />
              </el-icon>
              {{ store.isCapturing ? '正在截屏' : '已暂停' }}
            </el-tag>
          </div>
        </div>

        <div class="header-actions">
          <el-button @click="showSettings = true" class="icon-button">
            <el-icon><Setting /></el-icon>
            Settings
          </el-button>
          <el-button @click="handleExport" class="export-button" type="primary">
            Export
          </el-button>
        </div>
      </el-header>

      <!-- 主内容区 -->
      <el-main class="app-main">
        <div class="main-content">
          <!-- 左侧活动列表 -->
          <div class="left-panel">
            <ActivityListView
              :date="store.selectedDate"
              @session-click="handleSessionClick"
            />
          </div>

          <!-- 右侧总结 -->
          <div class="right-panel">
            <SummaryView />
          </div>
        </div>
      </el-main>

      <!-- 底部状态栏 -->
      <el-footer class="app-footer">
        <div class="footer-stats">
          <span>会话数: {{ store.systemStatus.storage_usage.session_count }}</span>
          <el-divider direction="vertical" />
          <span>帧数: {{ store.systemStatus.storage_usage.frame_count }}</span>
          <el-divider direction="vertical" />
          <span>存储: {{ store.formattedStorageUsage.total }}</span>
          <el-divider direction="vertical" />
          <span>保留天数: {{ store.appConfig.retention_days }}天</span>
        </div>
        <div class="footer-info">
          <span v-if="store.systemStatus.last_capture_time">
            最后截屏: {{ formatTime(store.systemStatus.last_capture_time) }}
          </span>
        </div>
      </el-footer>
    </el-container>

    <!-- 会话详情对话框 -->
    <SessionDetail
      v-model="showSessionDetail"
      :session-id="selectedSessionId"
      @close="handleSessionDetailClose"
    />

    <!-- 设置对话框 -->
    <SettingsDialog v-model="showSettings" />
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import {
  VideoCameraFilled,
  VideoPause,
  VideoPlay,
  MagicStick,
  Setting,
  Loading,
  Camera,
  More
} from '@element-plus/icons-vue'
import { useActivityStore } from './stores/activity'
import ActivityListView from './components/ActivityListView.vue'
import SummaryView from './components/SummaryView.vue'
import SessionDetail from './components/SessionDetail.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import dayjs from 'dayjs'
import { invoke } from '@tauri-apps/api/core'

const store = useActivityStore()

const showSessionDetail = ref(false)
const selectedSessionId = ref(null)
const showSettings = ref(false)
const statusTimer = ref(null)
const refreshTimer = ref(null) // 定时刷新数据
const selectedDateObj = ref(dayjs().format('YYYY-MM-DD'))

// 格式化时间
const formatTime = (timestamp) => {
  return dayjs(timestamp).format('HH:mm:ss')
}

// 切换截屏状态
const handleToggleCapture = async () => {
  const newState = !store.isCapturing
  const action = newState ? '恢复' : '暂停'

  try {
    await ElMessageBox.confirm(
      `确定要${action}截屏吗？`,
      '确认操作',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }
    )

    await store.toggleCapture(newState)
  } catch (error) {
    if (error !== 'cancel') {
      console.error('Failed to toggle capture:', error)
    }
  }
}

// 测试截屏功能
const handleTestCapture = async () => {
  try {
    const result = await invoke('test_capture')
    ElMessage.success(result)
  } catch (error) {
    ElMessage.error('截屏测试失败: ' + error)
    console.error('Test capture failed:', error)
  }
}

// 手动触发分析
const handleTriggerAnalysis = async () => {
  try {
    await ElMessageBox.confirm(
      '确定要手动触发分析吗？这将分析当前会话的所有截图。',
      '手动分析',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'info'
      }
    )

    await store.triggerAnalysis()
  } catch (error) {
    if (error !== 'cancel') {
      console.error('Failed to trigger analysis:', error)
    }
  }
}

// 处理更多操作下拉菜单
const handleMoreCommand = (command) => {
  switch (command) {
    case 'analyze':
      handleTriggerAnalysis()
      break
    case 'test-capture':
      handleTestCapture()
      break
  }
}

// 处理日期选择
const handleDateSelect = (date) => {
  console.log('Date selected:', date)
}

// 处理日期变化
const handleDateChange = (date) => {
  store.selectedDate = date
  store.fetchDaySessions(date)
}

// 处理导出
const handleExport = () => {
  ElMessage.info('导出功能开发中...')
}

// 处理会话点击
const handleSessionClick = (session) => {
  selectedSessionId.value = session.id
  showSessionDetail.value = true
}

// 处理会话详情关闭
const handleSessionDetailClose = () => {
  showSessionDetail.value = false
  selectedSessionId.value = null
}

// 定期更新系统状态
const startStatusTimer = () => {
  statusTimer.value = setInterval(() => {
    store.fetchSystemStatus()
  }, 5000) // 每5秒更新一次
}

// 定期刷新数据（每分钟）
const startRefreshTimer = () => {
  refreshTimer.value = setInterval(() => {
    refreshData()
  }, 60000) // 每60秒刷新一次
}

// 刷新当前数据
const refreshData = async () => {
  console.log('自动刷新数据...')
  // 刷新当天会话列表
  await store.fetchDaySessions(store.selectedDate)
  // TODO: 刷新月度摘要 - 需要后端实现该API
  // await store.fetchMonthlySummary(dayjs(store.selectedDate).format('YYYY-MM'))
  // 如果有选中的会话，刷新会话详情
  if (store.selectedSession?.session?.id) {
    await store.fetchSessionDetail(store.selectedSession.session.id)
  }
}

// 处理窗口激活事件
const handleWindowFocus = () => {
  console.log('窗口被激活，刷新数据...')
  refreshData()
  // 重新同步系统状态
  store.fetchSystemStatus()
}

// 处理窗口可见性变化
const handleVisibilityChange = () => {
  if (!document.hidden) {
    console.log('页面变为可见，刷新数据...')
    refreshData()
  }
}

// 初始化
onMounted(async () => {
  await store.initialize()
  startStatusTimer()
  startRefreshTimer()

  // 监听窗口激活事件
  window.addEventListener('focus', handleWindowFocus)
  // 监听页面可见性变化
  document.addEventListener('visibilitychange', handleVisibilityChange)
})

// 清理
onUnmounted(() => {
  if (statusTimer.value) {
    clearInterval(statusTimer.value)
  }
  if (refreshTimer.value) {
    clearInterval(refreshTimer.value)
  }
  // 移除事件监听
  window.removeEventListener('focus', handleWindowFocus)
  document.removeEventListener('visibilitychange', handleVisibilityChange)
})
</script>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

#app {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  height: 100vh;
  background: #0f0f0f;
}

.el-container {
  height: 100%;
}

.app-header {
  background: #1a1a1a;
  border-bottom: 1px solid #2d2d2d;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 32px;
  z-index: 100;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 24px;
}

.header-left h1 {
  font-size: 20px;
  color: #ffffff;
  font-weight: 600;
  margin: 0;
}

.date-selector {
  display: flex;
  align-items: center;
  gap: 12px;
}

.status-info {
  display: flex;
  gap: 10px;
}

.header-actions {
  display: flex;
  gap: 12px;
  align-items: center;
}

.icon-button {
  background: transparent;
  border: 1px solid #3d3d3d;
  color: #e0e0e0;
}

.icon-button:hover {
  background: #2d2d2d;
  border-color: #4d4d4d;
  color: #ffffff;
}

.export-button {
  background: #409EFF;
  border-color: #409EFF;
  color: white;
}

.export-button:hover {
  background: #66b1ff;
  border-color: #66b1ff;
}

.app-main {
  padding: 20px;
  overflow: hidden;
  background: #0f0f0f;
}

.main-content {
  height: 100%;
  display: flex;
  flex-direction: row;
  gap: 20px;
}

.left-panel {
  flex: 0 0 33.33%; /* 大约占 1/3 宽度 */
  height: 100%;
  overflow-y: auto;
  min-width: 0; /* 允许内容收缩 */
}

.right-panel {
  flex: 1; /* 占据剩余空间 */
  height: 100%;
  overflow-y: auto;
  min-width: 0; /* 允许内容收缩 */
}

.app-footer {
  background: #1a1a1a;
  border-top: 1px solid #2d2d2d;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 32px;
  font-size: 13px;
  color: #909399;
}

.footer-stats {
  display: flex;
  align-items: center;
  gap: 10px;
}

.footer-info {
  font-size: 12px;
}

/* 动画效果 */
@keyframes blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

.is-blinking {
  animation: blink 1.5s infinite;
}

@keyframes rotate {
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
}

.is-loading {
  animation: rotate 2s linear infinite;
}

/* Element Plus 黑色主题调整 */
.el-button {
  border-radius: 6px;
}

.el-tag {
  border-radius: 4px;
}

.el-card {
  border-radius: 8px;
  border: none;
  background: #1a1a1a;
  color: #e0e0e0;
}

.el-dialog {
  border-radius: 12px;
  background: #1a1a1a;
  border: 1px solid #2d2d2d;
}

.el-dialog__header {
  background: #1a1a1a;
  border-bottom: 1px solid #2d2d2d;
}

.el-dialog__title {
  color: #ffffff;
}

.el-dialog__body {
  background: #1a1a1a;
  color: #e0e0e0;
}

.el-empty__description {
  color: #666666;
}

/* 日期选择器黑色主题 */
.dark-date-picker {
  --el-color-primary: #409EFF;
  --el-fill-color-blank: #2d2d2d;
  --el-text-color-primary: #e0e0e0;
  --el-border-color: #3d3d3d;
}

.dark-date-picker .el-input__wrapper {
  background: #2d2d2d;
  border-color: #3d3d3d;
  box-shadow: none;
}

.dark-date-picker .el-input__wrapper:hover {
  background: #2d2d2d;
  border-color: #4d4d4d;
}

.dark-date-picker .el-input__inner {
  color: #e0e0e0;
}

/* 滚动条样式 */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: #1a1a1a;
}

::-webkit-scrollbar-thumb {
  background: #3d3d3d;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: #4d4d4d;
}

/* Element Plus Dropdown 黑色主题 */
.el-dropdown-menu {
  background: #2d2d2d;
  border: 1px solid #3d3d3d;
}

.el-dropdown-menu__item {
  color: #e0e0e0;
}

.el-dropdown-menu__item:hover {
  background: #3d3d3d;
  color: #ffffff;
}

/* Element Plus Popover 黑色主题 */
.el-popover {
  background: #2d2d2d;
  border: 1px solid #3d3d3d;
  color: #e0e0e0;
}

/* Element Plus Message Box 黑色主题 */
.el-message-box {
  background: #2d2d2d;
  border: 1px solid #3d3d3d;
}

.el-message-box__title {
  color: #ffffff;
}

.el-message-box__content {
  color: #e0e0e0;
}

/* Element Plus Switch 黑色主题 */
.el-switch {
  --el-switch-off-color: #3d3d3d;
}

.el-switch__label {
  color: #909399;
}

.el-switch__label.is-active {
  color: #e0e0e0;
}

/* 响应式调整 */
@media (max-width: 1200px) {
  .main-content .el-col:first-child {
    width: 100%;
    margin-bottom: 20px;
  }

  .main-content .el-col:last-child {
    width: 100%;
  }
}
</style>