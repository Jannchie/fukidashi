import { ref, watch } from 'vue'

/** 阅读偏好：按人记住 */
export interface ReaderPrefs {
  /** 最新一句的字号（px），其余层级按比例缩放 */
  fontSize: number
  ruby: boolean
  /** 有译文时是否显示 */
  translation: boolean
}

export const FONT_MIN = 14
export const FONT_MAX = 40
export const FONT_DEFAULT = 22
const KEY = 'fukidashi.reader'
/** 改名前的键，第一次启动时读过来 */
const OLD_KEY = 'yomu.reader'

function load(): ReaderPrefs {
  const base: ReaderPrefs = { fontSize: FONT_DEFAULT, ruby: true, translation: true }
  try {
    return { ...base, ...JSON.parse(localStorage.getItem(KEY) ?? localStorage.getItem(OLD_KEY) ?? '{}') }
  } catch {
    return base
  }
}

const prefs = ref<ReaderPrefs>(load())

watch(
  prefs,
  (p) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(p))
    } catch {}
  },
  { deep: true },
)

function setFont(px: number) {
  prefs.value.fontSize = Math.min(FONT_MAX, Math.max(FONT_MIN, Math.round(px)))
}

export function useReader() {
  return {
    prefs,
    bigger: () => setFont(prefs.value.fontSize + 2),
    smaller: () => setFont(prefs.value.fontSize - 2),
    resetFont: () => setFont(FONT_DEFAULT),
    setFont,
  }
}
