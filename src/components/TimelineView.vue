<!-- 时间线视图组件 - 显示一天的会话时间线 -->

<template>
  <div class="timeline-container">
    <div class="timeline-header">
      <div class="timeline-title">
        <el-icon style="color: #409eff; margin-right: 4px;"><Calendar /></el-icon>
        <h3>{{ simpleDateFormat }}</h3>
        <el-tag v-if="mergeStats && enableAggregation && mergeStats.savedCards > 0" type="success" size="small" class="merge-badge">
          已合并 {{ mergeStats.savedCards }} 项
        </el-tag>
      </div>

      <div class="timeline-actions">
        <!-- 聚合开关 -->
        <el-switch
          v-model="enableAggregation"
          active-text="智能聚合"
          inactive-text="全部显示"
          inline-prompt
          style="--el-switch-on-color: #409eff; --el-switch-off-color: #c0c4cc;"
        />

        <!-- 更多操作下拉菜单 -->
        <el-dropdown trigger="click" @command="handleCommand">
          <el-button size="small" circle>
            <el-icon><More /></el-icon>
          </el-button>
          <template #dropdown>
            <el-dropdown-menu>
              <el-dropdown-item command="regenerate">
                <el-icon><RefreshRight /></el-icon>
                重新生成时间线
              </el-dropdown-item>
              <el-dropdown-item command="refresh">
                <el-icon><Refresh /></el-icon>
                刷新数据
              </el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
      </div>
    </div>

    <div class="timeline-content" v-loading="store.loading.sessions">
      <!-- 时间线显示 -->
      <el-scrollbar v-if="sessions.length > 0" class="timeline-scrollbar">
        <div class="timeline-chart">
          <!-- 时间刻度 -->
          <div class="time-scale">
            <div
              v-for="hour in timeScale"
              :key="hour"
              class="time-tick"
              :style="{ top: getTimePosition(hour) + 'px' }"
            >
              <span class="time-label">{{ formatHour(hour) }}</span>
              <div class="time-line"></div>
            </div>
          </div>

          <!-- 当前时间指示线 -->
          <div
            v-if="showCurrentTimeLine"
            class="current-time-line"
            :style="currentTimeLineStyle"
          >
            <div class="current-time-label">{{ currentTimeLabel }}</div>
            <div class="current-time-bar"></div>
          </div>

          <!-- 活动区块 -->
          <div class="activity-blocks">
            <!-- 会话区块（仅在没有卡片数据时作为回退展示） -->
            <template v-if="showSessionFallback">
              <div
                v-for="session in sessions"
                :key="session.id"
                class="activity-block session-block"
                :class="{ 'is-active': isActiveSession(session) }"
                :style="getBlockStyle(session)"
                @click="selectSession(session)"
                @mouseenter="(e) => handleMouseEnter(e, session)"
                @mouseleave="hoveredSession = null"
                @mousemove="(e) => updateTooltipPosition(e)"
              >
                <div class="block-content">
                  <div class="block-header">
                    <OSIcons
                      v-if="session.device_type"
                      :type="getDeviceIcon(session.device_type)"
                      :size="14"
                      :style="{ color: getDeviceColor(session.device_name), flexShrink: 0 }"
                    />
                    <el-tag
                      v-if="parseSessionTags(session.tags)[0]"
                      size="small"
                      :color="getSessionColor(session)"
                      effect="dark"
                      class="block-tag"
                    >
                      {{ getCategoryName(parseSessionTags(session.tags)[0]?.category) }}
                    </el-tag>
                    <div class="block-title">
                      <span class="block-title-text">{{ getSessionDisplayTitle(session) }}</span>
                      <span class="block-time">
                        {{ formatTime(session.start_time) }} - {{ formatTime(session.end_time) }}
                      </span>
                    </div>
                  </div>
                  <div class="block-footer">
                    <div class="block-duration">
                      <el-icon size="10"><Timer /></el-icon>
                      {{ formatDuration(session.start_time, session.end_time) }}
                    </div>
                  </div>
                  <div v-if="session.video_path" class="block-icon">
                    <el-icon><VideoPlay /></el-icon>
                  </div>
                </div>
              </div>
            </template>

            <!-- 时间线卡片区块 -->
            <template v-for="(card, index) in displayTimelineCards" :key="card.id || `card-${index}`">
              <div
                class="activity-block timeline-card-block"
                :class="{ 'is-merged': card.mergedCount && card.mergedCount > 1 }"
                :style="getTimelineCardStyle(card)"
                @click="selectTimelineCard(card)"
                @mouseenter="(e) => handleCardMouseEnter(e, card)"
                @mouseleave="hoveredCard = null"
                @mousemove="(e) => updateCardTooltipPosition(e)"
              >
                <div class="block-content">
                <div class="block-header">
                  <OSIcons
                    v-if="card.device_type"
                    :type="getDeviceIcon(card.device_type)"
                    :size="14"
                    :style="{ color: getDeviceColor(card.device_name), flexShrink: 0 }"
                  />
                  <el-tag
                    size="small"
                    :color="getCategoryColor(card.category || 'Other')"
                    effect="dark"
                    class="block-tag"
                  >
                    {{ getCategoryName(card.category || 'Other') }}
                  </el-tag>
                  <div class="block-title">
                    <span class="block-title-text">{{ getCardDisplayTitle(card) }}</span>
                    <span v-if="card.mergedCount > 1" class="merged-count">×{{ card.mergedCount }}</span>
                    <span class="block-time">
                      {{ formatTime(card.start_time) }} - {{ formatTime(card.end_time) }}
                    </span>
                  </div>
                </div>
                  <div class="block-footer">
                    <div class="block-duration">
                      <el-icon size="10"><Timer /></el-icon>
                      {{ formatDuration(card.start_time, card.end_time) }}
                    </div>
                  </div>
                </div>
              </div>
            </template>
          </div>
        </div>
      </el-scrollbar>

      <el-empty v-else description="当天暂无活动记录" />
    </div>

    <!-- 悬浮提示框 -->
    <transition name="fade">
      <div
        v-if="hoveredSession || hoveredCard"
        class="session-tooltip"
        :style="tooltipStyle"
      >
        <!-- 会话悬浮提示 -->
        <template v-if="hoveredSession">
          <div class="tooltip-header">
            <h4>{{ hoveredSession.title }}</h4>
            <el-tag size="small" :color="getSessionColor(hoveredSession)">
              {{ getCategoryName(parseSessionTags(hoveredSession.tags)[0]?.category) }}
            </el-tag>
          </div>
          <div class="tooltip-summary">{{ hoveredSession.summary }}</div>
          <div class="tooltip-meta">
            <div class="tooltip-duration">
              <el-icon><Timer /></el-icon>
              {{ formatDuration(hoveredSession.start_time, hoveredSession.end_time) }}
            </div>
            <div v-if="hoveredSession.device_name" class="tooltip-device">
              <OSIcons
                :type="getDeviceIcon(hoveredSession.device_type)"
                :size="14"
                :style="{ color: getDeviceColor(hoveredSession.device_name) }"
              />
              <span :style="{ color: getDeviceColor(hoveredSession.device_name) }">
                {{ hoveredSession.device_name }}
              </span>
            </div>
          </div>
          <div class="tooltip-actions">
            <el-button size="small" @click.stop="viewDetail(hoveredSession)">
              查看详情
            </el-button>
            <el-button
              v-if="!hoveredSession.video_path"
              size="small"
              @click.stop="generateVideo(hoveredSession)"
            >
              生成视频
            </el-button>
          </div>
        </template>

        <!-- 时间线卡片悬浮提示 -->
        <template v-if="hoveredCard && !hoveredSession">
          <div class="tooltip-header">
            <h4>{{ getCardDisplayTitle(hoveredCard) }}</h4>
            <el-tag size="small" :color="getCategoryColor(hoveredCard.category || 'Other')">
              {{ getCategoryName(hoveredCard.category || 'Other') }}
            </el-tag>
            <span v-if="hoveredCard.mergedCount > 1" class="tooltip-merged-count">
              ×{{ hoveredCard.mergedCount }}
            </span>
          </div>
          <div v-if="hoveredCard.summary" class="tooltip-summary">{{ hoveredCard.summary }}</div>
          <div class="tooltip-meta">
            <div class="tooltip-duration">
              <el-icon><Timer /></el-icon>
              {{ formatDuration(hoveredCard.start_time, hoveredCard.end_time) }}
            </div>
            <div v-if="hoveredCard.device_name" class="tooltip-device">
              <OSIcons
                :type="getDeviceIcon(hoveredCard.device_type)"
                :size="14"
                :style="{ color: getDeviceColor(hoveredCard.device_name) }"
              />
              <span :style="{ color: getDeviceColor(hoveredCard.device_name) }">
                {{ hoveredCard.device_name }}
              </span>
            </div>
          </div>
          <div class="tooltip-actions">
            <el-button size="small" @click.stop="selectTimelineCard(hoveredCard)">
              查看详情
            </el-button>
          </div>
        </template>
      </div>
    </transition>
  </div>
