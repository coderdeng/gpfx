<template>
  <div class="portfolio">
    <div class="summary-row">
      <div class="panel summary">
        <div class="label dim">总市值</div>
        <div class="value">{{ fmt(p?.total_value) }}</div>
      </div>
      <div class="panel summary">
        <div class="label dim">总成本</div>
        <div class="value">{{ fmt(p?.total_cost) }}</div>
      </div>
      <div class="panel summary">
        <div class="label dim">浮动盈亏</div>
        <div class="value" :class="cls(p?.total_profit)">{{ fmt(p?.total_profit) }}</div>
      </div>
      <div class="panel summary">
        <div class="label dim">收益率</div>
        <div class="value" :class="cls(p?.total_profit_pct)">
          {{ pct(p?.total_profit_pct) }}
        </div>
      </div>
      <div class="panel summary">
        <div class="label dim">已实现盈亏</div>
        <div class="value" :class="cls(p?.total_realized)">{{ fmt(p?.total_realized) }}</div>
      </div>
      <div class="panel summary actions">
        <button @click="showTrades = true">历史交易复盘</button>
        <button class="primary" @click="form.open = !form.open">
          {{ form.open ? '收起' : '+ 添加持仓' }}
        </button>
      </div>
    </div>

    <div v-if="form.open" class="panel form">
      <div class="frow">
        <SearchBar placeholder="选择股票（代码/名称/拼音）" @select="onPick" />
      </div>
      <div class="frow inline">
        <label>股票 <input v-model="form.code" disabled /></label>
        <label>成本价 <input v-model.number="form.cost" type="number" step="0.01" /></label>
        <label>股数 <input v-model.number="form.shares" type="number" step="100" /></label>
        <button class="primary" :disabled="!canSave" @click="save">保存</button>
      </div>
      <div v-if="form.error" class="error-box">{{ form.error }}</div>
    </div>

    <div v-if="loading" class="panel loading">加载持仓组合中…</div>
    <div v-if="error" class="error-box">{{ error }}</div>

    <div v-if="p" class="panel">
      <div class="table-wrap">
        <table>
        <thead>
          <tr>
            <th>股票</th><th>成本</th><th>股数</th><th>现价</th>
            <th>市值</th><th>盈亏</th><th>收益率</th><th>占比</th>
            <th>评级</th><th>建议</th><th>止损位</th><th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in p.rows" :key="r.position.id">
            <td>
              <div class="pname">{{ r.position.name }}</div>
              <div class="dim pcode">{{ r.position.code }}</div>
            </td>
            <td>{{ r.position.cost.toFixed(2) }}</td>
            <td>{{ r.position.shares }}</td>
            <td>{{ r.close.toFixed(2) }}</td>
            <td>{{ fmt(r.market_value) }}</td>
            <td :class="cls(r.profit)">{{ fmt(r.profit) }}</td>
            <td :class="cls(r.profit_pct)">{{ pct(r.profit_pct) }}</td>
            <td class="dim">{{ r.weight_pct.toFixed(1) }}%</td>
            <td><span class="badge" :class="ratingCls(r.score.rating_level)">{{ r.score.rating }}</span></td>
            <td>
              <div class="action-line">
                <span class="badge" :class="actionCls(r.action)">{{ r.action }}</span>
                <div class="dim small">{{ r.headline }}</div>
              </div>
            </td>
            <td>{{ r.stop_loss ?? '—' }}</td>
            <td class="ops">
              <button class="small" @click="openAnalysis(r.position.code)">分析</button>
              <button class="small up-btn" @click="openTrade(r, 'buy')">买</button>
              <button class="small down-btn" @click="openTrade(r, 'sell')">卖</button>
              <button
                class="small danger"
                @click="remove(r.position)"
              >{{ pendingDelete === r.position.id ? '确认删除？' : '删除' }}</button>
            </td>
          </tr>
          <tr v-if="!p.rows.length">
            <td colspan="12" class="dim" style="text-align:center; padding: 30px">
              暂无持仓，点击右上角「+ 添加持仓」录入成本与股数
            </td>
          </tr>
        </tbody>
      </table>
      </div>
      <div v-for="w in p.warnings" :key="w" class="dim small warn-line">⚠ {{ w }}</div>
      <div class="dim small" style="margin-top: 8px">生成时间：{{ p.generated_at }} · 建议基于当前风险风格自动生成，仅供参考</div>
    </div>

    <TradeModal
      v-if="trade.row"
      :code="trade.row.position.code"
      :name="trade.row.position.name"
      :cost="trade.row.position.cost"
      :shares="trade.row.position.shares"
      :close="trade.row.close"
      :side="trade.side"
      @close="trade.row = null"
      @done="bumpTrades"
    />
    <TradesModal v-if="showTrades" @close="showTrades = false" />
  </div>
