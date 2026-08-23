/** 将 usage-summary 写回账号，并保留上次已拉到的 Grok Bot 周额度 */
export function applyCursorUsageSummary(account, summary) {
  if (!account || !summary) return account

  if (summary.membershipType) {
    account.membership_type = summary.membershipType
  }

  const incoming = summary.individualUsage || {}
  const prev = account.individual_usage || {}
  const usage = { ...prev, ...incoming }
  if (summary.billingCycleStart) usage.billingCycleStart = summary.billingCycleStart
  if (summary.billingCycleEnd) usage.billingCycleEnd = summary.billingCycleEnd
  if (!incoming.grokBot && !incoming.grok_bot && (prev.grokBot || prev.grok_bot)) {
    usage.grokBot = prev.grokBot || prev.grok_bot
  }
  account.individual_usage = Object.keys(usage).length > 0 ? usage : account.individual_usage
  account.updated_at = Math.floor(Date.now() / 1000)
  return account
}

export function remainingPercentFromUsed(used) {
  if (used === null || used === undefined || Number.isNaN(Number(used))) return null
  return Math.max(0, Math.round(100 - Number(used)))
}

export function grokBotRemainingPercent(grokBot) {
  if (!grokBot) return null
  const percentUsed = grokBot.percentUsed ?? grokBot.percent_used
  if (percentUsed !== null && percentUsed !== undefined) {
    return remainingPercentFromUsed(percentUsed)
  }
  const used = Number(grokBot.used)
  const limit = Number(grokBot.limit)
  if (Number.isFinite(used) && Number.isFinite(limit) && limit > 0) {
    return remainingPercentFromUsed((used / limit) * 100)
  }
  const remaining = Number(grokBot.remaining)
  if (Number.isFinite(remaining) && Number.isFinite(limit) && limit > 0) {
    return Math.max(0, Math.round((remaining / limit) * 100))
  }
  return null
}

export function getQuotaBarClass(percent) {
  if (percent === null || percent === undefined) return 'bg-text-muted'
  if (percent < 10) return 'bg-danger'
  if (percent < 30) return 'bg-warning'
  return 'bg-success'
}
