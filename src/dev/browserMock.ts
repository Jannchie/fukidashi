/**
 * 在普通浏览器里打开 dev 页面时模拟 Tauri 后端，方便调界面（hover、排版这类得真操作才看得出的东西）。
 * 只在 DEV 且不在 Tauri 窗口里时由 main.ts 动态加载，不会进生产包。
 * 台词、振假名、译文都取自真实运行时的输出。
 */
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { emit } from '@tauri-apps/api/event'
import type { Game, ProcessInfo, RubySegment } from '../composables/useFukidashi'

const r = (text: string, ruby?: string): RubySegment => (ruby ? { text, ruby } : { text })

const LINES: [string, RubySegment[], string][] = [
  ['千明「そういうわけにはいかないよ。ほら、貸して」',
    [r('千明', 'ちあき'), r('「そういうわけにはいかないよ。ほら、'), r('貸', 'か'), r('して」')],
    '千明「那可不行。来，给我吧」'],
  ['蓮「あ……」', [r('蓮', 'れん'), r('「あ……」')], '莲「啊……」'],
  ['千明「重いでしょ、これ。私が持つから」',
    [r('千明', 'ちあき'), r('「'), r('重', 'おも'), r('いでしょ、これ。'), r('私', 'わたし'), r('が'), r('持', 'も'), r('つから」')],
    '千明「这很重吧。我来拿吧」'],
  ['蓮「……ありがとうございます」', [r('蓮', 'れん'), r('「……ありがとうございます」')], '莲「……谢谢」'],
  ['そう言って、彼女は少しだけ笑った。',
    [r('そう'), r('言', 'い'), r('って、'), r('彼女', 'かのじょ'), r('は'), r('少', 'すこ'), r('しだけ'), r('笑', 'わら'), r('った。')],
    '说完，她微微一笑。'],
]

const PROCESSES: ProcessInfo[] = [
  { pid: 42200, name: 'おしかけ奥様と、キミの嘘.exe', path: 'E:\\GAME\\おしかけ奥様と、キミの嘘\\おしかけ奥様と、キミの嘘.exe', title: 'おしかけ奥様と、キミの嘘', description: 'TVP(KIRIKIRI) Z core / Scripting Platform for Win32', icon: null, is64: false },
  { pid: 25876, name: 'Code.exe', path: 'C:\\Program Files\\Microsoft VS Code\\Code.exe', title: 'README.md - Visual Studio Code', description: 'Visual Studio Code', icon: null, is64: true },
  { pid: 26916, name: 'msedge.exe', path: 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', title: 'Microsoft Edge', description: 'Microsoft Edge', icon: null, is64: true },
]

const GAMES: Game[] = [
  { path: PROCESSES[0].path, name: PROCESSES[0].name, title: PROCESSES[0].title, hcode: 'HQX-C@20DA90:おしかけ奥様と、キミの嘘.exe', autoAttach: true, lastPlayed: Date.now() },
  { path: 'E:\\GAME\\サクラノ詩\\sakura.exe', name: 'sakura.exe', title: 'サクラノ詩 －櫻の森の上を舞う－', hcode: null, autoAttach: false, lastPlayed: Date.now() - 86400000 },
]

let timer: number | undefined
/** 新句子的 id 接在模拟的历史记录后面 */
let id = 1000

/** 模拟库里已有的记录：两次游玩，各 225 句，隔了一天 */
const PAST = Array.from({ length: 450 }, (_, k) => {
  const [text, ruby, tr] = LINES[k % LINES.length]
  const at = Date.now() - (k < 225 ? 2 : 1) * 86400000 + k * 8000
  return { id: k + 1, key: 'k1', text: `${text}（${k + 1}）`, ruby: [...ruby, r(`（${k + 1}）`)], at, translating: false, tr }
})

function page(before: number | null, limit: number) {
  const older = PAST.filter((s) => before === null || s.id < before)
  const slice = older.slice(-limit)
  return {
    sentences: slice.map(({ tr: _, ...s }) => s),
    translations: slice.map((s) => [s.id, s.tr]),
    more: older.length > limit,
  }
}

function play(detail: string, game: string | null) {
  clearInterval(timer)
  if (game) emit('fukidashi://event', { kind: 'backlog', game, page: page(null, 200) })
  emit('fukidashi://event', { kind: 'status', connected: true, detail })
  emit('fukidashi://event', {
    kind: 'threads',
    active: 'k1',
    pinned: null,
    warmingUp: false,
    threads: [
      { key: 'k1', name: 'krkrz2', hcode: 'HQX-C@20DA90', sample: LINES[0][0], count: 42, uniqueCount: 40, avgLen: 18, kanaRatio: 0.62, score: 84, perGlyph: false },
      { key: 'k2', name: 'EmbedKrkrZ', hcode: 'ENHVXN-8@29EF3', sample: '千明', count: 40, uniqueCount: 3, avgLen: 2, kanaRatio: 0, score: 21, perGlyph: false },
    ],
  })
  let i = 0
  const next = () => {
    const [text, ruby, tr] = LINES[i++ % LINES.length]
    const sid = id++
    emit('fukidashi://event', { kind: 'sentence', id: sid, key: 'k1', text, ruby, at: Date.now(), translating: true })
    // 模拟流式输出：逐字吐出
    const chars = [...tr]
    chars.forEach((_, k) => setTimeout(() => emit('fukidashi://event', {
      kind: 'translation', id: sid, text: chars.slice(0, k + 1).join(''), error: null, done: k === chars.length - 1,
    }), 400 + k * 60))
  }
  next()
  timer = window.setInterval(next, 2500)
}

mockIPC(
  (cmd, args) => {
    switch (cmd) {
      case 'list_processes': return PROCESSES
      case 'session_info': return { connected: false, detail: '未连接', game: null }
      case 'plugin:app|version': return '0.1.2'
      case 'plugin:updater|check': return null
      case 'hook_status': return 'v12.1.2.4'
      case 'list_games': return GAMES
      case 'list_models': return ['sakura-galtransl-14b-v3.8:latest', 'qwen3:14b']
      case 'attach_process': return play('已附加到 おしかけ奥様と、キミの嘘.exe · 32 位', PROCESSES[0].path)
      case 'start_replay': return play('演示模式', null)
      case 'history_page': {
        const { before, limit } = args as { before: number | null; limit: number }
        return page(before, limit)
      }
      case 'stop':
        clearInterval(timer)
        emit('fukidashi://event', { kind: 'status', connected: false, detail: '已停止' })
        return
      default: return null
    }
  },
  { shouldMockEvents: true },
)

// 窗口按钮和标题栏拖动要用到当前窗口的信息
mockWindows('main')

console.info('[fukidashi] 浏览器模式：后端已模拟')
