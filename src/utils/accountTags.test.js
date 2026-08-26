import test from 'node:test'
import assert from 'node:assert/strict'

import {
  DEFAULT_TAG_COLOR,
  MAX_ACCOUNT_TAGS,
  accountHasAnyTag,
  accountTagNames,
  accountTags,
  collectAccountTags,
  countAccountsByTag,
  countAccountsWithoutTag,
  normalizeTagList,
  setAccountTags
} from './accountTags.js'

test('读取以 tags 为准，缺失时回退旧的 tag / tag_color', () => {
  assert.deepEqual(
    accountTags({ tags: [{ name: 'Work', color: '#111111' }], tag: '旧值', tag_color: '#222222' }),
    [{ name: 'Work', color: '#111111' }]
  )

  // 升级前只有旧字段的存量账号照样要显示出来
  assert.deepEqual(accountTags({ tag: '自用', tag_color: '#3b82f6' }), [
    { name: '自用', color: '#3b82f6' }
  ])

  // 旧字段没颜色时补默认色，别渲染出透明 badge
  assert.deepEqual(accountTags({ tag: '自用' }), [{ name: '自用', color: DEFAULT_TAG_COLOR }])

  assert.deepEqual(accountTags({ tags: [] }), [])
  assert.deepEqual(accountTags(null), [])
})

test('超出上限截断，同名不区分大小写去重', () => {
  const tags = normalizeTagList([
    { name: 'A', color: '#100000' },
    { name: 'a', color: '#200000' },
    { name: 'B' },
    { name: 'C' },
    { name: 'D' }
  ])
  assert.equal(tags.length, MAX_ACCOUNT_TAGS)
  // 同名保留首次出现的颜色，避免筛选器里出现 A / a 两个条目
  assert.deepEqual(tags.map((t) => t.name), ['A', 'B', 'C'])
  assert.equal(tags[0].color, '#100000')
})

test('空白与非法项被丢掉，不会产生空标签', () => {
  assert.deepEqual(normalizeTagList([{ name: '  ' }, null, undefined, { color: '#fff' }]), [])
  assert.deepEqual(normalizeTagList('not-an-array'), [])
  // 顺带支持纯字符串数组，导入的旧数据可能是这个形状
  assert.deepEqual(normalizeTagList(['  x  ']), [{ name: 'x', color: DEFAULT_TAG_COLOR }])
})

test('写回时同步旧字段镜像，供旧版本客户端读取', () => {
  const account = { tag: '旧', tag_color: '#000000', updated_at: 1 }
  setAccountTags(account, [
    { name: 'Team', color: '#abcdef' },
    { name: 'Paid', color: '#123456' }
  ])

  assert.equal(account.tags.length, 2)
  assert.equal(account.tag, 'Team')
  assert.equal(account.tag_color, '#abcdef')
  assert.ok(account.updated_at > 1)
})

test('清空标签时旧字段写成空串而不是留下残值', () => {
  const account = { tags: [{ name: 'x', color: '#fff' }], tag: 'x', tag_color: '#fff' }
  setAccountTags(account, [])

  assert.deepEqual(account.tags, [])
  assert.equal(account.tag, '')
  assert.equal(account.tag_color, '')
})

test('筛选按「含任一标签」命中', () => {
  const account = { tags: [{ name: 'Work' }, { name: 'Paid' }] }

  assert.ok(accountHasAnyTag(account, new Set(['paid'])))
  assert.ok(accountHasAnyTag(account, new Set(['work', 'other'])))
  assert.equal(accountHasAnyTag(account, new Set(['other'])), false)
  assert.equal(accountHasAnyTag(account, new Set()), false)
  assert.deepEqual(accountTagNames(account), ['Work', 'Paid'])
})

test('计数按标签分桶，多标签账号会被重复计入', () => {
  const accounts = [
    { tags: [{ name: 'Work' }, { name: 'Paid' }] },
    { tag: 'Work' },
    { tags: [] },
    {}
  ]

  assert.deepEqual(countAccountsByTag(accounts), { Work: 2, Paid: 1 })
  // 3 个桶里的计数之和（3）大于有标签的账号数（2），这是预期的
  assert.equal(countAccountsWithoutTag(accounts), 2)
  assert.deepEqual(collectAccountTags(accounts).map((t) => t.name), ['Paid', 'Work'])
})
