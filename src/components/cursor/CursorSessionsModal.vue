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

    <div class="max-h-[60vh] overflow-y-auto">
      <div v-if="loading && !sessions.length" class="flex justify-center py-10">
        <span class="btn-spinner btn-spinner--lg"></span>
      </div>

      <div v-else-if="error" class="text-danger text-center py-8 px-4 break-words">
        {{ error }}
      </div>

      <div v-else-if="!sessions.length" class="text-text-muted text-center py-10">
        {{ $t('cursorSessions.empty') }}
      </div>

      <table v-else class="w-full text-sm">
        <thead>
          <tr>
            <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.device') }}</th>
            <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.type') }}</th>
            <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.ip') }}</th>
            <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.location') }}</th>
            <th class="text-left text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.lastActive') }}</th>
            <th class="text-right text-xs font-medium text-text-muted uppercase tracking-wider py-3 px-3 border-b border-border">{{ $t('cursorSessions.columns.actions') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="session in sessions" :key="session.id" class="hover:bg-hover align-top">
            <td class="py-3 px-3 border-b border-border text-text-strong">
              <div class="flex items-center gap-1.5 flex-wrap">
                <span class="break-all">{{ session.device || $t('cursorSessions.unknownDevice') }}</span>
                <span v-if="session.isCurrent" class="badge badge--sm badge--success-tech">
                  {{ $t('cursorSessions.currentBadge') }}
                </span>
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
            <td class="py-3 px-3 border-b border-border text-text-strong">{{ typeLabel(session.sessionType) }}</td>
            <td class="py-3 px-3 border-b border-border text-text-strong tabular-nums">{{ session.ipAddress || '-' }}</td>
            <td class="py-3 px-3 border-b border-border text-text-strong">{{ session.location || '-' }}</td>
            <td class="py-3 px-3 border-b border-border text-text-strong tabular-nums">{{ formatTime(session.lastActiveAt) }}</td>
            <td class="py-3 px-3 border-b border-border text-right">
              <button
                class="btn btn--danger btn--sm"
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

    <template #footer>
      <span class="text-[11px] text-text-muted">{{ $t('cursorSessions.apiNote') }}</span>
    </template>
  </BaseModal>
</template>

<script setup>
import { computed, onMounted, ref } from 'vue'
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

const formatTime = (value) => {
  if (!value) return '-'
  const numeric = Number(value)
  // 毫秒时间戳可能以字符串形式返回，秒级的要补成毫秒
  const date = Number.isFinite(numeric) && numeric > 0
    ? new Date(numeric > 1e12 ? numeric : numeric * 1000)
    : new Date(value)
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString()
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
  // 踢掉当前会话会连带作废本工具存的 Session Token，单独提示
  const confirmed = await window.$confirm?.({
    title: session.isCurrent
      ? $t('cursorSessions.revokeCurrentConfirmTitle')
      : $t('cursorSessions.revokeConfirmTitle'),
    message: session.isCurrent
      ? $t('cursorSessions.revokeCurrentConfirmMessage', { device })
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
