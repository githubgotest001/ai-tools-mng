import { computed } from 'vue'
import {
  billingCycleRange,
  getQuotaBarClass,
  getQuotaTextClass,
  grokBotRemainingPercent,
  planPoolSpend,
  planRemainingPercentFromCents,
  planSpendLabel,
  remainingPercentFromUsed,
  spendLabel
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

  // 分池金额：已用来自聚合事件按 tier 汇总，池总额靠百分比反推。
  // Grok Bot 走的是另一套接口，官方只给百分比，没有任何金额可显示。
  const poolSpend = computed(() => planPoolSpend(plan.value))

  const autoSpend = computed(() => spendLabel(poolSpend.value?.auto))
  const apiSpend = computed(() => spendLabel(poolSpend.value?.api))
  const totalSpend = computed(() => spendLabel(poolSpend.value?.total))

  const poolSpendLabels = computed(() => ({
    auto: autoSpend.value,
    api: apiSpend.value,
    total: totalSpend.value
  }))

  // 卡片/表格只展示 Auto / API / Bot 三条分池进度条（口径为剩余可用），
  // 套餐与 Total 口径留给用量详情弹窗；没有数据的池不渲染。
  const quotaBars = computed(() => {
    const items = []
    if (autoRemainingPercent.value !== null) {
      items.push({ key: 'auto', percent: autoRemainingPercent.value, spend: autoSpend.value })
    }
    if (apiRemainingPercent.value !== null) {
      items.push({ key: 'api', percent: apiRemainingPercent.value, spend: apiSpend.value })
    }
    if (grokBotRemaining.value !== null) {
      items.push({ key: 'grokBot', percent: grokBotRemaining.value, spend: '' })
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
    autoSpend,
    apiSpend,
    totalSpend,
    poolSpendLabels,
    quotaBars,
    billingCycle,
    getQuotaBarClass,
    getQuotaTextClass
  }
}
