<!-- 日历视图组件 - 显示每日活动概览 -->

<template>
  <div class="calendar-container">
    <div class="calendar-header">
      <h2>活动日历</h2>
      <el-button-group>
        <el-button @click="previousMonth">
          <el-icon><ArrowLeft /></el-icon>
        </el-button>
        <el-button @click="currentMonth">本月</el-button>
        <el-button @click="nextMonth">
          <el-icon><ArrowRight /></el-icon>
        </el-button>
      </el-button-group>
    </div>

    <el-calendar v-model="currentDate" ref="calendar">
      <template #date-cell="{ data }">
        <div
          class="calendar-cell"
          @click="selectDate(data.day)"
          :class="{
            'is-selected': isSelectedDate(data.day),
            'is-prev-month': data.type === 'prev-month',
            'is-next-month': data.type === 'next-month'
          }"
        >
          <div class="date">{{ data.day.split('-').slice(2).join('') }}</div>
          <div class="activities-preview" v-if="getDateActivities(data.day)">
            <ActivitySummary :activity="getDateActivities(data.day)" />
          </div>
        </div>
      </template>
    </el-calendar>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { ArrowLeft, ArrowRight } from '@element-plus/icons-vue'
import { useActivityStore } from '../stores/activity'
import dayjs from 'dayjs'
import ActivitySummary from './ActivitySummary.vue'

const store = useActivityStore()
const calendar = ref()
const currentDate = ref(new Date())
const refreshTimer = ref(null) // 定时刷新定时器

const emit = defineEmits(['date-select'])

// 获取某日的活动数据
const getDateActivities = (dateStr) => {
  return store.activities.find(a => a.date === dateStr)
}

// 判断是否是选中的日期
const isSelectedDate = (dateStr) => {
  return dateStr === store.selectedDate
}

// 选择日期
const selectDate = async (dateStr) => {
  await store.fetchDaySessions(dateStr)
  emit('date-select', new Date(dateStr))
}

// 上个月
const previousMonth = () => {
  const current = dayjs(currentDate.value)
  currentDate.value = current.subtract(1, 'month').toDate()
  refreshActivities()
}

// 下个月
const nextMonth = () => {
  const current = dayjs(currentDate.value)
  currentDate.value = current.add(1, 'month').toDate()
  refreshActivities()
}

// 回到本月
const currentMonth = () => {
  currentDate.value = new Date()
  refreshActivities()
}

// 刷新活动数据
const refreshActivities = async () => {
  const current = dayjs(currentDate.value)
  const startDate = current.startOf('month').format('YYYY-MM-DD')
  const endDate = current.endOf('month').format('YYYY-MM-DD')
  await store.fetchActivities(startDate, endDate)
}

// 启动定时刷新
const startRefreshTimer = () => {
  // 清除旧的定时器
  if (refreshTimer.value) {
    clearInterval(refreshTimer.value)
  }
  // 每分钟刷新一次
  refreshTimer.value = setInterval(() => {
    console.log('[CalendarView] 自动刷新日历数据')
    refreshActivities()
  }, 60000)
}

// 处理窗口激活事件
const handleWindowFocus = () => {
  console.log('[CalendarView] 窗口被激活，刷新日历数据')
  refreshActivities()
}

// 处理页面可见性变化
const handleVisibilityChange = () => {
  if (!document.hidden) {
    console.log('[CalendarView] 页面变为可见，刷新日历数据')
    refreshActivities()
  }
}

// 监听当前日期变化
watch(currentDate, () => {
  refreshActivities()
})

onMounted(() => {
  refreshActivities()
  startRefreshTimer()

  // 添加事件监听
  window.addEventListener('focus', handleWindowFocus)
  document.addEventListener('visibilitychange', handleVisibilityChange)
})

onUnmounted(() => {
  // 清除定时器
  if (refreshTimer.value) {
    clearInterval(refreshTimer.value)
  }
  // 移除事件监听
  window.removeEventListener('focus', handleWindowFocus)
  document.removeEventListener('visibilitychange', handleVisibilityChange)
})
</script>

<style scoped>
.calendar-container {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: white;
  border-radius: 8px;
  padding: 20px;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.1);
}

.calendar-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.calendar-header h2 {
  margin: 0;
  color: #303133;
}

:deep(.el-calendar) {
  flex: 1;
}

.calendar-cell {
  height: 100%;
  padding: 8px;
  cursor: pointer;
  transition: all 0.3s;
  border-radius: 4px;
  position: relative;
}

.calendar-cell:hover {
  background-color: #f5f7fa;
}

.calendar-cell.is-selected {
  background-color: #ecf5ff;
  border: 1px solid #409eff;
}

/* 上月和下月日期样式 */
.calendar-cell.is-prev-month,
.calendar-cell.is-next-month {
  opacity: 0.4;
  background-color: #fafafa;
}

.calendar-cell.is-prev-month .date,
.calendar-cell.is-next-month .date {
  color: #c0c4cc;
}

.calendar-cell.is-prev-month:hover,
.calendar-cell.is-next-month:hover {
  background-color: #f5f7fa;
  opacity: 0.6;
}

.date {
  font-size: 14px;
  font-weight: bold;
  color: #303133;
  margin-bottom: 4px;
}

.activities-preview {
  margin-top: 4px;
}

:deep(.el-calendar__body) {
  padding: 12px 0;
}

:deep(.el-calendar-table) {
  width: 100%;
}

:deep(.el-calendar-table td) {
  border: 1px solid #ebeef5;
}

:deep(.el-calendar-table td.is-today) {
  background-color: #f0f9ff;
}

:deep(.el-calendar-table .el-calendar-day) {
  height: 80px;
  padding: 0;
}
</style>