</template>

<script setup>
import { computed, watch, ref, onMounted, onUnmounted, nextTick } from 'vue'
import { Refresh, RefreshRight, Timer, VideoPlay, More, Calendar } from '@element-plus/icons-vue'
import OSIcons from './icons/OSIcons.vue'
import { useActivityStore } from '../stores/activity'
import { ElMessage, ElMessageBox } from 'element-plus'
import { invoke } from '@tauri-apps/api/core'
import dayjs from 'dayjs'

const props = defineProps({
  date: {
    type: String,
    required: true
  },
  selectedDevice: {
    type: String,
    default: 'all'
  }
})

const emit = defineEmits(['session-click'])

const store = useActivityStore()

// 重新生成时间线的加载状态
const regeneratingTimeline = ref(false)

// 悬浮的会话
const hoveredSession = ref(null)

// 悬浮的时间线卡片
const hoveredCard = ref(null)

// 提示框样式
const tooltipStyle = ref({})

// 当前时间线
const currentTimeLabel = ref('')
const currentTimeLineStyle = ref({})
const showCurrentTimeLine = ref(false)

// 更新当前时间线
const updateCurrentTimeLine = () => {
  const now = dayjs()
  const currentHour = now.hour()
  const currentMinute = now.minute()
  const currentTime = currentHour + currentMinute / 60

  // 检查是否在时间轴范围内
  if (currentTime >= TIMELINE_START_HOUR && currentTime <= TIMELINE_END_HOUR) {
    showCurrentTimeLine.value = true
    currentTimeLabel.value = now.format('HH:mm')
    currentTimeLineStyle.value = {
      top: `${getTimePosition(currentTime)}px`
    }
  } else {
    showCurrentTimeLine.value = false
  }
}

// 时间轴配置
const TIMELINE_START_HOUR = 0             // 开始时间（小时）- 从0点开始显示完整24小时
const TIMELINE_END_HOUR = 23              // 结束时间（小时）
const HOUR_HEIGHT = 120                   // 每小时的高度（像素） - 调整高度以适应24小时
const TIMELINE_PADDING = 60               // 时间轴顶部和底部的内边距
const TIMELINE_OVERLAP_BUFFER = 10        // 判定重叠时的缓冲像素
const TIMELINE_COLUMN_GAP_PERCENT = 2     // 同一时间段多列展示时的列间距（百分比）
const MIN_SESSION_HEIGHT = 56             // 会话块最小高度
const MIN_CARD_HEIGHT = 28                // 时间线卡片的推荐最小高度
const MIN_CARD_VISUAL_HEIGHT = 16         // 时间线卡片可视化最小高度，防止过细
const TIMELINE_CARD_VERTICAL_GAP = 6      // 卡片之间的垂直留白
const MERGE_MAX_GAP_MINUTES = 5           // 合并同标题卡片允许的最大时间间隔（分钟） - 减小到5分钟避免过度合并

// 格式化的日期
const formattedDate = computed(() => {
  return dayjs(props.date).format('YYYY年MM月DD日')
})

// 简化的日期格式（用于头部显示）
const simpleDateFormat = computed(() => {
  const targetDate = dayjs(props.date)
  const today = dayjs().startOf('day')
  const diff = targetDate.diff(today, 'day')

  // 相对日期显示
  if (diff === 0) return '今天'
  if (diff === -1) return '昨天'
  if (diff === -2) return '前天'
  if (diff === 1) return '明天'

  // 一周内显示周几
  if (diff >= -7 && diff <= 7) {
    const weekdays = ['周日', '周一', '周二', '周三', '周四', '周五', '周六']
    return `${weekdays[targetDate.day()]} ${targetDate.format('M/D')}`
  }

  // 其他日期
  return targetDate.format('M月D日')
})

// 处理下拉菜单命令
const handleCommand = (command) => {
  if (command === 'regenerate') {
    regenerateTimeline()
  } else if (command === 'refresh') {
    refreshSessions()
  }
}

// 时间刻度数组
const timeScale = computed(() => {
  const scale = []
  for (let hour = TIMELINE_START_HOUR; hour <= TIMELINE_END_HOUR; hour++) {
    scale.push(hour)
  }
  return scale
})

// 格式化小时
const formatHour = (hour) => {
  return `${hour.toString().padStart(2, '0')}:00`
}

// 会话列表（根据选中的设备过滤）
const sessions = computed(() => {
  if (props.selectedDevice === 'all') {
    return store.daySessions
  }
  return store.daySessions.filter(session =>
    session.device_name === props.selectedDevice
  )
})

// 是否启用智能聚合
const enableAggregation = ref(true)


