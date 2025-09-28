<!-- 添加标签对话框组件 -->

<template>
  <el-dialog
    v-model="dialogVisible"
    title="添加活动标签"
    width="500px"
    @close="handleClose"
  >
    <el-form :model="form" label-width="100px">
      <el-form-item label="活动类别" required>
        <el-select v-model="form.category" placeholder="请选择活动类别">
          <el-option
            v-for="item in categories"
            :key="item.value"
            :label="`${item.emoji} ${item.label}`"
            :value="item.value"
          >
            <span :style="{ color: item.color }">{{ item.emoji }} {{ item.label }}</span>
          </el-option>
        </el-select>
      </el-form-item>

      <el-form-item label="置信度">
        <el-slider
          v-model="form.confidence"
          :min="0"
          :max="100"
          show-input
        />
      </el-form-item>

      <el-form-item label="关键词">
        <el-tag
          v-for="tag in form.keywords"
          :key="tag"
          closable
          @close="removeKeyword(tag)"
        >
          {{ tag }}
        </el-tag>
        <el-input
          v-if="inputVisible"
          ref="inputRef"
          v-model="inputValue"
          size="small"
          @keyup.enter="confirmKeyword"
          @blur="confirmKeyword"
        />
        <el-button
          v-else
          size="small"
          @click="showInput"
        >
          + 添加关键词
        </el-button>
      </el-form-item>
    </el-form>

    <template #footer>
      <span class="dialog-footer">
        <el-button @click="handleClose">取消</el-button>
        <el-button type="primary" @click="handleConfirm">确定</el-button>
      </span>
    </template>
  </el-dialog>
</template>

<script setup>
import { ref, computed, nextTick, watch } from 'vue'

const props = defineProps({
  visible: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(['update:visible', 'confirm'])

const dialogVisible = computed({
  get: () => props.visible,
  set: (value) => emit('update:visible', value)
})

const form = ref({
  category: '',
  confidence: 80,
  keywords: []
})

const inputVisible = ref(false)
const inputValue = ref('')
const inputRef = ref(null)

// 从localStorage或默认值加载标签
const loadCategories = () => {
  const saved = localStorage.getItem('customTags')
  if (saved) {
    try {
      const tags = JSON.parse(saved)
      return tags.map(tag => ({
        value: tag.value,
        label: tag.label,
        color: tag.color,
        emoji: tag.emoji,
        description: tag.description
      }))
    } catch (e) {
      console.error('Failed to load saved tags:', e)
    }
  }

  // 默认标签列表
  return [
    { value: 'work', label: '工作', emoji: '💼', color: '#409EFF' },
    { value: 'meeting', label: '会议', emoji: '👥', color: '#F56C6C' },
    { value: 'coding', label: '编程', emoji: '💻', color: '#7C4DFF' },
    { value: 'research', label: '研究', emoji: '🔍', color: '#00BCD4' },
    { value: 'learning', label: '学习', emoji: '📚', color: '#67C23A' },
    { value: 'writing', label: '写作', emoji: '✍️', color: '#FF9800' },
    { value: 'design', label: '设计', emoji: '🎨', color: '#E91E63' },
    { value: 'communication', label: '沟通', emoji: '💬', color: '#FFC107' },
    { value: 'planning', label: '规划', emoji: '📋', color: '#4CAF50' },
    { value: 'data_analysis', label: '数据分析', emoji: '📊', color: '#795548' },
    { value: 'entertainment', label: '娱乐', emoji: '🎮', color: '#FF69B4' },
    { value: 'social_media', label: '社交媒体', emoji: '📱', color: '#9C27B0' },
    { value: 'shopping', label: '购物', emoji: '🛒', color: '#FF5722' },
    { value: 'finance', label: '财务', emoji: '💰', color: '#607D8B' },
    { value: 'break', label: '休息', emoji: '☕', color: '#E6A23C' },
    { value: 'exercise', label: '运动', emoji: '🏃', color: '#8BC34A' },
    { value: 'personal', label: '个人事务', emoji: '🏠', color: '#03A9F4' },
    { value: 'idle', label: '空闲', emoji: '⏸️', color: '#909399' },
    { value: 'other', label: '其他', emoji: '📌', color: '#6C757D' }
  ]
}

const categories = ref(loadCategories())

// 显示输入框
const showInput = () => {
  inputVisible.value = true
  nextTick(() => {
    inputRef.value?.focus()
  })
}

// 确认关键词
const confirmKeyword = () => {
  if (inputValue.value) {
    form.value.keywords.push(inputValue.value)
  }
  inputVisible.value = false
  inputValue.value = ''
}

// 移除关键词
const removeKeyword = (tag) => {
  const index = form.value.keywords.indexOf(tag)
  if (index > -1) {
    form.value.keywords.splice(index, 1)
  }
}

// 确认
const handleConfirm = () => {
  if (!form.value.category) {
    ElMessage.warning('请选择活动类别')
    return
  }

  const tag = {
    category: form.value.category,
    confidence: form.value.confidence / 100,
    keywords: form.value.keywords
  }

  emit('confirm', tag)
  handleClose()
}

// 关闭
const handleClose = () => {
  dialogVisible.value = false
  // 重置表单
  form.value = {
    category: '',
    confidence: 80,
    keywords: []
  }
  inputVisible.value = false
  inputValue.value = ''
}
</script>

<style scoped>
.el-tag + .el-tag {
  margin-left: 10px;
}

.el-input {
  width: 120px;
  margin-left: 10px;
}

.el-button--small {
  margin-left: 10px;
}
</style>