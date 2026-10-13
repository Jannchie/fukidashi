<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import Modal from './Modal.vue'
import { JIcon } from '@jannchie/icons-vue'
import { IconRefresh } from '@jannchie/icons'
import { PRESETS, TARGETS, useTranslator } from '../composables/useTranslator'

const emit = defineEmits<{ close: [] }>()
const { config, listModels } = useTranslator()

const models = ref<string[]>([])
const loading = ref(false)
const error = ref('')

const preset = computed(() => PRESETS.find((p) => p.url === config.value.baseUrl)?.label ?? '')

async function refresh() {
  loading.value = true
  error.value = ''
  try {
    models.value = await listModels(config.value.baseUrl, config.value.apiKey)
    // 还没选模型时默认选第一个，省一步
    if (!config.value.model && models.value.length) config.value.model = models.value[0]
  } catch (e) {
    models.value = []
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

// 换了服务地址，模型列表就要重新拉
let timer: number | undefined
watch(
  () => [config.value.baseUrl, config.value.apiKey],
  () => {
    clearTimeout(timer)
    timer = window.setTimeout(refresh, 400)
  },
)

onMounted(refresh)
</script>

<template>
  <Modal title="翻译" :width="460" @close="emit('close')">
    <label class="toggle">
      <input v-model="config.enabled" type="checkbox" />
      实时翻译正文
    </label>

    <div class="field">
      <div class="label-row">
        <label for="svc">服务</label>
        <span class="presets">
          <button v-for="p in PRESETS" :key="p.label" class="preset" :class="{ on: preset === p.label }"
                  @click="config.baseUrl = p.url">
            {{ p.label }}
          </button>
        </span>
      </div>
      <input id="svc" v-model.trim="config.baseUrl" spellcheck="false"
             placeholder="OpenAI 兼容接口地址，如 http://127.0.0.1:11434/v1" />
    </div>

    <div class="field">
      <div class="label-row">
        <label for="model">模型</label>
        <button class="ibtn" :disabled="loading" data-tip="重新读取模型列表" aria-label="刷新模型列表" @click="refresh">
          <JIcon :icon="IconRefresh" />
        </button>
      </div>
      <select id="model" v-model="config.model" :disabled="!models.length && !config.model">
        <option v-if="!models.length && !config.model" value="">（没有可用模型）</option>
        <!-- 服务暂时连不上时也保留已选的模型，不要把配置冲掉 -->
        <option v-if="config.model && !models.includes(config.model)" :value="config.model">
          {{ config.model }}
        </option>
        <option v-for="m in models" :key="m" :value="m">{{ m }}</option>
      </select>
      <p v-if="error" class="hint bad">{{ error }}</p>
    </div>

    <div class="row">
      <div class="field">
        <label for="target">译成</label>
        <select id="target" v-model="config.target">
          <option v-for="t in TARGETS" :key="t" :value="t">{{ t }}</option>
        </select>
      </div>
      <div class="field">
        <label for="ctx">上下文句数</label>
        <select id="ctx" v-model.number="config.context">
          <option v-for="n in [0, 1, 3, 5, 8]" :key="n" :value="n">{{ n }}</option>
        </select>
      </div>
    </div>

    <div class="field">
      <label for="key">API Key</label>
      <input id="key" v-model.trim="config.apiKey" type="password" spellcheck="false"
             placeholder="本地服务留空即可" />
    </div>

    <p class="hint">
      模型由 Ollama / LM Studio / llama.cpp 等服务运行，Fukidashi 只负责发请求。
      日译中推荐 Sakura-GalTransl（专为 galgame 训练），Sakura / GalTransl 系列会按模型名自动套用各自的提示词格式。
    </p>
  </Modal>
</template>

<style scoped>
.presets {
  display: flex;
  gap: 4px;
}
.preset {
  padding: 2px 8px;
  font-size: 11.5px;
  color: var(--muted);
  border-color: var(--line-soft);
}
.preset:hover {
  color: var(--ink);
  background: var(--hover);
}
.preset.on,
.preset.on:hover {
  color: var(--accent);
  background: var(--accent-soft);
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
}

.row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.bad {
  color: var(--accent);
}
</style>