// 时间线卡片列表（优先从timeline_cards获取，否则从会话数据生成）
const rawTimelineCards = computed(() => {
  const cards = []

  sessions.value.forEach(session => {
    // 优先尝试使用 timeline_cards 字段
    if (session.timeline_cards) {
      try {
        const sessionCards = JSON.parse(session.timeline_cards)
        if (Array.isArray(sessionCards)) {
          sessionCards.forEach((card, idx) => {
            cards.push({
              ...card,
              id: `${session.id}-card-${idx}`,
              sessionId: session.id,
              sessionTitle: session.title
            })
          })
          return // 如果有 timeline_cards 就使用它
        }
      } catch (e) {
        console.error('解析时间线卡片失败:', e)
      }
    }

    // 如果没有 timeline_cards，从会话本身创建一个卡片
    const tags = parseSessionTags(session.tags)
    const category = tags?.[0]?.category || 'Work'

    cards.push({
      id: `session-${session.id}`,
      title: session.title || '未命名活动',
      category: category,
      summary: session.summary || '',
      detailed_summary: session.detailed_summary || '',
      start_time: session.start_time,
      end_time: session.end_time,
      sessionId: session.id,
      sessionTitle: session.title,
      video_preview_path: session.video_path,
      device_name: session.device_name,
      device_type: session.device_type
    })
  })

  if (cards.length === 0) {
    return []
  }

  const uniqueCards = cards.filter((card, index, self) =>
    index === self.findIndex((c) => (
      c.start_time === card.start_time &&
      c.end_time === card.end_time &&
      (c.title || '') === (card.title || '') &&
      (c.sessionId || '') === (card.sessionId || '')
    ))
  )

  return uniqueCards.sort((a, b) => new Date(a.start_time) - new Date(b.start_time))
})

// 过滤后的时间线卡片（移除空闲状态）
const filteredTimelineCards = computed(() => {
  if (rawTimelineCards.value.length === 0) {
    return []
  }

  return rawTimelineCards.value.filter(card => {
    const category = (card.category || 'Other').toLowerCase()
    return category !== 'idle'
  })
})

// 是否需要回退到会话区块展示
const showSessionFallback = computed(() => {
  return sessions.value.length > 0 && filteredTimelineCards.value.length === 0
})

// 显示的时间线卡片（根据是否聚合返回不同的数据）
const displayTimelineCards = computed(() => {
  const cards = filteredTimelineCards.value

  if (cards.length === 0) {
    return []
  }

  // 如果启用聚合，合并相同标题的卡片
  if (enableAggregation.value) {
    const mergedCards = mergeTimelineCardsByTitle(cards)
    return applyTimelineCardLayout(mergedCards)
  }

  // 不聚合时，直接返回应用布局后的卡片
  return applyTimelineCardLayout(cards)
})

// 聚合统计信息
const mergeStats = computed(() => {
  if (!enableAggregation.value) return null

  // 计算被合并的卡片数量
  let mergedCount = 0
  displayTimelineCards.value.forEach(card => {
    if (card.mergedCount && card.mergedCount > 1) {
      mergedCount += (card.mergedCount - 1)
    }
  })

  return {
    savedCards: mergedCount
  }
})

// 用于兼容旧代码的时间线卡片列表
const timelineCards = computed(() => {
  // 这个变为空数组，因为现在使用 displayTimelineCards
  return []
})

