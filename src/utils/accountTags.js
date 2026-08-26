/**
 * 账号多标签的统一读写入口（Cursor / OpenAI / Windsurf / Antigravity / Claude）。
 *
 * 存储上有两套字段并存：
 * - `tags`：真正的标签列表 `[{ name, color }]`，最多 MAX_ACCOUNT_TAGS 个；
 * - `tag` / `tag_color`：第一个标签的镜像，写入时始终同步。
 *
 * 镜像不能删：Postgres 同步和 WebDAV 备份会把账号整体丢给别的客户端，
 * 旧版本只认 `tag` / `tag_color`。同理读取时 `tags` 为空要回退到旧字段，
 * 否则升级前打的标签会在界面上凭空消失。
 */

export const MAX_ACCOUNT_TAGS = 3

/**
 * 标签默认色。
 *
 * style.css 里的 `--tag-default` 实际从未定义过，各处都是靠这个字面值兜底的，
 * 改动前先确认没有别的地方还在假设旧值。
 */
export const DEFAULT_TAG_COLOR = '#f97316'

function normalizeTag(raw) {
  if (!raw) return null
  const name = typeof raw === 'string' ? raw.trim() : String(raw.name ?? '').trim()
  if (!name) return null
  const rawColor = typeof raw === 'string' ? '' : raw.color
  const color = typeof rawColor === 'string' && rawColor.trim() ? rawColor.trim() : DEFAULT_TAG_COLOR
  return { name, color }
}

/**
 * 去重 + 截断到上限。
 *
 * 同名标签按不区分大小写判重，保留首次出现的那个（连带它的颜色），
 * 避免「Work」和「work」在筛选器里变成两个条目。
 */
export function normalizeTagList(list) {
  if (!Array.isArray(list)) return []
  const seen = new Set()
  const result = []
  for (const raw of list) {
    const tag = normalizeTag(raw)
    if (!tag) continue
    const key = tag.name.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    result.push(tag)
    if (result.length >= MAX_ACCOUNT_TAGS) break
  }
  return result
}

/**
 * 读出账号的标签列表。
 *
 * `tags` 为空时回退到旧的 `tag` / `tag_color`，所以没经过多标签改造的
 * 存量账号也能正常显示。
 */
export function accountTags(account) {
  if (!account) return []
  const tags = normalizeTagList(account.tags)
  if (tags.length > 0) return tags
  return normalizeTagList([{ name: account.tag, color: account.tag_color }])
}

/** 标签名数组，供筛选、搜索、计数使用 */
export function accountTagNames(account) {
  return accountTags(account).map((tag) => tag.name)
}

/** 账号是否至少命中一个目标标签（目标集合需为小写名） */
export function accountHasAnyTag(account, lowerNames) {
  if (!lowerNames || lowerNames.size === 0) return false
  return accountTags(account).some((tag) => lowerNames.has(tag.name.toLowerCase()))
}

/**
 * 写回标签列表，同时同步旧字段镜像，并返回落库后的结果。
 *
 * 清空标签时旧字段写成空串而不是 undefined：各平台的 update_account 会把
 * 整个账号对象序列化过去，`undefined` 会被 JSON 丢掉，导致旧值残留。
 */
export function setAccountTags(account, list) {
  if (!account) return []
  const tags = normalizeTagList(list)
  account.tags = tags
  account.tag = tags[0]?.name ?? ''
  account.tag_color = tags[0]?.color ?? ''
  account.updated_at = Math.floor(Date.now() / 1000)
  return tags
}

/** 汇总一批账号用过的标签（名字 → 颜色），按名称排序，用于建议列表与筛选器 */
export function collectAccountTags(accounts) {
  const map = new Map()
  for (const account of accounts ?? []) {
    for (const tag of accountTags(account)) {
      const key = tag.name.toLowerCase()
      if (!map.has(key)) map.set(key, tag)
    }
  }
  return Array.from(map.values()).sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'))
}

/**
 * 每个标签下的账号数。
 *
 * 一个账号挂 3 个标签就会在 3 个桶里各计一次，所以各项之和会大于账号总数——
 * 筛选器展示的是「该标签有多少账号」，不是占比。
 */
export function countAccountsByTag(accounts) {
  const counts = {}
  for (const account of accounts ?? []) {
    for (const name of accountTagNames(account)) {
      counts[name] = (counts[name] || 0) + 1
    }
  }
  return counts
}

/** 完全没有标签的账号数 */
export function countAccountsWithoutTag(accounts) {
  return (accounts ?? []).filter((account) => accountTags(account).length === 0).length
}

/** 供 TagEditorModal 做标签建议：把账号伪装成它认识的 token 形状 */
export function accountsAsTagTokens(accounts) {
  return (accounts ?? []).map((account) => ({ tags: accountTags(account) }))
}
