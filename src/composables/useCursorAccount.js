import { computed, inject, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { isCursorSessionInvalid } from '../utils/cursorUsage'
import {
  credentialHealth,
  expiryTextClass,
  maskEmail,
  remainingLabel
} from '../utils/cursorToken'

/**
 * Cursor 账号卡片与表格行共用的展示口径和操作。
 *
 * 两个视图之前各自维护一份复制 / 导出 / 生成机器码 / 菜单分发的实现，
 * 已经出现过只改了一边（i18n key 不一致）的情况；抽到这里之后两边只剩模板差异。
 *
 * @param {Object} props 需要 `account`、`showRealEmail`
 * @param {Function} emit 需要能 emit `delete`、`machine-id-generated`
 */
export function useCursorAccount(props, emit) {
  const { t: $t, locale } = useI18n()

  // 由 CursorAccountManager 提供的统一时钟；单独渲染（如测试）时退回本地时间
  const nowSeconds = inject('cursorNowSeconds', ref(Math.floor(Date.now() / 1000)))

  const showUsageModal = ref(false)
  const showSessionsModal = ref(false)
  const isGeneratingMachineId = ref(false)

  const hasSessionToken = computed(() => !!props.account.token?.workos_cursor_session_token)

  const displayEmail = computed(() =>
    props.showRealEmail ? props.account.email : maskEmail(props.account.email)
  )

  // ===== 凭证过期 =====

  const credential = computed(() => credentialHealth(props.account, nowSeconds.value))

  // 上次刷新配额撞 401 的账号已经有专门的「Session 失效」徽章，这里只补事前预警
  const sessionInvalid = computed(() => isCursorSessionInvalid(props.account))

  /** 状态的短文案：已过期 / `45d 后过期`；有效期未知返回空串 */
  const stateLabel = (state) => {
    if (state.status === 'expired') return $t('platform.cursor.credential.expired')
    if (state.status === 'unknown') return ''
    return $t('platform.cursor.credential.expiringIn', {
      remaining: remainingLabel(state.remainingSeconds)
    })
  }

  /**
   * 一条凭证的展示数据。
   * - `date`：完整日期，卡片正文用；`dateTime`：带时分，进 tooltip；
   * - `remaining`：`45d` / `已过期`，紧跟在日期后面；
   * - `textClass`：只在紧张 / 过期时着色。
   */
  const expiryCell = (kind, state, timestamp) => ({
    kind,
    status: state.status,
    date: timestamp ? formatDate(timestamp) : '-',
    dateTime: timestamp ? formatDateTime(timestamp) : '',
    remaining: state.status === 'expired'
      ? $t('platform.cursor.credential.expired')
      : remainingLabel(state.remainingSeconds),
    stateLabel: stateLabel(state),
    textClass: expiryTextClass(state.status)
  })

  const accessExpiry = computed(() =>
    expiryCell('access', credential.value.access, props.account.token?.expiry_timestamp)
  )
  const sessionExpiry = computed(() =>
    expiryCell('session', credential.value.session, props.account.token?.session_expiry_timestamp)
  )

  /**
   * 卡片上只放一条到期日：有 Session 就是 Session（刷配额 / 拉用量都靠它，也是判定依据），
   * Access Token 方式添加的账号才退回 Access。两条都要看的话在 tooltip 里。
   */
  const primaryExpiry = computed(() =>
    credential.value.primary === 'session' ? sessionExpiry.value : accessExpiry.value
  )

  const primaryExpiryLabel = computed(() =>
    $t(`platform.cursor.credential.${credential.value.primary}`)
  )

  const credentialBadge = computed(() => {
    if (sessionInvalid.value) return null
    const { status, primary, access, session } = credential.value
    const state = primary === 'session' ? session : access
    if (status === 'expired') {
      return {
        variant: 'danger',
        text: $t('platform.cursor.credential.expiredBadge', {
          kind: $t(`platform.cursor.credential.${primary}`)
        })
      }
    }
    if (status === 'expiring') {
      return {
        variant: 'warning',
        text: $t('platform.cursor.credential.expiringBadge', {
          remaining: remainingLabel(state.remainingSeconds)
        })
      }
    }
    return null
  })

  /** tooltip 里的一行：`Session：2026/10/26 14:11 · 45d 后过期`；没有该凭证时标「无」 */
  const describeCell = (cell) => {
    const kind = $t(`platform.cursor.credential.${cell.kind}`)
    if (!cell.dateTime) {
      return $t('platform.cursor.credential.tooltipLineShort', {
        kind,
        date: $t('platform.cursor.credential.none')
      })
    }
    if (!cell.stateLabel) {
      return $t('platform.cursor.credential.tooltipLineShort', { kind, date: cell.dateTime })
    }
    return $t('platform.cursor.credential.tooltipLine', {
      kind,
      date: cell.dateTime,
      state: cell.stateLabel
    })
  }

  // 每一行一条凭证，最后接口径说明；.v-tooltip 按 pre-line 渲染，换行会保留
  const credentialTooltip = computed(() =>
    [
      describeCell(sessionExpiry.value),
      describeCell(accessExpiry.value),
      $t('platform.cursor.credential.hint')
    ].join('\n')
  )

  // ===== Session 失效徽章（沿用原有语义） =====

  const sessionInvalidTooltip = computed(() => {
    const hint = $t('platform.cursor.sessionInvalidHint')
    const at = props.account.session_invalid_at
    return at
      ? `${hint} · ${$t('platform.cursor.sessionInvalidAt', { time: formatDateTime(at) })}`
      : hint
  })

  // ===== 套餐徽章 =====

  const getMembershipBadgeClass = (type) => {
    const base = 'badge badge--sm uppercase'
    switch (type?.toLowerCase()) {
      case 'ultra':
        return `${base} bg-gradient-to-r from-rose-400 to-pink-500 text-white border-pink-500/50 shadow-sm shadow-pink-500/30`
      case 'pro':
        return `${base} bg-gradient-to-r from-amber-400 to-amber-500 text-amber-900 border-amber-500/50`
      case 'pro plus':
        return `${base} bg-gradient-to-r from-emerald-400 to-teal-500 text-white border-teal-500/50`
      default:
        return base
    }
  }

  // ===== 日期 =====

  function formatDate(timestamp) {
    if (!timestamp) return '-'
    return new Date(timestamp * 1000).toLocaleDateString(locale.value, {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit'
    })
  }

  function formatDateTime(timestamp) {
    if (!timestamp) return '-'
    return new Date(timestamp * 1000).toLocaleString(locale.value, {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit'
    })
  }

  // ===== 复制 =====

  const copyToClipboard = async (value, successKey, missingKey) => {
    if (!value) {
      window.$notify?.error($t(missingKey))
      return
    }
    try {
      await navigator.clipboard.writeText(value)
      window.$notify?.success($t(successKey))
    } catch {
      window.$notify?.error($t('messages.copyFailed'))
    }
  }

  const copyEmail = async () => {
    try {
      await navigator.clipboard.writeText(props.account.email)
      window.$notify?.success($t('messages.emailNoteCopied'))
    } catch {
      window.$notify?.error($t('messages.copyEmailNoteFailed'))
    }
  }

  const copyAccessToken = () =>
    copyToClipboard(props.account.token?.access_token, 'messages.accessTokenCopied', 'messages.noAccessToken')

  const copySessionToken = () =>
    copyToClipboard(
      props.account.token?.workos_cursor_session_token,
      'messages.sessionTokenCopied',
      'messages.noSessionToken'
    )

  // ===== 机器码 / 导出 =====

  const generateAndBindMachineId = async () => {
    if (isGeneratingMachineId.value) return
    isGeneratingMachineId.value = true
    try {
      const result = await invoke('cursor_generate_and_bind_machine_id', { accountId: props.account.id })
      window.$notify?.success(result.message || $t('platform.cursor.messages.machineIdGenerated'))
      emit('machine-id-generated', props.account.id)
    } catch (err) {
      console.error('Generate machine ID error:', err)
      window.$notify?.error(err?.message || err || $t('platform.cursor.messages.machineIdGenerateFailed'))
    } finally {
      isGeneratingMachineId.value = false
    }
  }

  const exportAccount = async () => {
    try {
      const { save } = await import('@tauri-apps/plugin-dialog')
      const { writeTextFile } = await import('@tauri-apps/plugin-fs')

      const jsonData = await invoke('cursor_export_accounts', { accountIds: [props.account.id] })
      const defaultFileName = `cursor_account_${props.account.email.replace(/[^a-zA-Z0-9]/g, '_')}.json`
      const filePath = await save({
        defaultPath: defaultFileName,
        filters: [{ name: 'JSON', extensions: ['json'] }]
      })
      if (!filePath) return

      await writeTextFile(filePath, jsonData)
      window.$notify?.success($t('platform.cursor.messages.exportSuccess'))
    } catch (err) {
      console.error('Export account error:', err)
      if (err?.message?.includes('Cancelled') || err?.code === 'Cancelled') return
      window.$notify?.error(err?.message || err || $t('platform.cursor.messages.exportFailed'))
    }
  }

  // ===== 菜单分发 =====

  const handleMenuClick = async (type, close) => {
    close?.()
    switch (type) {
      case 'copyAccessToken':
        await copyAccessToken()
        break
      case 'copySessionToken':
        await copySessionToken()
        break
      case 'activeSessions':
        showSessionsModal.value = true
        break
      case 'generateMachineId':
        await generateAndBindMachineId()
        break
      case 'export':
        await exportAccount()
        break
      case 'delete':
        emit('delete', props.account.id)
        break
    }
  }

  return {
    showUsageModal,
    showSessionsModal,
    isGeneratingMachineId,
    hasSessionToken,
    displayEmail,
    credential,
    credentialBadge,
    credentialTooltip,
    accessExpiry,
    sessionExpiry,
    primaryExpiry,
    primaryExpiryLabel,
    sessionInvalid,
    sessionInvalidTooltip,
    getMembershipBadgeClass,
    formatDate,
    formatDateTime,
    copyEmail,
    copyAccessToken,
    copySessionToken,
    generateAndBindMachineId,
    exportAccount,
    handleMenuClick
  }
}
