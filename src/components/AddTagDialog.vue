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
            :label="item.label"
            :value="item.value"
          >
            <span :style="{ color: item.color }">{{ item.label }}</span>
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

const categories = [
  { value: 'Work', label: '工作', color: '#409EFF' },
  { value: 'Personal', label: '私人', color: '#67C23A' },
  { value: 'Break', label: '休息', color: '#E6A23C' },
  { value: 'Idle', label: '空闲', color: '#909399' },
  { value: 'Meeting', label: '会议', color: '#F56C6C' },
  { value: 'Coding', label: '编程', color: '#7C4DFF' },
  { value: 'Research', label: '研究', color: '#00BCD4' },
  { value: 'Communication', label: '沟通', color: '#FFC107' },
  { value: 'Entertainment', label: '娱乐', color: '#FF69B4' },
  { value: 'Other', label: '其他', color: '#795548' }
]

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