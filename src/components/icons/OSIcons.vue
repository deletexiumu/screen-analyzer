<!-- 操作系统图标组件 - 使用本地图标文件 -->

<template>
  <img
    v-if="iconUrl"
    :src="iconUrl"
    :width="size"
    :height="size"
    :alt="type"
    class="os-icon"
    @error="handleImageError"
  />
  <span v-else class="os-icon-fallback" :style="{ width: size + 'px', height: size + 'px' }">
    {{ fallbackText }}
  </span>
</template>

<script setup>
import { computed, ref } from 'vue'
import macosIcon from './macos.ico'
import windowsIcon from './microsoft.ico'
import linuxIcon from './linux.ico'

const props = defineProps({
  type: {
    type: String,
    default: 'unknown',
    validator: (value) => ['windows', 'macos', 'linux', 'unknown'].includes(value)
  },
  size: {
    type: Number,
    default: 16
  }
})

// 图标 URL 映射（本地资源）
const iconUrls = {
  macos: macosIcon,
  windows: windowsIcon,
  linux: linuxIcon,
  unknown: ''
}

// 备用文本
const fallbackTexts = {
  macos: '🍎',
  windows: '🪟',
  linux: '🐧',
  unknown: '💻'
}

// 图标加载失败标记
const imageError = ref(false)

// 计算图标 URL
const iconUrl = computed(() => {
  if (imageError.value) return null
  return iconUrls[props.type] || iconUrls.unknown
})

// 计算备用文本
const fallbackText = computed(() => {
  return fallbackTexts[props.type] || fallbackTexts.unknown
})

// 处理图片加载错误
const handleImageError = () => {
  imageError.value = true
}
</script>

<style scoped>
.os-icon {
  display: inline-block;
  vertical-align: middle;
  object-fit: contain;
}

.os-icon-fallback {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  vertical-align: middle;
  font-size: 12px;
  line-height: 1;
}
</style>