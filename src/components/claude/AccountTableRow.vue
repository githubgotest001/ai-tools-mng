<template>
  <tr
    :class="[
      'group transition-colors duration-200',
      'hover:bg-accent/6',
      isSelected ? 'bg-accent/10' : ''
    ]"
    @click="handleRowClick"
  >
    <!-- 多选框 -->
    <td class="w-11 text-center py-3.5 border-b border-border/50 align-middle whitespace-nowrap text-[13px] text-text relative first-cell">
      <div class="inline-flex items-center justify-center h-5 cursor-pointer align-middle leading-none" @click.stop="toggleSelection">
        <div class="checkbox-inner" :class="{ 'checked': isSelected }">
          <svg v-if="isSelected" class="h-3 w-3" viewBox="0 0 24 24" fill="currentColor">
            <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" />
          </svg>
        </div>
      </div>
    </td>

    <!-- 服务名称 -->
    <td class="px-2.5 py-3.5 border-b border-border/50 align-middle text-[13px] text-text">
      <span class="font-medium text-text block truncate" v-tooltip="account.service_name">{{ account.service_name }}</span>
    </td>

    <!-- 网站地址 -->
    <td class="px-2.5 py-3.5 border-b border-border/50 align-middle text-[13px] text-text">
      <a
        v-if="account.website_url"
        :href="account.website_url"
        class="text-accent no-underline hover:underline truncate block"
        v-tooltip="account.website_url"
        @click.stop.prevent="openExternalUrl"
      >{{ displayUrl }}</a>
      <span v-else class="text-text-muted">-</span>
    </td>

    <!-- 到期时间 -->
    <td class="px-2.5 py-3.5 border-b border-border/50 align-middle text-[13px] text-text">
      <span :class="expiryStatusClass">{{ formattedExpiryDate }}</span>
      <span v-if="daysLeftText" class="text-[11px] opacity-80 ml-1">({{ daysLeftText }})</span>
    </td>

    <!-- 标签 -->
    <td class="px-2.5 py-3.5 border-b border-border/50 align-middle text-[13px] text-text">
      <TagBadges
        :account="account"
        :max="1"
        badge-class="max-w-[90px]"
        @edit="openTagEditor"
      />
    </td>

    <!-- 操作 -->
    <td class="px-2.5 py-3.5 border-b border-border/50 align-middle whitespace-nowrap text-[13px] text-text">
      <div class="flex items-center justify-center gap-1.5">
        <button class="btn btn--ghost btn--icon-sm" @click="$emit('edit', account)" v-tooltip="$t('common.edit')">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
            <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"/>
          </svg>
        </button>
        <button class="btn btn--ghost btn--icon-sm text-danger" @click="$emit('delete', account)" v-tooltip="$t('common.delete')">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
            <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"/>
          </svg>
        </button>
      </div>
    </td>
  </tr>

  <!-- 标签编辑模态框 -->
  <TagEditorModal
    v-model:visible="showTagEditor"
    :token="accountAsToken"
    :all-tokens="allAccountsAsTokens"
    :max-tags="MAX_ACCOUNT_TAGS"
    @save="handleTagSave"
    @clear="handleTagClear"
  />
</template>

<script setup>
import { computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import TagBadges from '../common/TagBadges.vue'
import TagEditorModal from '../token/TagEditorModal.vue'
import { useAccountTags } from '../../composables/useAccountTags'
import { MAX_ACCOUNT_TAGS } from '../../utils/accountTags'

const { t: $t } = useI18n()

const props = defineProps({
  account: {
    type: Object,
    required: true
  },
  isSelected: {
    type: Boolean,
    default: false
  },
  selectionMode: {
    type: Boolean,
    default: false
  },
  allAccounts: {
    type: Array,
    default: () => []
  }
})

const emit = defineEmits(['edit', 'delete', 'select', 'account-updated'])

// 选择和点击
const toggleSelection = () => {
  emit('select', props.account.id)
}

const handleRowClick = () => {
  if (props.selectionMode) {
    toggleSelection()
  } else {
    emit('edit', props.account)
  }
}

// 标签相关
const {
  showTagEditor,
  accountAsToken,
  allAccountsAsTokens,
  openTagEditor,
  handleTagSave,
  handleTagClear
} = useAccountTags(props, emit)

// 打开外部链接
const openExternalUrl = async () => {
  const url = props.account.website_url
  if (!url) return
  try {
    await invoke('open_url', { url })
  } catch (error) {
    console.error('Failed to open URL:', error)
  }
}

// 简化显示的 URL（移除 https:// 等前缀）
const displayUrl = computed(() => {
  if (!props.account.website_url) return ''
  return props.account.website_url.replace(/^https?:\/\//i, '').replace(/\/$/, '')
})

// 格式化到期日期
const formattedExpiryDate = computed(() => {
  if (!props.account.expiry_date) return $t('subscriptions.noExpiry')
  const date = new Date(props.account.expiry_date * 1000)
  return date.toLocaleDateString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit'
  })
})

const daysLeft = computed(() => {
  if (!props.account.expiry_date) return null
  const now = Date.now()
  const expiry = props.account.expiry_date * 1000
  return Math.ceil((expiry - now) / (1000 * 60 * 60 * 24))
})

const daysLeftText = computed(() => {
  if (daysLeft.value === null) return ''
  if (daysLeft.value < 0) return $t('subscriptions.expired')
  if (daysLeft.value === 0) return $t('subscriptions.expirestoday')
  return $t('subscriptions.daysLeft', { days: daysLeft.value })
})

// 过期状态样式
const expiryStatusClass = computed(() => {
  if (!props.account.expiry_date) return 'text-text-muted'
  if (daysLeft.value === null) return 'text-text-muted'
  if (daysLeft.value < 0) return 'text-danger'
  if (daysLeft.value < 10) return 'text-danger'
  if (daysLeft.value < 20) return 'text-warning'
  return 'text-success'
})
</script>
