import test from 'node:test'
import assert from 'node:assert/strict'

import {
  EXPIRING_THRESHOLD_SECONDS,
  accessTokenState,
  credentialHealth,
  expiryState,
  expiryTextClass,
  hasSessionToken,
  maskEmail,
  remainingLabel,
  runWithConcurrency,
  sessionTokenState
} from './cursorToken.js'

const NOW = 1_800_000_000
const DAY = 24 * 60 * 60

test('过期时间戳翻成三档状态，缺失时为 unknown', () => {
  assert.equal(expiryState(NOW + 30 * DAY, NOW).status, 'valid')
  assert.equal(expiryState(NOW + 3 * DAY, NOW).status, 'expiring')
  assert.equal(expiryState(NOW - 1, NOW).status, 'expired')
  assert.equal(expiryState(NOW, NOW).status, 'expired')

  assert.equal(expiryState(undefined, NOW).status, 'unknown')
  assert.equal(expiryState(null, NOW).status, 'unknown')
  assert.equal(expiryState(0, NOW).status, 'unknown')
  assert.equal(expiryState('abc', NOW).status, 'unknown')
})

test('即将过期阈值边界：正好 7 天算 valid，少一秒算 expiring', () => {
  assert.equal(expiryState(NOW + EXPIRING_THRESHOLD_SECONDS, NOW).status, 'valid')
  assert.equal(expiryState(NOW + EXPIRING_THRESHOLD_SECONDS - 1, NOW).status, 'expiring')
})

test('Session 被标记失效后无论 exp 多远都算 expired', () => {
  const account = {
    session_invalid_at: NOW - 100,
    token: { session_expiry_timestamp: NOW + 30 * DAY }
  }
  assert.equal(sessionTokenState(account, NOW).status, 'expired')

  // 清掉标记后回到按 exp 判断
  account.session_invalid_at = null
  assert.equal(sessionTokenState(account, NOW).status, 'valid')
})

test('Access Token 方式添加的账号没有 session，以 Access 为主凭证', () => {
  const account = { token: { expiry_timestamp: NOW + 40 * DAY } }
  assert.equal(accessTokenState(account, NOW).status, 'valid')
  assert.equal(sessionTokenState(account, NOW).status, 'unknown')

  const health = credentialHealth(account, NOW)
  assert.equal(health.primary, 'access')
  assert.equal(health.status, 'valid')

  account.token.expiry_timestamp = NOW - 1
  assert.equal(credentialHealth(account, NOW).status, 'expired')
  assert.equal(hasSessionToken(account), false)
})

test('持有 Session 的账号只看 Session，Access 过期不算凭证问题', () => {
  const withSession = (accessExp, sessionExp) => credentialHealth(
    {
      token: {
        expiry_timestamp: accessExp,
        session_expiry_timestamp: sessionExp,
        workos_cursor_session_token: 'user::jwt'
      }
    },
    NOW
  )

  assert.equal(withSession(NOW + 40 * DAY, NOW + 40 * DAY).status, 'valid')
  assert.equal(withSession(NOW + 40 * DAY, NOW + 2 * DAY).status, 'expiring')
  // Access 已过期但 Session 还在：可以用 Session 重新换 Access，不报错
  assert.equal(withSession(NOW - 1, NOW + 40 * DAY).status, 'valid')
  assert.equal(withSession(NOW + 2 * DAY, NOW - 1).status, 'expired')
  assert.equal(withSession(NOW + 40 * DAY, NOW + 40 * DAY).primary, 'session')

  // 空白 session token 不算持有
  assert.equal(hasSessionToken({ token: { workos_cursor_session_token: '   ' } }), false)
  assert.equal(credentialHealth({}, NOW).status, 'unknown')
  assert.equal(credentialHealth(null, NOW).status, 'unknown')
})

test('剩余时长只给单一单位的粗粒度文案', () => {
  assert.equal(remainingLabel(12 * DAY + 3600), '12d')
  assert.equal(remainingLabel(DAY), '1d')
  assert.equal(remainingLabel(5 * 3600 + 59), '5h')
  assert.equal(remainingLabel(1800), '<1h')
  assert.equal(remainingLabel(0), '')
  assert.equal(remainingLabel(-5), '')
  assert.equal(remainingLabel(null), '')
})

test('过期状态着色只在异常时出现', () => {
  assert.equal(expiryTextClass('expired'), 'text-danger')
  assert.equal(expiryTextClass('expiring'), 'text-warning')
  assert.equal(expiryTextClass('valid'), '')
  assert.equal(expiryTextClass('unknown'), '')
})

test('邮箱部分遮罩保留首尾字符与域名，不同账号仍可区分', () => {
  assert.equal(maskEmail('john.doe@gmail.com'), 'j******e@gmail.com')
  assert.equal(maskEmail('abc@x.com'), 'a*c@x.com')
  assert.equal(maskEmail('abcd@x.com'), 'a**d@x.com')
  // 超长本地部分最多 6 个星号，避免卡片被撑爆
  assert.equal(maskEmail('averyveryverylongname@example.org'), 'a******e@example.org')
  assert.notEqual(maskEmail('alice@cursor.com'), maskEmail('bob@cursor.com'))
})

test('过短的本地部分全部打星，避免被直接还原', () => {
  assert.equal(maskEmail('ab@x.com'), '**@x.com')
  assert.equal(maskEmail('a@x.com'), '**@x.com')
})

test('非邮箱输入原样返回，空值不抛错', () => {
  assert.equal(maskEmail('not-an-email'), 'not-an-email')
  assert.equal(maskEmail(''), '')
  assert.equal(maskEmail(null), '')
  assert.equal(maskEmail(undefined), '')
})

test('限流并发保持结果顺序，单个失败不影响其它任务', async () => {
  let inFlight = 0
  let peak = 0
  const results = await runWithConcurrency(
    [1, 2, 3, 4, 5, 6],
    async (n) => {
      inFlight++
      peak = Math.max(peak, inFlight)
      await new Promise((resolve) => setTimeout(resolve, 5))
      inFlight--
      if (n === 4) throw new Error('boom')
      return n * 10
    },
    2
  )

  assert.equal(peak, 2)
  assert.deepEqual(
    results.map((r) => (r.status === 'fulfilled' ? r.value : r.reason.message)),
    [10, 20, 30, 'boom', 50, 60]
  )
})

test('限流并发对空列表与非法并发度都能收敛', async () => {
  assert.deepEqual(await runWithConcurrency([], async () => 1), [])
  const results = await runWithConcurrency([1, 2], async (n) => n, 0)
  assert.deepEqual(results.map((r) => r.value), [1, 2])
  const generous = await runWithConcurrency([1], async (n) => n, 99)
  assert.deepEqual(generous.map((r) => r.value), [1])
})
