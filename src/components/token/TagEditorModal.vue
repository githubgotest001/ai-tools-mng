<template>
  <BaseModal
    :visible="visible"
    :title="modalTitle"
    :close-on-overlay="true"
    :body-scroll="false"
    modal-class="!max-w-[360px]"
    @close="handleClose"
  >
    <div class="flex flex-col gap-2.5 overflow-visible">
      <label class="label">{{ $t('tokenForm.tagLabel') }}</label>

      <!-- 多标签模式：已加上的标签先列出来，点 × 移除 -->
      <div v-if="isMultiMode && committedTags.length" class="flex flex-wrap items-center gap-1.5">
        <span
          v-for="(tag, index) in committedTags"
          :key="tag.name"
          class="badge editable"
          :style="{ '--tag-color': tag.color }"
        >
          {{ tag.name }}
          <button
            type="button"
            class="ml-0.5 opacity-60 hover:opacity-100"
            :title="$t('tokenForm.clearTag')"
            @click="removeTag(index)"
          >
            ×
          </button>
        </span>
      </div>

      <div class="flex gap-2.5 items-center">
        <div class="dropdown flex-1" @click="showTagSuggestions = true">
          <input
            ref="tagNameInputRef"
            v-model="editingTagName"
            type="text"
            class="input !pr-9"
            :placeholder="$t('tokenForm.tagPlaceholder')"
            maxlength="20"
            :disabled="isTagLimitReached"
            @input="handleTagInput"
            @focus="showTagSuggestions = true"
            @blur="handleTagBlur"
            @keydown.enter.prevent="commitDraft"
            @click.stop="showTagSuggestions = true"
          />
          <button
            v-if="editingTagName"
            type="button"
            class="btn btn--ghost btn--icon-sm absolute right-1.5 top-1/2 -translate-y-1/2"
            :title="$t('tokenForm.clearTag')"
            @click="editingTagName = ''"
          >
            ×
          </button>
          <Transition name="dropdown">
            <div
              v-if="showTagSuggestions && filteredTagSuggestions.length > 0"
              class="dropdown-menu dropdown-menu--left w-full max-h-[200px] overflow-y-auto"
              @mousedown.prevent
            >
              <div
                v-for="suggestion in filteredTagSuggestions"
                :key="suggestion.name"
                class="dropdown-item"
                @mousedown.prevent="selectTagSuggestion(suggestion)"
              >
                <span
                  class="w-4.5 h-4.5 rounded-md shrink-0 shadow-sm"
                  :style="{ backgroundColor: suggestion.color }"
                ></span>
                <span class="text-[14px] text-text">{{ suggestion.name }}</span>
              </div>
            </div>
          </Transition>
        </div>
        <div class="relative shrink-0">
          <div
            class="w-[42px] h-[42px] border-2 border-border rounded-full shadow-sm"
            :style="{ backgroundColor: editingTagColor }"
          ></div>
          <input
            ref="tagColorInputRef"
            type="color"
            v-model="editingTagColor"
            :title="$t('tokenForm.tagColorPicker')"
            class="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
          />
        </div>
      </div>
      <div v-if="editingTagName" class="flex items-center gap-2.5 mt-3">
        <span class="badge editable" :style="{ '--tag-color': editingTagColor }">
          {{ editingTagName }}
        </span>
        <button
          v-if="isMultiMode"
          type="button"
          class="btn btn--secondary btn--sm"
          @click="commitDraft"
        >
          {{ $t('tokenForm.addTag') }}
        </button>
      </div>

      <!-- 多标签模式的上限提示：说明还能加几个，而不是让输入框莫名其妙点不动 -->
      <div v-if="isMultiMode" class="text-[12px] text-text-muted">
        {{ isTagLimitReached
          ? $t('tokenForm.tagLimitReached', { max: maxTags })
          : $t('tokenForm.tagLimitHint', { max: maxTags }) }}
      </div>
      <!-- 批量模式提示 -->
      <div v-if="isBatchMode" class="mt-3 px-3.5 py-3 bg-accent-tech border border-border-accent-tech rounded-lg text-[13px] text-accent">
        {{ $t('tokenList.batchTagHint', { count: tokens?.length || 0 }) }}
      </div>
    </div>

    <template #footer>
      <button @click="handleClear" class="btn btn--secondary btn--md" v-if="showClearButton">
        {{ $t('tokenForm.clearTag') }}
      </button>
      <button @click="handleClose" class="btn btn--ghost btn--md">
        {{ $t('common.cancel') }}
      </button>
      <button @click="handleSave" class="btn btn--primary btn--md">
        {{ $t('common.confirm') }}
      </button>
    </template>
  </BaseModal>
</template>

<script setup>
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import BaseModal from '../common/BaseModal.vue'
import { DEFAULT_TAG_COLOR } from '../../utils/accountTags'

const { t } = useI18n()

const resolveCssVar = (name, fallback) => {
  if (typeof window === 'undefined') return fallback
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return value || fallback
}

const props = defineProps({
  visible: {
    type: Boolean,
    default: false
  },
  // 单个 token 编辑模式
  token: {
    type: Object,
    default: null
  },
  // 批量编辑模式
  tokens: {
    type: Array,
    default: null
  },
  // 所有 tokens，用于标签建议
  allTokens: {
    type: Array,
    default: () => []
  },
  // 允许的标签数上限；保持 1 即旧的单标签行为，调用方无需改动
  maxTags: {
    type: Number,
    default: 1
  }
})

const emit = defineEmits(['update:visible', 'save', 'clear'])

