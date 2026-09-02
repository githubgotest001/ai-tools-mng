import { ref, watch } from 'vue'

/**
 * 持久化到 localStorage 的 ref。
 *
 * 平台管理页在返回「平台选择」时会被整体卸载，卡片/列表、每页条数、排序方向这些
 * 偏好每次都退回默认值。这里把它们落到 localStorage，下次挂载直接恢复。
 *
 * @param {string} key localStorage 键名
 * @param {*} defaultValue 默认值；也决定了序列化方式（原始值直接存，对象存 JSON）
 * @param {Object} [options]
 * @param {(value: *) => boolean} [options.validate] 恢复时的校验，不通过则回落默认值，
 *        用于枚举型偏好（如 'card' | 'table'）防止脏数据把界面打空
 * @param {(value: *) => *} [options.serialize] 自定义序列化（如 Set → Array）
 * @param {(raw: *) => *} [options.deserialize] 自定义反序列化
 */
export function usePersistedState(key, defaultValue, options = {}) {
  const { validate, serialize, deserialize } = options

  const read = () => {
    try {
      const raw = localStorage.getItem(key)
      if (raw === null) return defaultValue
      const parsed = JSON.parse(raw)
      const value = deserialize ? deserialize(parsed) : parsed
      if (validate && !validate(value)) return defaultValue
      return value
    } catch {
      return defaultValue
    }
  }

  const state = ref(read())

  watch(
    state,
    (value) => {
      try {
        const payload = serialize ? serialize(value) : value
        localStorage.setItem(key, JSON.stringify(payload))
      } catch (error) {
        console.warn(`Failed to persist ${key}`, error)
      }
    },
    { deep: typeof defaultValue === 'object' && defaultValue !== null }
  )

  return state
}

/** 枚举型偏好的校验器工厂：`oneOf(['card', 'table'])` */
export function oneOf(allowed) {
  const set = new Set(allowed)
  return (value) => set.has(value)
}
