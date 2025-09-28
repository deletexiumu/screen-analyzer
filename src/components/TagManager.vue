<!-- 标签管理组件 -->

<template>
  <div class="tag-manager">
    <div class="header">
      <h3>标签管理</h3>
      <el-button type="primary" size="small" @click="showAddDialog">
        <el-icon><Plus /></el-icon>
        添加标签
      </el-button>
    </div>

    <div class="tag-list">
      <el-table :data="tags" style="width: 100%">
        <el-table-column label="预览" width="100">
          <template #default="scope">
            <div class="tag-preview">
              <span :style="{ color: scope.row.color }">
                {{ scope.row.emoji }} {{ scope.row.label }}
              </span>
            </div>
          </template>
        </el-table-column>

        <el-table-column prop="value" label="标签ID" width="150" />

        <el-table-column prop="label" label="名称" width="120" />

        <el-table-column label="表情" width="80">
          <template #default="scope">
            <span class="emoji">{{ scope.row.emoji }}</span>
          </template>
        </el-table-column>

        <el-table-column label="颜色" width="120">
          <template #default="scope">
            <div class="color-display">
              <div class="color-box" :style="{ backgroundColor: scope.row.color }"></div>
              <span>{{ scope.row.color }}</span>
            </div>
          </template>
        </el-table-column>

        <el-table-column label="描述" min-width="200">
          <template #default="scope">
            <span>{{ scope.row.description || '-' }}</span>
          </template>
        </el-table-column>

        <el-table-column label="操作" width="150" fixed="right">
          <template #default="scope">
            <el-button
              type="primary"
              link
              size="small"
              @click="editTag(scope.row)"
            >
              编辑
            </el-button>
            <el-popconfirm
              title="确定要删除这个标签吗？"
              @confirm="deleteTag(scope.row.value)"
            >
              <template #reference>
                <el-button
                  type="danger"
                  link
                  size="small"
                  :disabled="isDefaultTag(scope.row.value)"
                >
                  删除
                </el-button>
              </template>
            </el-popconfirm>
          </template>
        </el-table-column>
      </el-table>
    </div>

    <div class="footer">
      <el-button @click="resetToDefault">恢复默认标签</el-button>
      <el-button type="primary" @click="saveTags">保存修改</el-button>
    </div>

    <!-- 添加/编辑标签对话框 -->
    <el-dialog
      v-model="dialogVisible"
      :title="isEditMode ? '编辑标签' : '添加标签'"
      width="500px"
      @close="resetForm"
    >
      <el-form :model="form" :rules="rules" ref="formRef" label-width="100px">
        <el-form-item label="标签ID" prop="value">
          <el-input
            v-model="form.value"
            placeholder="例如: work, study"
            :disabled="isEditMode"
          />
        </el-form-item>

        <el-form-item label="显示名称" prop="label">
          <el-input v-model="form.label" placeholder="例如: 工作, 学习" />
        </el-form-item>

        <el-form-item label="表情图标" prop="emoji">
          <div class="emoji-selector">
            <el-input
              v-model="form.emoji"
              placeholder="选择或输入表情"
              style="width: 200px"
            />
            <div class="emoji-quick-select">
              <span
                v-for="emoji in commonEmojis"
                :key="emoji"
                class="emoji-option"
                @click="form.emoji = emoji"
              >
                {{ emoji }}
              </span>
            </div>
          </div>
        </el-form-item>

        <el-form-item label="颜色" prop="color">
          <div class="color-selector">
            <el-color-picker v-model="form.color" />
            <div class="color-quick-select">
              <div
                v-for="color in presetColors"
                :key="color"
                class="color-option"
                :style="{ backgroundColor: color }"
                @click="form.color = color"
              ></div>
            </div>
          </div>
        </el-form-item>

        <el-form-item label="描述">
          <el-input
            v-model="form.description"
            type="textarea"
            rows="2"
            placeholder="可选的描述信息"
          />
        </el-form-item>
      </el-form>

      <template #footer>
        <el-button @click="dialogVisible = false">取消</el-button>
        <el-button type="primary" @click="submitForm">
          {{ isEditMode ? '保存修改' : '添加' }}
        </el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, reactive, computed } from 'vue'
