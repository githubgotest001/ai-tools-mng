/**
 * Cursor 账号列表的排序 / 筛选口径。
 *
 * 单独抽出来是因为「订阅计划」和「账期到期」两条规则都有空值与别名的边界，
 * 放在组件里既读不清也测不到。
 */

/**
 * 订阅计划等级权重，数值越大等级越高。
 *
 * 官方 membershipType 的写法并不统一（`pro plus` / `pro_plus` / `Pro Plus` 都出现过），
 * 所以先按 normalizeMembershipType 归一再查表；查不到的计划视为「未知」，
 * 排序时无论升降序都垫底，避免新计划名混进中间位置造成误解。
 */
const MEMBERSHIP_RANKS = {
  free: 0,
  free_trial: 1,
  trial: 1,
  pro: 2,
  pro_student: 3,
  student: 3,
  pro_plus: 4,
  ultra: 5,
  team: 6,
  enterprise: 7
}

/** 归一化计划名：小写、去空白、空格与连字符统一成下划线 */
export function normalizeMembershipType(type) {
  if (!type) return ''
  return String(type).trim().toLowerCase().replace(/[\s-]+/g, '_')
}

/**
 * 筛选与统计用的计划键：保持列表上显示的原样（小写），缺失按 free 计。
 * 与徽标展示的原始文案一致，所以不做下划线归一。
 */
export function membershipTypeKey(account) {
  const raw = account?.membership_type
  const key = raw ? String(raw).trim().toLowerCase() : ''
  return key || 'free'
}

/** 计划等级权重，未知计划返回 null */
export function membershipRank(type) {
  const key = normalizeMembershipType(type) || 'free'
  return key in MEMBERSHIP_RANKS ? MEMBERSHIP_RANKS[key] : null
}

/** 账期到期时间戳（毫秒），兼容 camelCase 与 snake_case；缺失或非法返回 null */
export function billingCycleEndTime(account) {
  const usage = account?.individual_usage
  const raw = usage?.billingCycleEnd || usage?.billing_cycle_end
  if (!raw) return null
  const time = new Date(raw).getTime()
  return Number.isNaN(time) ? null : time
}

/** 同级时的稳定次键：邮箱字典序，与排序方向无关，保证同计划/同账期内次序不跳动 */
function compareByEmail(a, b) {
  return (a?.email || '').localeCompare(b?.email || '')
}

/**
 * 按订阅计划排序：升序 = 低等级在前（free → ultra），降序反之。
 * 未知计划恒定排最后，同等级按邮箱字典序。
 */
export function compareByMembership(a, b, order = 'desc') {
  const rankA = membershipRank(a?.membership_type)
  const rankB = membershipRank(b?.membership_type)
  if (rankA === null || rankB === null) {
    if (rankA !== rankB) return rankA === null ? 1 : -1
  } else if (rankA !== rankB) {
    return order === 'desc' ? rankB - rankA : rankA - rankB
  }
  return compareByEmail(a, b)
}

/**
 * 按账期到期时间排序：升序 = 先到期的在前，降序 = 后到期的在前。
 * 没有账期数据的账号恒定排最后，账期相同按邮箱字典序。
 */
export function compareByBillingCycleEnd(a, b, order = 'desc') {
  const endA = billingCycleEndTime(a)
  const endB = billingCycleEndTime(b)
  if (endA === null || endB === null) {
    if (endA !== endB) return endA === null ? 1 : -1
  } else if (endA !== endB) {
    return order === 'desc' ? endB - endA : endA - endB
  }
  return compareByEmail(a, b)
}

/** 会员类型多选筛选：未选（等价于「全部」）放行，否则命中任一选中项即可 */
export function matchesMembershipTypes(account, selectedTypes) {
  if (!selectedTypes || selectedTypes.size === 0) return true
  return selectedTypes.has(membershipTypeKey(account))
}
