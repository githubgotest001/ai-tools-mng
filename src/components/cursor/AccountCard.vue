<template>
  <div
    class="bg-surface border border-border rounded-lg p-3 cursor-pointer relative group transition-all duration-150 hover:bg-hover hover:border-border-strong"
    :class="{
      'opacity-60 pointer-events-none': isSwitching,
      'border-accent bg-accent/5': isSelected
    }"
    @click="handleCardClick"
  >
    <!-- 头部：选择框 + 邮箱标题 -->
    <div class="flex items-center gap-2 mb-3 pr-8">
      <!-- 选择框 -->
      <div
        class="selection-checkbox"
        :class="{ 'visible': selectionMode || isSelected }"
        @click.stop="toggleSelection"
      >
        <div class="checkbox-inner" :class="{ 'checked': isSelected }">
          <svg v-if="isSelected" class="h-3 w-3" viewBox="0 0 24 24" fill="currentColor">
            <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" />
          </svg>
        </div>
      </div>

      <!-- 邮箱 -->
      <div
        class="inline-flex items-center gap-1 cursor text-[15px] font-semibold text-text truncate flex-1"
        v-tooltip="account.email"
        @click.stop="copyEmail"
      >
        <span>{{ displayEmail }}</span>
      </div>

      <!-- Session 失效标识：套餐仍是上次成功刷新的数据，这里提示数据已不再更新 -->
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
      <!-- 凭证事前预警：按 JWT exp 判断，过期前 7 天开始提示 -->
      <span
        v-else-if="credentialBadge"
        class="badge badge--sm shrink-0"
        :class="credentialBadge.variant === 'danger' ? 'badge--danger-tech' : 'badge--warning-tech'"
        v-tooltip="credentialTooltip"
      >
        <svg class="w-3 h-3" viewBox="0 0 24 24" fill="currentColor">
          <path d="M11.99 2C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm.5-13H11v6l5.25 3.15.75-1.23-4.5-2.67z"/>
        </svg>
        {{ credentialBadge.text }}
      </span>
    </div>

    <!-- 右上角状态徽章 -->
    <div class="absolute right-3 top-3 z-10 flex items-center gap-1.5">
      <span v-if="isCurrent" class="badge badge--success-tech">
        <span class="status-dot text-success"></span>
        {{ $t('platform.cursor.status.current') }}
      </span>
    </div>

    <!-- 右上角按钮组（悬停显示） -->
    <div
      class="absolute right-3 top-3 z-20 flex items-center gap-1.5 opacity-0 group-hover:opacity-100 transition-opacity"
      :class="{ 'opacity-100': isMenuOpen }"
      @click.stop
    >
      <!-- 切换按钮：有其它账号正在切换时整体锁住，避免并发写 Cursor 的状态库 -->
      <button
        v-if="!isCurrent"
        @click="$emit('switch', account.id)"
        class="w-7 h-7 rounded border-none bg-surface text-text-secondary cursor-pointer flex items-center justify-center shadow-sm hover:bg-hover hover:text-accent transition-colors disabled:cursor-not-allowed disabled:opacity-50"
        :disabled="isSwitching || switchLocked"
        v-tooltip="switchLocked && !isSwitching ? $t('platform.cursor.switchLocked') : $t('platform.cursor.switch')"
      >
        <svg v-if="!isSwitching" class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="currentColor">
          <path d="M6.99 11L3 15l3.99 4v-3H14v-2H6.99v-3zM21 9l-3.99-4v3H10v2h7.01v3L21 9z"/>
        </svg>
        <span v-else class="btn-spinner btn-spinner--sm text-accent"></span>
      </button>

      <!-- 刷新配额按钮 -->
      <button
        v-if="hasSessionToken"
        @click="$emit('refresh-quota', account.id)"
        class="w-7 h-7 rounded border-none bg-surface text-text-secondary cursor-pointer flex items-center justify-center shadow-sm hover:bg-hover hover:text-accent transition-colors"
        :disabled="isRefreshing"
        v-tooltip="$t('platform.cursor.refreshQuota')"
      >
        <svg v-if="!isRefreshing" class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="currentColor">
          <path d="M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08c-.82 2.33-3.04 4-5.65 4-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z"/>
        </svg>
        <span v-else class="btn-spinner btn-spinner--sm text-accent"></span>
      </button>

      <!-- 操作菜单 -->
      <FloatingDropdown
        ref="menuRef"
        placement="bottom-end"
        :close-on-select="true"
        @open="isMenuOpen = true"
        @close="isMenuOpen = false"
      >
        <template #trigger>
          <button
            class="w-7 h-7 rounded border-none bg-surface text-text-secondary cursor-pointer flex items-center justify-center shadow-sm hover:bg-hover hover:text-text transition-colors"
            v-tooltip="$t('app.moreOptions')"
          >
            <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="currentColor">
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

    <!-- 属性列表 -->
    <div class="flex flex-col gap-1.5">
      <!-- 创建时间 -->
      <div v-if="account.created_at" class="flex items-center gap-1 min-h-6">
        <div class="flex items-center gap-1.5 w-[90px] shrink-0 text-text-muted text-xs">
          <svg class="w-3.5 h-3.5 shrink-0 opacity-70" viewBox="0 0 24 24" fill="currentColor">
            <path d="M11.99 2C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm.5-13H11v6l5.25 3.15.75-1.23-4.5-2.67z"/>
          </svg>
          <span>{{ $t('tokenCard.createdAt') }}</span>
        </div>
        <div class="flex-1 text-[13px] text-text-muted truncate">
          {{ formatDateTime(account.created_at) }}
        </div>
      </div>

      <!-- 主凭证到期：有 Session 展示 Session，否则展示 Access；两条完整时间都在悬浮里 -->
      <div class="flex items-center gap-1 min-h-6" v-tooltip="credentialTooltip">
        <div class="flex items-center gap-1.5 w-[90px] shrink-0 text-text-muted text-xs">
          <svg class="w-3.5 h-3.5 shrink-0 opacity-70" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12.65 10C11.83 7.67 9.61 6 7 6c-3.31 0-6 2.69-6 6s2.69 6 6 6c2.61 0 4.83-1.67 5.65-4H17v4h4v-4h2v-4H12.65zM7 14c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2z"/>
          </svg>
          <span>{{ primaryExpiryLabel }}</span>
        </div>
        <div class="flex-1 min-w-0 flex items-center gap-1.5 text-[13px] tabular-nums whitespace-nowrap">
          <span :class="primaryExpiry.textClass || 'text-text-muted'">{{ primaryExpiry.date }}</span>
          <template v-if="primaryExpiry.remaining">
            <span class="text-text-muted/50">·</span>
            <span class="text-[12px] font-medium" :class="primaryExpiry.textClass || 'text-text-muted'">{{ primaryExpiry.remaining }}</span>
          </template>
        </div>
      </div>

      <!-- 订阅计划 -->
      <div v-if="account.membership_type" class="flex items-center gap-1 min-h-6">
        <div class="flex items-center gap-1.5 w-[90px] shrink-0 text-text-muted text-xs">
          <svg class="w-3.5 h-3.5 shrink-0 opacity-70" viewBox="0 0 24 24" fill="currentColor">
            <path d="M20 4H4c-1.11 0-1.99.89-1.99 2L2 18c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V6c0-1.11-.89-2-2-2zm0 14H4v-6h16v6zm0-10H4V6h16v2z"/>
          </svg>
          <span>{{ $t('platform.cursor.membershipType') }}</span>
        </div>
        <div class="flex-1 min-w-0">
          <MembershipBadge :membership-type="account.membership_type" :badge-class="getMembershipBadgeClass(account.membership_type)" />
        </div>
      </div>

      <!-- 账期：独立一行避免截断；悬停显示含时分的完整区间 -->
      <div v-if="billingCycle" class="flex items-center gap-1 min-h-6">
        <div class="flex items-center gap-1.5 w-[90px] shrink-0 text-text-muted text-xs">
          <svg class="w-3.5 h-3.5 shrink-0 opacity-70" viewBox="0 0 24 24" fill="currentColor">
            <path d="M17 12h-5v5h5v-5zM16 1v2H8V1H6v2H5c-1.11 0-1.99.9-1.99 2L3 19c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2h-1V3h-2zm3 18H5V8h14v11z"/>
          </svg>
          <span>{{ $t('platform.cursor.billingCycleShort') }}</span>
        </div>
        <div class="flex-1 text-[13px] text-text-muted tabular-nums" v-tooltip="billingCycleTooltip">
          {{ billingCycle.short }}
        </div>
      </div>

      <!-- Auto / API / Bot 分池剩余进度条（无数据的池不渲染） -->
      <template v-if="hasSessionToken">
        <div v-for="item in quotaBars" :key="item.key" class="flex items-center gap-1 min-h-6">
          <div class="flex items-center gap-1.5 w-[90px] shrink-0 text-text-muted text-xs">
            <svg class="w-3.5 h-3.5 shrink-0 opacity-70" viewBox="0 0 24 24" fill="currentColor">
              <path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM7 12h2v5H7zm4-3h2v8h-2zm4-3h2v11h-2z"/>
            </svg>
            <span v-tooltip="quotaBarHint(item)">{{ $t(`platform.cursor.${item.key}Available`) }}</span>
          </div>
          <div class="flex-1 flex items-center gap-1">
            <div class="flex-1 h-1.5 bg-muted rounded overflow-hidden">
              <div class="h-full rounded transition-all"
                   :class="getQuotaBarClass(item.percent)"
                   :style="{ width: item.percent + '%' }">
              </div>
            </div>
            <span class="text-[11px] font-medium tabular-nums w-8 text-right" :class="getQuotaTextClass(item.percent)">
              {{ item.percent }}%
            </span>
          </div>
        </div>

        <!-- 查看用量入口：与其它属性行一致，左侧配额 label + 右侧按钮 -->
        <div class="flex items-center gap-1 min-h-6">
          <div class="flex items-center gap-1.5 w-[90px] shrink-0 text-text-muted text-xs">
            <svg class="w-3.5 h-3.5 shrink-0 opacity-70" viewBox="0 0 24 24" fill="currentColor">
              <path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM7 12h2v5H7zm4-3h2v8h-2zm4-3h2v11h-2z"/>
            </svg>
            <span>{{ $t('platform.cursor.quotaLabel') }}</span>
          </div>
          <div class="flex-1 text-[13px]">
            <button
              @click.stop="showUsageModal = true"
              class="inline-flex items-center gap-1 px-2 py-0.5 text-xs text-accent bg-accent/10 border border-accent/30 rounded hover:bg-accent/20 transition-colors cursor-pointer"
            >
              <svg class="w-3 h-3" viewBox="0 0 24 24" fill="currentColor">
                <path d="M3 3h2v18H3V3zm16 8h2v10h-2V11zm-8 4h2v6h-2v-6zm4-8h2v14h-2V7zm-8 6h2v8H7v-8z"/>
              </svg>
              {{ $t('cursorUsage.viewUsage') }}
            </button>
          </div>
        </div>
      </template>

      <!-- 标签 -->
      <div class="flex items-center gap-1 min-h-6">
        <div class="flex items-center gap-1.5 w-[90px] shrink-0 text-text-muted text-xs">
          <svg class="w-3.5 h-3.5 shrink-0 opacity-70" viewBox="0 0 24 24" fill="currentColor">
            <path d="M17.63 5.84C17.27 5.33 16.67 5 16 5L5 5.01C3.9 5.01 3 5.9 3 7v10c0 1.1.9 1.99 2 1.99L16 19c.67 0 1.27-.33 1.63-.84L22 12l-4.37-6.16z"/>
          </svg>
          <span>{{ $t('subscriptions.fields.tag') }}</span>
        </div>
        <div class="flex-1 min-w-0 text-[13px]">
          <TagBadges
            :account="account"
            :max="2"
            empty-style="text"
            @edit="openTagEditor"
          />
        </div>
      </div>
    </div>
  </div>

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
  /** 有任一账号正在切换时为 true，所有卡片的切换按钮一起禁用 */
  switchLocked: { type: Boolean, default: false },
  isRefreshing: { type: Boolean, default: false },
  isSelected: { type: Boolean, default: false },
  selectionMode: { type: Boolean, default: false },
  showRealEmail: { type: Boolean, default: true },
  allAccounts: { type: Array, default: () => [] }
})

const emit = defineEmits(['switch', 'delete', 'select', 'account-updated', 'account-synced', 'machine-id-generated', 'refresh-quota'])

const menuRef = ref(null)
const isMenuOpen = ref(false)

const {
  showUsageModal,
  showSessionsModal,
  isGeneratingMachineId,
  hasSessionToken,
  displayEmail,
  credentialBadge,
  credentialTooltip,
  primaryExpiry,
  primaryExpiryLabel,
  sessionInvalid,
  sessionInvalidTooltip,
  getMembershipBadgeClass,
  formatDateTime,
  copyEmail,
  handleMenuClick
} = useCursorAccount(props, emit)

const {
  grokBotResetLabel,
  quotaBars,
  billingCycle,
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

const billingCycleTooltip = computed(() =>
  billingCycle.value ? `${$t('cursorUsage.billingCycle')}: ${billingCycle.value.full}` : ''
)

const toggleSelection = () => emit('select', props.account.id)

const handleCardClick = () => {
  if (props.selectionMode) toggleSelection()
}

// 标签操作
const {
  showTagEditor,
  accountAsToken,
  allAccountsAsTokens,
  openTagEditor,
  handleTagSave,
  handleTagClear
} = useAccountTags(props, emit)
</script>
