<script setup lang="ts">
import { ref } from 'vue'
import type { ThreadStat } from '../composables/useFukidashi'
import { JIcon } from '@jannchie/icons-vue'
import { IconChevronUp, IconPin, IconPinOff } from '@jannchie/icons'

defineProps<{
  threads: ThreadStat[]
  activeKey: string | null
  pinnedKey: string | null
  activeThread: ThreadStat | null
}>()

const emit = defineEmits<{ pin: [key: string | null] }>()

const open = ref(false)

/** 分数映射到一个语义档位，让用户不必读数字就知道靠不靠谱 */
function tier(score: number) {
  if (score >= 60) return 'good'
  if (score >= 30) return 'fair'
  return 'weak'
}
</script>

<template>
  <div class="picker" :class="{ open }">
    <button class="bar" @click="open = !open">
      <JIcon class="caret" :class="{ up: open }" :icon="IconChevronUp" :size="14" aria-hidden="true" />

      <template v-if="activeThread">
        <JIcon v-if="pinnedKey" class="mode-icon" :icon="IconPin" :size="13" aria-label="已锁定" />
        <span v-else class="mode">自动</span>
        <span class="fn">{{ activeThread.name || '未知钩子' }}</span>
        <span class="dot" :class="tier(activeThread.score)" aria-hidden="true"></span>
        <span class="score">{{ Math.round(activeThread.score) }}</span>
      </template>
      <template v-else>
        <span class="mode">来源</span>
        <span class="fn dim">尚未识别</span>
      </template>

      <span class="count">{{ threads.length }} 路</span>
    </button>

    <div v-if="open" class="list">
      <p class="tip">
        自动选择按假名占比、句长、重复率打分。选错了就手动锁定一路。
      </p>

      <button v-for="t in threads" :key="t.key" class="item"
              :class="{ on: t.key === activeKey }"
              @click="emit('pin', t.key === pinnedKey ? null : t.key)">
        <span class="dot" :class="tier(t.score)" aria-hidden="true"></span>
        <span class="meta">
          <span class="fn">{{ t.name || '未知' }}</span>
          <span class="tags">
            <span v-if="t.perGlyph" class="tag">逐字</span>
            <span class="tag">{{ t.count }} 行</span>
            <span class="tag">均 {{ Math.round(t.avgLen) }} 字</span>
            <span class="tag">假名 {{ Math.round(t.kanaRatio * 100) }}%</span>
          </span>
          <span class="sample">{{ t.sample || '（无内容）' }}</span>
        </span>
        <span class="right">
          <span class="score">{{ Math.round(t.score) }}</span>
          <JIcon v-if="t.key === pinnedKey" class="lock" :icon="IconPin" :size="13" aria-label="已锁定" />
        </span>
      </button>

      <p v-if="!threads.length" class="tip center">还没有收到任何文本</p>

      <button v-if="pinnedKey" class="release" @click="emit('pin', null)">
        <JIcon :icon="IconPinOff" :size="13" />交回自动选择
      </button>
    </div>
  </div>
</template>

<style scoped>
.picker {
  border-top: 1px solid var(--line);
  background: var(--surface);
  flex: none;
}

.bar {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 7px 14px;
  border-radius: 0;
  font-size: 12px;
}
.bar:hover {
  background: var(--hover);
}

.caret {
  flex: none;
  color: var(--muted);
  transition: transform var(--t);
  transform: rotate(180deg);
}
.caret.up {
  transform: rotate(0deg);
}

.mode-icon {
  flex: none;
  color: var(--accent);
}

.mode {
  padding: 1px 6px;
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--accent);
  background: var(--accent-soft);
  border-radius: 3px;
}

.fn {
  font-family: var(--f-mono);
  font-size: 11.5px;
  color: var(--ink-soft);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fn.dim {
  color: var(--muted);
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex: none;
}
.dot.good { background: var(--good); }
.dot.fair { background: var(--accent); }
.dot.weak { background: var(--muted); }

.score {
  font-family: var(--f-mono);
  font-size: 11px;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}

.count {
  margin-left: auto;
  font-size: 11px;
  color: var(--muted);
}

.list {
  max-height: 280px;
  overflow-y: auto;
  padding: 4px 8px 10px;
  border-top: 1px solid var(--line-soft);
}

.tip {
  margin: 8px 6px 10px;
  font-size: 11.5px;
  line-height: 1.7;
  color: var(--muted);
}
.tip.center {
  text-align: center;
  padding: 12px 0;
}

.item {
  display: flex;
  gap: 10px;
  align-items: flex-start;
  width: 100%;
  padding: 8px 8px;
  text-align: left;
  border-bottom: 1px solid var(--line-soft);
  border-radius: 0;
}
.item:last-of-type {
  border-bottom: 0;
}
.item:hover {
  background: var(--hover);
}
.item.on,
.item.on:hover {
  background: var(--accent-soft);
}
.item .dot {
  margin-top: 6px;
}

.meta {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  flex: 1;
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.tag {
  padding: 0 5px;
  font-size: 10px;
  color: var(--muted);
  background: var(--surface-2);
  border: 1px solid var(--line-soft);
  border-radius: 3px;
  font-variant-numeric: tabular-nums;
}
.item.on .tag {
  background: color-mix(in srgb, var(--surface) 60%, transparent);
}

.sample {
  font-family: var(--f-jp);
  font-size: 12px;
  color: var(--ink-soft);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.right {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 3px;
  flex: none;
}

.lock {
  color: var(--accent);
}

.release {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 100%;
  margin-top: 8px;
  padding: 6px;
  font-size: 11.5px;
  color: var(--muted);
  border-color: var(--line);
}
.release:hover {
  color: var(--ink);
  background: var(--hover);
}
</style>
