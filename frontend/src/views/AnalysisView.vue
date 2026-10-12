<template>
  <div class="analysis">
    <div class="search-row">
      <SearchBar @select="onSelect" />
    </div>

    <div v-if="loading" class="panel loading">分析中，请稍候…</div>
    <div v-if="error" class="error-box">{{ error }}</div>

    <template v-if="d">
      <div class="snapshot panel">
        <span class="sname">{{ d.name }}</span>
        <span class="scode dim">{{ d.code }}.{{ d.has_position ? '持仓中' : '未持仓' }}</span>
        <span class="sclose" :class="d.snapshot.pct_chg >= 0 ? 'up' : 'down'">
          {{ d.snapshot.close.toFixed(2) }}
          <small>{{ d.snapshot.pct_chg >= 0 ? '+' : '' }}{{ d.snapshot.pct_chg.toFixed(2) }}%</small>
        </span>
        <span class="dim sdate">{{ d.snapshot.date }} 收盘 · 换手 {{ d.snapshot.turnover }}%</span>
      </div>

      <div class="grid-main">
        <KLineChart :data="d" :trades="codeTrades" />
        <ScoreCard :score="d.score" :valuation="d.valuation" :financials="d.financials" />
      </div>

      <div class="grid-bottom">
        <AdvicePanel :advice="d.advice" :close="d.snapshot.close" @trade="onTrade" />
        <ForecastPanel :forecast="d.forecast" />
      </div>

      <SessionPanel v-if="session" :report="session" style="margin-top: 12px" />

      <AiAdvicePanel
        :advice="aiAdvice"
        :loading="aiLoading"
        :code="d?.code"
        style="margin-top: 12px"
        @refresh="loadAi"
      />

      <TradeModal
        v-if="trade.show"
        :code="d.code"
        :name="d.name"
        :cost="d.position_cost"
        :shares="d.position_shares"
        :close="d.snapshot.close"
        :side="trade.side"
        :price0="trade.price"
        :shares0="trade.shares"
        @close="trade.show = false"
        @done="bumpTrades"
      />
    </template>

    <div v-else-if="!loading && !error" class="panel loading">
      在上方搜索框输入股票代码或名称开始分析
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, watch, onMounted } from 'vue'
import { api } from '../api'
import { store, bumpTrades } from '../store'
import SearchBar from '../components/SearchBar.vue'
import KLineChart from '../components/KLineChart.vue'
import ScoreCard from '../components/ScoreCard.vue'
import AdvicePanel from '../components/AdvicePanel.vue'
import ForecastPanel from '../components/ForecastPanel.vue'
import SessionPanel from '../components/SessionPanel.vue'
import AiAdvicePanel from '../components/AiAdvicePanel.vue'
import TradeModal from '../components/TradeModal.vue'

const d = ref(null)
const loading = ref(false)
const error = ref('')
const session = ref(null)
const aiAdvice = ref(null)
const aiLoading = ref(false)
const trade = reactive({ show: false, side: 'buy', price: null, shares: null })
const codeTrades = ref([])
let currentCode = null

// 该股历史交易（用于K线买卖点标注）
async function loadTrades(code) {
  try {
    codeTrades.value = await api.trades(code)
  } catch {
    codeTrades.value = []
  }
}

// 当日盘面分析
async function loadSession(code) {
  try {
    session.value = await api.session(code)
  } catch {
    session.value = null
  }
}

// AI 分析建议：不自动触发，等用户在面板点击按钮后再生成
async function loadAi() {
  if (!currentCode || aiLoading.value) return
  aiLoading.value = true
  try {
    aiAdvice.value = await api.ai(currentCode, store.risk)
  } catch (e) {
    aiAdvice.value = {
      configured: true,
      model: '',
      content: `⚠️ ${e.message}`,
      generated_at: '',
      cached: false,
    }
  } finally {
    aiLoading.value = false
  }
}

// 打开交易弹窗：按建议预填补仓价位/股数，其余默认按最新价
function onTrade(side) {
  const a = d.value.advice
  trade.side = side
  trade.price = null
  trade.shares = null
  if (side === 'buy' && a.add_math) {
    trade.price = a.add_math.add_price
    trade.shares = a.add_math.add_shares
  }
  trade.show = true
}

async function load(code) {
  currentCode = code
  loading.value = true
  error.value = ''
  aiAdvice.value = null // 换股/刷新后清空旧分析，等用户点击按钮再生成
  try {
    d.value = await api.analysis(code, store.risk)
    loadTrades(code)
    loadSession(code)
  } catch (e) {
    d.value = null
    error.value = `分析失败：${e.message}`
  } finally {
    loading.value = false
  }
}

function onSelect(hit) {
  store.analysisCode = hit.code
  load(hit.code)
}

onMounted(() => {
  load(store.analysisCode || '600519')
})

watch(
  () => store.risk,
  () => {
    if (currentCode) load(currentCode)
  }
)
watch(
  () => store.tradeVersion,
  () => {
    if (currentCode) {
      load(currentCode)
      loadTrades(currentCode)
    }
  }
)
watch(
  () => store.analysisCode,
  (code) => {
    if (code && code !== currentCode) load(code)
  }
)
</script>

<style scoped>
.search-row { max-width: 520px; margin-bottom: 12px; }
.loading { text-align: center; color: var(--text-dim); padding: 40px; }
.snapshot {
  display: flex;
  align-items: baseline;
  gap: 12px;
  padding: 12px 16px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}
.sname { font-size: 18px; font-weight: 700; }
.sclose { font-size: 20px; font-weight: 700; font-family: Menlo, monospace; }
.sclose small { font-size: 13px; }
.grid-main {
  display: grid;
  grid-template-columns: minmax(0, 2.6fr) minmax(300px, 1fr);
  gap: 12px;
  margin-bottom: 12px;
}
.grid-bottom {
  display: grid;
  grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr);
  gap: 12px;
}
@media (max-width: 1100px) {
  .grid-main, .grid-bottom { grid-template-columns: 1fr; }
}
</style>
