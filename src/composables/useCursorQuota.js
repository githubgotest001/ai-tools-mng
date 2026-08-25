import { computed } from 'vue'
import {
  billingCycleRange,
  getQuotaBarClass,
  getQuotaTextClass,
  grokBotRemainingPercent,
  planRemainingPercentFromCents,
  planSpendLabel,
  remainingPercentFromUsed
} from '../utils/cursorUsage'

export function useCursorQuota(getAccount) {
  const account = computed(() => getAccount())

  const plan = computed(() => account.value?.individual_usage?.plan || null)

  const planRemainingPercent = computed(() => planRemainingPercentFromCents(plan.value))

  const planSpend = computed(() => planSpendLabel(plan.value))

  const showPlanQuota = computed(() => planRemainingPercent.value !== null)

  const totalRemainingPercent = computed(() =>
    remainingPercentFromUsed(plan.value?.totalPercentUsed)
  )

  const autoRemainingPercent = computed(() =>
    remainingPercentFromUsed(plan.value?.autoPercentUsed)
  )

  const apiRemainingPercent = computed(() =>
    remainingPercentFromUsed(plan.value?.apiPercentUsed)
  )

  const grokBot = computed(() => {
    const usage = account.value?.individual_usage
    return usage?.grokBot || usage?.grok_bot || null
  })

  const grokBotRemaining = computed(() => grokBotRemainingPercent(grokBot.value))

  const showGrokBot = computed(() => grokBotRemaining.value !== null)

  const grokBotResetLabel = computed(() => {
    const end = grokBot.value?.periodEnd || grokBot.value?.period_end
    if (!end) return ''
    const date = new Date(end)
    if (Number.isNaN(date.getTime())) return ''
    return date.toLocaleDateString()
  })

  // 卡片/表格只展示一条主进度条：优先套餐口径（对应仪表盘横幅），
  // 团队/企业账号没有套餐美分字段时退回 Total 池口径。
  const primaryQuota = computed(() => {
    if (planRemainingPercent.value !== null) {
      return { key: 'plan', percent: planRemainingPercent.value }
    }
    if (totalRemainingPercent.value !== null) {
      return { key: 'total', percent: totalRemainingPercent.value }
    }
    return null
  })

  // 其余口径压成一行文字摘要；主条已占用的口径不再重复出现
  const secondaryQuotas = computed(() => {
    const items = []
    if (primaryQuota.value?.key !== 'total' && totalRemainingPercent.value !== null) {
      items.push({ key: 'total', percent: totalRemainingPercent.value })
    }
    if (autoRemainingPercent.value !== null) {
      items.push({ key: 'auto', percent: autoRemainingPercent.value })
    }
    if (apiRemainingPercent.value !== null) {
      items.push({ key: 'api', percent: apiRemainingPercent.value })
    }
    if (grokBotRemaining.value !== null) {
      items.push({ key: 'grokBot', percent: grokBotRemaining.value })
    }
    return items
  })

  const billingCycle = computed(() =>
    billingCycleRange(account.value?.individual_usage)
  )

  return {
    planRemainingPercent,
    planSpend,
    showPlanQuota,
    totalRemainingPercent,
    autoRemainingPercent,
    apiRemainingPercent,
    grokBotRemaining,
    showGrokBot,
    grokBotResetLabel,
    primaryQuota,
    secondaryQuotas,
    billingCycle,
    getQuotaBarClass,
    getQuotaTextClass
  }
}
