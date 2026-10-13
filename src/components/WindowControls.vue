<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { JIcon } from '@jannchie/icons-vue'
import { IconMinus, IconSquare, IconWindowRestore, IconX } from '@jannchie/icons'

// 标题栏和顶栏合成一条：系统边框关掉（decorations: false），窗口按钮自己画。
// 尺寸、关闭键悬停变红都照 Windows 11 原生标题栏，用户不用重新学。
const win = getCurrentWindow()
const maximized = ref(false)

let unlisten: (() => void) | undefined
onMounted(async () => {
  const sync = async () => {
    maximized.value = await win.isMaximized().catch(() => false)
  }
  await sync()
  unlisten = await win.onResized(sync)
})
onUnmounted(() => unlisten?.())
</script>

<template>
  <div class="wc">
    <button class="wbtn" aria-label="最小化" @click="win.minimize()">
      <JIcon :icon="IconMinus" :size="15" />
    </button>
    <button class="wbtn" :aria-label="maximized ? '还原' : '最大化'" @click="win.toggleMaximize()">
      <JIcon :icon="maximized ? IconWindowRestore : IconSquare" :size="13" />
    </button>
    <button class="wbtn close" aria-label="关闭" @click="win.close()">
      <JIcon :icon="IconX" :size="15" />
    </button>
  </div>
</template>

<style scoped>
.wc {
  display: flex;
  align-self: stretch;
  margin-left: 6px;
}

.wbtn {
  display: grid;
  place-items: center;
  width: 46px;
  height: 100%;
  padding: 0;
  color: var(--ink-soft);
  background: transparent;
  border: 0;
  border-radius: 0;
}
.wbtn:hover {
  color: var(--ink);
  background: var(--hover);
}
.wbtn:active {
  background: var(--press);
}
.wbtn.close:hover {
  color: #fff;
  background: #C42B1C;
}
.wbtn.close:active {
  color: #fff;
  background: #A52B1E;
}
</style>