import { Plus } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { useActivityStore } from '../stores/activity'

const store = useActivityStore()

// 预设的标签列表（精简为6类）
const defaultTags = [
  {
    value: 'work',
    label: '工作',
    emoji: '💼',
    color: '#409EFF',
    description: '编程、写作、设计、数据分析、会议、规划等专业工作'
  },
  {
    value: 'communication',
    label: '沟通',
    emoji: '💬',
    color: '#FFC107',
    description: '聊天、邮件、视频会议、团队协作等沟通交流'
  },
  {
    value: 'learning',
    label: '学习',
    emoji: '📚',
    color: '#67C23A',
    description: '阅读、观看教程、研究、在线课程等学习活动'
  },
  {
    value: 'personal',
    label: '个人',
    emoji: '🏠',
    color: '#FF69B4',
    description: '娱乐、购物、社交媒体、财务等个人活动'
  },
  {
    value: 'idle',
    label: '空闲',
    emoji: '⏸️',
    color: '#909399',
    description: '无活动或锁屏状态（系统自动识别）'
  },
  {
    value: 'other',
    label: '其他',
    emoji: '📌',
    color: '#6C757D',
    description: '休息、运动等其他未分类活动'
  }
]

// 旧标签到新标签的映射（用于兼容历史数据）
const categoryMapping = {
  // 工作类
  'work': 'work',
  'coding': 'work',
  'writing': 'work',
  'design': 'work',
  'planning': 'work',
  'data_analysis': 'work',
  // 沟通类
  'communication': 'communication',
  'meeting': 'communication',
  // 学习类
  'learning': 'learning',
  'research': 'learning',
  // 个人类
  'personal': 'personal',
  'entertainment': 'personal',
  'social_media': 'personal',
  'shopping': 'personal',
  'finance': 'personal',
  // 空闲类
  'idle': 'idle',
  // 其他类
  'other': 'other',
  'break': 'other',
  'exercise': 'other'
}

// 标签映射函数
function mapCategoryToNew(oldCategory) {
  // 先尝试从映射表查找
  const mapped = categoryMapping[oldCategory?.toLowerCase()]
  if (mapped) return mapped

  // 如果找不到，尝试从defaultTags中查找
  const tag = defaultTags.find(t => t.value === oldCategory || t.label === oldCategory)
  if (tag) return tag.value

  // 默认返回other
  return 'other'
}

// 获取标签显示信息
function getTagDisplay(category) {
  const mapped = mapCategoryToNew(category)
  const tag = defaultTags.find(t => t.value === mapped)
  return tag || { value: 'other', label: '其他', emoji: '📌', color: '#6C757D' }
}

// 常用emoji快速选择
const commonEmojis = [
  '💼', '👥', '💻', '🔍', '📚', '✍️', '🎨', '💬',
  '📋', '📊', '🎮', '📱', '🛒', '💰', '☕', '🏃',
  '🏠', '⏸️', '📌', '🎯', '⭐', '🚀', '🔧', '📈',
  '🎵', '📷', '🎬', '🍕', '✅', '❌', '⚡', '🔥'
]

// 预设颜色
const presetColors = [
  '#409EFF', '#67C23A', '#E6A23C', '#F56C6C', '#909399',
  '#7C4DFF', '#00BCD4', '#FFC107', '#FF69B4', '#795548',
  '#E91E63', '#9C27B0', '#FF9800', '#4CAF50', '#03A9F4',
  '#FF5722', '#607D8B', '#8BC34A', '#6C757D', '#3F51B5'
]

// 当前标签列表
const tags = ref([...defaultTags])

// 对话框相关
const dialogVisible = ref(false)
const isEditMode = ref(false)
const formRef = ref()

// 表单数据
const form = reactive({
  value: '',
  label: '',
  emoji: '',
  color: '#409EFF',
  description: ''
})

