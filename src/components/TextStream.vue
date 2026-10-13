<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import type { Sentence, Translation } from '../composables/useFukidashi'
import type { ReaderPrefs } from '../composables/useReader'
import { JIcon } from '@jannchie/icons-vue'
import { IconArrowDownToLine } from '@jannchie/icons'

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
function divider(i: number): string | null {
  const s = props.sentences[i]
  const prev = props.sentences[i - 1]
  if (prev && s.at - prev.at < SESSION_GAP) return null
  return fmt.format(s.at)
}

function toBottom() {
  const el = scroller.value
  if (!el) return
  el.scrollTop = el.scrollHeight
  lastTop = el.scrollTop
}

// 新句子、晚到的译文、调字号、窗口变窄换行……一切让内容变高的情况都在这里统一贴底，
// ResizeObserver 在布局后、绘制前回调，不会先闪一帧再跳
let ro: ResizeObserver | undefined
onMounted(() => {
  ro = new ResizeObserver(() => {
    if (stickToBottom.value) toBottom()
  })
  if (scroller.value) ro.observe(scroller.value)
  if (content.value) ro.observe(content.value)
})
onUnmounted(() => ro?.disconnect())

// 窗口被完全遮住时 WebView 会暂停渲染，ResizeObserver 不回调；按数据变化再贴一次底兜住
watch(
  () => [props.sentences.length, props.translations],
  () => {
    if (stickToBottom.value) toBottom()
  },
  { flush: 'post' },
)

function jumpToLatest() {
  stickToBottom.value = true
  toBottom()
}
</script>

<template>
  <div class="stream">
    <div ref="scroller" class="scroller" :style="{ '--fs': `${prefs.fontSize}px` }" @scroll.passive="onScroll">
      <div ref="content">
        <p v-if="hasOlder" class="older">往上翻加载更早的记录…</p>
        <template v-for="(s, i) in sentences" :key="s.id">
          <p v-if="divider(i)" class="when"><span>{{ divider(i) }}</span></p>
          <div class="entry" :class="{ latest: i === sentences.length - 1 }">
            <p class="line">
              <template v-if="prefs.ruby && s.ruby">
                <template v-for="(seg, j) in s.ruby" :key="j">
                  <ruby v-if="seg.ruby">{{ seg.text }}<rt>{{ seg.ruby }}</rt></ruby>
                  <template v-else>{{ seg.text }}</template>
                </template>
              </template>
              <template v-else>{{ s.text }}</template>
            </p>
            <template v-if="prefs.translation && translations[s.id]">
              <p v-if="translations[s.id].text" class="tr" :class="{ streaming: translations[s.id].streaming }">{{ translations[s.id].text }}</p>
              <p v-else-if="translations[s.id].error" class="tr failed">{{ translations[s.id].error }}</p>
              <p v-else class="tr waiting">翻译中…</p>
            </template>
          </div>
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

/* 字号全部从 --fs（最新一句的字号）按比例推出来，调一个值整体缩放 */
.entry {
  margin: 0 0 calc(var(--fs) * 0.8);
}

.line {
  margin: 0;
  font-family: var(--f-jp);
  font-size: calc(var(--fs) * 0.78);
  /* 留出振假名的位置，免得上下行挤在一起 */
  line-height: 2.05;
  color: var(--ink-soft);
  transition: color var(--t);
  /* 日文没有词间空格，靠这个避免标点落在行首 */
  line-break: strict;
  overflow-wrap: anywhere;
}

/* 振假名：小、弱、不参与选中复制（复制出来的应是原文） */
rt {
  font-size: 0.5em;
  color: var(--muted);
  letter-spacing: 0;
  user-select: none;
}

/* 最新一句是主角：更大、最高对比度 */
.latest .line {
  font-size: var(--fs);
  line-height: 1.95;
  color: var(--ink);
  letter-spacing: 0.01em;
}
.latest rt {
  color: var(--ink-soft);
}

/* 译文跟在原文下面，始终比原文弱一级：读的是日文，译文只是兜底 */
.tr {
  margin: 2px 0 0;
  font-size: calc(var(--fs) * 0.62);
  line-height: 1.75;
  color: var(--muted);
}
.latest .tr {
  font-size: calc(var(--fs) * 0.72);
  color: var(--ink-soft);
}
/* 流式输出时末尾一个闪烁的光标，表示还没说完 */
.tr.streaming::after {
  content: '▍';
  margin-left: 1px;
  color: var(--accent);
  animation: blink 1s steps(2, start) infinite;
}
@keyframes blink {
  to { visibility: hidden; }
}
.tr.waiting {
  opacity: 0.6;
}
.tr.failed {
  font-size: 12px;
  color: var(--accent);
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
