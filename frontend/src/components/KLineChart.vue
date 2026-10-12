<template>
  <div class="panel kline-wrap">
    <div class="head">
      <h3>日K线（前复权）</h3>
      <div class="tabs">
        <button
          v-for="t in subTabs"
          :key="t.key"
          :class="{ primary: sub === t.key }"
          @click="sub = t.key"
        >{{ t.label }}</button>
      </div>
    </div>
    <div ref="el" class="kline"></div>
  </div>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
import * as echarts from 'echarts'

const props = defineProps({
  data: { type: Object, required: true },
  // 该股历史交易（用于在K线上标注买卖点）
  trades: { type: Array, default: () => [] },
})
const el = ref(null)
const sub = ref('rsi')
const subTabs = [
  { key: 'rsi', label: 'RSI' },
  { key: 'kdj', label: 'KDJ' },
]

let chart = null
const UP = '#ee3b3b'
const DOWN = '#12b76a'
const DIM = '#8b96a8'

function optSeries(name, data, color, xi, yi, width = 1) {
  return {
    name, type: 'line', data, symbol: 'none', xAxisIndex: xi, yAxisIndex: yi,
    lineStyle: { width, color }, color, emphasis: { disabled: true },
  }
}

// ---------- 悬浮提示格式化（价格 2 位 / 指标 2 位 / RSI·KDJ 1 位 / 量带单位） ----------
const MA_COLORS = { MA5: '#f5a623', MA10: '#4aa8ff', MA20: '#c678dd', MA60: '#8bc34a' }

function isMobile() {
  return typeof window !== 'undefined' && window.innerWidth <= 760
}

// 小屏：浮层固定贴图表右上角，彻底避免左右溢出被屏幕裁切
function mobileTooltipPos(point, params, dom, rect, size) {
  const viewW = size?.viewSize?.width ?? window.innerWidth
  const tipW = size?.contentSize?.width ?? dom.offsetWidth
  return { top: 4, left: Math.max(4, viewW - tipW - 4) }
}

function fmt2(v) {
  return v == null || Number.isNaN(v) ? '—' : Number(v).toFixed(2)
}
function fmtVol(v) {
  if (v == null || Number.isNaN(v)) return '—'
  return v >= 1e8 ? (v / 1e8).toFixed(2) + '亿手'
       : v >= 1e4 ? (v / 1e4).toFixed(2) + '万手'
       : v.toFixed(0) + '手'
}
function makeTooltip(bars, trades) {
  return (params) => {
    if (!params || !params.length) return ''
    const byName = {}
    for (const p of params) byName[p.seriesName] = p.value
    const i = params[0].dataIndex
    const bar = bars[i]
    const lines = [params[0].axisValue]

    // 蜡烛图数值直接取后端数据（ECharts 的 params.value 首位带 dataIndex，不可直接解构）
    if (bar) {
      const up = bar.close >= bar.open
      const cc = up ? UP : DOWN
      lines.push(
        `开 ${fmt2(bar.open)}　收 <b style="color:${cc}">${fmt2(bar.close)}</b>`,
        `高 ${fmt2(bar.high)}　低 ${fmt2(bar.low)}`,
        `涨跌幅 <b style="color:${cc}">${bar.pct_chg ? bar.pct_chg.toFixed(2) : '—'}%</b>`
      )
      // 当日成交量 + 量比（前5日均量口径，不含当日）
      lines.push(`成交量 ${fmtVol(bar.volume)}`)
      if (i >= 5) {
        const base = bars.slice(i - 5, i).reduce((s, b) => s + b.volume, 0) / 5
        if (base > 0) {
          const r = bar.volume / base
          let [label, color] = r >= 3 ? ['剧烈放量', UP]
            : r >= 1.5 ? ['放量', UP]
            : r >= 0.8 ? ['平量', DIM]
            : r >= 0.5 ? ['缩量', DOWN]
            : ['显著缩量', DOWN]
          lines.push(`量比 <b style="color:${color}">${r.toFixed(2)}（${label}）</b>`)
        }
      }
      // 当日的历史交易（复盘标注）
      for (const t of trades || []) {
        if ((t.created || '').slice(0, 10) !== bar.date) continue
        if (t.side === 'buy') {
          lines.push(`<span style="color:${UP}">▣ 买入 ${t.shares}股 @ ${fmt2(t.price)}</span>${t.note ? ' · ' + t.note : ''}`)
        } else {
          const pnl = t.realized_pnl != null ? `，盈亏 ${t.realized_pnl >= 0 ? '+' : ''}${t.realized_pnl.toFixed(0)}` : ''
          lines.push(`<span style="color:${DOWN}">▣ 卖出 ${t.shares}股 @ ${fmt2(t.price)}${pnl}</span>${t.note ? ' · ' + t.note : ''}`)
        }
      }
    }
    const ma = Object.keys(MA_COLORS)
      .filter((n) => byName[n] != null)
      .map((n) => `<span style="color:${MA_COLORS[n]}">${n} ${fmt2(byName[n])}</span>`)
      .join('　')
    if (ma) lines.push(ma)
    if (bar && bar.amount) lines.push(`成交额 ${(bar.amount / 1e8).toFixed(2)} 亿元`)
    if (bar && bar.turnover) lines.push(`换手率 ${Number(bar.turnover).toFixed(2)}%`)

    const macd = ['DIF', 'DEA', 'MACD柱']
      .filter((n) => byName[n] != null)
      .map((n) => `${n === 'MACD柱' ? '柱' : n} ${fmt2(byName[n])}`)
      .join('　')
    if (macd) lines.push(macd)

    const osc = ['RSI6', 'RSI12', 'RSI24', 'K', 'D', 'J']
      .filter((n) => byName[n] != null)
      .map((n) => `${n} ${Number(byName[n]).toFixed(1)}`)
      .join('　')
    if (osc) lines.push(osc)
    return lines.join('<br/>')
  }
}

