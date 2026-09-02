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
    <td class="w-11 text-center px-2.5 py-3.5 border-b border-border/50 align-top whitespace-nowrap text-[13px] text-text relative first-cell">
      <div class="inline-flex items-center justify-center h-5 cursor-pointer align-middle leading-none" @click.stop="toggleSelection">
        <div class="checkbox-inner" :class="{ 'checked': isSelected }">
          <svg v-if="isSelected" class="h-3 w-3" viewBox="0 0 24 24" fill="currentColor">
            <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" />
          </svg>
        </div>
      </div>
    </td>

    <!-- 标签 -->
    <td class="w-[140px] px-2.5 py-3.5 border-b border-border/50 align-top whitespace-nowrap text-[13px] text-text">
      <TagBadges
        :account="account"
        :max="1"
        badge-class="max-w-[120px]"
        @edit="openTagEditor"
      />
    </td>

    <!-- 邮箱 -->
    <td class="px-2.5 py-3.5 border-b border-border/50 align-top whitespace-nowrap text-[13px] text-text">
      <div class="flex items-center gap-1.5">
        <div class="text-copyable min-w-0" @click.stop="copyEmail" v-tooltip="account.email">
          <span class="text-copyable__content">{{ displayEmail }}</span>
        </div>
        <span v-if="isCurrent" class="badge badge--sm badge--success-tech shrink-0">
          <span class="status-dot text-success"></span>
          {{ $t('platform.cursor.status.current') }}
        </span>
        <MembershipBadge
          v-if="account.membership_type"
          class="shrink-0"
          :membership-type="account.membership_type"
          :badge-class="getMembershipBadgeClass(account.membership_type)"
        />
        <span
          v-if="sessionInvalid"
          class="badge badge--sm badge--danger-tech shrink-0"
          v-tooltip="sessionInvalidTooltip"
        >
          <svg class="w-3 h-3" viewBox="0 0 24 24" fill="currentColor">
            <path d="M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z"/>
          </svg>
          {{ $t('platform.cursor.sessionInvalid') }}
        </span>
        <span
          v-else-if="credentialBadge"
          class="badge badge--sm shrink-0"
          :class="credentialBadge.variant === 'danger' ? 'badge--danger-tech' : 'badge--warning-tech'"
          v-tooltip="credentialTooltip"
        >
          {{ credentialBadge.text }}
        </span>
      </div>
    </td>

    <!-- 可用额度：Auto / API / Bot 分池剩余进度条 -->
    <td class="w-[200px] px-2.5 py-3.5 border-b border-border/50 align-top whitespace-nowrap text-[12px] text-text-muted">
      <div v-if="hasSessionToken && quotaBars.length" class="flex flex-col gap-1">
        <div v-for="item in quotaBars" :key="item.key" class="flex items-center gap-1">
          <span class="w-[4.5rem] shrink-0 text-text-muted/60 truncate" v-tooltip="quotaBarHint(item)">{{ $t(`platform.cursor.${item.key}Available`) }}</span>
          <div class="flex-1 h-1.5 bg-muted rounded overflow-hidden">
            <div class="h-full rounded transition-all"
                 :class="getQuotaBarClass(item.percent)"
                 :style="{ width: item.percent + '%' }">
            </div>
          </div>
          <span class="text-[11px] font-medium tabular-nums w-7 text-right" :class="getQuotaTextClass(item.percent)">{{ item.percent }}%</span>
        </div>
      </div>
      <span v-else class="text-text-muted/50">-</span>
    </td>

    <!-- 过期时间：按剩余时长着色，过期前 7 天转黄、过期转红 -->
    <td class="w-[105px] px-2.5 py-3.5 border-b border-border/50 align-top whitespace-nowrap text-[12px] text-text-muted">
      <div class="flex flex-col gap-0.5 tabular-nums">
        <div class="flex items-center gap-1" v-tooltip="expiryTooltip('access', accessExpiry)">
          <span class="text-text-muted/60">A:</span>
          <span :class="accessExpiry.textClass">{{ accessExpiry.date }}</span>
        </div>
        <div class="flex items-center gap-1" v-tooltip="expiryTooltip('session', sessionExpiry)">
          <span class="text-text-muted/60">S:</span>
          <span :class="sessionExpiry.textClass">{{ sessionExpiry.date }}</span>
        </div>
      </div>
    </td>

    <!-- 配额 -->
    <td class="w-[120px] px-2.5 py-3.5 border-b border-border/50 align-top whitespace-nowrap text-[13px] text-text">
      <button
        v-if="hasSessionToken"
        @click.stop="showUsageModal = true"
        class="inline-flex items-center gap-1 px-2 py-0.5 text-xs text-accent bg-accent/10 border border-accent/30 rounded hover:bg-accent/20 transition-colors cursor-pointer"
        v-tooltip="$t('cursorUsage.viewUsage')"
      >
        <svg class="w-3 h-3" viewBox="0 0 24 24" fill="currentColor">
          <path d="M3 3h2v18H3V3zm16 8h2v10h-2V11zm-8 4h2v6h-2v-6zm4-8h2v14h-2V7zm-8 6h2v8H7v-8z"/>
        </svg>
        {{ $t('cursorUsage.viewUsageShort') }}
      </button>
      <span v-else class="text-text-muted/50">-</span>
    </td>

    <!-- 操作 -->
    <td class="w-[80px] px-2.5 py-3.5 border-b border-border/50 align-top whitespace-nowrap text-[13px] text-text text-center">
      <div class="flex items-center justify-center gap-1">
        <button
          v-if="!isCurrent"
          @click.stop="$emit('switch', account.id)"
          class="btn btn--ghost btn--icon-sm"
          :disabled="isSwitching || switchLocked"
          v-tooltip="switchLocked && !isSwitching ? $t('platform.cursor.switchLocked') : $t('platform.cursor.switch')"
        >
          <svg v-if="!isSwitching" class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M6.99 11L3 15l3.99 4v-3H14v-2H6.99v-3zM21 9l-3.99-4v3H10v2h7.01v3L21 9z" />
          </svg>
          <span v-else class="btn-spinner btn-spinner--xs text-accent" aria-hidden="true"></span>
        </button>

        <button
          v-if="hasSessionToken"
          @click.stop="$emit('refresh-quota', account.id)"
          class="btn btn--ghost btn--icon-sm"
          :disabled="isRefreshing"
          v-tooltip="$t('platform.cursor.refreshQuota')"
        >
          <svg v-if="!isRefreshing" class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08c-.82 2.33-3.04 4-5.65 4-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z"/>
          </svg>
          <span v-else class="btn-spinner btn-spinner--xs text-accent" aria-hidden="true"></span>
        </button>

        <FloatingDropdown ref="menuRef" placement="bottom-end" :close-on-select="true" @click.stop>
          <template #trigger>
            <button class="btn btn--ghost btn--icon-sm" v-tooltip="$t('app.moreOptions')">
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z"/>
              </svg>
            </button>
          </template>
          <template #default="{ close }">
            <CursorAccountMenu
              :has-session-token="hasSessionToken"
              :is-generating-machine-id="isGeneratingMachineId"
              @select="(type) => handleMenuClick(type, close)"
            />
          </template>
        </FloatingDropdown>
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

  <!-- 使用详情模态框 -->
  <CursorUsageModal
    v-if="showUsageModal"
    :account="account"
    @close="showUsageModal = false"
    @account-synced="(id) => $emit('account-synced', id)"
  />

  <!-- 登录设备模态框 -->
  <CursorSessionsModal
    v-if="showSessionsModal"
    :account="account"
    @close="showSessionsModal = false"
  />
