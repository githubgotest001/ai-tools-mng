<template>
  <BaseModal
    :visible="true"
    :title="$t('cursorUsage.title')"
    :close-on-overlay="true"
    :body-scroll="false"
    modal-class="!max-w-[900px]"
    @close="handleClose"
  >
    <template #header>
      <div class="flex items-center gap-3 flex-1">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor" class="text-accent">
          <path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM9 17H7v-7h2v7zm4 0h-2V7h2v10zm4 0h-2v-4h2v4z"/>
        </svg>
        <h3 class="modal-title">{{ $t('cursorUsage.title') }}</h3>
      </div>
      <button @click="refresh" class="btn btn--ghost btn--icon" :disabled="eventsLoading">
        <svg v-if="!eventsLoading" width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
          <path d="M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08c-.82 2.33-3.04 4-5.65 4-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z"/>
        </svg>
        <span v-else class="btn-spinner"></span>
      </button>
    </template>

    <!-- 时间范围筛选：快捷区间 + 自定义起止日期 -->
    <div class="flex flex-wrap items-center gap-2 mb-4">
      <div class="flex flex-wrap items-center gap-1.5">
        <button
          v-for="preset in rangePresets"
          :key="preset.key"
          class="px-2 py-1 rounded border text-[12px] leading-none transition-colors"
          :class="rangePreset === preset.key
            ? 'bg-accent/15 text-accent border-accent/30'
            : 'bg-transparent text-text-muted border-border hover:text-text-strong'"
          @click="applyPreset(preset.key)"
        >
          {{ preset.label }}
        </button>
      </div>
      <div class="flex items-center gap-1.5 ml-auto">
        <input
          v-model="startDateStr"
          type="date"
          class="input !w-[138px] h-8 px-2 py-1 text-[12px]"
          @change="onDateInputChange"
        />
        <span class="text-[12px] text-text-muted">{{ $t('cursorUsage.dateTo') }}</span>
        <input
          v-model="endDateStr"
          type="date"
          class="input !w-[138px] h-8 px-2 py-1 text-[12px]"
          @change="onDateInputChange"
        />
      </div>
    </div>

    <!-- 基本信息区域（跟随所选时间范围，由明细事件汇总，与下方合计同口径） -->
    <div class="mb-6">
      <div v-if="eventsLoading && !usageEvents.length" class="flex justify-center py-8">
        <span class="btn-spinner btn-spinner--lg"></span>
      </div>
      <div v-else-if="eventsError && !usageEvents.length" class="text-danger text-center py-4">{{ eventsError }}</div>
      <div v-else class="grid grid-cols-3 gap-4">
        <div class="card hover:translate-y-0 p-4">
          <div class="text-xs text-text-muted mb-1">{{ $t('cursorUsage.totalInputTokens') }}</div>
          <div class="text-lg font-semibold text-text-strong" v-tooltip="formatNumberFull(summaryTotals.inputTokens)">{{ formatTokenUnit(summaryTotals.inputTokens) }}</div>
          <div v-if="summaryTotals.cacheWriteTokens" class="text-[11px] text-text-muted mt-1">{{ $t('cursorUsage.cacheWrite') }}: <span v-tooltip="formatNumberFull(summaryTotals.cacheWriteTokens)">{{ formatTokenUnit(summaryTotals.cacheWriteTokens) }}</span></div>
        </div>
        <div class="card hover:translate-y-0 p-4">
          <div class="text-xs text-text-muted mb-1">{{ $t('cursorUsage.totalOutputTokens') }}</div>
          <div class="text-lg font-semibold text-text-strong" v-tooltip="formatNumberFull(summaryTotals.outputTokens)">{{ formatTokenUnit(summaryTotals.outputTokens) }}</div>
          <div v-if="summaryTotals.cacheReadTokens" class="text-[11px] text-text-muted mt-1">{{ $t('cursorUsage.cacheRead') }}: <span v-tooltip="formatNumberFull(summaryTotals.cacheReadTokens)">{{ formatTokenUnit(summaryTotals.cacheReadTokens) }}</span></div>
        </div>
        <div class="card hover:translate-y-0 p-4">
          <div class="text-xs text-text-muted mb-1 cursor-help" v-tooltip="$t('cursorUsage.totalCostHint')">{{ $t('cursorUsage.totalCost') }}</div>
          <div v-if="planBreakdownLabel" class="text-[11px] text-text-muted mb-1 cursor-help" v-tooltip="$t('cursorUsage.onPlanHint')">{{ $t('cursorUsage.onPlan') }}: {{ planBreakdownLabel }}</div>
          <div class="text-lg font-semibold text-text-strong">${{ formatCents(summaryTotals.chargedCents) }}</div>
          <div v-if="individualUsage?.onDemand?.enabled" class="text-[11px] text-text-muted mt-1 cursor-help" v-tooltip="$t('cursorUsage.onDemandHint')">{{ $t('cursorUsage.onDemand') }}: ${{ formatCents(individualUsage.onDemand.used) }}{{ onDemandLimitLabel }}</div>
          <div v-if="freeCreditCents > 0" class="text-[11px] text-success mt-1 cursor-help" v-tooltip="$t('cursorUsage.freeCreditDeductedHint')">{{ $t('cursorUsage.freeCreditDeducted') }}: ${{ formatCents(freeCreditCents) }}</div>
        </div>
      </div>
      <div v-if="showPlanQuota || totalRemainingPercent !== null || autoRemainingPercent !== null || apiRemainingPercent !== null || showGrokBot" class="mt-3 grid gap-3" :class="quotaGridClass">
        <div v-if="showPlanQuota" class="rounded-md border border-border px-3 py-2">
          <div class="text-[11px] text-text-muted" v-tooltip="$t('platform.cursor.planAvailableHint')">{{ $t('platform.cursor.planAvailable') }}</div>
          <div class="text-sm font-semibold tabular-nums">{{ planRemainingPercent }}%</div>
          <div v-if="planSpend" class="text-[11px] text-text-muted tabular-nums">{{ planSpend }}</div>
        </div>
        <div v-if="totalRemainingPercent !== null" class="rounded-md border border-border px-3 py-2">
          <div class="text-[11px] text-text-muted" v-tooltip="$t('platform.cursor.totalAvailableHint')">{{ $t('platform.cursor.totalAvailable') }}</div>
          <div class="text-sm font-semibold tabular-nums">{{ totalRemainingPercent }}%</div>
          <div v-if="totalSpend" class="text-[11px] text-text-muted tabular-nums" v-tooltip="$t('platform.cursor.poolLimitEstimated')">{{ totalSpend }}</div>
        </div>
        <div v-if="autoRemainingPercent !== null" class="rounded-md border border-border px-3 py-2">
          <div class="text-[11px] text-text-muted" v-tooltip="$t('platform.cursor.autoAvailableHint')">{{ $t('platform.cursor.autoAvailable') }}</div>
          <div class="text-sm font-semibold tabular-nums">{{ autoRemainingPercent }}%</div>
          <div v-if="autoSpend" class="text-[11px] text-text-muted tabular-nums" v-tooltip="$t('platform.cursor.poolLimitEstimated')">{{ autoSpend }}</div>
        </div>
        <div v-if="apiRemainingPercent !== null" class="rounded-md border border-border px-3 py-2">
          <div class="text-[11px] text-text-muted" v-tooltip="$t('platform.cursor.apiAvailableHint')">{{ $t('platform.cursor.apiAvailable') }}</div>
          <div class="text-sm font-semibold tabular-nums">{{ apiRemainingPercent }}%</div>
          <div v-if="apiSpend" class="text-[11px] text-text-muted tabular-nums" v-tooltip="$t('platform.cursor.poolLimitEstimated')">{{ apiSpend }}</div>
        </div>
        <div v-if="showGrokBot" class="rounded-md border border-border px-3 py-2">
          <div class="text-[11px] text-text-muted" v-tooltip="$t('platform.cursor.grokBotAvailableHint')">{{ $t('platform.cursor.grokBotAvailable') }}</div>
          <div class="text-sm font-semibold tabular-nums">{{ grokBotRemaining }}%</div>
        </div>
      </div>
    </div>

    <!-- Tab 切换 + 模型筛选 -->
    <div class="border-b border-border mb-4 flex items-end justify-between gap-3">
      <div class="flex gap-4">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          :class="[
            'px-4 py-2 text-sm font-medium border-b-2 transition-colors',
            activeTab === tab.key
              ? 'text-accent border-accent'
              : 'text-text-muted border-transparent hover:text-text-strong'
          ]"
          @click="activeTab = tab.key"
        >
          {{ tab.label }}
        </button>
      </div>
      <div class="flex items-center gap-2 pb-1.5">
        <span v-if="!eventsLoading && !eventsError" class="text-[11px] text-text-muted whitespace-nowrap">
          {{ $t('cursorUsage.eventsCount', { count: filteredTotals.count }) }}
        </span>
        <select
          v-model="selectedModel"
          class="input !w-auto max-w-[220px] h-7 !px-2 !py-0.5 text-[12px]"
          :disabled="eventsLoading || !modelOptions.length"
        >
          <option value="">{{ $t('cursorUsage.allModels') }}</option>
          <option v-for="model in modelOptions" :key="model" :value="model">{{ model }}</option>
        </select>
      </div>
    </div>

    <!-- 图表区域 -->
    <div v-if="isChartView" class="max-h-[calc(65vh-120px)] overflow-y-auto">
      <div v-if="eventsLoading" class="flex justify-center py-8">
        <span class="btn-spinner btn-spinner--lg"></span>
      </div>
      <div v-else-if="eventsError" class="text-danger text-center py-4">{{ eventsError }}</div>
      <CursorUsageCharts v-else :usage-events="filteredEvents" :granularity="chartGranularity" />
    </div>

    <!-- 使用事件表格 -->
    <template v-else>
      <div v-if="!eventsLoading && (fetchCapped || displayTruncated)" class="text-[11px] text-text-muted mb-2">
        {{ truncatedHint }}
      </div>
      <div class="overflow-x-auto max-h-[400px] overflow-y-auto">
        <div v-if="eventsLoading" class="flex justify-center py-8">
          <span class="btn-spinner btn-spinner--lg"></span>
        </div>
        <div v-else-if="eventsError" class="text-danger text-center py-4">{{ eventsError }}</div>
        <div v-else-if="!filteredEvents.length" class="text-text-muted text-center py-8">
          {{ $t('cursorUsage.noEvents') }}
        </div>
        <table v-else class="w-full text-sm border-separate border-spacing-0">
          <thead>
            <tr>
              <th class="sticky top-0 z-10 bg-surface text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-4 border-b border-border">{{ $t('cursorUsage.timestamp') }}</th>
              <th class="sticky top-0 z-10 bg-surface text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-4 border-b border-border">{{ $t('cursorUsage.model') }}</th>
              <th class="sticky top-0 z-10 bg-surface text-right text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-4 border-b border-border">{{ $t('cursorUsage.inputTokens') }}</th>
              <th class="sticky top-0 z-10 bg-surface text-right text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-4 border-b border-border">{{ $t('cursorUsage.outputTokens') }}</th>
              <th class="sticky top-0 z-10 bg-surface text-right text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-4 border-b border-border">{{ $t('cursorUsage.cost') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(event, index) in displayEvents" :key="index" class="hover:bg-hover">
              <td class="py-3 px-4 border-b border-border text-text-strong">{{ formatTimestamp(event.timestamp) }}</td>
              <td class="py-3 px-4 border-b border-border text-text-strong">{{ event.model }}</td>
              <td class="py-3 px-4 border-b border-border text-text-strong text-right">
                <span v-tooltip="formatNumberFull(event.tokenUsage?.inputTokens) + (toNum(event.tokenUsage?.cacheWriteTokens) ? `\n${$t('cursorUsage.cacheWrite')}: ${formatNumberFull(event.tokenUsage?.cacheWriteTokens)}` : '')">
                  {{ formatTokenUnit(event.tokenUsage?.inputTokens) }}
                </span>
              </td>
              <td class="py-3 px-4 border-b border-border text-text-strong text-right">
                <span v-tooltip="formatNumberFull(event.tokenUsage?.outputTokens) + (toNum(event.tokenUsage?.cacheReadTokens) ? `\n${$t('cursorUsage.cacheRead')}: ${formatNumberFull(event.tokenUsage?.cacheReadTokens)}` : '')">
                  {{ formatTokenUnit(event.tokenUsage?.outputTokens) }}
                </span>
              </td>
              <td class="py-3 px-4 border-b border-border text-text-strong text-right whitespace-nowrap">
                <template v-if="event.tokenUsage?.totalCents">
                  <span v-tooltip="eventCostTooltip(event)">${{ (event.tokenUsage.totalCents / 100).toFixed(4) }}</span>
                  <span v-if="isFreeCreditEvent(event)" class="ml-1 inline-block px-1 py-px rounded bg-success/15 text-success text-[10px] leading-4 align-middle">{{ $t('cursorUsage.freeCredit') }}</span>
                </template>
                <template v-else>-</template>
              </td>
            </tr>
          </tbody>
          <tfoot>
            <tr class="font-semibold">
              <td class="sticky bottom-0 z-10 bg-muted py-2.5 px-4 border-t border-border text-text-strong">{{ $t('cursorUsage.totalWithCount', { count: filteredTotals.count }) }}</td>
              <td class="sticky bottom-0 z-10 bg-muted py-2.5 px-4 border-t border-border text-text-strong">
                {{ selectedModel || $t('cursorUsage.modelsCount', { count: filteredTotals.models }) }}
              </td>
              <td class="sticky bottom-0 z-10 bg-muted py-2.5 px-4 border-t border-border text-text-strong text-right">
                <span v-tooltip="formatNumberFull(filteredTotals.inputTokens) + (filteredTotals.cacheWriteTokens ? `\n${$t('cursorUsage.cacheWrite')}: ${formatNumberFull(filteredTotals.cacheWriteTokens)}` : '')">
                  {{ formatTokenUnit(filteredTotals.inputTokens) }}
                </span>
              </td>
              <td class="sticky bottom-0 z-10 bg-muted py-2.5 px-4 border-t border-border text-text-strong text-right">
                <span v-tooltip="formatNumberFull(filteredTotals.outputTokens) + (filteredTotals.cacheReadTokens ? `\n${$t('cursorUsage.cacheRead')}: ${formatNumberFull(filteredTotals.cacheReadTokens)}` : '')">
                  {{ formatTokenUnit(filteredTotals.outputTokens) }}
                </span>
              </td>
              <td class="sticky bottom-0 z-10 bg-muted py-2.5 px-4 border-t border-border text-text-strong text-right whitespace-nowrap">
                <div class="cursor-help" v-tooltip="$t('cursorUsage.totalsCostTooltip')">
                  <div>{{ totalCostLabel }}</div>
                  <div v-if="totalsChargedDiffers" class="text-[10px] font-normal text-text-muted">{{ $t('cursorUsage.chargedLabel') }} {{ totalChargedLabel }}</div>
                </div>
              </td>
            </tr>
          </tfoot>
        </table>
      </div>
    </template>
  </BaseModal>
</template>

<script setup>
import { ref, computed, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import BaseModal from '../common/BaseModal.vue'
import CursorUsageCharts from './CursorUsageCharts.vue'
import {
  applyCursorUsageSummary,
  isCursorSessionExpiredError,
  markCursorSessionInvalid
} from '../../utils/cursorUsage'
import { useCursorQuota } from '../../composables/useCursorQuota'

const { t: $t } = useI18n()

const props = defineProps({
  account: {
    type: Object,
    required: true
  }
})

const emit = defineEmits(['close', 'account-synced'])

// 状态
const eventsLoading = ref(false)
const eventsError = ref(null)
const usageEvents = ref([])
const eventsTotalCount = ref(0)
const activeTab = ref('details')

// 时间范围（rangeStartTs/rangeEndTs 为 null 表示查询全部历史数据）
const rangePreset = ref('today')
const startDateStr = ref('')
const endDateStr = ref('')
const rangeStartTs = ref(null)
const rangeEndTs = ref(null)
const selectedModel = ref('')

// 竞态保护：只应用最后一次请求的结果
let eventsSeq = 0

const PAGE_SIZE = 1000
const MAX_PAGES = 10
const MAX_DISPLAY_ROWS = 1000
const FREE_CREDIT_KIND = 'USAGE_EVENT_KIND_FREE_CREDIT'

const tabs = computed(() => [
  { key: 'details', label: $t('cursorUsage.tabDetails') },
  { key: 'charts', label: $t('cursorUsage.tabCharts') }
])

const sessionToken = computed(() => props.account.token?.workos_cursor_session_token)
const individualUsage = computed(() => props.account.individual_usage)
const {
  planRemainingPercent,
  planSpend,
  showPlanQuota,
  totalRemainingPercent,
  autoRemainingPercent,
  apiRemainingPercent,
  grokBotRemaining,
  showGrokBot,
  autoSpend,
  apiSpend,
  totalSpend
} = useCursorQuota(() => props.account)
const QUOTA_GRID_CLASSES = ['grid-cols-1', 'grid-cols-1', 'grid-cols-2', 'grid-cols-3', 'grid-cols-4', 'grid-cols-5']
const quotaGridClass = computed(() => {
  const count = [
    showPlanQuota.value,
    totalRemainingPercent.value !== null,
    autoRemainingPercent.value !== null,
    apiRemainingPercent.value !== null,
    showGrokBot.value
  ].filter(Boolean).length
  return QUOTA_GRID_CLASSES[count]
})
const isChartView = computed(() => activeTab.value === 'charts')

const billingCycle = computed(() => {
  const usage = individualUsage.value
  if (!usage?.billingCycleStart || !usage?.billingCycleEnd) return null
  const start = new Date(usage.billingCycleStart).getTime()
  const end = new Date(usage.billingCycleEnd).getTime()
  if (isNaN(start) || isNaN(end)) return null
  return { start, end }
})

const rangePresets = computed(() => {
  const presets = [
    { key: 'today', label: $t('cursorUsage.rangeToday') },
    { key: 'yesterday', label: $t('cursorUsage.rangeYesterday') },
    { key: 'thisWeek', label: $t('cursorUsage.rangeThisWeek') },
    { key: 'thisMonth', label: $t('cursorUsage.rangeThisMonth') },
    { key: 'last7', label: $t('cursorUsage.rangeLast7Days') },
    { key: 'last30', label: $t('cursorUsage.rangeLast30Days') }
  ]
  if (billingCycle.value) {
    presets.push({ key: 'billing', label: $t('cursorUsage.billingCycle') })
  }
  presets.push({ key: 'all', label: $t('cursorUsage.rangeAll') })
  return presets
})

const formatCents = (value) => {
  if (!value && value !== 0) return '0.00'
  return (value / 100).toFixed(2)
}

// 明细项缺失（团队/企业账号）时整项略过，不能渲染成 $0.00
const planBreakdownLabel = computed(() => {
  const breakdown = individualUsage.value?.plan?.breakdown
  if (!breakdown) return ''
  return [
    ['included', $t('cursorUsage.included')],
    ['bonus', $t('cursorUsage.bonus')]
  ]
    .filter(([key]) => typeof breakdown[key] === 'number')
    .map(([key, label]) => `${label}: $${formatCents(breakdown[key])}`)
    .join(' ')
})

// limit 为空表示未设置 On-Demand 上限，不能渲染成 $0.00
const onDemandLimitLabel = computed(() => {
  const limit = individualUsage.value?.onDemand?.limit
  return limit === null || limit === undefined ? '' : ` / $${formatCents(limit)}`
})

// --- 日期工具（全部按本地时区处理） ---

const toDateStr = (date) => {
  const y = date.getFullYear()
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

const parseDateStr = (str) => {
  const [y, m, d] = str.split('-').map(Number)
  return new Date(y, m - 1, d)
}

const startOfDayTs = (str) => parseDateStr(str).getTime()

const endOfDayTs = (str) => {
  const date = parseDateStr(str)
  date.setHours(23, 59, 59, 999)
  return date.getTime()
}

const presetDates = (key) => {
  const today = new Date()
  const start = new Date(today)
  switch (key) {
    case 'yesterday':
      start.setDate(start.getDate() - 1)
      return { start, end: new Date(start) }
    case 'thisWeek':
      // 周一为一周起点
      start.setDate(start.getDate() - (today.getDay() + 6) % 7)
      break
    case 'thisMonth':
      start.setDate(1)
      break
    case 'last7':
      start.setDate(start.getDate() - 6)
      break
    case 'last30':
      start.setDate(start.getDate() - 29)
      break
  }
  return { start, end: today }
}

const applyPreset = (key) => {
  rangePreset.value = key
  if (key === 'all') {
    startDateStr.value = ''
    endDateStr.value = ''
    rangeStartTs.value = null
    rangeEndTs.value = null
  } else if (key === 'billing') {
    const cycle = billingCycle.value
    if (!cycle) return
    // 账期用精确的账单周期时刻查询，输入框仅展示对应日期
    startDateStr.value = toDateStr(new Date(cycle.start))
    endDateStr.value = toDateStr(new Date(cycle.end))
    rangeStartTs.value = cycle.start
    rangeEndTs.value = cycle.end
  } else {
    const { start, end } = presetDates(key)
    startDateStr.value = toDateStr(start)
    endDateStr.value = toDateStr(end)
    rangeStartTs.value = startOfDayTs(startDateStr.value)
    rangeEndTs.value = endOfDayTs(endDateStr.value)
  }
  refetchRangeData()
}

const onDateInputChange = () => {
  if (!startDateStr.value && !endDateStr.value) return
  // 只填一端时自动补全另一端
  if (!startDateStr.value) startDateStr.value = endDateStr.value
  if (!endDateStr.value) {
    const todayStr = toDateStr(new Date())
    endDateStr.value = todayStr < startDateStr.value ? startDateStr.value : todayStr
  }
  // 起止倒置时自动交换
  if (startDateStr.value > endDateStr.value) {
    ;[startDateStr.value, endDateStr.value] = [endDateStr.value, startDateStr.value]
  }
  rangePreset.value = 'custom'
  rangeStartTs.value = startOfDayTs(startDateStr.value)
  rangeEndTs.value = endOfDayTs(endDateStr.value)
  refetchRangeData()
}

// --- 格式化 ---

const toNum = (value) => {
  if (!value) return 0
  return parseInt(value) || 0
}

const formatTokenUnit = (value) => {
  const n = toNum(value)
  if (n === 0) return '0'
  if (n >= 1e9) return (n / 1e9).toFixed(2) + 'B'
  if (n >= 1e6) return (n / 1e6).toFixed(2) + 'M'
  if (n >= 1e3) return (n / 1e3).toFixed(1) + 'K'
  return n.toString()
}

const formatNumberFull = (value) => {
  const n = toNum(value)
  return n.toLocaleString()
}

const formatTimestamp = (timestamp) => {
  if (!timestamp) return '-'
  // 处理毫秒时间戳（数字或字符串）和 ISO 字符串
  let date
  if (typeof timestamp === 'number') {
    date = new Date(timestamp)
  } else if (typeof timestamp === 'string') {
    // 尝试作为数字解析（毫秒时间戳字符串）
    const num = Number(timestamp)
    if (!isNaN(num) && num > 1000000000000) {
      date = new Date(num)
    } else {
      // 尝试作为 ISO 字符串解析
      date = new Date(timestamp)
    }
  } else {
    return '-'
  }
  return isNaN(date.getTime()) ? '-' : date.toLocaleString()
}

// --- 数据获取 ---

// 获取使用事件（范围内分页拉全，汇总卡片/明细/图表共用同一份数据）。
// 注：不再调用聚合接口 —— 其费用口径（不含按需已计费部分）与明细对不上、
// 模型归因与明细不一致，且跨后端分片边界的时间范围会直接报 400/500。
const fetchEvents = async () => {
  if (!sessionToken.value) {
    eventsError.value = 'No session token'
    return
  }

  const seq = ++eventsSeq
  eventsLoading.value = true
  eventsError.value = null

  try {
    let events = []
    let totalCount = 0
    // 全部模式（无日期）与范围模式统一分页拉取：接口不传分页时默认只回 100 条
    const baseParams = {
      sessionToken: sessionToken.value,
      teamId: 0,
      pageSize: PAGE_SIZE
    }
    if (rangeStartTs.value !== null) {
      baseParams.startDate = String(rangeStartTs.value)
      baseParams.endDate = String(rangeEndTs.value)
    }
    let page = 1
    while (page <= MAX_PAGES) {
      const result = await invoke('cursor_get_filtered_usage_events', { ...baseParams, page })
      if (seq !== eventsSeq) return
      const chunk = result?.usageEventsDisplay || []
      events = events.concat(chunk)
      totalCount = result?.totalUsageEventsCount || events.length
      // 服务端可能按更小的页大小截断，只要还没拿满就继续翻页
      if (!chunk.length || events.length >= totalCount) break
      page++
    }
    if (seq !== eventsSeq) return
    usageEvents.value = events
    eventsTotalCount.value = Math.max(totalCount, events.length)
  } catch (e) {
    if (seq !== eventsSeq) return
    eventsError.value = e.toString()
    console.error('Failed to fetch usage events:', e)
  } finally {
    if (seq === eventsSeq) eventsLoading.value = false
  }
}

const refetchRangeData = () => {
  fetchEvents()
}

// --- 模型筛选与合计 ---

const modelOptions = computed(() => {
  const models = new Set()
  for (const event of usageEvents.value) {
    models.add(event.model || 'Unknown')
  }
  return [...models].sort((a, b) => a.localeCompare(b))
})

// 切换时间范围后所选模型可能不再存在，自动回退到全部
watch(modelOptions, (options) => {
  if (selectedModel.value && !options.includes(selectedModel.value)) {
    selectedModel.value = ''
  }
})

const filteredEvents = computed(() => {
  if (!selectedModel.value) return usageEvents.value
  return usageEvents.value.filter(e => (e.model || 'Unknown') === selectedModel.value)
})

const displayEvents = computed(() => filteredEvents.value.slice(0, MAX_DISPLAY_ROWS))

const isFreeCreditEvent = (event) => event.kind === FREE_CREDIT_KIND

// 实际计费美分：赠送额度与未计费请求为 0
const eventChargedCents = (event) => {
  if (typeof event.chargedCents === 'number') return event.chargedCents
  // 后端未透传 chargedCents 时的退化处理
  if (isFreeCreditEvent(event)) return 0
  return event.tokenUsage?.totalCents || 0
}

const eventCostTooltip = (event) => {
  if (isFreeCreditEvent(event)) return $t('cursorUsage.freeCreditTooltip')
  const total = event.tokenUsage?.totalCents || 0
  const charged = eventChargedCents(event)
  if (Math.abs(charged - total) >= 0.5) {
    return $t('cursorUsage.partialChargeTooltip', { charged: (charged / 100).toFixed(4) })
  }
  return ''
}

const filteredTotals = computed(() => {
  const totals = {
    count: filteredEvents.value.length,
    models: 0,
    inputTokens: 0,
    outputTokens: 0,
    cacheWriteTokens: 0,
    cacheReadTokens: 0,
    costCents: 0,
    chargedCents: 0
  }
  const models = new Set()
  for (const event of filteredEvents.value) {
    models.add(event.model || 'Unknown')
    totals.chargedCents += eventChargedCents(event)
    const usage = event.tokenUsage
    if (!usage) continue
    totals.inputTokens += usage.inputTokens || 0
    totals.outputTokens += usage.outputTokens || 0
    totals.cacheWriteTokens += usage.cacheWriteTokens || 0
    totals.cacheReadTokens += usage.cacheReadTokens || 0
    totals.costCents += usage.totalCents || 0
  }
  totals.models = models.size
  return totals
})

const formatCostDollars = (cents) => {
  const dollars = cents / 100
  return '$' + (Math.abs(dollars) >= 1 ? dollars.toFixed(2) : dollars.toFixed(4))
}

const totalCostLabel = computed(() => formatCostDollars(filteredTotals.value.costCents))
const totalChargedLabel = computed(() => formatCostDollars(filteredTotals.value.chargedCents))
const totalsChargedDiffers = computed(() =>
  Math.abs(filteredTotals.value.costCents - filteredTotals.value.chargedCents) >= 0.5
)

// 当前范围内赠送额度抵扣的用量（按 API 标价），来自全部事件，不受模型筛选影响
const freeCreditCents = computed(() => {
  let sum = 0
  for (const event of usageEvents.value) {
    if (isFreeCreditEvent(event)) sum += event.tokenUsage?.totalCents || 0
  }
  return sum
})

// 汇总卡片：由当前范围内全部事件（不受模型筛选影响）推导，费用为实际计费口径
const summaryTotals = computed(() => {
  const totals = { inputTokens: 0, outputTokens: 0, cacheWriteTokens: 0, cacheReadTokens: 0, chargedCents: 0 }
  for (const event of usageEvents.value) {
    totals.chargedCents += eventChargedCents(event)
    const usage = event.tokenUsage
    if (!usage) continue
    totals.inputTokens += usage.inputTokens || 0
    totals.outputTokens += usage.outputTokens || 0
    totals.cacheWriteTokens += usage.cacheWriteTokens || 0
    totals.cacheReadTokens += usage.cacheReadTokens || 0
  }
  return totals
})

const fetchCapped = computed(() => eventsTotalCount.value > usageEvents.value.length)
const displayTruncated = computed(() => filteredEvents.value.length > displayEvents.value.length)

const truncatedHint = computed(() => {
  if (fetchCapped.value) {
    return $t('cursorUsage.eventsFetchCapped', {
      loaded: usageEvents.value.length,
      total: eventsTotalCount.value
    })
  }
  return $t('cursorUsage.eventsTruncated', {
    shown: displayEvents.value.length,
    total: filteredEvents.value.length
  })
})

// 范围不超过 2 天时图表按小时聚合
const chartGranularity = computed(() => {
  if (rangeStartTs.value === null || rangeEndTs.value === null) return 'day'
  return rangeEndTs.value - rangeStartTs.value <= 2 * 24 * 60 * 60 * 1000 ? 'hour' : 'day'
})

// --- 摘要刷新 ---

// 刷新用量摘要（更新账期和配额）
const fetchUsageSummary = async () => {
  if (!sessionToken.value) return
  try {
    const summary = await invoke('cursor_get_usage_summary', {
      sessionToken: sessionToken.value,
      accessToken: props.account.token?.access_token || null
    })
    applyCursorUsageSummary(props.account, summary)
    await invoke('cursor_update_account', { account: props.account })
    emit('account-synced', props.account.id)
  } catch (e) {
    // 失败时不写回摘要，账号原有 membership_type 保持不变
    console.error('Failed to fetch usage summary:', e)
    if (isCursorSessionExpiredError(e)) {
      if (markCursorSessionInvalid(props.account)) {
        try {
          await invoke('cursor_update_account', { account: props.account })
          emit('account-synced', props.account.id)
        } catch (saveError) {
          console.error('Failed to persist session invalid flag:', saveError)
        }
      }
      window.$notify?.error($t('platform.cursor.messages.sessionExpired'))
    } else {
      window.$notify?.error($t('platform.cursor.messages.refreshFailed', { error: e?.message || e }))
    }
  }
}

const refresh = async () => {
  if (eventsLoading.value) return
  await fetchUsageSummary()
  refetchRangeData()
}

const handleClose = () => {
  emit('close')
}

onMounted(() => {
  applyPreset(rangePreset.value)
})
</script>
