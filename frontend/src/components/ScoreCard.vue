<template>
  <div class="panel score-card">
    <h3>综合评分</h3>
    <div ref="gaugeEl" class="gauge"></div>
    <div class="verdict">
      <span class="big" :class="cls">{{ score.rating }}</span>
      <span class="dim">{{ ratingText }}</span>
    </div>
    <div class="breakdown">
      <span>技术面 <b>{{ fmt1(score.tech_total) }}</b></span>
      <span :class="score.fundamental_adj > 0 ? 'up' : score.fundamental_adj < 0 ? 'down' : 'dim'">
        基本面 {{ score.fundamental_adj > 0 ? '+' : '' }}{{ fmt1(score.fundamental_adj) }}
      </span>
      <span class="dim">→ 综合 {{ fmt1(score.total) }}</span>
    </div>
    <div v-if="valuation" class="val-row">
      <div class="val-item"><span class="dim">市盈率</span><b>{{ peText }}</b></div>
      <div class="val-item"><span class="dim">市净率</span><b>{{ valuation.pb ?? '—' }}</b></div>
      <div class="val-item"><span class="dim">总市值</span><b>{{ valuation.market_cap ?? '—' }} 亿</b></div>
    </div>
    <div v-if="financials" class="val-row">
      <div class="val-item"><span class="dim">营收增速</span><b :class="clsPct(financials.revenue_yoy)">{{ pct(financials.revenue_yoy) }}</b></div>
      <div class="val-item"><span class="dim">净利增速</span><b :class="clsPct(financials.profit_yoy)">{{ pct(financials.profit_yoy) }}</b></div>
      <div class="val-item"><span class="dim">ROE</span><b>{{ num1(financials.roe) }}%</b></div>
      <div class="val-item"><span class="dim">毛利率</span><b>{{ num1(financials.gross_margin) }}%</b></div>
    </div>
    <div v-if="financials?.report_date" class="dim report-date">财报报告期：{{ financials.report_date }} · 东财 F10</div>
    <div v-if="score.fundamental_notes?.length" class="fund-notes dim">
      <div v-for="(n, i) in score.fundamental_notes" :key="i">· {{ n }}</div>
    </div>
    <ul class="factors">
      <li v-for="f in score.factors" :key="f.name">
        <div class="row">
          <span class="fname">{{ f.name }}</span>
          <span class="dim w">权重 {{ (f.weight * 100).toFixed(0) }}%</span>
          <span :class="['fscore', f.score > 0 ? 'up' : f.score < 0 ? 'down' : 'dim']">
            {{ f.score > 0 ? '+' : '' }}{{ f.score.toFixed(0) }}
          </span>
        </div>
        <div class="bar">
          <div class="bar-mid"></div>
          <div
            class="bar-fill"
            :style="barStyle(f.score)"
          ></div>
        </div>
        <div class="reasons dim">
          <div v-for="(r, i) in f.reasons" :key="i">· {{ r }}</div>
        </div>
      </li>
    </ul>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onBeforeUnmount, watch } from 'vue'
import * as echarts from 'echarts'

const props = defineProps({
  score: { type: Object, required: true },
  valuation: { type: Object, default: null },
  financials: { type: Object, default: null },
})
const gaugeEl = ref(null)
let chart = null

const UP = '#ee3b3b'
const DOWN = '#12b76a'

const cls = computed(() => {
  const l = props.score.rating_level
  if (l >= 2) return 'strong-bull badge'
  if (l === 1) return 'bull badge'
  if (l === 0) return 'neutral badge'
  if (l === -1) return 'bear badge'
  return 'strong-bear badge'
})

const ratingText = computed(() => {
  const t = props.score.total
  return `综合分 ${t.toFixed(1)}（区间 -100 ~ +100）`
})

const peText = computed(() => {
  const pe = props.valuation?.pe
  if (pe == null) return '—'
  return pe < 0 ? '亏损' : String(pe)
})

function fmt1(v) {
  return (v ?? 0).toFixed(1)
}

function pct(v) {
  if (v == null) return '—'
  return `${v > 0 ? '+' : ''}${v.toFixed(1)}%`
}
function clsPct(v) {
  if (v > 0) return 'up'
  if (v < 0) return 'down'
  return 'dim'
}
function num1(v) {
  return v == null ? '—' : v.toFixed(1)
}

function barStyle(s) {
  const half = 50
  const w = (Math.abs(s) / 100) * half
  if (s >= 0) {
    return { left: '50%', width: `${w}%`, background: UP }
  }
  return { left: `${50 - w}%`, width: `${w}%`, background: DOWN }
}

function renderGauge() {
  if (!chart) return
  const t = props.score.total
  chart.setOption({
    series: [
      {
        type: 'gauge',
        min: -100,
        max: 100,
        startAngle: 210,
        endAngle: -30,
        radius: '105%',
        center: ['50%', '62%'],
        axisLine: {
          lineStyle: {
            width: 10,
            color: [
              [0.3, DOWN],
              [0.425, '#7cc9a4'],
              [0.575, '#566173'],
              [0.7, '#f0883e'],
              [1, UP],
            ],
          },
        },
        pointer: { length: '60%', width: 4, itemStyle: { color: 'auto' } },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: { show: false },
        detail: { show: false },
        data: [{ value: t }],
      },
    ],
  })
}

onMounted(() => {
  chart = echarts.init(gaugeEl.value)
  renderGauge()
})
onBeforeUnmount(() => chart && chart.dispose())
watch(() => props.score, renderGauge)
</script>

<style scoped>
.score-card { display: flex; flex-direction: column; }
.gauge { height: 130px; }
.verdict { text-align: center; margin-bottom: 8px; }
.big { font-size: 18px; font-weight: 700; margin-right: 8px; }
.breakdown {
  display: flex; justify-content: center; gap: 10px;
  font-size: 12px; margin-bottom: 8px;
  padding-bottom: 8px; border-bottom: 1px dashed var(--border);
}
.breakdown b { font-family: Menlo, monospace; }
.val-row { display: flex; gap: 8px; margin-bottom: 6px; }
.val-item {
  flex: 1; text-align: center;
  background: var(--panel-2); border: 1px solid var(--border); border-radius: 6px;
  padding: 5px 4px; font-size: 12px;
}
.val-item span { display: block; font-size: 11px; margin-bottom: 2px; }
.val-item b { font-family: Menlo, monospace; }
.fund-notes { font-size: 12px; line-height: 1.6; margin-bottom: 4px; }
.report-date { font-size: 11px; margin-bottom: 4px; }
.factors { list-style: none; margin: 0; padding: 0; }
.factors li { padding: 8px 0; border-top: 1px solid var(--border); }
.row { display: flex; align-items: baseline; gap: 8px; }
.fname { font-weight: 600; }
.w { font-size: 11px; }
.fscore { margin-left: auto; font-family: Menlo, monospace; font-weight: 700; }
.bar {
  position: relative;
  height: 5px;
  background: #232b3d;
  border-radius: 3px;
  margin: 6px 0;
}
.bar-mid { position: absolute; left: 50%; top: -2px; bottom: -2px; width: 1px; background: #3a4457; }
.bar-fill { position: absolute; top: 0; bottom: 0; border-radius: 3px; }
.reasons { font-size: 12px; line-height: 1.6; }
</style>
