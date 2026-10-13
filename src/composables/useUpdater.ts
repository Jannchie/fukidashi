import { ref } from 'vue'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { flog } from './useFukidashi'

/** 有新版本时才有值 */
const available = ref<Update | null>(null)
/** 下载进度 0–1；null 表示没在下载 */
const progress = ref<number | null>(null)
const error = ref('')

/**
 * 启动后在后台查一次更新。只提示、不自动安装：
 * 安装会重启应用，正在读的游戏会被断开，时机交给用户。
 */
async function checkLater(delay = 5000) {
  // dev 下的版本号和发布版对不上，查了也没意义
  if (import.meta.env.DEV) return
  setTimeout(async () => {
    try {
      available.value = await check()
      if (available.value) flog('info', `updater: 有新版本 ${available.value.version}`)
    } catch (e) {
      // 离线、GitHub 访问不了都很常见，不打扰用户
      flog('warn', `updater: 检查失败 ${e}`)
    }
  }, delay)
}

async function install() {
  const u = available.value
  if (!u || progress.value !== null) return
  error.value = ''
  progress.value = 0
  let total = 0
  let got = 0
  try {
    await u.downloadAndInstall((ev) => {
      if (ev.event === 'Started') total = ev.data.contentLength ?? 0
      else if (ev.event === 'Progress') {
        got += ev.data.chunkLength
        progress.value = total ? got / total : 0
      }
    })
    await relaunch()
  } catch (e) {
    error.value = `更新失败：${e}`
    progress.value = null
  }
}

export function useUpdater() {
  return { available, progress, error, checkLater, install }
}
