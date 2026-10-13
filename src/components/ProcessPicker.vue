<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import Modal from './Modal.vue'
import { JIcon } from '@jannchie/icons-vue'
import { IconChevronDown, IconEye, IconEyeOff, IconHistory, IconPlay, IconPlug, IconRefresh, IconTrash, IconZap } from '@jannchie/icons'
import { flog, useFukidashi, type Game, type HookStatus, type ProcessInfo } from '../composables/useFukidashi'

const props = defineProps<{
  list: () => Promise<ProcessInfo[]>
  status: () => Promise<HookStatus>
}>()

const emit = defineEmits<{
  attach: [proc: ProcessInfo]
  demo: []
  close: []
}>()

const processes = ref<ProcessInfo[]>([])
const query = ref('')
const loading = ref(false)
const selected = ref<ProcessInfo | null>(null)

const hook = ref<HookStatus | null>(null)

const { listGames, setGameAuto, forgetGame } = useFukidashi()

/** 玩过的游戏，按 exe 路径对上正在运行的进程 */
const games = ref<Game[]>([])
const showGames = ref(false)
const norm = (path: string) => path.replace(/^\\\\\?\\/, '').toLowerCase()
const known = computed(() => new Map(games.value.map((g) => [norm(g.path), g])))
const gameOf = (p: ProcessInfo) => known.value.get(norm(p.path))
const running = computed(() => new Set(processes.value.map((p) => norm(p.path))))

/** 有可见主窗口的进程。游戏一定在这里，默认只看这一段；玩过的排最前。 */
const apps = computed(() => {
  const windowed = processes.value.filter((p) => p.title)
  return [...windowed.filter((p) => gameOf(p)), ...windowed.filter((p) => !gameOf(p))]
})

async function toggleAuto(g: Game) {
  g.autoAttach = !g.autoAttach
  await setGameAuto(g.path, g.autoAttach)
}

async function forget(g: Game) {
  await forgetGame(g.path)
  games.value = games.value.filter((x) => x !== g)
}
const showAll = ref(false)

/**
 * 单个词对一个字段的得分：开头命中 > 包含 > 按顺序出现（模糊）。
 * 字段越"像名字"权重越高，路径只兜底。
 */
function termScore(term: string, p: ProcessInfo): number {
  const fields: [string | null, number][] = [
    [p.title, 100],
    [p.description, 90],
    [p.name, 80],
    [p.path, 30],
  ]
  let best = 0
  for (const [raw, w] of fields) {
    if (!raw) continue
    const f = raw.toLowerCase()
    const i = f.indexOf(term)
    let s = 0
    if (i === 0) s = 3
    else if (i > 0) s = 2
    else if (isSubsequence(term, f)) s = 1
    best = Math.max(best, s * w)
  }
  return best
}

function isSubsequence(needle: string, hay: string) {
  let j = 0
  for (const c of hay) if (c === needle[j] && ++j === needle.length) return true
  return false
}

function search(src: ProcessInfo[], q: string) {
  const terms = q.split(/\s+/).filter(Boolean)
  const scored: [ProcessInfo, number][] = []
  for (const p of src) {
    let total = 0
    for (const t of terms) {
      const s = termScore(t, p)
      if (!s) { total = 0; break }
      total += s
    }
    // 有窗口的同分时排前面
    if (total) scored.push([p, total + (p.title ? 1 : 0)])
  }
  // 稳定排序：同分保持后端给的顺序（窗口 Z 序）
  return scored.sort((a, b) => b[1] - a[1]).map(([p]) => p)
}

const query_ = computed(() => query.value.trim().toLowerCase())

const filtered = computed(() => {
  const src = showAll.value ? processes.value : apps.value
  const q = query_.value
  return (q ? search(src, q) : src).slice(0, 300)
})

/** 只看应用时搜不到，但全部进程里有 —— 提示一下，免得用户以为没有 */
const hiddenMatches = computed(() =>
  !showAll.value && query_.value ? search(processes.value, query_.value).length - filtered.value.length : 0,
)

function label(p: ProcessInfo) {
  return p.title || p.description || p.name
}

/** 列表变化后保证有选中项，默认选最上面那个（通常是刚刚在玩的游戏） */
watch(filtered, (list) => {
  if (!selected.value || !list.some((p) => p.pid === selected.value!.pid)) {
    selected.value = list[0] ?? null
  }
})

const listEl = ref<HTMLElement | null>(null)

