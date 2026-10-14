<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { JIcon } from '@jannchie/icons-vue'
import { IconMinus, IconPlus } from '@jannchie/icons'
import { FONT_MAX, FONT_MIN, useReader } from '../composables/useReader'
import { useWindowMode } from '../composables/useWindowMode'

const emit = defineEmits<{ close: [] }>()
const { prefs, bigger, smaller, setFont } = useReader()
const { prefs: win } = useWindowMode()

const root = ref<HTMLElement | null>(null)

// 点面板外面或按 Esc 关闭。用 pointerdown：等到 click 时触发按钮自己的 click 会再把它打开。
function onDown(e: PointerEvent) {
  const anchor = root.value?.parentElement
  if (anchor && !anchor.contains(e.target as Node)) emit('close')
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
onMounted(() => {
  window.addEventListener('pointerdown', onDown, true)
  window.addEventListener('keydown', onKey)
})
onUnmounted(() => {
  window.removeEventListener('pointerdown', onDown, true)
  window.removeEventListener('keydown', onKey)
})
</script>

<template>
  <div ref="root" class="menu" role="dialog" aria-label="阅读设置">
    <div class="row">
      <span class="name">字号</span>
      <div class="stepper">
        <button class="ibtn" data-tip="缩小 · Ctrl −" aria-label="缩小字号"
                :disabled="prefs.fontSize <= FONT_MIN" @click="smaller">
          <JIcon :icon="IconMinus" />
        </button>
        <input type="range" :min="FONT_MIN" :max="FONT_MAX" step="2" :value="prefs.fontSize"
               aria-label="字号" @input="setFont(+($event.target as HTMLInputElement).value)" />
        <button class="ibtn" data-tip="放大 · Ctrl +" aria-label="放大字号"
                :disabled="prefs.fontSize >= FONT_MAX" @click="bigger">
          <JIcon :icon="IconPlus" />
        </button>
        <span class="val">{{ prefs.fontSize }}</span>
      </div>
    </div>

    <label class="row switch">
      <span class="name">振假名</span>
      <input v-model="prefs.ruby" type="checkbox" />
    </label>

    <label class="row switch">
      <span class="name">显示译文</span>
      <input v-model="prefs.translation" type="checkbox" />
    </label>

    <div class="row">
      <span class="name">叠加背景</span>
      <div class="stepper">
        <input v-model.number="win.overlayOpacity" type="range" min="0" max="90" step="5" aria-label="叠加模式背景浓度" />
        <span class="val">{{ win.overlayOpacity }}%</span>
      </div>
    </div>

    <p class="preview" :style="{ fontSize: `${prefs.fontSize}px` }">
      <ruby v-if="prefs.ruby">彼女<rt>かのじょ</rt></ruby><template v-else>彼女</template>は静かに笑った。
    </p>
  </div>
</template>

<style scoped>
.menu {
  position: absolute;
  top: calc(100% + 6px);
  right: 8px;
  z-index: 60;
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 268px;
  padding: 10px 12px 12px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 10px;
  box-shadow: var(--shadow);
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-height: 34px;
}

.name {
  font-size: 13px;
  color: var(--ink-soft);
}

.stepper {
  display: flex;
  align-items: center;
  gap: 2px;
}
.stepper input {
  width: 96px;
  accent-color: var(--accent);
}
.val {
  min-width: 24px;
  text-align: right;
  font-family: var(--f-mono);
  font-size: 12px;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}

.switch {
  padding: 0 4px 0 0;
  border-radius: var(--r);
  cursor: pointer;
}
.switch input {
  width: 16px;
  height: 16px;
  margin: 0;
  accent-color: var(--accent);
  cursor: pointer;
}

.preview {
  margin: 6px 0 0;
  padding: 10px 12px;
  font-family: var(--f-jp);
  line-height: 2;
  color: var(--ink);
  background: var(--surface-2);
  border-radius: var(--r);
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.preview rt {
  font-size: 0.5em;
  color: var(--muted);
}
</style>
