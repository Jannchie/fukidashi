import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface TranslatorConfig {
  enabled: boolean
  /** OpenAI 兼容接口的根地址，形如 http://127.0.0.1:11434/v1 */
  baseUrl: string
  model: string
  apiKey: string
  /** 目标语言，直接写进提示词 */
  target: string
  /** 带上前几句作上下文 */
  context: number
}

/** 常见本地推理服务的默认地址。它们都提供 OpenAI 兼容接口。 */
export const PRESETS = [
  { label: 'Ollama', url: 'http://127.0.0.1:11434/v1' },
  { label: 'LM Studio', url: 'http://127.0.0.1:1234/v1' },
  { label: 'llama.cpp', url: 'http://127.0.0.1:8080/v1' },
] as const

export const TARGETS = ['简体中文', '繁體中文', 'English'] as const

const KEY = 'fukidashi.translator'
/** 改名前的键，第一次启动时读过来 */
const OLD_KEY = 'yomu.translator'

const DEFAULTS: TranslatorConfig = {
  enabled: false,
  baseUrl: PRESETS[0].url,
  model: '',
  apiKey: '',
  target: TARGETS[0],
  context: 3,
}

function load(): TranslatorConfig {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(KEY) ?? localStorage.getItem(OLD_KEY) ?? '{}') }
  } catch {
    return { ...DEFAULTS }
  }
}

const config = ref<TranslatorConfig>(load())

/** 开着且选了模型才算可用 */
const ready = computed(() => config.value.enabled && !!config.value.model)

// 任何改动都立刻生效：存本地，同步给 Rust。启动时也会推一次。
watch(
  config,
  (c) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(c))
    } catch {}
    invoke('set_translator', { config: c }).catch(() => {})
  },
  { deep: true, immediate: true },
)

const listModels = (baseUrl: string, apiKey: string) =>
  invoke<string[]>('list_models', { baseUrl, apiKey })

export function useTranslator() {
  return { config, ready, listModels }
}