const normalizeCardTitle = (card) => {
  const source = (card.title && card.title !== 'null' && card.title !== 'undefined')
    ? card.title
    : card.summary || card.sessionTitle || ''
  return source
    .toString()
    .trim()
    .toLowerCase()
    .replace(/[\s·•,，。；;、!?！？：“”"'`~]+/g, '')
}

// 合并相邻且标题相同的卡片，减少时间线重复噪音
const mergeTimelineCardsByTitle = (cards) => {
  const merged = []
  let currentGroup = null

  const flushGroup = () => {
    if (!currentGroup) return
    merged.push(createMergedCardFromGroup(currentGroup, merged.length))
    currentGroup = null
  }

  cards.forEach(card => {
    const normalizedTitle = normalizeCardTitle(card)
    if (!normalizedTitle) {
      flushGroup()
      merged.push(createMergedCardFromGroup(createCardGroup(card, normalizedTitle), merged.length))
      currentGroup = null
      return
    }

    if (!currentGroup) {
      currentGroup = createCardGroup(card, normalizedTitle)
      return
    }

    if (!canMergeIntoGroup(currentGroup, card, normalizedTitle)) {
      flushGroup()
      currentGroup = createCardGroup(card, normalizedTitle)
      return
    }

    extendCardGroup(currentGroup, card)
  })

  flushGroup()
  return merged
}

const createCardGroup = (card, normalizedTitle) => ({
  normalizedTitle,
  category: card.category || 'Other',
  firstCard: { ...card },
  cards: [card],
  startTime: card.start_time,
  endTime: card.end_time,
  summaries: card.summary ? [card.summary.trim()] : [],
  detailedSummaries: card.detailed_summary ? [card.detailed_summary.trim()] : [],
  sessionIds: card.sessionId ? [card.sessionId] : [],
  videoPaths: card.video_preview_path ? [card.video_preview_path] : []
})

const extendCardGroup = (group, card) => {
  group.cards.push(card)

  if (dayjs(card.start_time).isBefore(dayjs(group.startTime))) {
    group.startTime = card.start_time
  }
  if (dayjs(card.end_time).isAfter(dayjs(group.endTime))) {
    group.endTime = card.end_time
  }

  if (card.summary && card.summary.trim()) {
    group.summaries.push(card.summary.trim())
  }

  if (card.detailed_summary && card.detailed_summary.trim()) {
    group.detailedSummaries.push(card.detailed_summary.trim())
  }

  if (card.sessionId) {
    group.sessionIds.push(card.sessionId)
  }

  if (card.video_preview_path) {
    group.videoPaths.push(card.video_preview_path)
  }
}

const canMergeIntoGroup = (group, card, normalizedTitle) => {
  if (!normalizedTitle) return false
  if (normalizedTitle !== group.normalizedTitle) return false
  const category = card.category || 'Other'
  if (category !== group.category) return false

  const lastEnd = dayjs(group.endTime)
  const currentStart = dayjs(card.start_time)
  const gapMinutes = currentStart.diff(lastEnd, 'minute', true)

  // 只合并真正连续的卡片（间隔小于等于5分钟）
  // 如果间隔是负数，说明有时间重叠，也应该合并
  return gapMinutes <= MERGE_MAX_GAP_MINUTES && gapMinutes >= -5
}

const createMergedCardFromGroup = (group, groupIndex) => {
  const sessionIds = Array.from(new Set(group.sessionIds.filter(Boolean)))
  const mergedVideoPaths = Array.from(new Set(group.videoPaths.filter(Boolean)))
  const uniqueSummaries = Array.from(new Set(group.summaries.filter(Boolean)))
  const uniqueDetailedSummaries = Array.from(new Set(group.detailedSummaries.filter(Boolean)))

  const mergedSummary = buildMergedSummary(uniqueSummaries)
  const mergedDetailed = buildMergedDetailedSummary(uniqueDetailedSummaries)

  const mergedCount = group.cards.length
  const baseId = group.firstCard.id || `card-${groupIndex}`

  return {
    ...group.firstCard,
    start_time: group.startTime,
    end_time: group.endTime,
    summary: mergedSummary || group.firstCard.summary,
    detailed_summary: mergedDetailed || group.firstCard.detailed_summary,
    mergedCount,
    mergedCards: group.cards,
    sessionIds,
    sessionId: sessionIds.length === 1 ? sessionIds[0] : null,
    mergedVideoPreviewPaths: mergedVideoPaths,
    video_preview_path: group.firstCard.video_preview_path || mergedVideoPaths[0] || null,
    id: mergedCount > 1 ? `merged-${groupIndex}-${baseId}` : baseId
  }
}

const buildMergedSummary = (summaries) => {
  if (!summaries || summaries.length === 0) return ''
  if (summaries.length === 1) return summaries[0]
  const maxItems = 3
  const preview = summaries.slice(0, maxItems).join(' / ')
  return summaries.length > maxItems ? `${preview} 等${summaries.length}条` : preview
}

const buildMergedDetailedSummary = (summaries) => {
  if (!summaries || summaries.length === 0) return ''
  if (summaries.length === 1) return summaries[0]
  const maxItems = 3
  const preview = summaries.slice(0, maxItems).map(item => `• ${item}`).join('\n')
  return summaries.length > maxItems ? `${preview}\n• ...共${summaries.length}条记录` : preview
}

// 根据时间、列数与垂直间距计算时间线卡片的布局
const applyTimelineCardLayout = (cards) => {
  const sorted = [...cards].sort((a, b) => new Date(a.start_time) - new Date(b.start_time))
  const activeCards = []

  sorted.forEach(card => {
    card._startPos = timeToPosition(card.start_time)
    card._endPos = timeToPosition(card.end_time)

    // 清理已经结束的卡片（使用实际结束时间而不是视觉结束位置）
    for (let i = activeCards.length - 1; i >= 0; i--) {
      const active = activeCards[i]
      // 使用实际的结束位置来判断是否重叠
      if (card._startPos >= active._endPos + TIMELINE_OVERLAP_BUFFER) {
        activeCards.splice(i, 1)
      }
    }

    // 检查与活动卡片的重叠，分配列
    let column = 0
    let foundColumn = false
    while (!foundColumn) {
      let canUseColumn = true
      for (const active of activeCards) {
        if (active._column === column) {
          // 检查时间范围是否重叠
          const overlap = !(card._startPos >= active._endPos + TIMELINE_OVERLAP_BUFFER ||
                           card._endPos <= active._startPos - TIMELINE_OVERLAP_BUFFER)
          if (overlap) {
            canUseColumn = false
            break
          }
        }
      }
      if (canUseColumn) {
        foundColumn = true
      } else {
        column++
      }
    }

    card._column = column
    card._rawHeight = Math.max(card._endPos - card._startPos, MIN_CARD_HEIGHT)
    card._height = card._rawHeight
    card._visualEnd = card._endPos  // 视觉结束位置等于实际结束位置
    activeCards.push(card)
  })

  const columns = new Map()
  sorted.forEach(card => {
    const column = card._column || 0
    if (!columns.has(column)) {
      columns.set(column, [])
    }
    columns.get(column).push(card)
  })

  // 不再调整高度，保持原始高度以避免合并卡片被压缩
  columns.forEach(cardsInColumn => {
    cardsInColumn.forEach((card) => {
      // 保持卡片的原始高度，确保合并的卡片显示完整的时间跨度
      const rawHeight = Math.max(card._endPos - card._startPos, MIN_CARD_HEIGHT)
      card._height = rawHeight
      card._visualEnd = card._endPos
    })
  })

  sorted.forEach(card => {
    const overlapping = sorted.filter(other => {
      if (other === card) return true
      return !(
        card._visualEnd + TIMELINE_OVERLAP_BUFFER <= other._startPos ||
        card._startPos >= other._visualEnd + TIMELINE_OVERLAP_BUFFER
      )
    })
    const maxColumnIndex = overlapping.length
      ? Math.max(...overlapping.map(item => item._column))
      : 0
    card._totalColumns = maxColumnIndex + 1
  })

  return sorted
}

// 格式化时间
const formatTime = (timestamp) => {
  return dayjs(timestamp).format('HH:mm')
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

// 解析会话标签
const parseSessionTags = (tagsJson) => {
  try {
    return JSON.parse(tagsJson) || []
  } catch {
    return []
  }
}

// 获取会话颜色
const getSessionColor = (session) => {
  const tags = parseSessionTags(session.tags)
  if (tags.length === 0) return '#909399'

  const mainCategory = tags[0].category
  return getCategoryColor(mainCategory)
}

// 获取时间在时间轴上的位置
const getTimePosition = (hour) => {
  return TIMELINE_PADDING + (hour - TIMELINE_START_HOUR) * HOUR_HEIGHT
}

// 将时间转换为时间轴上的位置
const timeToPosition = (timeStr) => {
  const time = dayjs(timeStr)
  const hours = time.hour()
  const minutes = time.minute()

  // 处理超出范围的时间
  if (hours < TIMELINE_START_HOUR) {
    return TIMELINE_PADDING
  }
  if (hours > TIMELINE_END_HOUR) {
    return TIMELINE_PADDING + (TIMELINE_END_HOUR - TIMELINE_START_HOUR + 1) * HOUR_HEIGHT
  }

  const totalMinutes = (hours - TIMELINE_START_HOUR) * 60 + minutes
  return TIMELINE_PADDING + (totalMinutes / 60) * HOUR_HEIGHT
}

// 获取活动区块的样式
const getBlockStyle = (session) => {
  const startPos = timeToPosition(session.start_time)
  const endPos = timeToPosition(session.end_time)
  const height = Math.max(endPos - startPos, MIN_SESSION_HEIGHT)

  const tags = parseSessionTags(session.tags)
  const mainCategory = tags[0]?.category || 'Other'
  const color = getCategoryColor(mainCategory)

  return {
    top: `${startPos}px`,
    height: `${height}px`,
    backgroundColor: color + '15', // 15%透明度，会话区块更淡
    borderColor: color,
    borderLeftColor: color,
    left: '0',
    right: '0'
  }
}

// 获取时间线卡片的样式
const getTimelineCardStyle = (card) => {
  const startPos = card._startPos ?? timeToPosition(card.start_time)
  const endPos = card._endPos ?? timeToPosition(card.end_time)
  const height = card._height ?? Math.max(endPos - startPos, MIN_CARD_HEIGHT)
  const color = getCategoryColor(card.category || 'Other')

  // 如果有设备信息，使用设备颜色作为左边框
  const deviceColor = card.device_name ? getDeviceColor(card.device_name) : color

  // 计算卡片的水平位置和宽度
  const column = card._column ?? 0
  const totalColumns = card._totalColumns ?? 1

  // 如果只有一列（没有重叠），占满宽度并左对齐
  if (totalColumns === 1) {
    return {
      top: `${startPos}px`,
      height: `${height}px`,
      backgroundColor: color + '20', // 20%透明度
      borderColor: color,
      borderLeftColor: deviceColor, // 使用设备颜色作为左边框
      borderLeftWidth: '4px',
      left: '0',
      width: '100%',
      zIndex: 3
    }
  }

  // 多列并列显示：第一列左对齐，其他列依次排列
  // 每个卡片宽度根据总列数平均分配，留出间距
  const gapPx = 8 // 列间距
  const totalGapPx = (totalColumns - 1) * gapPx

  // 计算可用宽度（假设容器宽度为100%，需要转换为calc）
  // 每列宽度 = (100% - 总间距) / 列数
  const columnWidthPercent = `calc((100% - ${totalGapPx}px) / ${totalColumns})`

  // 计算左边距
  // 第0列: 0
  // 第1列: (100% - 总间距) / 列数 + 间距
  // 第2列: 2 * ((100% - 总间距) / 列数 + 间距)
  const leftPosition = column === 0
    ? '0'
    : `calc(${column} * ((100% - ${totalGapPx}px) / ${totalColumns} + ${gapPx}px))`

  return {
    top: `${startPos}px`,
    height: `${height}px`,
    backgroundColor: color + '20', // 20%透明度
    borderColor: color,
    borderLeftColor: deviceColor, // 使用设备颜色作为左边框
    borderLeftWidth: '4px',
    left: leftPosition,
    width: columnWidthPercent,
    zIndex: 3
  }
}

// 选择时间线卡片
const selectTimelineCard = async (card) => {
  // 可以显示卡片详情
  console.log('选中时间线卡片:', card)
  const displayTitle = card.title || (card.summary ? card.summary.substring(0, 30) + '...' : '未命名活动')
  const mergedSuffix = card.mergedCount > 1 ? ` ×${card.mergedCount}` : ''

  // 获取所有相关的会话ID
  const candidateSessionIds = Array.isArray(card.sessionIds) && card.sessionIds.length > 0
    ? card.sessionIds
    : (card.sessionId ? [card.sessionId] : [])

  // 如果是聚合卡片且有多个会话，询问是否批量生成视频
  if (candidateSessionIds.length > 1) {
    try {
      await ElMessageBox.confirm(
        `该活动包含 ${candidateSessionIds.length} 个会话段，是否批量生成视频并清理无效会话？`,
        `批量处理: ${displayTitle}`,
        {
          confirmButtonText: '批量处理',
          cancelButtonText: '查看详情',
          type: 'info',
          distinguishCancelAndClose: true
        }
      )

      // 用户选择批量处理
      await batchProcessMergedSessions(card, candidateSessionIds)
    } catch (action) {
      if (action === 'cancel') {
        // 用户选择查看详情
        const targetSessionId = candidateSessionIds[0]
        if (targetSessionId) {
          const session = sessions.value.find(s => s.id === targetSessionId)
          if (session) {
            selectSession(session)
          }
        }
      }
    }
  } else {
    // 单个会话，直接显示详情
    ElMessage.info(`活动: ${displayTitle}${mergedSuffix}`)
    const targetSessionId = candidateSessionIds[0]
    if (targetSessionId) {
      const session = sessions.value.find(s => s.id === targetSessionId)
      if (session) {
        selectSession(session)
      }
    }
  }
}

// 获取卡片标题
const getCardTitle = (card) => {
  if (card.title && card.title.trim()) {
    return card.title
  }
  if (card.summary && card.summary.trim()) {
    // 如果没有标题，使用摘要的前20个字符
    return card.summary.length > 20 ? card.summary.substring(0, 20) + '...' : card.summary
  }
  return '活动记录'
}

// 获取卡片显示标题（用于界面显示）
const getCardDisplayTitle = (card) => {
  // 优先使用title
  if (card.title && card.title.trim() && card.title !== 'null' && card.title !== 'undefined') {
    const title = card.title.trim()
    return title.length > 35 ? title.substring(0, 35) + '...' : title
  }
  // 其次使用summary
  if (card.summary && card.summary.trim() && card.summary !== 'null' && card.summary !== 'undefined') {
    const summary = card.summary.trim()
    return summary.length > 35 ? summary.substring(0, 35) + '...' : summary
  }
  // 使用关联的会话标题
  if (card.sessionTitle && card.sessionTitle.trim()) {
    const sessionTitle = card.sessionTitle.trim()
    return sessionTitle.length > 35 ? sessionTitle.substring(0, 35) + '...' : sessionTitle
  }
  // 使用类别作为后备
  if (card.category) {
    return `${getCategoryName(card.category)}活动`
  }
  // 默认标题
  return '活动记录'
}

// 获取会话显示标题
const getSessionDisplayTitle = (session) => {
  // 优先使用title
  if (session.title && session.title.trim() && session.title !== 'null' && session.title !== 'undefined') {
    return session.title.length > 40 ? session.title.substring(0, 40) + '...' : session.title
  }
  // 其次使用summary
  if (session.summary && session.summary.trim() && session.summary !== 'null' && session.summary !== 'undefined') {
    return session.summary.length > 40 ? session.summary.substring(0, 40) + '...' : session.summary
  }
  // 解析tags获取类别
  const tags = parseSessionTags(session.tags)
  if (tags && tags.length > 0 && tags[0].category) {
    return `${getCategoryName(tags[0].category)}会话`
  }
  // 默认标题
  return '会话记录'
}

// 获取类别标签类型
const getCategoryTagType = (category) => {
  const types = {
    'Work': '',
    'Personal': 'success',
    'Break': 'warning',
    'Idle': 'info',
    'Meeting': 'danger',
    'Coding': '',
    'Research': 'success',
    'Communication': 'warning',
    'Entertainment': 'danger',
    'Other': 'info'
  }
  return types[category] || 'info'
}

// 处理鼠标移入会话
const handleMouseEnter = (event, session) => {
  hoveredSession.value = session
  hoveredCard.value = null
  updateTooltipPosition(event)
}

// 处理鼠标移入时间线卡片
const handleCardMouseEnter = (event, card) => {
  hoveredCard.value = card
  hoveredSession.value = null
  updateTooltipPosition(event)
}

// 更新提示框位置 - 跟随鼠标位置
const updateTooltipPosition = (event) => {
  if (!hoveredSession.value && !hoveredCard.value) return

  // 提示框在鼠标右侧显示
  const mouseX = event.clientX
  const mouseY = event.clientY

  // 检查是否会超出右边界
  const tooltipWidth = 350 // 提示框最大宽度
  const windowWidth = window.innerWidth

  let left = mouseX + 15 // 鼠标右侧15px

  // 如果超出右边界，则显示在鼠标左侧
  if (left + tooltipWidth > windowWidth - 20) {
    left = mouseX - tooltipWidth - 15
  }

  tooltipStyle.value = {
    top: `${mouseY - 30}px`, // 稍微偏上显示
    left: `${left}px`
  }
}

// 更新卡片提示框位置
const updateCardTooltipPosition = updateTooltipPosition

// 类别映射配置（支持新旧类别）
const categoryConfig = {
  // 新的6类标签
  'work': { name: '工作', emoji: '💼', color: '#409EFF' },
  'communication': { name: '沟通', emoji: '💬', color: '#FFC107' },
  'learning': { name: '学习', emoji: '📚', color: '#67C23A' },
  'personal': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'idle': { name: '空闲', emoji: '⏸️', color: '#909399' },
  'other': { name: '其他', emoji: '📌', color: '#6C757D' },

  // 兼容旧的类别名称（首字母大写）
  'Work': { name: '工作', emoji: '💼', color: '#409EFF' },
  'Coding': { name: '工作', emoji: '💼', color: '#409EFF' },
  'coding': { name: '工作', emoji: '💼', color: '#409EFF' },
  'Writing': { name: '工作', emoji: '💼', color: '#409EFF' },
  'writing': { name: '工作', emoji: '💼', color: '#409EFF' },
  'Design': { name: '工作', emoji: '💼', color: '#409EFF' },
  'design': { name: '工作', emoji: '💼', color: '#409EFF' },
  'Planning': { name: '工作', emoji: '💼', color: '#409EFF' },
  'planning': { name: '工作', emoji: '💼', color: '#409EFF' },
  'DataAnalysis': { name: '工作', emoji: '💼', color: '#409EFF' },
  'data_analysis': { name: '工作', emoji: '💼', color: '#409EFF' },

  'Communication': { name: '沟通', emoji: '💬', color: '#FFC107' },
  'Meeting': { name: '沟通', emoji: '💬', color: '#FFC107' },
  'meeting': { name: '沟通', emoji: '💬', color: '#FFC107' },

  'Learning': { name: '学习', emoji: '📚', color: '#67C23A' },
  'Research': { name: '学习', emoji: '📚', color: '#67C23A' },
  'research': { name: '学习', emoji: '📚', color: '#67C23A' },

  'Personal': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'Entertainment': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'entertainment': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'SocialMedia': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'social_media': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'Shopping': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'shopping': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'Finance': { name: '个人', emoji: '🏠', color: '#FF69B4' },
  'finance': { name: '个人', emoji: '🏠', color: '#FF69B4' },

  'Idle': { name: '空闲', emoji: '⏸️', color: '#909399' },

  'Other': { name: '其他', emoji: '📌', color: '#6C757D' },
  'Break': { name: '其他', emoji: '📌', color: '#6C757D' },
  'break': { name: '其他', emoji: '📌', color: '#6C757D' },
  'Exercise': { name: '其他', emoji: '📌', color: '#6C757D' },
  'exercise': { name: '其他', emoji: '📌', color: '#6C757D' },
}

// 获取类别颜色
const getCategoryColor = (category) => {
  if (!category) return '#909399'
  const config = categoryConfig[category] || categoryConfig[category.toLowerCase()]
  return config?.color || '#909399'
}

// 获取类别名称（不含emoji）
const getCategoryName = (category) => {
  if (!category) return '其他'
  const config = categoryConfig[category] || categoryConfig[category.toLowerCase()]
  return config?.name || category
}

// 获取类别emoji
const getCategoryEmoji = (category) => {
  if (!category) return '📌'
  const config = categoryConfig[category] || categoryConfig[category.toLowerCase()]
  return config?.emoji || '📌'
}

// 获取类别的完整显示（emoji + 名称）
const getCategoryFullDisplay = (category) => {
  const name = getCategoryName(category)
  const emoji = getCategoryEmoji(category)
  return `${emoji} ${name}`
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

  // 使用设备名称生成一致的颜色
  let hash = 0
  for (let i = 0; i < deviceName.length; i++) {
    hash = deviceName.charCodeAt(i) + ((hash << 5) - hash)
  }

  // 预定义的柔和颜色集合
  const colors = [
    '#409EFF', // 蓝色
    '#67C23A', // 绿色
    '#E6A23C', // 橙色
    '#F56C6C', // 红色
    '#909399', // 灰色
    '#9C27B0', // 紫色
    '#00BCD4', // 青色
    '#FF9800', // 深橙色
  ]

  return colors[Math.abs(hash) % colors.length]
}

// 获取设备显示名称（简化长名称）
const getDeviceDisplayName = (deviceName) => {
  if (!deviceName) return '未知设备'
  // 如果设备名称太长，截断显示
  return deviceName.length > 15 ? deviceName.substring(0, 15) + '...' : deviceName
}

// 判断是否是当前选中的会话
const isActiveSession = (session) => {
  return store.selectedSession?.session?.id === session.id
}

// 选择会话
const selectSession = (session) => {
  store.fetchSessionDetail(session.id)
  emit('session-click', session)
}

// 查看详情
const viewDetail = (session) => {
  selectSession(session)
}


// 批量处理聚合的会话
const batchProcessMergedSessions = async (card, sessionIds) => {
  const displayTitle = card.title || card.summary || '未命名活动'
  let deletedCount = 0
  let successCount = 0
  let errors = []

  const loading = ElMessage({
    message: `正在批量处理 ${sessionIds.length} 个会话...`,
    type: 'info',
    duration: 0 // 不自动关闭
  })

  for (const sessionId of sessionIds) {
    try {
      // 使用静默模式生成视频，避免重复提示
      await store.generateVideo(sessionId, 20, true)
      successCount++
    } catch (error) {
      if (error?.type === 'SESSION_DELETED') {
        deletedCount++
      } else {
        errors.push(`会话 ${sessionId}: ${error}`)
      }
    }
  }

  // 关闭加载提示
  loading.close()

  // 刷新会话列表
  await refreshSessions()

  // 刷新月度活动数据（日历组件需要）
  const current = dayjs(props.date)
  const startDate = current.startOf('month').format('YYYY-MM-DD')
  const endDate = current.endOf('month').format('YYYY-MM-DD')
  await store.fetchActivities(startDate, endDate)

  // 显示处理结果汇总
  const results = []
  if (successCount > 0) {
    results.push(`${successCount} 个视频生成成功`)
  }
  if (deletedCount > 0) {
    results.push(`${deletedCount} 个空会话已清理`)
  }
  if (errors.length > 0) {
    results.push(`${errors.length} 个处理失败`)
    console.error('批量处理错误:', errors)
  }

  if (results.length > 0) {
    const message = `处理完成：${results.join('，')}`
    if (errors.length > 0) {
      ElMessage.warning(message)
    } else {
      ElMessage.success(message)
    }
  }
}

// 生成视频
const generateVideo = async (session) => {
  try {
    await ElMessageBox.confirm(
      `确定要为会话"${session.title}"生成视频吗？`,
      '生成视频',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'info'
      }
    )

    const videoPath = await store.generateVideo(session.id)

    // 刷新会话列表以显示视频标记
    await refreshSessions()

    ElMessage.success(`视频已生成：${videoPath}`)
  } catch (error) {
    if (error !== 'cancel') {
      console.error('Failed to generate video:', error)

      // 如果会话已被删除，隐藏悬浮提示框并刷新
      if (error?.type === 'SESSION_DELETED') {
        hoveredSession.value = null
        // 刷新会话列表
        await refreshSessions()
      }
    }
  }
}

// 刷新会话列表
const refreshSessions = async () => {
  await store.fetchDaySessions(props.date)
}

// 滚动到当前时间段的中心
const scrollToActiveTime = () => {
  // 获取当前时间
  const now = dayjs()
  const currentHour = now.hour()
  const currentMinute = now.minute()

  // 计算当前时间的小时数（包含分钟的小数部分）
  const currentTime = currentHour + currentMinute / 60

  // 如果当前时间在时间轴范围内，滚动到当前时间
  if (currentTime >= TIMELINE_START_HOUR && currentTime <= TIMELINE_END_HOUR) {
    // 计算滚动位置 - 将当前时间放在视口上半部分（约45%位置）
    const viewportHeight = window.innerHeight - 200 // 减去头部和底部的高度
    const scrollPosition = getTimePosition(currentTime) - viewportHeight * 0.45

    // 获取滚动容器并滚动
    const scrollContainer = document.querySelector('.timeline-scrollbar .el-scrollbar__wrap')
    if (scrollContainer) {
      setTimeout(() => {
        scrollContainer.scrollTop = Math.max(0, scrollPosition)
      }, 100)
    }
  } else if (sessions.value.length > 0) {
    // 如果当前时间不在范围内，但有会话数据，滚动到会话中心
    const times = sessions.value.map(s => ({
      start: dayjs(s.start_time),
      end: dayjs(s.end_time)
    }))

    const earliestTime = times.reduce((min, t) => t.start.isBefore(min) ? t.start : min, times[0].start)
    const latestTime = times.reduce((max, t) => t.end.isAfter(max) ? t.end : max, times[0].end)

    // 计算中间时间点
    const centerHour = earliestTime.hour() + (latestTime.hour() - earliestTime.hour()) / 2

    // 计算滚动位置 - 放在视口45%位置
    const viewportHeight = window.innerHeight - 200
    const scrollPosition = getTimePosition(centerHour) - viewportHeight * 0.45

    // 获取滚动容器并滚动
    const scrollContainer = document.querySelector('.timeline-scrollbar .el-scrollbar__wrap')
    if (scrollContainer) {
      setTimeout(() => {
        scrollContainer.scrollTop = Math.max(0, scrollPosition)
      }, 100)
    }
  }
}

// 重新生成时间线
const regenerateTimeline = async () => {
  try {
    await ElMessageBox.confirm(
      `确定要重新生成${formattedDate.value}的时间线吗？这将清空当天已有的时间线记录并基于视频分段重新生成。`,
      '重新生成时间线',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }
    )

    regeneratingTimeline.value = true
    const result = await invoke('regenerate_timeline', { date: props.date })
    ElMessage.success(result)

    // 刷新会话列表以显示新的时间线
    await refreshSessions()
  } catch (error) {
    if (error !== 'cancel') {
      console.error('Failed to regenerate timeline:', error)
      ElMessage.error(`重新生成时间线失败: ${error}`)
    }
  } finally {
    regeneratingTimeline.value = false
  }
}

// 监听日期变化
watch(() => props.date, (newDate) => {
  if (newDate) {
    refreshSessions()
  }
})

// 定时刷新数据
const refreshTimer = ref(null)

// 启动定时刷新
const startRefreshTimer = () => {
  if (refreshTimer.value) {
    clearInterval(refreshTimer.value)
  }
  refreshTimer.value = setInterval(() => {
    console.log('[TimelineView] 自动刷新时间线数据')
    refreshSessions()
    updateCurrentTimeLine()
  }, 60000) // 每分钟刷新
}

// 处理窗口激活
const handleWindowFocus = () => {
  console.log('[TimelineView] 窗口被激活，刷新时间线')
  refreshSessions()
  updateCurrentTimeLine()
  scrollToActiveTime()
}

// 处理页面可见性
const handleVisibilityChange = () => {
  if (!document.hidden) {
    console.log('[TimelineView] 页面变为可见，刷新时间线')
    refreshSessions()
    updateCurrentTimeLine()
  }
}

// 监听会话列表变化，自动滚动到活动时间段
watch(sessions, async (newSessions) => {
  if (newSessions && newSessions.length > 0) {
    await nextTick()
    scrollToActiveTime()
  }
})

// 监听聚合模式切换，自动滚动到当前时间
watch(enableAggregation, async (newValue, oldValue) => {
  // 只有在切换到非聚合模式（全部显示）时才滚动
  if (!newValue && oldValue !== undefined) {
    await nextTick()
    // 延迟一点执行，确保 DOM 已经更新
    setTimeout(() => {
      scrollToActiveTime()
    }, 100)
  }
})

// 组件挂载后自动滚动到当前时间并更新时间线
onMounted(async () => {
  // 首先加载会话数据
  await refreshSessions()

  // 更新当前时间线
  updateCurrentTimeLine()

  // 滚动到当前时间
  scrollToActiveTime()

  // 启动定时刷新
  startRefreshTimer()

  // 添加事件监听
  window.addEventListener('focus', handleWindowFocus)
  document.addEventListener('visibilitychange', handleVisibilityChange)
})

// 组件销毁时清理
onUnmounted(() => {
  if (refreshTimer.value) {
    clearInterval(refreshTimer.value)
  }
  // 移除事件监听
  window.removeEventListener('focus', handleWindowFocus)
  document.removeEventListener('visibilitychange', handleVisibilityChange)
})

// 监听悬浮会话变化，初始化提示框
watch(hoveredSession, (newSession) => {
  if (!newSession) {
    tooltipStyle.value = {}
  }
})

// 监听悬浮卡片变化，初始化提示框
watch(hoveredCard, (newCard) => {
  if (!newCard) {
    tooltipStyle.value = {}
  }
})
</script>

<style scoped>
.timeline-container {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: #1a1a1a;
  border-radius: 8px;
  padding: 20px;
  border: 1px solid #2d2d2d;
}

.timeline-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
  padding: 0 4px;
}