</template>

<script setup>
import { ref, reactive, computed, watch, onMounted } from 'vue'
import { api } from '../api'
import { store, bumpTrades } from '../store'
import SearchBar from '../components/SearchBar.vue'
import TradeModal from '../components/TradeModal.vue'
import TradesModal from '../components/TradesModal.vue'

const p = ref(null)
const loading = ref(false)
const error = ref('')
const form = reactive({ open: false, code: '', name: '', cost: null, shares: null, error: '' })
const trade = reactive({ row: null, side: 'buy' })
const showTrades = ref(false)

function openTrade(row, side) {
  trade.side = side
  trade.row = row
}

const canSave = computed(
  () => form.code && form.cost > 0 && form.shares > 0
)

async function load() {
  loading.value = true
  error.value = ''
  try {
    p.value = await api.portfolio(store.risk)
  } catch (e) {
    error.value = `加载失败：${e.message}`
  } finally {
    loading.value = false
  }
}
onMounted(load)
watch(() => store.risk, load)
watch(() => store.tradeVersion, load)

function onPick(hit) {
  form.code = hit.code
  form.name = hit.name
}

async function save() {
  form.error = ''
  try {
    await api.addPosition('/api/positions', {
      code: form.code,
      name: form.name || form.code,
      cost: form.cost,
      shares: form.shares,
    })
    form.open = false
    form.code = ''
    form.cost = null
    form.shares = null
    load()
  } catch (e) {
    form.error = e.message
  }
}

const pendingDelete = ref(null)

async function remove(pos) {
  // 两段式确认：第一次点击进入待确认态，再点一次才真正删除
  if (pendingDelete.value !== pos.id) {
    pendingDelete.value = pos.id
    setTimeout(() => {
      if (pendingDelete.value === pos.id) pendingDelete.value = null
    }, 4000)
    return
  }
  pendingDelete.value = null
  try {
    await api.deletePosition(pos.id)
    load()
  } catch (e) {
    error.value = e.message
  }
}

function openAnalysis(code) {
  store.analysisCode = code
  store.view = 'analysis'
}

function fmt(v) {
  if (v == null) return '—'
  return v.toLocaleString('zh-CN', { maximumFractionDigits: 0 })
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
function ratingCls(l) {
  if (l >= 2) return 'strong-bull'
  if (l === 1) return 'bull'
  if (l === 0) return 'neutral'
  if (l === -1) return 'bear'
  return 'strong-bear'
}
function actionCls(a) {
  if (/止损|离场|回避/.test(a)) return 'strong-bear'
  if (/减仓/.test(a)) return 'bear'
  if (/加仓|补仓|建仓|持有/.test(a)) return 'bull'
  return 'neutral'
}
</script>

<style scoped>
.summary-row { display: flex; gap: 12px; margin-bottom: 12px; flex-wrap: wrap; }
.summary { flex: 1; min-width: 140px; }
.summary .label { font-size: 12px; margin-bottom: 4px; }
.summary .value { font-size: 20px; font-weight: 700; font-family: Menlo, monospace; }
.summary.actions { display: flex; align-items: center; justify-content: center; gap: 8px; flex-wrap: wrap; }
.summary.actions button { white-space: nowrap; }
.form { margin-bottom: 12px; }
.frow { margin-bottom: 10px; }
.frow.inline { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }
.frow.inline label { display: flex; align-items: center; gap: 6px; color: var(--text-dim); }
.frow.inline input { width: 110px; }
.pname { font-weight: 600; }
.pcode { font-size: 11px; font-family: Menlo, monospace; }
.action-line .small { font-size: 11px; line-height: 1.4; max-width: 260px; white-space: normal; display: block; }
.ops { display: flex; gap: 6px; }
button.small { padding: 3px 8px; font-size: 12px; }
.up-btn { color: var(--up); border-color: rgba(238,59,59,0.4); }
.down-btn { color: var(--down); border-color: rgba(18,183,106,0.4); }
.warn-line { margin-top: 4px; }
</style>
