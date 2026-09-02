/**
 * Cursor 账号凭证的过期口径与展示辅助。
 *
 * 后端 `TokenData` 上有 `is_expired()` / `is_session_expired()`，但列表从不展示，
 * 用户只能等某次刷新配额返回 401 才看到「Session 失效」。这里把 JWT 里的 exp
 * 提前翻成三档状态，让卡片和表格在过期之前就给出预警。
 */

const DAY_SECONDS = 24 * 60 * 60

/** 剩余不足 7 天视为「即将过期」——Session 通常有效 2 周左右，7 天足够安排换 token */
export const EXPIRING_THRESHOLD_SECONDS = 7 * DAY_SECONDS

/**
 * 把过期时间戳（秒）翻译成状态。
 *
 * 返回 `{ status, remainingSeconds }`：
 * - `unknown`：没有时间戳（Access Token 方式添加的账号没有 session；旧数据没解析出 exp）；
 * - `expired`：已过当前时间；
 * - `expiring`：剩余不足 EXPIRING_THRESHOLD_SECONDS；
 * - `valid`：其余。
 * `nowSeconds` 可注入，便于测试与避免同一帧内多次取时间。
 */
export function expiryState(expiryTimestamp, nowSeconds = Math.floor(Date.now() / 1000)) {
  const expiry = Number(expiryTimestamp)
  if (!Number.isFinite(expiry) || expiry <= 0) {
    return { status: 'unknown', remainingSeconds: null }
  }
  const remainingSeconds = expiry - nowSeconds
  if (remainingSeconds <= 0) {
    return { status: 'expired', remainingSeconds }
  }
  if (remainingSeconds < EXPIRING_THRESHOLD_SECONDS) {
    return { status: 'expiring', remainingSeconds }
  }
  return { status: 'valid', remainingSeconds }
}

/** Access Token 过期状态 */
export function accessTokenState(account, nowSeconds) {
  return expiryState(account?.token?.expiry_timestamp, nowSeconds)
}

/**
 * Session Token 过期状态。
 *
 * 账号被标记 `session_invalid_at`（刷新配额撞 401）时无论 exp 多久都算过期：
 * 服务端已经拒绝了它，本地的 exp 只是个下界。
 */
export function sessionTokenState(account, nowSeconds) {
  if (account?.session_invalid_at) {
    return { status: 'expired', remainingSeconds: 0 }
  }
  return expiryState(account?.token?.session_expiry_timestamp, nowSeconds)
}

/** 账号是否持有 Session Token（决定主凭证是 Session 还是 Access） */
export function hasSessionToken(account) {
  return typeof account?.token?.workos_cursor_session_token === 'string'
    && account.token.workos_cursor_session_token.trim().length > 0
}

/**
 * 账号整体凭证健康度。
 *
 * 有 Session 的账号只看 Session：Access Token 可以随时用 Session 走 PKCE 重新换取
 * （切换账号时后台就会这么做），它过期不构成问题，拿它报「凭证过期」是误报。
 * 只有 Access Token 方式添加的账号（没有 Session）才以 Access 为准。
 *
 * 返回 `{ status, primary, access, session }`，`primary` 标明判定依据是哪一个凭证，
 * 界面据此决定卡片上展示哪一个到期日。
 */
export function credentialHealth(account, nowSeconds) {
  const access = accessTokenState(account, nowSeconds)
  const session = sessionTokenState(account, nowSeconds)
  const primary = hasSessionToken(account) ? 'session' : 'access'
  const status = primary === 'session' ? session.status : access.status
  return { status, primary, access, session }
}

/** 剩余时长的紧凑文案：`3d` / `5h` / `<1h`；已过期或未知返回空串 */
export function remainingLabel(remainingSeconds) {
  if (!Number.isFinite(remainingSeconds) || remainingSeconds <= 0) return ''
  if (remainingSeconds >= DAY_SECONDS) return `${Math.floor(remainingSeconds / DAY_SECONDS)}d`
  if (remainingSeconds >= 3600) return `${Math.floor(remainingSeconds / 3600)}h`
  return '<1h'
}

/** 过期状态对应的文字着色：与配额条一致，健康时保持安静 */
export function expiryTextClass(status) {
  switch (status) {
    case 'expired':
      return 'text-danger'
    case 'expiring':
      return 'text-warning'
    default:
      return ''
  }
}

/**
 * 部分遮罩邮箱，保留首尾字符与域名：`john.doe@gmail.com` → `j******e@gmail.com`。
 *
 * 之前「隐藏真实邮箱」把所有账号都渲染成同一个占位邮箱，截图时根本分不清哪张卡是哪个号。
 * 遮罩后长度与首尾字符仍能区分账号，又不暴露完整地址。
 * 本地部分不足 3 个字符时全部打星，避免 `ab@x.com` 被还原。
 */
export function maskEmail(email) {
  if (typeof email !== 'string' || !email.includes('@')) return email ?? ''
  const at = email.lastIndexOf('@')
  const local = email.slice(0, at)
  const domain = email.slice(at)
  if (local.length <= 2) {
    return `${'*'.repeat(Math.max(local.length, 2))}${domain}`
  }
  const stars = '*'.repeat(Math.min(local.length - 2, 6))
  return `${local[0]}${stars}${local[local.length - 1]}${domain}`
}

/**
 * 限流并发地执行一组异步任务，保持结果顺序。
 *
 * 批量刷新配额时每个账号要打 3 个请求，串行 20 个账号要等半分钟；
 * 全并发又容易撞 Cursor 的 429。默认 3 路并发是实测比较稳的折中。
 * 单个任务的异常不会中断整体，结果形如 `{ status: 'fulfilled', value }` / `{ status: 'rejected', reason }`。
 */
export async function runWithConcurrency(items, worker, concurrency = 3) {
  const list = Array.from(items ?? [])
  const results = new Array(list.length)
  const limit = Math.max(1, Math.min(Number(concurrency) || 1, list.length || 1))
  let cursor = 0

  const run = async () => {
    while (cursor < list.length) {
      const index = cursor++
      try {
        results[index] = { status: 'fulfilled', value: await worker(list[index], index) }
      } catch (reason) {
        results[index] = { status: 'rejected', reason }
      }
    }
  }

  await Promise.all(Array.from({ length: limit }, run))
  return results
}
