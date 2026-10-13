<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { flog, useFukidashi } from './composables/useFukidashi'
import TextStream from './components/TextStream.vue'
import ThreadPicker from './components/ThreadPicker.vue'
import ProcessPicker from './components/ProcessPicker.vue'
import TranslatorSettings from './components/TranslatorSettings.vue'
import { useTranslator } from './composables/useTranslator'
import type { ProcessInfo } from './composables/useFukidashi'
import { JIcon } from '@jannchie/icons-vue'
import { IconCloudDownload, IconGamepad, IconLanguages, IconPlay, IconPlugOff, IconTypography } from '@jannchie/icons'
import ReaderMenu from './components/ReaderMenu.vue'
import WindowControls from './components/WindowControls.vue'
import { useReader } from './composables/useReader'
import { useUpdater } from './composables/useUpdater'
import { useDragRegion } from './composables/useDragRegion'

const {
  sentences,
  translations,
  threads,
  activeKey,
  activeThread,
  pinnedKey,
  connected,
  warmingUp,
  detail,
  lastError,
  bind,
  listProcesses,
  hookStatus,
  attach,
  startReplay,
  stop,
  pin,
  hasOlder,
  loadOlder,
} = useFukidashi()

const drag = useDragRegion()

const showPicker = ref(false)
const showTranslator = ref(false)

const showReader = ref(false)
const reader = useReader()

// Ctrl + = / - / 0 调字号，和浏览器缩放的习惯一致
function onKey(e: KeyboardEvent) {
  if (!e.ctrlKey || e.altKey) return
  if (e.key === '=' || e.key === '+') reader.bigger()
  else if (e.key === '-') reader.smaller()
  else if (e.key === '0') reader.resetFont()
  else return
  e.preventDefault()
}
onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))

const { config: trConfig, ready: trReady } = useTranslator()

watch(showPicker, (v) => flog('info', `App: showPicker=${v}`))
// 连上了（包括后台自动连接）就收起选择器，免得用户再点一次把会话打断
watch(connected, (v) => {
  if (v) showPicker.value = false
})

onMounted(bind)

const updater = useUpdater()
onMounted(() => updater.checkLater())
const updateTip = computed(() => {
  const u = updater.available.value
  if (!u) return ''
  if (updater.progress.value !== null) return `正在下载 ${Math.round(updater.progress.value * 100)}%`
  return `更新到 v${u.version}（会重启）`
})

async function onAttach(p: ProcessInfo) {
  showPicker.value = false
  await attach(p)
}

async function onDemo() {
  showPicker.value = false
  await startReplay()
}
</script>

