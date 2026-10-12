<template>
  <div class="search" @keydown.esc="open = false">
    <input
      v-model="q"
      :placeholder="placeholder"
      @input="onInput"
      @focus="onInput"
      @keydown.down.prevent="move(1)"
      @keydown.up.prevent="move(-1)"
      @keydown.enter="pick(hits[active])"
    />
    <ul v-if="open && hits.length" class="dropdown">
      <li
        v-for="(h, i) in hits"
        :key="h.code"
        :class="{ active: i === active }"
        @mousedown.prevent="pick(h)"
        @mousemove="active = i"
      >
        <span class="code">{{ h.code }}</span>
        <span class="name">{{ h.name }}</span>
        <span class="market dim">{{ h.market }}</span>
      </li>
    </ul>
  </div>
</template>

<script setup>
import { ref } from 'vue'
import { api } from '../api'

const props = defineProps({ placeholder: { type: String, default: '搜索股票：代码 / 名称 / 拼音，如 600519、茅台、gzmt' } })
const emit = defineEmits(['select'])

const q = ref('')
const hits = ref([])
const open = ref(false)
const active = ref(0)
let timer = null

function onInput() {
  clearTimeout(timer)
  if (!q.value.trim()) {
    hits.value = []
    open.value = false
    return
  }
  timer = setTimeout(async () => {
    try {
      hits.value = await api.search(q.value.trim())
      active.value = 0
      open.value = true
    } catch { /* 静默失败 */ }
  }, 250)
}

function move(d) {
  if (!hits.value.length) return
  active.value = (active.value + d + hits.value.length) % hits.value.length
}

function pick(h) {
  if (!h) return
  emit('select', h)
  open.value = false
}
</script>

<style scoped>
.search { position: relative; width: 100%; }
input { width: 100%; font-size: 14px; padding: 9px 12px; }
.dropdown {
  position: absolute;
  z-index: 100;
  top: 100%;
  left: 0;
  right: 0;
  margin: 4px 0 0;
  padding: 4px;
  list-style: none;
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.45);
  max-height: 320px;
  overflow-y: auto;
}
li { display: flex; gap: 10px; padding: 8px 10px; border-radius: 6px; cursor: pointer; }
li.active { background: rgba(74,168,255,0.12); }
.code { color: var(--accent); font-family: Menlo, monospace; }
.market { margin-left: auto; font-size: 12px; }
</style>
