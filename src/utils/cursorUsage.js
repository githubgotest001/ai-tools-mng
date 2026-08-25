/**
 * 判断刷新配额的报错是不是 session 失效（后端 `Session expired (HTTP 401)`）。
 *
 * 这类失败必须提示重新登录，而不是把账号套餐当成 free 写回去。
 */
export function isCursorSessionExpiredError(error) {
  const text = typeof error === 'string' ? error : (error?.message ?? String(error ?? ''))
  return /session expired|http 401|http 403/i.test(text)
}

/** 账号当前是否被标记为 session 失效（以最近一次刷新的 401/403 结论为准） */
export function isCursorSessionInvalid(account) {
  return !!account?.session_invalid_at
}

/**
 * 标记账号 session 失效，返回是否有改动（无改动就不必再写库）。
 * 时间戳同时用于 UI 显示「什么时候确认失效的」。
 */
export function markCursorSessionInvalid(account) {
  if (!account || account.session_invalid_at) return false
  account.session_invalid_at = Math.floor(Date.now() / 1000)
  account.updated_at = account.session_invalid_at
  return true
}

/** 清除 session 失效标记，返回是否有改动 */
export function clearCursorSessionInvalid(account) {
  if (!account?.session_invalid_at) return false
  account.session_invalid_at = null
  account.updated_at = Math.floor(Date.now() / 1000)
  return true
}

/** 将 usage-summary 写回账号，并保留上次已拉到的 Grok Bot 周额度 */
export function applyCursorUsageSummary(account, summary) {
  if (!account || !summary) return account

  // 能拿到摘要说明 session 还有效，顺手清掉上次的失效标记
  clearCursorSessionInvalid(account)

  // 只认非空字符串：摘要缺 membershipType 时保留账号原有套餐，
  // 免得一次异常响应把 Ultra/Pro 抹成别的值
  if (typeof summary.membershipType === 'string' && summary.membershipType.trim()) {
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

function toFiniteNumber(value) {
  if (value === null || value === undefined || value === '') return null
  const num = Number(value)
  return Number.isFinite(num) ? num : null
}

function clampPercent(percent) {
  return Math.min(100, Math.max(0, Math.round(percent)))
}

/**
 * 把官方的「已用百分比」翻成剩余百分比。
 *
 * 用于 autoPercentUsed / apiPercentUsed / totalPercentUsed —— 它们分别是
 * Spending 页 Cursor Models / Other Models / Total 三条进度条，也是实际
 * 限流依据。totalPercentUsed 是两池花费除以两池合计预算，不是另外两个的均值。
 */
export function remainingPercentFromUsed(used) {
  const percentUsed = toFiniteNumber(used)
  if (percentUsed === null) return null
  return clampPercent(100 - percentUsed)
}

/**
 * 解析仪表盘横幅 “You've used X% of your included usage” 的分子分母。
 *
 * 官方说明该横幅用的是 includedSpend / limit，对应 usage-summary 里的
 * breakdown.included / limit，所以优先取 breakdown.included；它缺失时
 * 才退回 remaining、used（used 含 bonus 花费，会略高于横幅）。
 */
function planIncludedSpend(plan) {
  if (!plan) return null
  const limit = toFiniteNumber(plan.limit)
  if (limit === null || limit <= 0) return null

  const included = toFiniteNumber(plan.breakdown?.included)
  if (included !== null) return { spent: included, limit }

  const remaining = toFiniteNumber(plan.remaining)
  if (remaining !== null) return { spent: limit - remaining, limit }

  const used = toFiniteNumber(plan.used)
  if (used !== null) return { spent: used, limit }

  return null
}

/**
 * 套餐（included usage）剩余百分比，对齐仪表盘横幅文案。
 *
 * 这条口径只对应横幅那句话，不对应 Spending 页的进度条：进度条来自
 * autoPercentUsed / apiPercentUsed / totalPercentUsed，分母是两个独立
 * 用量池的预算，而 limit 是套餐月费美分（Pro = 2000），两者量级不同。
 */
export function planRemainingPercentFromCents(plan) {
  const spend = planIncludedSpend(plan)
  if (!spend) return null
  return clampPercent(100 - (spend.spent / spend.limit) * 100)
}

/** 套餐额度的金额说明，如 `$12.88 / $20.00` */
export function planSpendLabel(plan) {
  const spend = planIncludedSpend(plan)
  if (!spend) return ''
  return `$${(spend.spent / 100).toFixed(2)} / $${(spend.limit / 100).toFixed(2)}`
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
    return clampPercent((remaining / limit) * 100)
  }
  return null
}

export function getQuotaBarClass(percent) {
  if (percent === null || percent === undefined) return 'bg-text-muted'
  if (percent < 10) return 'bg-danger'
  if (percent < 30) return 'bg-warning'
  return 'bg-success'
}

/** 摘要行里百分比数字的着色：额度紧张才提示，健康时保持安静 */
export function getQuotaTextClass(percent) {
  if (percent === null || percent === undefined) return 'text-text-muted'
  if (percent < 10) return 'text-danger'
  if (percent < 30) return 'text-warning'
  return 'text-text-muted'
}

function parseIsoDate(value) {
  if (!value) return null
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

/**
 * 套餐账期起止（usage-summary 的 billingCycleStart / billingCycleEnd）。
 *
 * 返回 `{ short, full, hasTime }`：
 * - short：卡片内联的紧凑日期 `08/02 – 09/02`，不带时分以免挤爆一行；
 * - full：tooltip 用的完整格式。官方时间戳带时分（续费的精确时刻），
 *   所以 full 形如 `2026/08/02 14:11 – 2026/09/02 14:11`；起止都落在
 *   本地 0 点视为只有日期的旧数据，省掉 `00:00` 噪音。
 * 任一端缺失或非法时返回 null，避免渲染出残缺区间。
 * 账号数据可能来自 Rust 序列化（camelCase）或旧数据（snake_case），两种都认。
 */
export function billingCycleRange(usage) {
  const start = parseIsoDate(usage?.billingCycleStart || usage?.billing_cycle_start)
  const end = parseIsoDate(usage?.billingCycleEnd || usage?.billing_cycle_end)
  if (!start || !end) return null
  const pad = (n) => String(n).padStart(2, '0')
  const hasTime = [start, end].some((d) => d.getHours() !== 0 || d.getMinutes() !== 0)
  const short = (d) => `${pad(d.getMonth() + 1)}/${pad(d.getDate())}`
  const full = (d) => {
    const date = `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())}`
    return hasTime ? `${date} ${pad(d.getHours())}:${pad(d.getMinutes())}` : date
  }
  return {
    short: `${short(start)} – ${short(end)}`,
    full: `${full(start)} – ${full(end)}`,
    hasTime
  }
}
