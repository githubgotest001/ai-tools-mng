import test from 'node:test'
import assert from 'node:assert/strict'

import {
  billingCycleEndTime,
  compareByBillingCycleEnd,
  compareByMembership,
  matchesMembershipTypes,
  membershipRank,
  membershipTypeKey
} from './cursorSort.js'

const account = (email, membership, billingCycleEnd) => ({
  email,
  membership_type: membership,
  individual_usage: billingCycleEnd ? { billingCycleEnd } : undefined
})

const emailsSorted = (accounts, compare, order) =>
  [...accounts].sort((a, b) => compare(a, b, order)).map((a) => a.email)

test('计划等级权重覆盖官方写法的空格与大小写差异', () => {
  assert.equal(membershipRank('free'), 0)
  assert.equal(membershipRank('Pro'), 2)
  assert.equal(membershipRank('pro plus'), membershipRank('pro_plus'))
  assert.ok(membershipRank('ultra') > membershipRank('pro plus'))
  assert.equal(membershipRank(null), 0)
  assert.equal(membershipRank('galaxy-brain'), null)
})

test('计划筛选键沿用列表展示的原始小写文案，缺失按 free 计', () => {
  assert.equal(membershipTypeKey({ membership_type: 'Pro Plus' }), 'pro plus')
  assert.equal(membershipTypeKey({ membership_type: '  ULTRA ' }), 'ultra')
  assert.equal(membershipTypeKey({}), 'free')
})

test('按订阅计划排序：升序低等级在前，未知计划恒定垫底', () => {
  const accounts = [
    account('c@x.com', 'ultra'),
    account('a@x.com', 'pro'),
    account('d@x.com', 'galaxy-brain'),
    account('b@x.com', 'free')
  ]
  assert.deepEqual(emailsSorted(accounts, compareByMembership, 'asc'), [
    'b@x.com',
    'a@x.com',
    'c@x.com',
    'd@x.com'
  ])
  assert.deepEqual(emailsSorted(accounts, compareByMembership, 'desc'), [
    'c@x.com',
    'a@x.com',
    'b@x.com',
    'd@x.com'
  ])
})

test('同一计划内按邮箱字典序，方向不影响次键', () => {
  const accounts = [account('b@x.com', 'pro'), account('a@x.com', 'pro')]
  assert.deepEqual(emailsSorted(accounts, compareByMembership, 'asc'), ['a@x.com', 'b@x.com'])
  assert.deepEqual(emailsSorted(accounts, compareByMembership, 'desc'), ['a@x.com', 'b@x.com'])
})

test('账期到期时间兼容 snake_case，非法值当作缺失', () => {
  assert.equal(
    billingCycleEndTime({ individual_usage: { billing_cycle_end: '2026-09-02T00:00:00.000Z' } }),
    Date.parse('2026-09-02T00:00:00.000Z')
  )
  assert.equal(billingCycleEndTime({ individual_usage: { billingCycleEnd: 'oops' } }), null)
  assert.equal(billingCycleEndTime({}), null)
})

test('按账期到期排序：升序先到期在前，无账期数据恒定垫底', () => {
  const accounts = [
    account('late@x.com', 'pro', '2026-12-01T00:00:00.000Z'),
    account('none@x.com', 'pro'),
    account('soon@x.com', 'pro', '2026-09-02T00:00:00.000Z')
  ]
  assert.deepEqual(emailsSorted(accounts, compareByBillingCycleEnd, 'asc'), [
    'soon@x.com',
    'late@x.com',
    'none@x.com'
  ])
  assert.deepEqual(emailsSorted(accounts, compareByBillingCycleEnd, 'desc'), [
    'late@x.com',
    'soon@x.com',
    'none@x.com'
  ])
})

test('会员类型多选：未选放行，选中则命中任一即可', () => {
  const pro = account('a@x.com', 'pro')
  const ultra = account('b@x.com', 'ultra')
  const free = account('c@x.com', null)

  assert.equal(matchesMembershipTypes(pro, new Set()), true)
  const selected = new Set(['pro', 'ultra'])
  assert.equal(matchesMembershipTypes(pro, selected), true)
  assert.equal(matchesMembershipTypes(ultra, selected), true)
  assert.equal(matchesMembershipTypes(free, selected), false)
  assert.equal(matchesMembershipTypes(free, new Set(['free'])), true)
})
