<template>
  <BaseModal
    :visible="true"
    :title="$t('cursorSessions.title')"
    :close-on-overlay="true"
    :body-scroll="false"
    modal-class="!max-w-[860px]"
    @close="$emit('close')"
  >
    <template #header>
      <div class="flex items-center gap-3 flex-1 min-w-0">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor" class="text-accent shrink-0">
          <path d="M4 6h16v10H4V6zm0-2c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h5v2h6v-2h5c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2H4z"/>
        </svg>
        <div class="min-w-0">
          <h3 class="modal-title">{{ $t('cursorSessions.title') }}</h3>
          <div class="text-xs text-text-muted truncate">{{ account.email }}</div>
        </div>
      </div>
      <button
        class="btn btn--ghost btn--icon"
        :disabled="loading"
        v-tooltip="$t('cursorSessions.refresh')"
        @click="fetchSessions"
      >
        <svg v-if="!loading" width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
          <path d="M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08c-.82 2.33-3.04 4-5.65 4-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z"/>
        </svg>
        <span v-else class="btn-spinner"></span>
      </button>
    </template>

    <div v-if="loading && !sessions.length" class="flex justify-center py-10">
      <span class="btn-spinner btn-spinner--lg"></span>
    </div>

    <div v-else-if="error" class="text-danger text-center py-8 px-4 break-words">
      {{ error }}
    </div>

    <div v-else-if="!sessions.length" class="text-text-muted text-center py-10">
      {{ $t('cursorSessions.empty') }}
    </div>

    <template v-else>
      <!-- 风险提示：本工具依赖的网页会话被踢会导致 Token 失效 -->
      <div class="mb-3 flex items-start gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/5 px-3 py-2.5">
        <svg class="shrink-0 mt-0.5 text-amber-500" width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
          <path d="M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z"/>
        </svg>
        <p class="m-0 text-xs leading-relaxed text-text-secondary">{{ $t('cursorSessions.securityNote') }}</p>
      </div>

      <!-- 设备类型筛选 -->
      <div class="mb-3 flex flex-wrap items-center gap-1.5">
        <span class="mr-0.5 text-xs text-text-muted">{{ $t('cursorSessions.filterLabel') }}</span>
        <button
          type="button"
          :class="['session-type-chip', typeFilter === 'all' ? 'session-type-chip--active' : '']"
          @click="typeFilter = 'all'"
        >
          {{ $t('cursorSessions.filterAll') }}
          <span class="opacity-70 tabular-nums">{{ sessions.length }}</span>
        </button>
        <button
          v-for="[typeKey, count] in typeCounts"
          :key="typeKey"
          type="button"
          :class="['session-type-chip', typeFilter === typeKey ? 'session-type-chip--active' : '']"
          @click="typeFilter = typeKey"
        >
          {{ chipLabel(typeKey) }}
          <span class="opacity-70 tabular-nums">{{ count }}</span>
        </button>
      </div>

      <div class="max-h-[52vh] overflow-y-auto">
        <div v-if="!filteredSessions.length" class="text-text-muted text-center py-10">
          {{ $t('cursorSessions.emptyFiltered') }}
        </div>

        <table v-else class="w-full text-sm">
          <thead>
            <tr>
              <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.device') }}</th>
              <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.type') }}</th>
              <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.ip') }}</th>
              <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.location') }}</th>
              <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.onlineFor') }}</th>
              <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.lastActive') }}</th>
              <th class="text-right text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.actions') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="session in filteredSessions"
              :key="session.id"
              class="hover:bg-hover align-top"
            >
            <td class="py-3 px-3 border-b border-border text-text-strong">
              <div class="flex items-center gap-1.5 flex-wrap">
                <span class="break-all">{{ session.device || $t('cursorSessions.unknownDevice') }}</span>
              </div>
              <div
                v-if="session.createdAt"
                class="text-[11px] text-text-muted mt-0.5"
              >
                {{ $t('cursorSessions.columns.createdAt') }}: {{ formatTime(session.createdAt) }}
              </div>
              <button
                class="text-[11px] text-text-muted hover:text-accent mt-0.5 transition-colors"
                @click="toggleDetails(session.id)"
              >
                {{ $t('cursorSessions.details') }} {{ expandedId === session.id ? '▴' : '▾' }}
              </button>
              <pre
                v-if="expandedId === session.id"
                class="mt-1 max-w-[280px] overflow-x-auto rounded bg-muted/60 p-2 text-[11px] leading-snug text-text-muted"
              >{{ formatRaw(session.raw) }}</pre>
            </td>
            <td class="py-3 px-3 border-b border-border text-text-strong">
              <span
                v-if="isWebSession(session)"
                class="badge badge--sm badge--warning"
                v-tooltip="$t('cursorSessions.webTypeHint')"
              >
                {{ typeLabel(session.sessionType) }}
              </span>
              <template v-else>{{ typeLabel(session.sessionType) }}</template>
            </td>
            <td class="py-3 px-3 border-b border-border text-text-strong tabular-nums">{{ session.ipAddress || '-' }}</td>
            <td class="py-3 px-3 border-b border-border text-text-strong">{{ session.location || '-' }}</td>
            <td class="py-3 px-3 border-b border-border text-text-strong tabular-nums whitespace-nowrap">
              <span
                v-if="onlineDurationMs(session) !== null"
                v-tooltip="preciseDuration(onlineDurationMs(session))"
                class="cursor-default"
              >
                {{ relativeDuration(onlineDurationMs(session)) }}
              </span>
              <template v-else>-</template>
            </td>
            <td class="py-3 px-3 border-b border-border text-text-strong tabular-nums">{{ formatTime(session.lastActiveAt) }}</td>
            <td class="py-3 px-3 border-b border-border text-right">
              <button
                class="btn btn--danger btn--sm whitespace-nowrap"
                :disabled="revokingId === session.id"
                @click="confirmRevoke(session)"
              >
                <span v-if="revokingId === session.id" class="btn-spinner btn-spinner--xs"></span>
                {{ revokingId === session.id ? $t('cursorSessions.revoking') : $t('cursorSessions.revoke') }}
              </button>
            </td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>

    <template #footer>
      <span class="text-[11px] text-text-muted">{{ $t('cursorSessions.apiNote') }}</span>
    </template>
  </BaseModal>