function move(delta: number) {
  const list = filtered.value
  if (!list.length) return
  const i = list.findIndex((p) => p.pid === selected.value?.pid)
  const next = Math.min(list.length - 1, Math.max(0, (i < 0 ? -1 : i) + delta))
  selected.value = list[next]
  nextTick(() => {
    listEl.value?.querySelector('.proc.on')?.scrollIntoView({ block: 'nearest' })
  })
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') { e.preventDefault(); move(1) }
  else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1) }
  else if (e.key === 'Enter') { e.preventDefault(); confirm() }
}

const searchEl = ref<HTMLInputElement | null>(null)

async function refresh() {
  loading.value = true
  try {
    ;[processes.value, games.value] = await Promise.all([props.list(), listGames()])
    flog('info', `ProcessPicker: 拿到 ${processes.value.length} 个进程`)
  } catch (e) {
    flog('error', `ProcessPicker: 拉进程列表失败 ${e}`)
  } finally {
    loading.value = false
  }
}

function confirm() {
  if (!selected.value || !hook.value?.ok) return
  emit('attach', selected.value)
}

onMounted(async () => {
  searchEl.value?.focus()
  refresh()
  hook.value = await props.status()
})

</script>

<template>
  <Modal title="连接到游戏" @close="emit('close')">

      <!-- 就位时这里只是一行状态，用户不需要做任何事 -->
      <p v-if="hook" class="bundle" :class="{ bad: !hook.ok }">
        <span class="chip" :class="hook.ok ? 'ok' : 'warn'">{{ hook.ok ? '内置' : '缺失' }}</span>
        <template v-if="hook.ok">LunaHook {{ hook.text }} · 按游戏位数自动选择 32/64 位</template>
        <template v-else>{{ hook.text }}</template>
      </p>

      <div class="field grow">
        <div class="label-row">
          <label for="q">选择游戏</label>
          <span class="tools">
            <button class="ibtn" :class="{ on: showAll }" :aria-pressed="showAll"
                    :data-tip="showAll ? '只看带窗口的程序' : '显示全部进程（含后台）'" aria-label="显示全部进程"
                    @click="showAll = !showAll">
              <JIcon :icon="showAll ? IconEye : IconEyeOff" />
            </button>
            <button class="ibtn" :class="{ spin: loading }" :disabled="loading" data-tip="刷新进程列表"
                    aria-label="刷新" @click="refresh">
              <JIcon :icon="IconRefresh" />
            </button>
          </span>
        </div>
        <input id="q" ref="searchEl" v-model="query" spellcheck="false"
               placeholder="搜索窗口标题、程序名或路径 · ↑↓ 选择 · Enter 连接" @keydown="onKey" />

        <ul ref="listEl" class="list">
          <li v-for="p in filtered" :key="p.pid">
            <button
              class="proc"
              :class="{ on: selected?.pid === p.pid }"
              :title="p.path"
              @click="selected = p"
              @dblclick="selected = p; confirm()"
            >
              <img v-if="p.icon" class="picon" :src="p.icon" alt="" />
              <span v-else class="picon blank">{{ p.name.charAt(0).toUpperCase() }}</span>
              <span class="pmain">
                <span class="ptitle">
                  <JIcon v-if="gameOf(p)" class="known" :icon="IconHistory" :size="13" aria-label="玩过" />
                  {{ label(p) }}
                </span>
                <span class="pmeta">
                  <span class="pexe">{{ p.name }}</span>
                  <span class="pid">{{ p.pid }}</span>
                  <span v-if="p.is64 !== null" class="bits">{{ p.is64 ? '64 位' : '32 位' }}</span>
                </span>
              </span>
            </button>
          </li>
          <li v-if="!filtered.length && !loading" class="empty">
            <template v-if="hiddenMatches > 0">
              没有匹配的窗口，
              <button class="link" @click="showAll = true">在全部进程里有 {{ hiddenMatches }} 个匹配</button>
            </template>
            <template v-else-if="!showAll && !query_">没有找到带窗口的程序，先把游戏打开再刷新</template>
            <template v-else>没有匹配的进程</template>
          </li>
        </ul>
        <div v-if="games.length" class="games">
          <button class="games-bar" :aria-expanded="showGames" @click="showGames = !showGames">
            <JIcon :icon="IconHistory" :size="14" />
            <span>玩过的游戏 · {{ games.length }}</span>
            <span class="games-note">开着自动连接的，一启动就会连上</span>
            <JIcon class="caret" :class="{ up: showGames }" :icon="IconChevronDown" :size="14" />
          </button>
          <ul v-if="showGames" class="games-list">
            <li v-for="g in games" :key="g.path" class="game">
              <span class="gname" :title="g.path">
                <span class="live" :class="{ on: running.has(norm(g.path)) }"
                      :aria-label="running.has(norm(g.path)) ? '运行中' : '未运行'"></span>
                {{ g.title || g.name }}
              </span>
              <button class="ibtn" :class="{ on: g.autoAttach }" :aria-pressed="g.autoAttach"
                      :data-tip="g.autoAttach ? '自动连接：开' : '自动连接：关'" aria-label="自动连接"
                      @click="toggleAuto(g)">
                <JIcon :icon="IconZap" />
              </button>
              <button class="ibtn" data-tip="忘掉这个游戏" aria-label="忘掉这个游戏" @click="forget(g)">
                <JIcon :icon="IconTrash" />
              </button>
            </li>
          </ul>
        </div>

        <p v-if="filtered.length && hiddenMatches > 0" class="hint">
          另有 {{ hiddenMatches }} 个后台进程匹配，
          <button class="link" @click="showAll = true">显示全部</button>
        </p>
      </div>

      <template #footer>
        <button class="link" @click="emit('demo')">
          <JIcon :icon="IconPlay" :size="13" />没有游戏？先看演示
        </button>
        <button class="primary" :disabled="!selected || !hook?.ok" @click="confirm">
          <JIcon :icon="IconPlug" />连接
        </button>
      </template>
  </Modal>
