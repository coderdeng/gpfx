<template>
  <div class="watchlist">
    <div class="toolbar">
      <div class="search-row">
        <SearchBar placeholder="搜索并添加到观察列表（代码/名称/拼音）" @select="onPick" />
      </div>
      <button :disabled="loading" @click="load">{{ loading ? '刷新中…' : '↻ 刷新' }}</button>
    </div>

    <div v-if="error" class="error-box">{{ error }}</div>

    <div v-if="rows" class="panel">
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>股票</th><th>添加时间</th><th>基准价</th><th>最新价</th>
              <th>今日涨跌</th><th>自添加以来</th><th>添加后走势</th><th></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="r in rows" :key="r.code">
              <td class="clickable" title="查看个股分析" @click="openAnalysis(r.code)">
                <div class="pname">{{ r.name }}</div>
                <div class="dim pcode">{{ r.code }}</div>
              </td>
              <td class="dim">{{ r.added_at }}</td>
              <td>
                {{ r.base_close.toFixed(2) }}
                <div class="dim small">{{ r.base_date }}</div>
              </td>
              <td>
                {{ r.last_close.toFixed(2) }}
                <div class="dim small">{{ r.last_date }}</div>
              </td>
              <td :class="cls(r.pct_today)">{{ pct(r.pct_today) }}</td>
              <td :class="cls(r.pct_since_add)">
                <b>{{ pct(r.pct_since_add) }}</b>
                <div class="dim small">{{ (r.last_close - r.base_close).toFixed(2) }} 元</div>
              </td>
              <td>
                <svg
                  v-if="r.closes_since.length > 1"
                  class="spark"
                  viewBox="0 0 120 32"
                  preserveAspectRatio="none"
                >
                  <polyline
                    :points="sparkPts(r.closes_since)"
                    fill="none"
                    :stroke="r.pct_since_add >= 0 ? 'var(--up)' : 'var(--down)'"
                    stroke-width="1.5"
                  />
                </svg>
                <span v-else class="dim small">—</span>
              </td>
              <td class="ops">
                <button class="small" @click="openAnalysis(r.code)">分析</button>
                <button class="small danger" @click="remove(r)">
                  {{ pendingDelete === r.code ? '确认移除？' : '移除' }}
                </button>
              </td>
            </tr>
            <tr v-if="!rows.length">
              <td colspan="8" class="dim" style="text-align: center; padding: 30px">
                观察列表为空，在上方搜索框添加股票，系统会记录添加时的价格并跟踪此后的涨跌
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="dim small" style="margin-top: 8px">
        基准价 = 添加时最近收盘价（盘中添加则为当时最新价）· 自添加以来涨跌按前复权口径计算，仅供参考
      </div>
    </div>
    <div v-else-if="loading" class="panel loading">加载观察列表中…</div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api'
import { store } from '../store'
import SearchBar from '../components/SearchBar.vue'

const rows = ref(null)
const loading = ref(false)
const error = ref('')
const pendingDelete = ref(null)

async function load() {
  loading.value = true
  error.value = ''
  try {
    rows.value = await api.watchlist()
  } catch (e) {
    error.value = `加载失败：${e.message}`
  } finally {
    loading.value = false
  }
}
onMounted(load)

// 搜索选中即加入观察列表；重复添加等错误展示在页顶
async function onPick(hit) {
  error.value = ''
  try {
    await api.addWatch('/api/watchlist', { code: hit.code, name: hit.name })
    load()
  } catch (e) {
    error.value = e.message
  }
}

async function remove(r) {
  // 两段式确认：第一次点击进入待确认态，再点一次才真正移除
  if (pendingDelete.value !== r.code) {
    pendingDelete.value = r.code
    setTimeout(() => {
      if (pendingDelete.value === r.code) pendingDelete.value = null
    }, 4000)
    return
  }
  pendingDelete.value = null
  try {
    await api.removeWatch(r.code)
    load()
  } catch (e) {
    error.value = e.message
  }
}

function openAnalysis(code) {
  store.analysisCode = code
  store.view = 'analysis'
}

function pct(v) {
  if (v == null) return '—'
  return `${v > 0 ? '+' : ''}${v.toFixed(2)}%`
}
function cls(v) {
  if (v > 0) return 'up'
  if (v < 0) return 'down'
  return 'dim'
}

// 自添加以来的收盘迷你走势线（SVG polyline 坐标）
function sparkPts(closes) {
  const w = 120, h = 32, pad = 3
  const min = Math.min(...closes)
  const max = Math.max(...closes)
  const span = max - min || 1
  return closes
    .map((c, i) => {
      const x = pad + (i / (closes.length - 1)) * (w - 2 * pad)
      const y = h - pad - ((c - min) / span) * (h - 2 * pad)
      return `${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')
}
</script>

<style scoped>
.toolbar { display: flex; gap: 12px; align-items: flex-start; margin-bottom: 12px; }
.search-row { flex: 1; max-width: 520px; }
.clickable { cursor: pointer; }
.pname { font-weight: 600; }
.pcode { font-size: 11px; font-family: Menlo, monospace; }
.small { font-size: 11px; }
.ops { display: flex; gap: 6px; }
button.small { padding: 3px 8px; font-size: 12px; }
.spark { width: 120px; height: 32px; display: block; }
</style>