function buildOption(d) {
  const { bars, indicators: ind, forecast: fc, advice } = d
  const dates = bars.map((b) => b.date)
  const kdata = bars.map((b) => [b.open, b.close, b.low, b.high])
  const vols = bars.map((b) => ({
    value: b.volume,
    itemStyle: { color: b.pct_chg >= 0 ? UP : DOWN },
  }))

  const maDefs = [
    ['ma5', '#f5a623'], ['ma10', '#4aa8ff'],
    ['ma20', '#c678dd'], ['ma60', '#8bc34a'],
  ]

  // 主图标注：支撑 / 压力 / 成本 / 止损
  const markData = []
  for (const p of fc.supports || []) {
    markData.push({
      yAxis: p.price,
      label: { formatter: `支撑 ${p.price}`, color: DOWN },
      lineStyle: { color: DOWN, type: 'dashed', opacity: 0.7 },
    })
  }
  for (const p of fc.resistances || []) {
    markData.push({
      yAxis: p.price,
      label: { formatter: `压力 ${p.price}`, color: '#f0883e' },
      lineStyle: { color: '#f0883e', type: 'dashed', opacity: 0.7 },
    })
  }
  if (d.position_cost) {
    markData.push({
      yAxis: d.position_cost,
      label: { formatter: `成本 ${d.position_cost}`, color: '#f5d76e' },
      lineStyle: { color: '#f5d76e', type: 'dotted', width: 2 },
    })
  }
  if (advice?.stop_loss) {
    markData.push({
      yAxis: advice.stop_loss,
      label: { formatter: `止损 ${advice.stop_loss}`, color: '#ff6b6b' },
      lineStyle: { color: '#ff6b6b', type: 'dashed', width: 2 },
    })
  }

  // 副图2：RSI 或 KDJ
  const oscSeries = sub.value === 'rsi'
    ? [
        optSeries('RSI6', ind.rsi6, '#ee3b3b', 3, 3),
        optSeries('RSI12', ind.rsi12, '#f5d76e', 3, 3),
        optSeries('RSI24', ind.rsi24, '#4aa8ff', 3, 3),
      ]
    : [
        optSeries('K', ind.kdj_k, '#ee3b3b', 3, 3),
        optSeries('D', ind.kdj_d, '#4aa8ff', 3, 3),
        optSeries('J', ind.kdj_j, '#c678dd', 3, 3),
      ]

  const startPct = Math.max(0, 100 - (150 / bars.length) * 100)
  const markers = tradeMarkers(bars, props.trades)

  return {
    animation: false,
    backgroundColor: 'transparent',
    axisPointer: { link: [{ xAxisIndex: 'all' }], label: { backgroundColor: '#263043' } },
    tooltip: {
      trigger: 'axis',
      backgroundColor: 'rgba(22,27,38,0.95)',
      borderColor: '#263043',
      textStyle: { color: '#d7dde8', fontSize: 12 },
      confine: true, // 浮层限制在图表容器内，手机窄屏不被屏幕裁掉
      position: isMobile() ? mobileTooltipPos : undefined,
      maxWidth: '92%', // 内容过宽时换行，不撑出容器
      extraCssText: 'white-space:normal;word-break:break-all;',
      formatter: makeTooltip(bars, props.trades),
    },
    legend: {
      top: 2, left: 'center', icon: 'rect', itemWidth: 12, itemHeight: 3,
      textStyle: { color: DIM, fontSize: 11 },
      data: ['MA5', 'MA10', 'MA20', 'MA60', 'DIF', 'DEA'],
    },
    grid: [
      { left: 64, right: 76, top: 26, height: '41%' },
      { left: 64, right: 76, top: '56%', height: '11%' },
      { left: 64, right: 76, top: '71%', height: '11%' },
      { left: 64, right: 76, top: '86%', height: '11%' },
    ],
    xAxis: [0, 1, 2, 3].map((i) => ({
      type: 'category',
      data: dates,
      gridIndex: i,
      boundary: true,
      axisLine: { lineStyle: { color: '#263043' } },
      axisTick: { show: false },
      axisLabel: { show: i === 3, color: DIM, fontSize: 10 },
      splitLine: { show: false },
    })),
    yAxis: [
      {
        scale: true, gridIndex: 0, position: 'right',
        axisLabel: { color: DIM, fontSize: 10 },
        splitLine: { lineStyle: { color: '#1b2430' } },
      },
      {
        gridIndex: 1, position: 'right', splitNumber: 2,
        axisLabel: { color: DIM, fontSize: 10, formatter: (v) => (v >= 10000 ? v / 10000 + '万' : v) },
        splitLine: { show: false },
      },
      {
        gridIndex: 2, position: 'right', splitNumber: 2,
        axisLabel: { color: DIM, fontSize: 10 },
        splitLine: { show: false },
      },
      {
        gridIndex: 3, position: 'right', splitNumber: 2,
        axisLabel: { color: DIM, fontSize: 10 },
        splitLine: { show: false },
      },
    ],
    dataZoom: [
      { type: 'inside', xAxisIndex: [0, 1, 2, 3], start: startPct, end: 100 },
      {
        type: 'slider', xAxisIndex: [0, 1, 2, 3], start: startPct, end: 100,
        bottom: 2, height: 14, borderColor: '#263043',
        backgroundColor: '#161b26', fillerColor: 'rgba(74,168,255,0.15)',
        handleStyle: { color: '#4aa8ff' },
        textStyle: { color: DIM, fontSize: 10 },
      },
    ],
    series: [
      {
        name: 'K线',
        type: 'candlestick',
        data: kdata,
        xAxisIndex: 0,
        yAxisIndex: 0,
        itemStyle: { color: UP, color0: DOWN, borderColor: UP, borderColor0: DOWN },
        markLine: markData.length
          ? {
              symbol: 'none', silent: true,
              data: markData,
              label: { position: 'insideEndTop', fontSize: 10 },
            }
          : undefined,
      },
      ...maDefs.map(([key, color]) =>
        optSeries(key.toUpperCase(), ind[key], color, 0, 0)),
      {
        name: '成交量', type: 'bar', data: vols, xAxisIndex: 1, yAxisIndex: 1,
      },
      {
        name: 'MACD柱', type: 'bar', data: ind.hist, xAxisIndex: 2, yAxisIndex: 2,
        itemStyle: {
          color: (p) => (p.value >= 0 ? UP : DOWN),
        },
      },
      optSeries('DIF', ind.dif, '#f5d76e', 2, 2),
      optSeries('DEA', ind.dea, '#4aa8ff', 2, 2),
      ...oscSeries,
      ...(markers.length
        ? [{
            name: '交易点',
            type: 'scatter',
            xAxisIndex: 0,
            yAxisIndex: 0,
            data: markers,
            z: 12,
            cursor: 'pointer',
          }]
        : []),
    ],
  }
}

