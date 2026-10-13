import { ref, watch, type Ref } from 'vue'

/**
 * 存在 localStorage 里的一份设置：读出时补齐缺省字段，改动随即写回。
 * `oldKey` 是改名前的键，新键没有内容时从它读过来。
 */
export function persisted<T extends object>(key: string, oldKey: string, defaults: T): Ref<T> {
  let saved: Partial<T> = {}
  try {
    saved = JSON.parse(localStorage.getItem(key) ?? localStorage.getItem(oldKey) ?? '{}')
  } catch {}
  const value = ref({ ...defaults, ...saved }) as Ref<T>
  watch(
    value,
    (v) => {
      try {
        localStorage.setItem(key, JSON.stringify(v))
      } catch {}
    },
    { deep: true },
  )
  return value
}
