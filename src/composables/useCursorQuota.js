import { computed } from 'vue'
import {
  getQuotaBarClass,
  grokBotRemainingPercent,
  remainingPercentFromUsed
} from '../utils/cursorUsage'

export function useCursorQuota(getAccount) {
  const account = computed(() => getAccount())

  const autoRemainingPercent = computed(() =>
    remainingPercentFromUsed(account.value?.individual_usage?.plan?.autoPercentUsed)
  )

  const apiRemainingPercent = computed(() =>
    remainingPercentFromUsed(account.value?.individual_usage?.plan?.apiPercentUsed)
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

  return {
    autoRemainingPercent,
    apiRemainingPercent,
    grokBotRemaining,
    showGrokBot,
    grokBotResetLabel,
    getQuotaBarClass
  }
}