// 历史交易 → K线上的 B/S 标记点
function tradeMarkers(bars, trades) {
  if (!trades || !trades.length) return []
  const dateIdx = new Map(bars.map((b, i) => [b.date, i]))
  const out = []
  for (const t of trades) {
    const d = (t.created || '').slice(0, 10)
    let i = dateIdx.get(d)
    if (i == null) {
      // 非交易日下单：映射到该日期前最后一个交易日
      i = -1
      for (let k = bars.length - 1; k >= 0; k--) {
        if (bars[k].date <= d) { i = k; break }
      }
    }
    if (i < 0) continue
    const bar = bars[i]
    const buy = t.side === 'buy'
    out.push({
      value: [i, buy ? bar.low * 0.997 : bar.high * 1.003],
      symbol: 'triangle',
      symbolRotate: buy ? 0 : 180,
      symbolSize: 11,
      itemStyle: { color: buy ? UP : DOWN },
      label: {
        show: true,
        position: buy ? 'bottom' : 'top',
        formatter: buy ? 'B' : 'S',
        color: buy ? UP : DOWN,
        fontSize: 10,
        fontWeight: 'bold',
      },
    })
  }
  return out
}

function render() {
  if (chart && props.data?.bars?.length) {
    chart.setOption(buildOption(props.data), { notMerge: true })
  }
}

onMounted(() => {
  chart = echarts.init(el.value)
  render()
  window.addEventListener('resize', onResize)
  // 调试/自动化验证用：暴露图表实例
  window.__kline = chart
})
onBeforeUnmount(() => {
  window.removeEventListener('resize', onResize)
  if (chart) chart.dispose()
})
function onResize() {
  chart && chart.resize()
}
watch(() => props.data, render, { deep: false })
watch(() => props.trades, render, { deep: false })
watch(sub, render)
</script>

<style scoped>
.kline-wrap { padding-bottom: 8px; }
.head { display: flex; align-items: center; justify-content: space-between; }
.tabs { display: flex; gap: 6px; }
.tabs button { padding: 3px 10px; font-size: 12px; }
.kline { height: 640px; width: 100%; }

@media (max-width: 760px) {
  .kline { height: 460px; }
}
</style>