</template>

<script setup>
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import FloatingDropdown from '../common/FloatingDropdown.vue'
import TagBadges from '../common/TagBadges.vue'
import TagEditorModal from '../token/TagEditorModal.vue'
import CursorUsageModal from './CursorUsageModal.vue'
import CursorSessionsModal from './CursorSessionsModal.vue'
import CursorAccountMenu from './CursorAccountMenu.vue'
import MembershipBadge from './MembershipBadge.vue'
import { useAccountTags } from '../../composables/useAccountTags'
import { useCursorQuota } from '../../composables/useCursorQuota'
import { useCursorAccount } from '../../composables/useCursorAccount'
import { MAX_ACCOUNT_TAGS } from '../../utils/accountTags'

const { t: $t } = useI18n()

const props = defineProps({
  account: { type: Object, required: true },
  isCurrent: { type: Boolean, default: false },
  isSwitching: { type: Boolean, default: false },
  /** 有任一账号正在切换时为 true，所有行的切换按钮一起禁用 */
  switchLocked: { type: Boolean, default: false },
  isRefreshing: { type: Boolean, default: false },
  isSelected: { type: Boolean, default: false },
  selectionMode: { type: Boolean, default: false },
  showRealEmail: { type: Boolean, default: true },
  allAccounts: { type: Array, default: () => [] }
})

const emit = defineEmits(['switch', 'delete', 'select', 'account-updated', 'account-synced', 'machine-id-generated', 'refresh-quota'])

const menuRef = ref(null)

const {
  showUsageModal,
  showSessionsModal,
  isGeneratingMachineId,
  hasSessionToken,
  displayEmail,
  credentialBadge,
  credentialTooltip,
  accessExpiry,
  sessionExpiry,
  sessionInvalid,
  sessionInvalidTooltip,
  getMembershipBadgeClass,
  copyEmail,
  handleMenuClick
} = useCursorAccount(props, emit)

const {
  grokBotResetLabel,
  quotaBars,
  getQuotaBarClass,
  getQuotaTextClass
} = useCursorQuota(() => props.account)

const grokBotHint = computed(() => {
  const base = $t('platform.cursor.grokBotAvailableHint')
  return grokBotResetLabel.value
    ? `${base} · ${$t('platform.cursor.grokBotResets', { date: grokBotResetLabel.value })}`
    : base
})

// 每条进度条的 tooltip：金额（拿到才显示）在前，再接「剩余可用」口径与各池说明
const quotaBarHint = (item) => {
  const hint = item.key === 'grokBot' ? grokBotHint.value : $t(`platform.cursor.${item.key}AvailableHint`)
  const parts = [$t('platform.cursor.quotaRemainingHint'), hint]
  if (item.spend) {
    parts.unshift($t('platform.cursor.poolSpend', { spend: item.spend }))
  }
  return parts.join(' · ')
}

// 列宽只放得下日期，完整时刻与剩余时长放进 tooltip
const expiryTooltip = (kind, cell) => {
  const title = kind === 'access'
    ? $t('platform.cursor.accessTokenExpiry')
    : $t('platform.cursor.sessionExpiry')
  if (!cell.dateTime) return `${title} · ${$t('platform.cursor.credential.none')}`
  return cell.stateLabel
    ? `${title} · ${cell.dateTime} · ${cell.stateLabel}`
    : `${title} · ${cell.dateTime}`
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

const toggleSelection = () => emit('select', props.account.id)

const handleRowClick = () => {
  if (props.selectionMode) toggleSelection()
}
</script>