</template>

<style scoped>
.bundle {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  margin: 0;
  padding: 8px 10px;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--muted);
  background: var(--surface-2);
  border: 1px solid var(--line-soft);
  border-radius: var(--r);
}
.bundle.bad {
  color: var(--ink-soft);
  border-color: color-mix(in srgb, var(--accent) 35%, transparent);
}

.field.grow {
  flex: 1;
  /* 最多收缩到列表的最小高度，再矮就整个弹窗滚动 */
  min-height: auto;
}

.list {
  flex: 1;
  min-height: 140px;
  margin: 2px 0 0;
  padding: 4px;
  overflow-y: auto;
  list-style: none;
  background: var(--surface-2);
  border: 1px solid var(--line-soft);
  border-radius: var(--r);
}

.proc {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 6px 8px;
  text-align: left;
}
.proc:hover {
  background: var(--hover);
}
.proc.on,
.proc.on:hover {
  background: var(--accent-soft);
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
}

.picon {
  width: 22px;
  height: 22px;
  flex: none;
}
.picon.blank {
  display: grid;
  place-items: center;
  font-size: 11px;
  font-weight: 600;
  color: var(--muted);
  background: var(--surface);
  border: 1px solid var(--line-soft);
  border-radius: 4px;
}

.pmain {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}
.ptitle {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pmeta {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: 11px;
  color: var(--muted);
  min-width: 0;
}
.pexe {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pid {
  font-family: var(--f-mono);
  font-variant-numeric: tabular-nums;
}
.bits {
  flex: none;
}

.tools {
  display: flex;
  align-items: center;
  gap: 2px;
}

.spin :deep(svg) {
  animation: spin 0.9s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
.known {
  flex: none;
  margin-right: 4px;
  vertical-align: -2px;
  color: var(--accent);
}

.games {
  flex: none;
  border: 1px solid var(--line-soft);
  border-radius: var(--r);
}
.games-bar {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  padding: 6px 8px;
  font-size: 12px;
  color: var(--ink-soft);
  text-align: left;
}
.games-bar:hover {
  background: var(--hover);
}
.games-note {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--muted);
  font-size: 11px;
}
.caret {
  flex: none;
  color: var(--muted);
  transition: transform var(--t);
}
.caret.up {
  transform: rotate(180deg);
}
.games-list {
  max-height: 150px;
  margin: 0;
  padding: 2px 4px 4px;
  overflow-y: auto;
  list-style: none;
  border-top: 1px solid var(--line-soft);
}
.game {
  display: flex;
  align-items: center;
  gap: 2px;
}
.gname {
  flex: 1;
  min-width: 0;
  padding-left: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12.5px;
}
.live {
  display: inline-block;
  width: 6px;
  height: 6px;
  margin-right: 6px;
  vertical-align: 1px;
  border-radius: 50%;
  background: var(--line);
}
.live.on {
  background: var(--good);
}

.empty {
  padding: 16px;
  text-align: center;
  color: var(--muted);
}

</style>
