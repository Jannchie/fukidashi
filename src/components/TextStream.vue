<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import type { Sentence, Translation } from '../composables/useFukidashi'
import type { ReaderPrefs } from '../composables/useReader'
import { JIcon } from '@jannchie/icons-vue'
import { IconArrowDownToLine } from '@jannchie/icons'
import SentenceEntry from './SentenceEntry.vue'

const props = defineProps<{
  sentences: Sentence[]
  translations: Record<number, Translation>
  prefs: ReaderPrefs
  connected: boolean
  /** 还在观察各路来源、没下结论。此时不推正文，免得先闪一串垃圾再纠正。 */
  warmingUp: boolean
  /** 库里还有更早的记录 */
  hasOlder: boolean
  loadOlder: () => Promise<void>
}>()

const scroller = ref<HTMLElement | null>(null)
const content = ref<HTMLElement | null>(null)
/** 用户往回翻的时候不要把他拽回底部 */
const stickToBottom = ref(true)
let lastTop = 0

// 只有用户往上翻才算离开底部；内容变高不触发 scroll，不会误判
function onScroll() {
  const el = scroller.value
  if (!el) return
  const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 48
  if (atBottom) stickToBottom.value = true
  else if (el.scrollTop < lastTop) stickToBottom.value = false
  lastTop = el.scrollTop
  if (el.scrollTop < 400 && props.hasOlder) void older()
}

/** 接上更早的一页，并保持眼前这一句不动：内容从上面插进来，滚动位置要同步下移 */
let loading = false
async function older() {
  const el = scroller.value
  if (!el || loading) return
  loading = true
  const before = el.scrollHeight
  try {
    await props.loadOlder()
    await nextTick()
    el.scrollTop += el.scrollHeight - before
    lastTop = el.scrollTop
  } finally {
    loading = false
  }
}

/** 两句隔了这么久就算另一次游玩，中间插一条时间 */
const SESSION_GAP = 30 * 60 * 1000
const fmt = new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' })
const dividers = computed(() =>
  props.sentences.map((s, i) => {
    const prev = props.sentences[i - 1]
    return prev && s.at - prev.at < SESSION_GAP ? null : fmt.format(s.at)
  }),
)

function toBottom() {
  const el = scroller.value
  if (!el) return
  stopFollow()
  el.scrollTop = el.scrollHeight
  lastTop = el.scrollTop
}

// 内容变高时平滑地滚到底：每帧朝当前的底部逼近一段（指数缓动）。
// 目标每帧重新取，流式译文边长边追也不会顿挫；用户一往上翻，onScroll 取消贴底，这里随即停下。
const EASE_MS = 90
const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)')
let frame = 0
let guard = 0
function stopFollow() {
  cancelAnimationFrame(frame)
  clearTimeout(guard)
  frame = 0
}
function follow() {
  const el = scroller.value
  if (!el || frame) return
  const gap = el.scrollHeight - el.clientHeight - el.scrollTop
  // 一次进来一大屏（连上游戏时载入的记录）没必要演一遍，直接到底
  if (reduceMotion.matches || gap > el.clientHeight * 2) return toBottom()
  if (gap <= 0) return
  let pos = el.scrollTop
  let prev = performance.now()
  const step = (now: number) => {
    frame = 0
    if (!stickToBottom.value) return stopFollow()
    const target = el.scrollHeight - el.clientHeight
    const left = target - pos
    if (left <= 0.5) return toBottom()
    pos += left * (1 - Math.exp(-(now - prev) / EASE_MS))
    prev = now
    el.scrollTop = pos
    lastTop = el.scrollTop
    frame = requestAnimationFrame(step)
  }
  frame = requestAnimationFrame(step)
  // 窗口被遮住时 WebView 不跑动画帧，过一会儿还没到底就直接跳过去
  guard = window.setTimeout(() => {
    if (frame && stickToBottom.value) toBottom()
  }, 1000)
}

