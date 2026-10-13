import { persisted } from './persisted'

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

const prefs = persisted<ReaderPrefs>('fukidashi.reader', 'yomu.reader', {
  fontSize: FONT_DEFAULT,
  ruby: true,
  translation: true,
})

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
