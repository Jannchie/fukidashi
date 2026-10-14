import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow, PhysicalPosition, PhysicalSize } from '@tauri-apps/api/window'
import { persisted } from './persisted'

type Rect = { x: number, y: number, w: number, h: number }

/** 窗口模式偏好：置顶、叠加模式的背景浓度和位置 */
const prefs = persisted('fukidashi.window', 'fukidashi.window', {
  pinned: false,
  /** 叠加模式背景的不透明度，0–100 */
  overlayOpacity: 55,
  /** 叠加模式的窗口位置大小（物理像素），和普通模式分开记 */
  overlayRect: null as Rect | null,
  /** 进入叠加前的普通窗口位置；叠加中途退出应用时，下次启动靠它还原 */
  normalRect: null as Rect | null,
})

/** 叠加模式不跨启动保留：重开应用总是回到普通窗口 */
const overlay = ref(false)

async function readRect(): Promise<Rect> {
  const win = getCurrentWindow()
  const [p, s] = await Promise.all([win.outerPosition(), win.outerSize()])
  return { x: p.x, y: p.y, w: s.width, h: s.height }
}

async function applyRect(r: Rect) {
  const win = getCurrentWindow()
  await win.setSize(new PhysicalSize(r.w, r.h))
  await win.setPosition(new PhysicalPosition(r.x, r.y))
}

let hot: Array<[number, number, number, number]> = []

/** 前端报告叠加模式下仍要接收鼠标的区域（工具条、缩放角） */
function setHotZones(els: Array<HTMLElement | null | undefined>) {
  hot = els.filter((e): e is HTMLElement => !!e).map((e) => {
    const r = e.getBoundingClientRect()
    return [r.left, r.top, r.width, r.height]
  })
  if (overlay.value) invoke('set_overlay', { on: true, hot }).catch(() => {})
}

async function enterOverlay() {
  if (overlay.value) return
  prefs.value.normalRect = await readRect()
  overlay.value = true
  await getCurrentWindow().setAlwaysOnTop(true)
  if (prefs.value.overlayRect) await applyRect(prefs.value.overlayRect)
  await invoke('set_overlay', { on: true, hot })
}

async function exitOverlay() {
  if (!overlay.value) return
  prefs.value.overlayRect = await readRect()
  await invoke('set_overlay', { on: false, hot: [] })
  overlay.value = false
  if (prefs.value.normalRect) await applyRect(prefs.value.normalRect)
  prefs.value.normalRect = null
  await getCurrentWindow().setAlwaysOnTop(prefs.value.pinned)
}

let started = false
/** 启动时调一次：套用置顶；上次在叠加模式里退出的，把窗口放回普通位置 */
async function init() {
  if (started) return
  started = true
  if (prefs.value.normalRect) {
    await applyRect(prefs.value.normalRect).catch(() => {})
    prefs.value.normalRect = null
  }
  await getCurrentWindow().setAlwaysOnTop(prefs.value.pinned).catch(() => {})
  watch(() => prefs.value.pinned, (on) => {
    if (!overlay.value) getCurrentWindow().setAlwaysOnTop(on).catch(() => {})
  })
}

export function useWindowMode() {
  return {
    prefs,
    overlay,
    init,
    togglePin: () => { prefs.value.pinned = !prefs.value.pinned },
    enterOverlay,
    exitOverlay,
    setHotZones,
  }
}
