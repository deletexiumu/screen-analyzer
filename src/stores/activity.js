// Pinia Store - 活动状态管理

import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { ElMessage } from 'element-plus'
import dayjs from 'dayjs'

export const useActivityStore = defineStore('activity', {
  state: () => ({
    // 活动列表
    activities: [],
    // 当前选中的日期
    selectedDate: dayjs().format('YYYY-MM-DD'),
    // 当天的会话列表
    daySessions: [],
    // 选中的会话详情
    selectedSession: null,
    // 系统状态
    systemStatus: {
      is_capturing: false,
      is_processing: false,
      last_capture_time: null,
      last_process_time: null,
      current_session_frames: 0,
      storage_usage: {
        database_size: 0,
        frames_size: 0,
        videos_size: 0,
        total_size: 0,
        session_count: 0,
        frame_count: 0
      },
      last_error: null
    },
    // 应用配置
    appConfig: {
      retention_days: 7,
      llm_provider: 'openai',
      capture_interval: 1,
      summary_interval: 15,
      video_config: {
        auto_generate: true,
        speed_multiplier: 8.0,
        quality: 23,
        add_timestamp: true
      },
      ui_settings: null
    },
    // LLM提供商列表
    llmProviders: [],
    // 加载状态
    loading: {
      activities: false,
      sessions: false,
      sessionDetail: false,
      status: false
    }
  }),

  getters: {
    // 格式化的存储使用量
    formattedStorageUsage(state) {
      const formatSize = (bytes) => {
        if (bytes === 0) return '0 B'
        const k = 1024
        const sizes = ['B', 'KB', 'MB', 'GB']
        const i = Math.floor(Math.log(bytes) / Math.log(k))
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
      }

      return {
        database: formatSize(state.systemStatus.storage_usage.database_size),
        frames: formatSize(state.systemStatus.storage_usage.frames_size),
        videos: formatSize(state.systemStatus.storage_usage.videos_size),
        total: formatSize(state.systemStatus.storage_usage.total_size)
      }
    },

    // 是否正在捕获
    isCapturing(state) {
      return state.systemStatus.is_capturing
    },

    // 是否正在处理
    isProcessing(state) {
      return state.systemStatus.is_processing
    }
  },

  actions: {
    // 获取活动列表
    async fetchActivities(startDate, endDate) {
      this.loading.activities = true
      try {
        const activities = await invoke('get_activities', {
          startDate: startDate || dayjs().subtract(30, 'day').format('YYYY-MM-DD'),
          endDate: endDate || dayjs().format('YYYY-MM-DD')
        })
        this.activities = activities
      } catch (error) {
        ElMessage.error('获取活动列表失败: ' + error)
        console.error('Failed to fetch activities:', error)
      } finally {
        this.loading.activities = false
      }
    },

    // 获取某天的会话
    async fetchDaySessions(date) {
      this.loading.sessions = true
      this.selectedDate = date
      try {
        const sessions = await invoke('get_day_sessions', { date })
        this.daySessions = sessions
      } catch (error) {
        ElMessage.error('获取会话列表失败: ' + error)
        console.error('Failed to fetch day sessions:', error)
      } finally {
        this.loading.sessions = false
      }
    },

    // 获取会话详情
    async fetchSessionDetail(sessionId) {
      this.loading.sessionDetail = true
      try {
        const detail = await invoke('get_session_detail', { sessionId })
        this.selectedSession = detail
      } catch (error) {
        ElMessage.error('获取会话详情失败: ' + error)
        console.error('Failed to fetch session detail:', error)
      } finally {
        this.loading.sessionDetail = false
      }
    },

    // 获取系统状态
    async fetchSystemStatus() {
      this.loading.status = true
      try {
        const status = await invoke('get_system_status')
        this.systemStatus = status
      } catch (error) {
        console.error('Failed to fetch system status:', error)
      } finally {
        this.loading.status = false
      }
    },

    // 更新配置
    async updateConfig(config) {
      try {
        const updated = await invoke('update_config', { config })
        this.appConfig = {
          ...updated,
          video_config: { ...updated.video_config }
        }
        ElMessage.success('配置更新成功')
      } catch (error) {
        ElMessage.error('配置更新失败: ' + error)
        console.error('Failed to update config:', error)
      }
    },

    // 获取应用配置
    async fetchAppConfig() {
      try {
        const config = await invoke('get_app_config')
        this.appConfig = {
          ...config,
          video_config: { ...config.video_config }
        }
      } catch (error) {
        console.error('Failed to fetch app config:', error)
      }
    },

    // 切换截屏状态
    async toggleCapture(enabled) {
      try {
        await invoke('toggle_capture', { enabled })
        this.systemStatus.is_capturing = enabled
        ElMessage.success(enabled ? '已恢复截屏' : '已暂停截屏')
      } catch (error) {
        ElMessage.error('切换截屏状态失败: ' + error)
        console.error('Failed to toggle capture:', error)
      }
    },

    // 手动触发分析
    async triggerAnalysis() {
      try {
        this.systemStatus.is_processing = true
        const result = await invoke('trigger_analysis')
        if (result) {
          ElMessage.success(result)
        } else {
          ElMessage.success('分析任务已完成')
        }
        await this.fetchDaySessions(this.selectedDate)
      } catch (error) {
        ElMessage.error('触发分析失败: ' + error)
        console.error('Failed to trigger analysis:', error)
      } finally {
        await this.fetchSystemStatus()
        this.systemStatus.is_processing = false
      }
    },

    async retrySessionAnalysis(sessionId) {
      if (!sessionId) {
        ElMessage.warning('请选择需要重新解析的会话')
        return
      }

      try {
        this.systemStatus.is_processing = true
        const message = await invoke('retry_session_analysis', { sessionId })
        if (message) {
          ElMessage.success(message)
        } else {
          ElMessage.success('重新解析完成')
        }

        await Promise.all([
          this.fetchSessionDetail(sessionId),
          this.fetchDaySessions(this.selectedDate)
        ])
      } catch (error) {
        const errorStr = String(error)
        // 检测视频过短的错误
        if (errorStr.includes('VIDEO_TOO_SHORT')) {
          const { ElMessageBox } = await import('element-plus')
          ElMessageBox.confirm(
            '该会话时长过短（少于15分钟），无法进行AI分析。是否删除该会话？',
            '视频过短',
            {
              confirmButtonText: '删除',
              cancelButtonText: '保留',
              type: 'warning'
            }
          ).then(async () => {
            try {
              await this.deleteSession(sessionId)
              ElMessage.success('会话已删除')
            } catch (deleteError) {
              ElMessage.error('删除失败: ' + deleteError)
            }
          }).catch(() => {
            // 用户取消删除
          })
        } else {
          ElMessage.error('重新解析失败: ' + error)
          console.error('Failed to retry session analysis:', error)
        }
      } finally {
        await this.fetchSystemStatus()
        this.systemStatus.is_processing = false
      }
    },

    // 删除会话
    async deleteSession(sessionId) {
      try {
        await invoke('delete_session', { sessionId })
        // 刷新列表
        await this.fetchDaySessions(this.selectedDate)
        // 清空选中的会话详情
        if (this.selectedSession?.session?.id === sessionId) {
          this.selectedSession = null
        }
      } catch (error) {
        console.error('Failed to delete session:', error)
        throw error
      }
    },

    // 生成视频
    async generateVideo(sessionId, speedMultiplier = 20) {
      try {
        const videoPath = await invoke('generate_video', {
          sessionId,
          speedMultiplier
        })
        ElMessage.success('视频生成成功')
        return videoPath
      } catch (error) {
        ElMessage.error('生成视频失败: ' + error)
        console.error('Failed to generate video:', error)
        throw error
      }
    },

    // 清理存储
    async cleanupStorage() {
      try {
        await invoke('cleanup_storage')
        ElMessage.success('存储清理完成')
        await this.fetchSystemStatus()
      } catch (error) {
        ElMessage.error('清理存储失败: ' + error)
        console.error('Failed to cleanup storage:', error)
      }
    },

    // 获取存储统计
    async fetchStorageStats() {
      try {
        const stats = await invoke('get_storage_stats')
        this.systemStatus.storage_usage = stats
      } catch (error) {
        console.error('Failed to fetch storage stats:', error)
      }
    },

    // 配置LLM提供商
    async configureLLMProvider(provider, config) {
      try {
        await invoke('configure_llm_provider', { provider, config })
        ElMessage.success(`${provider} 配置成功`)
      } catch (error) {
        ElMessage.error('配置LLM提供商失败: ' + error)
        console.error('Failed to configure LLM provider:', error)
      }
    },

    // 获取LLM提供商列表
    async fetchLLMProviders() {
      try {
        const providers = await invoke('get_llm_providers')
        this.llmProviders = providers
      } catch (error) {
        console.error('Failed to fetch LLM providers:', error)
      }
    },

    // 添加手动标签
    async addManualTag(sessionId, tag) {
      try {
        await invoke('add_manual_tag', { sessionId, tag })
        ElMessage.success('标签添加成功')
        // 刷新会话详情
        if (this.selectedSession?.session?.id === sessionId) {
          await this.fetchSessionDetail(sessionId)
        }
      } catch (error) {
        ElMessage.error('添加标签失败: ' + error)
        console.error('Failed to add manual tag:', error)
      }
    },

    // 初始化
    async initialize() {
      await Promise.all([
        this.fetchAppConfig(),
        this.fetchActivities(),
        this.fetchSystemStatus(),
        this.fetchLLMProviders(),
        this.fetchDaySessions(this.selectedDate)
      ])
    }
  }
})