// 新句子、晚到的译文、调字号……内容变高时平滑贴底；滚动区本身尺寸变了（拖窗口）直接贴底。
// ResizeObserver 在布局后、绘制前回调，不会先闪一帧再跳
let ro: ResizeObserver | undefined
onMounted(() => {
  ro = new ResizeObserver((entries) => {
    if (!stickToBottom.value) return
    if (entries.some(e => e.target === scroller.value)) toBottom()
    else follow()
  })
  if (scroller.value) ro.observe(scroller.value)
  if (content.value) ro.observe(content.value)
})
onUnmounted(() => {
  ro?.disconnect()
  stopFollow()
})

// 窗口被完全遮住时 WebView 会暂停渲染，ResizeObserver 不回调；按数据变化再贴一次底兜住
watch(
  () => [props.sentences.length, props.translations],
  () => {
    if (stickToBottom.value) follow()
  },
  { flush: 'post' },
)

function jumpToLatest() {
  stickToBottom.value = true
  follow()
}
</script>

<template>
  <div class="stream">
    <div ref="scroller" class="scroller" :style="{ '--fs': `${prefs.fontSize}px` }" @scroll.passive="onScroll">
      <div ref="content">
        <p v-if="hasOlder" class="older">往上翻加载更早的记录…</p>
        <template v-for="(s, i) in sentences" :key="s.id">
          <p v-if="dividers[i]" class="when"><span>{{ dividers[i] }}</span></p>
          <SentenceEntry :s="s" :tr="translations[s.id]" :latest="i === sentences.length - 1"
                         :ruby="prefs.ruby" :show-tr="prefs.translation" />
        </template>
      </div>

      <div v-if="!sentences.length && connected" class="idle">
        <span class="pulse" aria-hidden="true"></span>
        <p v-if="warmingUp">
          正在识别文本来源……<br />
          <span class="hint">先多看几句再下结论，免得选错</span>
        </p>
        <p v-else>
          正在等待文本……<br />
          <span class="hint">在游戏里推进一句对话</span>
        </p>
      </div>
    </div>

    <button v-if="!stickToBottom && sentences.length" class="jump" data-tip="回到最新" data-tip-pos="top"
            aria-label="回到最新"
            @click="jumpToLatest">
      <JIcon :icon="IconArrowDownToLine" />
    </button>
  </div>
</template>

<style scoped>
.stream {
  position: relative;
  flex: 1;
  min-height: 0;
}

.scroller {
  height: 100%;
  overflow-y: auto;
  /* 滚动位置全由脚本管（贴底、往上翻接旧记录时保持眼前这句不动），关掉浏览器自带的锚定免得补偿两次 */
  overflow-anchor: none;
  padding: 20px 26px 28px;
  /* 正文可以选中复制 —— 查词、贴给别人问都需要 */
  user-select: text;
  cursor: text;
}

/* 每次游玩开头的时间：细线中间一个小标签，不抢正文 */
.when {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 6px 0 calc(var(--fs) * 0.8);
  font-family: var(--f-mono);
  font-size: 11px;
  color: var(--muted);
  user-select: none;
}
.when::before,
.when::after {
  content: '';
  flex: 1;
  border-top: 1px solid var(--line-soft);
}
.older {
  margin: 0 0 16px;
  font-size: 12px;
  color: var(--muted);
  text-align: center;
  user-select: none;
}

.idle {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  height: 100%;
  color: var(--muted);
  text-align: center;
  user-select: none;
}

.idle p {
  margin: 0;
  line-height: 1.9;
}

.hint {
  font-size: 12px;
  opacity: 0.75;
}

.pulse {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--accent);
  animation: breathe 1.9s ease-in-out infinite;
}

@keyframes breathe {
  0%, 100% { opacity: 0.25; transform: scale(0.85); }
  50%      { opacity: 1;    transform: scale(1); }
}

.jump {
  position: absolute;
  right: 22px;
  bottom: 16px;
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  padding: 0;
  border-radius: 50%;
  color: var(--ink);
  background: var(--surface-2);
  border-color: var(--line);
  box-shadow: var(--shadow);
}

.jump:hover {
  color: var(--ink);
  background: var(--surface-2);
  border-color: var(--muted);
}
.jump::after {
  right: 0;
}
</style>
