import { getCurrentWindow } from '@tauri-apps/api/window'

// 标题栏拖动窗口、双击最大化。
//
// 不用 Tauri 自带的 data-tauri-drag-region：它在 mousedown 时就发 start_dragging，
// 这条 IPC 是异步的，快速点一下时鼠标已经松开、系统的移动循环才开始，
// 窗口就会粘在光标上，直到再点一次。这里改成按住并移动几像素后才开始拖动。
const THRESHOLD = 4
const INTERACTIVE = 'button, a, input, select, textarea, label, [role], [tabindex], [data-no-drag]'

export function useDragRegion() {
  let start: { x: number, y: number } | null = null

  const draggable = (e: MouseEvent) => !(e.target as Element).closest(INTERACTIVE)

  function onPointerdown(e: PointerEvent) {
    start = e.button === 0 && draggable(e) ? { x: e.screenX, y: e.screenY } : null
  }

  function onPointermove(e: PointerEvent) {
    if (!start) return
    if (!(e.buttons & 1)) {
      start = null
      return
    }
    if (Math.hypot(e.screenX - start.x, e.screenY - start.y) < THRESHOLD) return
    start = null
    getCurrentWindow().startDragging()
  }

  function onPointerup() {
    start = null
  }

  function onDblclick(e: MouseEvent) {
    if (e.button === 0 && draggable(e)) getCurrentWindow().toggleMaximize()
  }

  // 拦住拖出文本选区和光标变成 I 形
  function onMousedown(e: MouseEvent) {
    if (e.button === 0 && draggable(e)) e.preventDefault()
  }

  return { onPointerdown, onPointermove, onPointerup, onDblclick, onMousedown }
}