.timeline-title {
  display: flex;
  align-items: center;
  gap: 12px;
}

.timeline-title h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: #ffffff;
}

.merge-badge {
  font-size: 11px !important;
}

.timeline-actions {
  display: flex;
  gap: 12px;
  align-items: center;
}

/* 自定义开关样式，使其更紧凑 */
.timeline-actions .el-switch {
  height: 20px;
}

.timeline-actions .el-switch__label {
  font-size: 12px;
  font-weight: normal;
}

/* 时间线组容器样式 */

.timeline-content {
  flex: 1;
  overflow: hidden;
  position: relative;
}

.timeline-scrollbar {
  height: 100%;
}

/* 自动滚动到活动时间段的中心 */
.timeline-scrollbar :deep(.el-scrollbar__wrap) {
  scroll-behavior: smooth;
}

/* 当前时间指示线 */
.current-time-line {
  position: absolute;
  left: -70px;
  right: 0;
  height: 2px;
  z-index: 100;
  pointer-events: none;
}

.current-time-label {
  position: absolute;
  left: 0;
  top: -10px;
  background: #ff6b6b;
  color: white;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}

.current-time-bar {
  position: absolute;
  left: 70px;
  right: 0;
  height: 2px;
  background: linear-gradient(90deg, #ff6b6b 0%, #ff8787 50%, rgba(255, 107, 107, 0.3) 100%);
  box-shadow: 0 1px 4px rgba(255, 107, 107, 0.4);
}

.current-time-bar::before {
  content: '';
  position: absolute;
  left: 0;
  top: -4px;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #ff6b6b;
  box-shadow: 0 0 8px rgba(255, 107, 107, 0.6);
}

.timeline-chart {
  position: relative;
  min-height: 3000px; /* (23-0+1) * 120 + 120 = 24 * 120 + 120 = 3000 */
  margin-left: 70px;
  margin-right: 30px;
  padding-bottom: 80px;
}

/* 时间刻度 */
.time-scale {
  position: absolute;
  left: -60px;
  top: 0;
  width: 60px;
  height: 100%;
}

.time-tick {
  position: absolute;
  width: 100%;
  display: flex;
  align-items: center;
}

.time-label {
  font-size: 12px;
  color: #666666;
  width: 45px;
  text-align: right;
  padding-right: 10px;
}

.time-line {
  position: absolute;
  left: 60px;
  width: calc(100vw - 140px);
  height: 1px;
  background-color: #2d2d2d;
}

/* 活动区块 */
.activity-blocks {
  position: relative;
  width: 100%;
  height: 100%;
}

.activity-block {
  position: absolute;
  border-left: 4px solid;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.3s;
  padding: 8px 12px;
  overflow: hidden;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.2);
  min-height: 60px;
  border: 1px solid #2d2d2d;
}