// 编辑状态
const defaultTagColor = resolveCssVar('--tag-default', DEFAULT_TAG_COLOR)
const editingTagName = ref('')
const editingTagColor = ref(defaultTagColor)
const showTagSuggestions = ref(false)
const tagNameInputRef = ref(null)
// 多标签模式下已确认的标签；单标签模式全程为空，走 draft 一条路
const committedTags = ref([])

// 计算属性
const isBatchMode = computed(() => props.tokens && props.tokens.length > 0)

const isMultiMode = computed(() => props.maxTags > 1)

const isTagLimitReached = computed(
  () => isMultiMode.value && committedTags.value.length >= props.maxTags
)

const modalTitle = computed(() => {
  if (isBatchMode.value) {
    return t('tokenList.batchEditTag')
  }
  return t('tokenList.editTag')
})

const showClearButton = computed(() => {
  if (isBatchMode.value) {
    // 批量模式：只要有任何一个 token 有标签就显示清除按钮
    return props.tokens?.some(token => tokenTags(token).length > 0)
  }
  // 单个模式：当前 token 有标签才显示
  return tokenTags(props.token).length > 0
})

/** 读出一条记录的标签：多标签的 `tags` 优先，回退到单标签字段 */
const tokenTags = (token) => {
  if (!token) return []
  if (Array.isArray(token.tags) && token.tags.length) {
    return token.tags
      .filter(tag => tag?.name?.trim())
      .map(tag => ({ name: tag.name.trim(), color: tag.color || defaultTagColor }))
  }
  return token.tag_name?.trim()
    ? [{ name: token.tag_name.trim(), color: token.tag_color || defaultTagColor }]
    : []
}

// 从所有 tokens 中提取已使用的标签（同名只留首次出现的颜色）
const existingTags = computed(() => {
  if (!props.allTokens) return []
  const tagMap = new Map()

  props.allTokens.forEach(token => {
    tokenTags(token).forEach(tag => {
      const key = tag.name.toLowerCase()
      if (!tagMap.has(key)) tagMap.set(key, tag)
    })
  })

  return Array.from(tagMap.values())
    .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()))
})

// 根据输入过滤标签建议；已经加上的标签不再出现在建议里
const filteredTagSuggestions = computed(() => {
  const chosen = new Set(committedTags.value.map(tag => tag.name.toLowerCase()))
  const available = existingTags.value.filter(tag => !chosen.has(tag.name.toLowerCase()))
  const input = editingTagName.value.trim().toLowerCase()

  if (!input) {
    return available
  }

  return available.filter(tag =>
    tag.name.toLowerCase().includes(input)
  )
})

// 监听 visible 变化，初始化编辑状态
watch(() => props.visible, (newVal) => {
  if (newVal) {
    committedTags.value = []
    if (isBatchMode.value) {
      // 批量模式：使用默认值
      editingTagName.value = ''
      editingTagColor.value = defaultTagColor
    } else if (props.token) {
      const current = tokenTags(props.token)
      if (isMultiMode.value) {
        // 多标签模式：已有标签全部进入待编辑列表，输入框留空用于继续添加
        committedTags.value = current.slice(0, props.maxTags)
        editingTagName.value = ''
        editingTagColor.value = defaultTagColor
      } else {
        // 单标签模式：沿用旧行为，输入框直接就是那个标签
        editingTagName.value = current[0]?.name || ''
        editingTagColor.value = current[0]?.color || defaultTagColor
      }
    }
    showTagSuggestions.value = false
  }
})

// 处理标签输入
const handleTagInput = () => {
  showTagSuggestions.value = true
}

// 处理标签输入框失焦
const handleTagBlur = () => {
  setTimeout(() => {
    showTagSuggestions.value = false
  }, 200)
}

// 选择标签建议：多标签模式下直接入列，省掉再点一次「添加」
const selectTagSuggestion = (suggestion) => {
  editingTagName.value = suggestion.name
  editingTagColor.value = suggestion.color
  showTagSuggestions.value = false
  if (isMultiMode.value) {
    commitDraft()
  }
}

/** 把输入框里的草稿收进标签列表（重名或超上限则忽略） */
const commitDraft = () => {
  if (!isMultiMode.value) return
  const name = editingTagName.value.trim()
  if (!name || isTagLimitReached.value) return
  const exists = committedTags.value.some(
    tag => tag.name.toLowerCase() === name.toLowerCase()
  )
  if (!exists) {
    committedTags.value = [...committedTags.value, { name, color: editingTagColor.value }]
  }
  editingTagName.value = ''
  editingTagColor.value = defaultTagColor
}

const removeTag = (index) => {
  committedTags.value = committedTags.value.filter((_, i) => i !== index)
}

// 关闭模态框
const handleClose = () => {
  emit('update:visible', false)
}

/**
 * 保存。
 *
 * 同时给出两种形状：`tagName` / `tagColor` 是旧调用方读的单标签字段，
 * `tags` 是多标签列表。这样单标签调用方完全不用改。
 */
const handleSave = () => {
  // 多标签模式下输入框还留着字就一并收进去，用户不必先点「添加」再点「确定」
  commitDraft()
  const tags = isMultiMode.value
    ? committedTags.value
    : (editingTagName.value.trim()
        ? [{ name: editingTagName.value.trim(), color: editingTagColor.value }]
        : [])

  emit('save', {
    tagName: tags[0]?.name ?? '',
    tagColor: tags[0]?.color ?? editingTagColor.value,
    tags
  })
  handleClose()
}

// 清除标签
const handleClear = () => {
  emit('clear')
  handleClose()
}
</script>