<template>
  <div class="app">
    <!-- 这条就是标题栏：按钮以外的地方可拖动窗口、双击最大化 -->
    <header class="top" @pointerdown="drag.onPointerdown" @pointermove="drag.onPointermove"
            @pointerup="drag.onPointerup" @mousedown="drag.onMousedown" @dblclick="drag.onDblclick">
      <img class="appicon" src="/icon.svg" alt="" width="16" height="16" draggable="false" />
      <span class="status" :class="{ live: connected }" aria-hidden="true"></span>
      <span class="detail">{{ detail }}</span>

      <div class="spacer"></div>

      <button v-if="updater.available.value" class="ibtn update" :class="{ busy: updater.progress.value !== null }"
              :data-tip="updateTip" :aria-label="updateTip" @click="updater.install">
        <JIcon :icon="IconCloudDownload" />
      </button>

      <div class="anchor" data-no-drag>
        <button class="ibtn" :class="{ on: showReader }" data-tip="阅读设置：字号、振假名、译文"
                aria-label="阅读设置" :aria-expanded="showReader" @click="showReader = !showReader">
          <JIcon :icon="IconTypography" />
        </button>
        <ReaderMenu v-if="showReader" @close="showReader = false" />
      </div>
      <button class="ibtn" :class="{ on: trReady }" :data-tip="trReady ? `翻译：${trConfig.model}` : '翻译（未开启）'"
              aria-label="翻译设置" @click="showTranslator = true">
        <JIcon :icon="IconLanguages" />
      </button>
      <button v-if="connected" class="ibtn" data-tip="断开游戏" aria-label="断开游戏" @click="stop">
        <JIcon :icon="IconPlugOff" />
      </button>
      <button class="ibtn" :data-tip="connected ? '切换游戏' : '连接到游戏'" aria-label="连接到游戏"
              @click="showPicker = true">
        <JIcon :icon="IconGamepad" />
      </button>
      <WindowControls />
    </header>

    <p v-if="lastError" class="error" role="alert">{{ lastError }}</p>
    <p v-if="updater.error.value" class="error" role="alert">{{ updater.error.value }}</p>

    <TextStream v-if="connected || sentences.length"
                :sentences="sentences" :translations="translations" :prefs="reader.prefs.value" :connected="connected" :warming-up="warmingUp"
                :has-older="hasOlder" :load-older="loadOlder" />

    <div v-else class="welcome">
      <img class="logo" src="/icon.svg" alt="" width="72" height="72" />
      <h1>吹き出し</h1>
      <p class="sub">连接正在运行的视觉小说，实时显示其中的日文文本。</p>
      <button class="cta" @click="showPicker = true">
        <JIcon :icon="IconGamepad" />连接到游戏
      </button>
      <button class="link" @click="onDemo">
        <JIcon :icon="IconPlay" :size="13" />没有游戏在跑？先看演示
      </button>
    </div>

    <ThreadPicker v-if="connected || threads.length"
                  :threads="threads" :active-key="activeKey"
                  :pinned-key="pinnedKey" :active-thread="activeThread"
                  @pin="pin" />

    <ProcessPicker v-if="showPicker" :list="listProcesses" :status="hookStatus"
                   @attach="onAttach" @demo="onDemo" @close="showPicker = false" />
    <TranslatorSettings v-if="showTranslator" @close="showTranslator = false" />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg);
}

.anchor {
  position: relative;
}

.top {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: none;
  height: 38px;
  /* 右边贴着窗口按钮，不留边 */
  padding: 0 0 0 12px;
  user-select: none;
  border-bottom: 1px solid var(--line);
  background: var(--surface);
}

.appicon {
  flex: none;
  margin-right: 6px;
}

.status {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--muted);
  flex: none;
  transition: background var(--t), box-shadow var(--t);
}
.status.live {
  background: var(--good);
  box-shadow: 0 0 0 3px var(--good-soft);
}

.detail {
  font-size: 12px;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 有新版本：强调色，不闪不跳，看见了就行 */
.ibtn.update {
  color: var(--accent);
}
.ibtn.update.busy {
  pointer-events: none;
  animation: breathe 1.4s ease-in-out infinite;
}
@keyframes breathe {
  50% { opacity: 0.45; }
}

.spacer {
  flex: 1;
}

.error {
  flex: none;
  margin: 0;
  padding: 7px 14px;
  font-size: 12px;
  color: var(--accent);
  background: var(--accent-soft);
  border-bottom: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
}

.welcome {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 24px;
  text-align: center;
}

.logo {
  margin-bottom: 6px;
}

.welcome h1 {
  margin: 0;
  font-family: var(--f-jp);
  font-size: 34px;
  font-weight: 500;
  letter-spacing: 0.16em;
  color: var(--ink);
}

.sub {
  margin: 0 0 14px;
  max-width: 22em;
  font-size: 12.5px;
  line-height: 1.85;
  color: var(--muted);
}

.cta {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 9px 22px;
  font-weight: 600;
  color: #fff;
  background: var(--accent);
  border-color: var(--accent);
}
.cta:hover {
  filter: brightness(1.08);
}

.link {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px;
  font-size: 12px;
  color: var(--muted);
  text-decoration: underline;
  text-underline-offset: 3px;
  text-decoration-color: var(--line);
}
.link:hover {
  color: var(--accent);
  text-decoration-color: currentColor;
}
</style>