.activity-block:hover {
  transform: translateX(2px);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
  z-index: 10;
  border-color: #3d3d3d;
}

.activity-block.is-active {
  border-left-width: 6px;
  background-color: #2d2d2d !important;
  opacity: 1 !important;
  border-color: #4d4d4d;
}

/* 会话区块和时间线卡片区块的不同样式 */
.session-block {
  z-index: 1;
  opacity: 0.85;
}

.session-block:hover {
  opacity: 1;
  z-index: 15;
}

.timeline-card-block {
  z-index: 2;
  border-left-width: 4px;
  background: linear-gradient(90deg, rgba(45,45,45,0.98) 0%, rgba(35,35,35,0.95) 100%);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
  min-height: 24px;
}

.timeline-card-block.is-merged {
  border-left-width: 6px;
  box-shadow: 0 3px 8px rgba(0, 0, 0, 0.4);
  background: linear-gradient(90deg, #2d2d2d 0%, #252525 100%);
}

.timeline-card-block:hover {
  transform: translateX(2px) scale(1.01);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
  z-index: 20;
}

.timeline-card-block .block-title-text {
  color: #e0e0e0;
}

.timeline-card-block .merged-count {
  background: #e6a23c;
  color: white;
  padding: 1px 5px;
  border-radius: 10px;
  font-size: 10px;
  margin-left: 6px;
  font-weight: bold;
  vertical-align: middle;
}

.timeline-card-block .block-time {
  font-size: 11px;
  color: #606266;
  font-weight: 500;
}

.block-category {
  margin-top: 6px;
}

.block-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 4px;
  gap: 8px;
}

