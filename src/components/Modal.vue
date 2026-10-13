<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { JIcon } from '@jannchie/icons-vue'
import { IconX } from '@jannchie/icons'

defineProps<{ title: string; width?: number }>()
const emit = defineEmits<{ close: [] }>()

/**
 * 遮罩点击关闭要求「按下和抬起都发生在遮罩上」。
 *
 * 只判断 click 不够：打开弹窗的那次点击，其监听器返回后浏览器会执行微任务检查点，
 * Vue 正是在那时把遮罩插进 DOM —— 这一次点击有可能被算到刚出现的遮罩头上，
 * 弹窗刚显示就被自己关掉。要求按下也在遮罩上就排除了这种情况，
 * 顺带修掉「面板内按下、拖到遮罩上松手」被误判成关闭的问题。
 */
let pressedOnScrim = false

function onScrimDown(e: MouseEvent) {
  pressedOnScrim = e.target === e.currentTarget
}

function onScrimClick(e: MouseEvent) {
  if (pressedOnScrim && e.target === e.currentTarget) emit('close')
  pressedOnScrim = false
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}

onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <div class="scrim" @mousedown="onScrimDown" @click="onScrimClick">
    <div class="modal" role="dialog" :aria-label="title" :style="{ width: `min(${width ?? 560}px, 100%)` }">
      <header>
        <h2>{{ title }}</h2>
        <button class="ibtn" data-tip="关闭 · Esc" aria-label="关闭" @click="emit('close')">
          <JIcon :icon="IconX" />
        </button>
      </header>
      <slot />
      <footer v-if="$slots.footer">
        <slot name="footer" />
      </footer>
    </div>
  </div>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  background: color-mix(in srgb, var(--bg) 70%, transparent);
  backdrop-filter: blur(6px);
  /* 这里必须用 flex 而不是 grid：grid 算行轨道高度时子项的百分比 max-height
     解析不出来（循环依赖），按规范会当成 none，于是轨道被列表的完整内容高度
     撑到几千像素，钳过高度的面板再被居中到轨道正中 —— 直接飞出视口。 */
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  z-index: 50;
}

.modal {
  display: flex;
  flex-direction: column;
  gap: 14px;
  /* 用视口单位而非 100%，高度不依赖父容器解析，杜绝上面那类循环依赖 */
  max-height: min(600px, calc(100vh - 48px));
  min-height: 0;
  overflow-y: auto;
  padding: 18px 20px 16px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 10px;
  box-shadow: var(--shadow);
}

header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  letter-spacing: -0.01em;
}

footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex: none;
}
</style>

<!-- 弹窗里的表单控件：所有弹窗共用，限定在 .modal 之内 -->
<style>
/* 窗口矮时整个弹窗滚动，而不是把各段压扁叠在一起；只有标了 grow 的段（如进程列表）可以收缩 */
.modal > * {
  flex-shrink: 0;
}
.modal > .grow {
  flex-shrink: 1;
}

.modal .field {
  display: flex;
  flex-direction: column;
  gap: 7px;
  min-height: 0;
}

.modal label {
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.03em;
  color: var(--muted);
  text-transform: uppercase;
}

.modal .label-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.modal input:not([type="checkbox"]),
.modal select {
  flex: 1;
  min-width: 0;
  padding: 7px 10px;
  font: inherit;
  color: var(--ink);
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r);
  user-select: text;
}
.modal input:focus,
.modal select:focus {
  outline: none;
  border-color: var(--accent);
}
.modal input::placeholder {
  color: var(--muted);
}

.modal .toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 400;
  letter-spacing: 0;
  text-transform: none;
  color: var(--ink-soft);
  cursor: pointer;
}
.modal .toggle input {
  flex: none;
  margin: 0;
  accent-color: var(--accent);
}

.modal .hint {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--muted);
}

.modal .chip {
  padding: 1px 6px;
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.04em;
  border-radius: 3px;
  flex: none;
}
.modal .chip.ok {
  color: var(--good);
  background: var(--good-soft);
}
.modal .chip.warn {
  color: var(--accent);
  background: var(--accent-soft);
}

.modal .primary {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 7px 16px;
  font-weight: 600;
  color: #fff;
  background: var(--accent);
  border-color: var(--accent);
}
.modal .primary:hover:not(:disabled) {
  filter: brightness(1.08);
}
.modal .primary:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.modal .link {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 2px;
  font-size: 12px;
  color: var(--muted);
  text-decoration: underline;
  text-underline-offset: 3px;
  text-decoration-color: var(--line);
}
.modal .link:hover:not(:disabled) {
  color: var(--accent);
  text-decoration-color: currentColor;
}
.modal .link:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
