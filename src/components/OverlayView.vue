<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { JIcon } from '@jannchie/icons-vue'
import { IconGrip, IconX } from '@jannchie/icons'
import type { Sentence, Translation } from '../composables/useFukidashi'
import type { ReaderPrefs } from '../composables/useReader'
import { useWindowMode } from '../composables/useWindowMode'
import SentenceEntry from './SentenceEntry.vue'

// 叠加模式：盖在游戏的文本框上，只显示最新一句。
// 窗口除了右上角的工具条和右下角的缩放角，鼠标都穿透给游戏（见 src-tauri/src/overlay.rs）。

const props = defineProps<{
  sentences: Sentence[]
  translations: Record<number, Translation>
  prefs: ReaderPrefs
}>()

const { prefs: win, exitOverlay, setHotZones } = useWindowMode()

const latest = computed(() => props.sentences.at(-1))

const tools = ref<HTMLElement | null>(null)
const corner = ref<HTMLElement | null>(null)

function report() {
  setHotZones([tools.value, corner.value])
}
onMounted(() => {
  report()
  window.addEventListener('resize', report)
})
onUnmounted(() => window.removeEventListener('resize', report))

function onMove(e: PointerEvent) {
  if (e.button === 0) getCurrentWindow().startDragging()
}
function onResize(e: PointerEvent) {
  if (e.button === 0) getCurrentWindow().startResizeDragging('SouthEast')
}
</script>

<template>
  <div class="overlay" :style="{ '--scrim': win.overlayOpacity / 100, '--fs': `${prefs.fontSize}px` }">
    <div class="body">
      <SentenceEntry v-if="latest" :s="latest" :tr="translations[latest.id]" :latest="false"
                     :ruby="prefs.ruby" :show-tr="prefs.translation" />
      <p v-else class="empty">等待文本……</p>
    </div>

    <div ref="tools" class="tools">
      <button class="ibtn grip" aria-label="拖动" @pointerdown="onMove">
        <JIcon :icon="IconGrip" />
      </button>
      <button class="ibtn" data-tip="退出叠加模式" aria-label="退出叠加模式" @click="exitOverlay">
        <JIcon :icon="IconX" />
      </button>
    </div>
    <div ref="corner" class="corner" aria-hidden="true" @pointerdown="onResize"></div>
  </div>
</template>

<style scoped>
/* 盖在游戏画面上：不跟主题走，固定深底浅字，再加一层字影，背景调到很淡也读得清 */
.overlay {
  --ink: #F3F5F8;
  --ink-soft: #DCE1E8;
  --muted: #AEB6C2;
  --hover: rgba(255, 255, 255, 0.12);
  --press: rgba(255, 255, 255, 0.18);
  position: relative;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  height: 100%;
  padding: 14px 18px;
  border-radius: 10px;
  background: rgba(14, 16, 20, var(--scrim));
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.75), 0 0 6px rgba(0, 0, 0, 0.35);
  overflow: hidden;
}

.body {
  min-height: 0;
  overflow: hidden;
}
.body :deep(.entry) {
  margin: 0;
}
.body :deep(.line) {
  color: var(--ink-soft);
}
.body :deep(.tr) {
  color: var(--ink);
}

.empty {
  margin: 0;
  color: var(--muted);
  font-size: 13px;
}

/* 工具条平时很淡，免得挡游戏；光标移上去才清楚 */
.tools {
  position: absolute;
  top: 4px;
  right: 4px;
  display: flex;
  gap: 2px;
  padding: 2px;
  border-radius: 8px;
  background: rgba(14, 16, 20, 0.55);
  opacity: 0.35;
  transition: opacity var(--t);
}
.tools:hover {
  opacity: 1;
}
.tools .ibtn {
  color: var(--ink-soft);
}
.grip {
  cursor: grab;
}
.tools [data-tip]::after {
  text-shadow: none;
}

.corner {
  position: absolute;
  right: 0;
  bottom: 0;
  width: 16px;
  height: 16px;
  cursor: nwse-resize;
  background: linear-gradient(135deg, transparent 50%, rgba(255, 255, 255, 0.28) 50%);
  border-bottom-right-radius: 10px;
  opacity: 0.5;
}
.corner:hover {
  opacity: 1;
}
</style>
