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

/** 美分转 `$12.88` */
export function formatCentsAsDollars(cents) {
  const value = toFiniteNumber(cents)
  return value === null ? '' : `$${(value / 100).toFixed(2)}`
}

/** `$12.88 / $20.00`；总额未知时退成 `$12.88` */
export function spendLabel(spend) {
  if (!spend) return ''
  const spent = formatCentsAsDollars(spend.spent)
  if (!spent) return ''
  const limit = formatCentsAsDollars(spend.limit)
  return limit ? `${spent} / ${limit}` : spent
}

/** 套餐额度的金额说明，如 `$12.88 / $20.00` */
export function planSpendLabel(plan) {
  return spendLabel(planIncludedSpend(plan))
}

/**
 * 百分比低于此值就不反推池预算：分母越小，除法把 Cursor 的取整误差放得越大。
 * 实测 10% 已用时反推误差约 0.001%，1% 时仍在可接受范围。
 */
const MIN_PERCENT_FOR_LIMIT_ESTIMATE = 1

/**
 * 由「已用金额 + 已用百分比」反推池预算（美分）。
 *
 * Cursor 不公开各池预算，但 percentUsed = spend / budget 是精确的，
 * 于是 budget = spend / percent。实测两个池分别落在 $450 / $45 这样的整元
 * 数上，所以结果按整元取整，抹掉浮点噪音（45000.24 → 45000）。
 *
 * percentUsed 达到 100 时是被官方截断的（超额仍显示 100），此时反推只会
 * 得到一个下界，必须返回 null 让调用方走别的路子。
 */
function estimateLimitCents(spentCents, percentUsed) {
  const spent = toFiniteNumber(spentCents)
  const percent = toFiniteNumber(percentUsed)
  if (spent === null || percent === null) return null
  if (percent < MIN_PERCENT_FOR_LIMIT_ESTIMATE || percent >= 100) return null
  return Math.round(spent / percent) * 100
}

/**
 * Auto / API / Total 三个用量池的「已用金额 + 池总额」（美分）。
 *
 * 已用金额来自 pool_spend 按 tier 汇总的聚合事件（autoSpendCents /
 * apiSpendCents），池总额靠百分比反推。某个池已经打满 100% 时反推不出总额，
 * 就用 Total 减掉另一个池补上——两池金额之和恰好等于 breakdown.total，
 * 两池预算之和也恰好等于 Total 池预算，实测都严丝合缝。
 *
 * 拿不到金额（缺 autoSpendCents，或团队账号没有 plan）时返回 null，
 * 界面据此只显示百分比，不能凭空造出 $0.00。
 */
export function planPoolSpend(plan) {
  const autoSpent = toFiniteNumber(plan?.autoSpendCents ?? plan?.auto_spend_cents)
  const apiSpent = toFiniteNumber(plan?.apiSpendCents ?? plan?.api_spend_cents)
  if (autoSpent === null && apiSpent === null) return null

  const totalSpent =
    toFiniteNumber(plan?.breakdown?.total) ??
    (autoSpent !== null && apiSpent !== null ? autoSpent + apiSpent : null)

  const totalLimit = estimateLimitCents(totalSpent, plan?.totalPercentUsed)
  let autoLimit = estimateLimitCents(autoSpent, plan?.autoPercentUsed)
  let apiLimit = estimateLimitCents(apiSpent, plan?.apiPercentUsed)

  if (totalLimit !== null) {
    if (autoLimit === null && apiLimit !== null) autoLimit = totalLimit - apiLimit
    else if (apiLimit === null && autoLimit !== null) apiLimit = totalLimit - autoLimit
  }

  const pool = (spent, limit) =>
    spent === null ? null : { spent, limit: limit === null || limit <= 0 ? null : limit }

  return {
    auto: pool(autoSpent, autoLimit),
    api: pool(apiSpent, apiLimit),
    total: pool(totalSpent, totalLimit)
  }
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