</template>

<script setup>
import { computed, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import BaseModal from '../common/BaseModal.vue'

const { t: $t } = useI18n()

const props = defineProps({
  account: { type: Object, required: true }
})

defineEmits(['close'])

const loading = ref(false)
const error = ref(null)
const sessions = ref([])
const revokingId = ref(null)
const expandedId = ref(null)
const typeFilter = ref('all')

const sessionToken = computed(() => props.account.token?.workos_cursor_session_token)

const SESSION_TYPE_LABELS = {
  SESSION_TYPE_WEB: 'web',
  SESSION_TYPE_CLIENT: 'client',
  SESSION_TYPE_MOBILE: 'mobile',
  SESSION_TYPE_CHROME_EXTENSION: 'chromeExtension'
}

// 官方前端只认这四种；出现新类型时原样显示，不要吞掉
const typeLabel = (sessionType) => {
  if (!sessionType) return '-'
  const key = SESSION_TYPE_LABELS[sessionType]
  return key ? $t(`cursorSessions.types.${key}`) : sessionType
}

const isWebSession = (session) => session.sessionType === 'SESSION_TYPE_WEB'

// ===== 设备类型筛选 =====
const UNKNOWN_TYPE_KEY = '__unknown__'
const KNOWN_TYPE_ORDER = Object.keys(SESSION_TYPE_LABELS)

const sessionTypeKey = (session) => session.sessionType || UNKNOWN_TYPE_KEY

// [typeKey, count]，已知类型按 web/桌面端/移动端/扩展 排序，未知类型排在最后
const typeCounts = computed(() => {
  const counts = new Map()
  for (const session of sessions.value) {
    const key = sessionTypeKey(session)
    counts.set(key, (counts.get(key) || 0) + 1)
  }
  const rank = (key) => {
    const index = KNOWN_TYPE_ORDER.indexOf(key)
    return index === -1 ? KNOWN_TYPE_ORDER.length : index
  }
  return [...counts.entries()].sort((a, b) => rank(a[0]) - rank(b[0]))
})

const chipLabel = (typeKey) =>
  typeKey === UNKNOWN_TYPE_KEY ? $t('cursorSessions.typeUnknown') : typeLabel(typeKey)

const filteredSessions = computed(() => {
  if (typeFilter.value === 'all') return sessions.value
  return sessions.value.filter((session) => sessionTypeKey(session) === typeFilter.value)
})

// 所选类型的设备被踢空（或刷新后消失）时自动回到「全部」
watch(typeCounts, (counts) => {
  if (typeFilter.value !== 'all' && !counts.some(([key]) => key === typeFilter.value)) {
    typeFilter.value = 'all'
  }
})

// 毫秒时间戳可能以字符串形式返回，秒级的要补成毫秒；解析不了返回 null
const parseTime = (value) => {
  if (!value) return null
  const numeric = Number(value)
  const date = Number.isFinite(numeric) && numeric > 0
    ? new Date(numeric > 1e12 ? numeric : numeric * 1000)
    : new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

const formatTime = (value) => {
  if (!value) return '-'
  const date = parseTime(value)
  return date ? date.toLocaleString() : String(value)
}

// ===== 在线时长（自登录时间起算） =====
const MINUTE_MS = 60 * 1000
const HOUR_MS = 60 * MINUTE_MS
const DAY_MS = 24 * HOUR_MS

// 时钟偏差可能算出轻微负值，钳到 0 当作刚登录
const onlineDurationMs = (session) => {
  const created = parseTime(session.createdAt)
  return created ? Math.max(0, Date.now() - created.getTime()) : null
}

// 列内只展示单一单位的粗粒度时长，保持可扫读
const relativeDuration = (ms) => {
  if (ms < MINUTE_MS) return $t('cursorSessions.duration.lessThanMinute')
  if (ms < HOUR_MS) return $t('cursorSessions.duration.minutes', { n: Math.floor(ms / MINUTE_MS) })
  if (ms < DAY_MS) return $t('cursorSessions.duration.hours', { n: Math.floor(ms / HOUR_MS) })
  return $t('cursorSessions.duration.days', { n: Math.floor(ms / DAY_MS) })
}

// hover 提示给出完整的天/时/分/秒分解
const preciseDuration = (ms) => {
  const days = Math.floor(ms / DAY_MS)
  const hours = Math.floor((ms % DAY_MS) / HOUR_MS)
  const minutes = Math.floor((ms % HOUR_MS) / MINUTE_MS)
  const seconds = Math.floor((ms % MINUTE_MS) / 1000)
  const parts = []
  if (days) parts.push($t('cursorSessions.duration.days', { n: days }))
  if (hours) parts.push($t('cursorSessions.duration.hours', { n: hours }))
  if (minutes) parts.push($t('cursorSessions.duration.minutes', { n: minutes }))
  if (seconds || !parts.length) parts.push($t('cursorSessions.duration.seconds', { n: seconds }))
  return $t('cursorSessions.duration.tooltip', { duration: parts.join(' ') })
}

const formatRaw = (raw) => JSON.stringify(raw ?? {}, null, 2)

const toggleDetails = (id) => {
  expandedId.value = expandedId.value === id ? null : id
}

const fetchSessions = async () => {
  if (loading.value) return
  if (!sessionToken.value) {
    error.value = $t('cursorSessions.noSessionToken')
    return
  }

  loading.value = true
  error.value = null
  try {
    const result = await invoke('cursor_list_sessions', { sessionToken: sessionToken.value })
    sessions.value = result?.sessions || []
  } catch (e) {
    error.value = `${$t('cursorSessions.loadFailed')}: ${e?.message || e}`
    sessions.value = []
  } finally {
    loading.value = false
  }
}

const confirmRevoke = async (session) => {
  const device = session.device || typeLabel(session.sessionType) || $t('cursorSessions.unknownDevice')
  // 本工具的 Session Token 也是网页会话且无法区分是哪一条，踢网页设备可能连带失效，单独提示
  const isWeb = isWebSession(session)
  const confirmed = await window.$confirm?.({
    title: isWeb
      ? $t('cursorSessions.revokeWebConfirmTitle')
      : $t('cursorSessions.revokeConfirmTitle'),
    message: isWeb
      ? $t('cursorSessions.revokeWebConfirmMessage', { device })
      : $t('cursorSessions.revokeConfirmMessage', { device }),
    variant: 'danger'
  })
  if (!confirmed) return

  revokingId.value = session.id
  try {
    await invoke('cursor_revoke_session', {
      sessionToken: sessionToken.value,
      sessionId: session.id,
      sessionType: session.sessionType || null
    })
    window.$notify?.success($t('cursorSessions.revokeSuccess'))
    await fetchSessions()
  } catch (e) {
    window.$notify?.error(`${$t('cursorSessions.revokeFailed')}: ${e?.message || e}`)
  } finally {
    revokingId.value = null
  }
}

onMounted(fetchSessions)
</script>

<style scoped>
.session-type-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.25rem 0.625rem;
  border-radius: 9999px;
  border: 1px solid var(--color-border);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 500;
  line-height: 1.4;
  cursor: pointer;
  transition: background-color 0.15s ease, border-color 0.15s ease, color 0.15s ease;
}

.session-type-chip:hover {
  background: var(--color-hover);
  color: var(--color-text);
}

.session-type-chip--active {
  border-color: var(--color-border-accent-tech);
  background: var(--color-accent-tech);
  color: var(--color-accent);
}
</style>
