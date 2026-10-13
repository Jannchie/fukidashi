<script setup lang="ts">
import type { Sentence, Translation } from '../composables/useFukidashi'

// 每句一个组件：流式译文每来一段只更新这一句，不牵动整个列表
defineProps<{
  s: Sentence
  tr?: Translation
  latest: boolean
  ruby: boolean
  showTr: boolean
}>()
</script>

<template>
  <div class="entry" :class="{ latest }">
    <p class="line">
      <template v-if="ruby && s.ruby">
        <template v-for="(seg, j) in s.ruby" :key="j">
          <ruby v-if="seg.ruby">{{ seg.text }}<rt>{{ seg.ruby }}</rt></ruby>
          <template v-else>{{ seg.text }}</template>
        </template>
      </template>
      <template v-else>{{ s.text }}</template>
    </p>
    <template v-if="showTr && tr">
      <p v-if="tr.text" class="tr" :class="{ streaming: tr.streaming }">{{ tr.text }}</p>
      <p v-else-if="tr.error" class="tr failed">{{ tr.error }}</p>
      <p v-else class="tr waiting">翻译中…</p>
    </template>
  </div>
</template>

<style scoped>
/* 字号全部从 --fs 按比例推出来，调一个值整体缩放。
   所有句子同一字号：新句到来时上一句不缩小，列表只做平移，滚动才顺 */
.entry {
  margin: 0 0 calc(var(--fs) * 0.8);
}

.line {
  margin: 0;
  font-family: var(--f-jp);
  font-size: calc(var(--fs) * 0.82);
  /* 留出振假名的位置，免得上下行挤在一起 */
  line-height: 2.05;
  color: var(--ink-soft);
  transition: color var(--t);
  /* 日文没有词间空格，靠这个避免标点落在行首 */
  line-break: strict;
  overflow-wrap: anywhere;
}

/* 振假名：小、弱、不参与选中复制（复制出来的应是原文） */
rt {
  font-size: 0.5em;
  color: var(--muted);
  letter-spacing: 0;
  user-select: none;
}

/* 最新一句只靠颜色突出，不改字号 */
.latest .line {
  color: var(--ink);
}
.latest rt {
  color: var(--ink-soft);
}

/* 译文跟在原文下面，始终比原文弱一级：读的是日文，译文只是兜底 */
.tr {
  margin: 2px 0 0;
  font-size: calc(var(--fs) * 0.66);
  line-height: 1.75;
  color: var(--muted);
  transition: color var(--t);
}
.latest .tr {
  color: var(--ink-soft);
}
/* 流式输出时末尾一个闪烁的光标，表示还没说完 */
.tr.streaming::after {
  content: '▍';
  margin-left: 1px;
  color: var(--accent);
  animation: blink 1s steps(2, start) infinite;
}
@keyframes blink {
  to { visibility: hidden; }
}
.tr.waiting {
  opacity: 0.6;
}
.tr.failed {
  font-size: 12px;
  color: var(--accent);
}
</style>
