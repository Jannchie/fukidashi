import { computed, ref, shallowRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface ThreadStat {
  key: string
  name: string
  hcode: string
  sample: string
  count: number
  uniqueCount: number
  avgLen: number
  kanaRatio: number
  score: number
  perGlyph: boolean
}

/** 振假名分段：拼起来就是整句，ruby 只出现在汉字片段上 */
export interface RubySegment {
  text: string
  ruby?: string
}

export interface Sentence {
  id: number
  key: string
  text: string
  ruby: RubySegment[] | null
  at: number
  /** 已交给翻译，译文随后按 id 推送 */
  translating: boolean
}

export interface ProcessInfo {
  pid: number
  name: string
  path: string
  /** 可见主窗口标题；有它就是前台应用 */
  title: string | null
  description: string | null
  /** PNG data URL */
  icon: string | null
  is64: boolean | null
}

/** 玩过的游戏：Rust 侧存在 games.json 里 */
export interface Game {
  path: string
  name: string
  title: string | null
  /** 上次手动锁定的钩子 */
  hcode: string | null
  autoAttach: boolean
  lastPlayed: number
}

/** 一句的译文状态：等待中、成功、失败三选一 */
export interface Translation {
  text?: string
  error?: string
  /** 流式输出中，text 还会继续变长 */
  streaming?: boolean
}

export interface HookStatus {
  ok: boolean
  text: string
}

type FukidashiEvent =
  | ({ kind: 'sentence' } & Sentence)
  | { kind: 'threads'; threads: ThreadStat[]; active: string | null; pinned: string | null; warmingUp: boolean }
  | { kind: 'status'; connected: boolean; detail: string }
  | { kind: 'translation'; id: number; text: string | null; error: string | null; done: boolean }
  | { kind: 'backlog'; game: string; page: HistoryPage }

/** 一页历史记录：从旧到新，附带当时的译文 */
interface HistoryPage {
  sentences: Sentence[]
  translations: [number, string][]
  more: boolean
}

/**
 * 界面上最多留这么多句。全部记录都在 Rust 侧的 history.db 里，
 * 这里只是显示窗口：超出就丢掉最旧的，往上翻时再从库里取回来。
 */
const MAX_VISIBLE = 1000
const PAGE = 200
/** 与 Rust 侧 lib.rs 的 EVENT 一致 */
const EVENT = 'fukidashi://event'

const sentences = shallowRef<Sentence[]>([])
/** 句子 id → 译文。条目存在但为空对象表示正在翻译。 */
const translations = shallowRef<Record<number, Translation>>({})
const threads = shallowRef<ThreadStat[]>([])
const activeKey = ref<string | null>(null)
/** 锁定的来源（手动锁定，或按上次的选择自动锁定）。null 表示交给自动选择。 */
const pinnedKey = ref<string | null>(null)
const connected = ref(false)
const warmingUp = ref(false)
const detail = ref('未连接')
const lastError = ref('')
/** 当前显示的是哪个游戏的记录（exe 路径）；演示模式为 null */
const historyGame = ref<string | null>(null)
/** 库里还有更早的记录 */
const hasOlder = ref(false)
let loadingOlder = false

let unlisten: UnlistenFn | null = null

/**
 * 把前端的未捕获异常也送进界面上那条错误栏。
 *
 * 桌面应用没有随手可开的控制台，一个静默崩掉的组件从外面看就是"界面莫名其妙没反应"，
 * 极难定位。宁可把丑话显式写在脸上。
 */
export function reportError(e: unknown) {
  const msg = e instanceof Error ? e.message : String(e)
  console.error(e)
  flog('error', msg)
  lastError.value = msg
}

/** 把日志送到 Rust 侧打进标准输出，dev 模式下能直接在终端里读到。 */
export function flog(level: string, msg: string) {
  invoke('log_front', { level, msg }).catch(() => {})
}

export function useFukidashi() {
  const activeThread = computed(
    () => threads.value.find((t) => t.key === activeKey.value) ?? null,
  )

  async function bind() {
    if (unlisten) return
    unlisten = await listen<FukidashiEvent>(EVENT, (e) => {
      const p = e.payload
      switch (p.kind) {
        case 'sentence': {
          const { kind: _, ...sentence } = p
          const next = sentences.value.concat(sentence)
          if (next.length > MAX_VISIBLE) {
            const dropped = next.splice(0, next.length - MAX_VISIBLE)
            const rest = { ...translations.value }
            for (const s of dropped) delete rest[s.id]
            translations.value = rest
            hasOlder.value = historyGame.value !== null
          }
          sentences.value = next
          if (p.translating) translations.value = { ...translations.value, [p.id]: {} }
          break
        }
        case 'translation':
          translations.value = {
            ...translations.value,
            [p.id]: { text: p.text ?? undefined, error: p.error ?? undefined, streaming: !p.done },
          }
          break
        case 'backlog':
          historyGame.value = p.game
          hasOlder.value = p.page.more
          sentences.value = p.page.sentences
          translations.value = toTranslations(p.page.translations)
          break
        case 'threads':
          threads.value = p.threads
          activeKey.value = p.active
          pinnedKey.value = p.pinned
          warmingUp.value = p.warmingUp
          break
        case 'status':
          connected.value = p.connected
          detail.value = p.detail
          break
      }
    })
  }

  function reset() {
    historyGame.value = null
    hasOlder.value = false
    sentences.value = []
    translations.value = {}
    threads.value = []
    activeKey.value = null
    pinnedKey.value = null
    warmingUp.value = false
    lastError.value = ''
  }

  async function guard<T>(fn: () => Promise<T>): Promise<T | null> {
    try {
      lastError.value = ''
      return await fn()
    } catch (e) {
      lastError.value = String(e)
      return null
    }
  }

  const listProcesses = () =>
    guard(() => invoke<ProcessInfo[]>('list_processes')).then((v) => v ?? [])

  /** 内置 LunaHook 的状态：就位时 text 是版本号，否则是缺失原因 */
  const hookStatus = (): Promise<HookStatus> =>
    invoke<string>('hook_status').then(
      (text) => ({ ok: true, text }),
      (e) => ({ ok: false, text: String(e) }),
    )

  /** 附加到游戏。钩子位数由 Rust 侧读目标 PE 头自动匹配。 */
  const attach = (p: ProcessInfo) => {
    reset()
    return guard(() =>
      invoke('attach_process', { pid: p.pid, name: p.name, path: p.path, title: p.title }),
    )
  }

  const listGames = () => invoke<Game[]>('list_games').catch(() => [] as Game[])
  const setGameAuto = (path: string, on: boolean) => invoke('set_game_auto', { path, on })
  const forgetGame = (path: string) => invoke('forget_game', { path })

  const startReplay = (path?: string) => {
    reset()
    return guard(() => invoke('start_replay', { path: path ?? null }))
  }

  /** 往上翻到顶时取更早的一页，接在最前面 */
  async function loadOlder() {
    const game = historyGame.value
    const first = sentences.value[0]
    if (!game || !first || !hasOlder.value || loadingOlder) return
    loadingOlder = true
    try {
      const page = await invoke<HistoryPage>('history_page', { game, before: first.id, limit: PAGE })
      if (historyGame.value !== game) return
      sentences.value = page.sentences.concat(sentences.value)
      translations.value = { ...toTranslations(page.translations), ...translations.value }
      hasOlder.value = page.more
    } catch (e) {
      reportError(e)
    } finally {
      loadingOlder = false
    }
  }

  const stop = () => guard(() => invoke('stop'))

  const pin = (key: string | null) => guard(() => invoke('pin_thread', { key }))

  return {
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
    listGames,
    setGameAuto,
    forgetGame,
    attach,
    startReplay,
    stop,
    pin,
    hasOlder,
    loadOlder,
  }
}

function toTranslations(rows: [number, string][]): Record<number, Translation> {
  return Object.fromEntries(rows.map(([id, text]) => [id, { text }]))
}