// 表单验证规则
const rules = {
  value: [
    { required: true, message: '请输入标签ID', trigger: 'blur' },
    { pattern: /^[a-z_]+$/, message: '标签ID只能包含小写字母和下划线', trigger: 'blur' }
  ],
  label: [
    { required: true, message: '请输入显示名称', trigger: 'blur' }
  ],
  emoji: [
    { required: true, message: '请选择或输入表情图标', trigger: 'blur' }
  ],
  color: [
    { required: true, message: '请选择颜色', trigger: 'change' }
  ]
}

// 判断是否为默认标签
const isDefaultTag = (value) => {
  return defaultTags.some(tag => tag.value === value)
}

// 显示添加对话框
const showAddDialog = () => {
  isEditMode.value = false
  dialogVisible.value = true
}

// 编辑标签
const editTag = (tag) => {
  isEditMode.value = true
  Object.assign(form, { ...tag })
  dialogVisible.value = true
}

// 删除标签
const deleteTag = (value) => {
  const index = tags.value.findIndex(tag => tag.value === value)
  if (index > -1) {
    tags.value.splice(index, 1)
    ElMessage.success('标签已删除')
  }
}

// 重置表单
const resetForm = () => {
  form.value = ''
  form.label = ''
  form.emoji = ''
  form.color = '#409EFF'
  form.description = ''
  formRef.value?.clearValidate()
}

// 提交表单
const submitForm = async () => {
  const valid = await formRef.value?.validate()
  if (!valid) return

  if (isEditMode.value) {
    // 编辑模式
    const index = tags.value.findIndex(tag => tag.value === form.value)
    if (index > -1) {
      tags.value[index] = { ...form }
      ElMessage.success('标签已更新')
    }
  } else {
    // 添加模式
    if (tags.value.some(tag => tag.value === form.value)) {
      ElMessage.error('标签ID已存在')
      return
    }
    tags.value.push({ ...form })
    ElMessage.success('标签已添加')
  }

  dialogVisible.value = false
  resetForm()
}

// 恢复默认标签
const resetToDefault = () => {
  tags.value = [...defaultTags]
  ElMessage.success('已恢复默认标签')
}

// 保存标签配置
const saveTags = async () => {
  try {
    // 保存到本地存储
    localStorage.setItem('customTags', JSON.stringify(tags.value))

    // 更新store中的配置
    await store.updateConfig({
      ...store.appConfig,
      ui_settings: {
        ...store.appConfig.ui_settings,
        custom_tags: tags.value
      }
    })

    ElMessage.success('标签配置已保存')
  } catch (error) {
    ElMessage.error('保存失败: ' + error.message)
  }
}

// 初始化时加载保存的标签
const loadSavedTags = () => {
  const saved = localStorage.getItem('customTags')
  if (saved) {
    try {
      tags.value = JSON.parse(saved)
    } catch (e) {
      console.error('Failed to load saved tags:', e)
    }
  }
}

// 组件挂载时加载
loadSavedTags()
</script>

<style scoped>
.tag-manager {
  padding: 20px;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.header h3 {
  margin: 0;
}

.tag-list {
  margin-bottom: 20px;
}

.tag-preview {
  font-weight: 500;
}

.emoji {
  font-size: 20px;
}

.color-display {
  display: flex;
  align-items: center;
  gap: 8px;
}

.color-box {
  width: 20px;
  height: 20px;
  border-radius: 4px;
  border: 1px solid #ddd;
}

.footer {
  display: flex;
  justify-content: space-between;
  padding-top: 20px;
  border-top: 1px solid #eee;
}

.emoji-selector {
  width: 100%;
}

.emoji-quick-select {
  margin-top: 10px;
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.emoji-option {
  cursor: pointer;
  font-size: 24px;
  padding: 4px;
  border: 1px solid transparent;
  border-radius: 4px;
  transition: all 0.3s;
}

.emoji-option:hover {
  background-color: #f0f0f0;
  border-color: #409EFF;
}

.color-selector {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.color-quick-select {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.color-option {
  width: 30px;
  height: 30px;
  border-radius: 4px;
  border: 2px solid transparent;
  cursor: pointer;
  transition: all 0.3s;
}

.color-option:hover {
  border-color: #333;
  transform: scale(1.1);
}
</style>