.block-duration {
  font-size: 10px;
  color: #909399;
  display: flex;
  align-items: center;
  gap: 2px;
}

.block-device {
  display: flex;
  align-items: center;
  gap: 3px;
  font-size: 10px;
  padding: 2px 6px;
  background: rgba(255, 255, 255, 0.8);
  border-radius: 8px;
  border: 1px solid rgba(0, 0, 0, 0.05);
}

.device-name {
  font-weight: 500;
  white-space: nowrap;
}

.block-content {
  position: relative;
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: flex-start;
  padding-top: 4px;
  min-width: 0;
}

.block-header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
  min-width: 0;
}

.block-tag {
  font-size: 10px !important;
  padding: 2px 6px !important;
  height: 18px !important;
  border-radius: 3px !important;
  margin-bottom: 0;
  color: white !important;
  border: none !important;
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
}

.block-title {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 0;
  flex: 1;
  min-width: 0;
}

.block-title-text {
  font-size: 12px;
  color: #e0e0e0;
  font-weight: 600;
  line-height: 1.4;
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.merged-count {
  font-size: 11px;
  color: #606266;
  background: #f2f6fc;
  border-radius: 10px;
  padding: 0 6px;
  line-height: 1.6;
  flex-shrink: 0;
}


.session-block .block-title {
  align-items: flex-start;
  gap: 4px;
}

.session-block .block-title-text {
  white-space: normal;
  text-overflow: unset;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.session-block .merged-count {
  display: none;
}

.session-block .block-time {
  color: #909399;
}

.block-time {
  font-size: 11px;
  color: #909399;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
  margin-top: 0;
}

.block-icon {
  position: absolute;
  top: 8px;
  right: 8px;
  color: #67c23a;
}

/* 悬浮提示框 */
.session-tooltip {
  position: fixed;
  background: #2d2d2d;
  border-radius: 10px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
  padding: 18px;
  z-index: 2000;
  max-width: 350px;
  min-width: 280px;
  transition: opacity 0.2s;
  pointer-events: none; /* 防止提示框干扰鼠标事件 */
  border: 1px solid #3d3d3d;
}

.tooltip-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
}

.tooltip-header h4 {
  margin: 0;
  color: #ffffff;
  font-size: 16px;
}

.tooltip-merged-count {
  background: #e6a23c;
  color: white;
  padding: 2px 6px;
  border-radius: 10px;
  font-size: 11px;
  margin-left: 8px;
  font-weight: bold;
}

.tooltip-summary {
  color: #b0b0b0;
  font-size: 14px;
  line-height: 1.5;
  margin-bottom: 12px;
}

.tooltip-meta {
  margin-bottom: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.tooltip-duration {
  display: flex;
  align-items: center;
  gap: 4px;
  color: #909399;
  font-size: 13px;
}

.tooltip-device {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 500;
}

.tooltip-actions {
  display: flex;
  gap: 8px;
}

/* 过渡动画 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
