<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import Modal from './Modal.vue'
import { JIcon } from '@jannchie/icons-vue'
import { IconCloudDownload, IconRefresh } from '@jannchie/icons'
import { useUpdater } from '../composables/useUpdater'
import { useFukidashi } from '../composables/useFukidashi'

const emit = defineEmits<{ close: [] }>()

const version = ref('')
const hook = ref('')
const updater = useUpdater()
const { hookStatus } = useFukidashi()

onMounted(async () => {
  version.value = await getVersion().catch(() => '')
  hook.value = (await hookStatus()).text
})

const state = computed(() => {
  if (updater.progress.value !== null) return `正在下载 ${Math.round(updater.progress.value * 100)}%，完成后自动重启`
  switch (updater.status.value) {
    case 'checking': return '正在检查…'
    case 'latest': return '已是最新版本'
    case 'available': return `发现新版本 v${updater.available.value?.version}`
    case 'failed': return '检查失败，请确认网络可以访问 GitHub'
    default: return ''
  }
})
</script>

<template>
  <Modal title="关于" :width="420" @close="emit('close')">
    <div class="rows">
      <div class="row">
        <span class="key">版本</span>
        <span class="val">v{{ version }}</span>
      </div>
      <div class="row">
        <span class="key">文本提取</span>
        <span class="val">LunaHook {{ hook }}</span>
      </div>
    </div>

    <p v-if="state" class="hint" role="status">{{ state }}</p>
    <p v-if="updater.error.value" class="hint err" role="alert">{{ updater.error.value }}</p>

    <template #footer>
      <span class="hint">新版本发布在 GitHub Releases</span>
      <button v-if="updater.status.value === 'available'" class="primary"
              :disabled="updater.progress.value !== null" @click="updater.install">
        <JIcon :icon="IconCloudDownload" :size="13" />更新并重启
      </button>
      <button v-else class="primary" :disabled="updater.status.value === 'checking'" @click="updater.checkNow">
        <JIcon :icon="IconRefresh" :size="13" />检查更新
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.rows {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  font-size: 13px;
}
.key {
  color: var(--muted);
}
.val {
  font-family: var(--f-mono);
  color: var(--ink);
  user-select: text;
}
.err {
  color: var(--danger);
}
</style>
