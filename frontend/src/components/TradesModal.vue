<template>
  <div class="mask" @click.self="$emit('close')">
    <div class="modal panel">
      <h3>历史交易 · 操作复盘</h3>
      <div v-if="loading" class="dim">加载中…</div>
      <template v-else-if="trades.length">
        <div class="stats">
          <div class="stat">
            <div class="dim label">总笔数</div>
            <b>{{ trades.length }}</b>
            <span class="dim sub">买 {{ buys }} / 卖 {{ sells.length }}</span>
          </div>
          <div class="stat">
            <div class="dim label">卖出胜率</div>
            <b :class="stats.winRate >= 50 ? 'up' : 'down'">{{ stats.winRate }}%</b>
            <span class="dim sub">{{ stats.wins }} 赢 / {{ sells.length - stats.wins }} 亏</span>
          </div>
          <div class="stat">
            <div class="dim label">已实现盈亏</div>
            <b :class="cls(stats.realized)">{{ fmt(stats.realized) }}</b>
          </div>
          <div class="stat">
            <div class="dim label">最佳 / 最差一笔</div>
            <b>
              <span class="up">{{ fmt(stats.best) }}</span>
              /
              <span :class="cls(stats.worst)">{{ fmt(stats.worst) }}</span>
            </b>
          </div>
        </div>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>时间</th><th>股票</th><th>方向</th><th>价格</th><th>股数</th>
                <th>费用</th><th>净盈亏</th><th class="hide-sm">当时评分</th><th class="hide-sm">当时建议</th><th>对照</th><th class="hide-sm">备注</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="t in trades" :key="t.id">
                <td class="dim">{{ t.created }}</td>
                <td>{{ t.name }} <span class="dim small">{{ t.code }}</span></td>
                <td><span :class="t.side === 'buy' ? 'up' : 'down'">{{ t.side === 'buy' ? '买入' : '卖出' }}</span></td>
                <td>{{ t.price.toFixed(2) }}</td>
                <td>{{ t.shares }}</td>
                <td class="dim">{{ t.fee != null ? t.fee.toFixed(2) : '—' }}</td>
                <td :class="t.realized_pnl == null ? 'dim' : cls(t.realized_pnl)">
                  {{ t.realized_pnl == null ? '—' : fmt(t.realized_pnl) }}
                </td>
                <td v-if="t.context" class="hide-sm" :class="t.context.score >= 0 ? 'up' : 'down'">
                  {{ t.context.score.toFixed(0) }} <span class="small">{{ t.context.rating }}</span>
                </td>
                <td v-else class="dim hide-sm">—</td>
                <td class="small hide-sm">{{ t.context?.advice_action || '—' }}</td>
                <td><span class="badge small-badge" :class="vsAdvice(t).cls">{{ vsAdvice(t).text }}</span></td>
                <td class="dim small hide-sm">{{ t.note || '—' }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <div class="dim tip">
          「对照」= 该笔操作与下单时点系统建议的一致性：✓ 跟随建议 / ✗ 逆建议 / ○ 中性（当时无持仓数据或方向未明的建议）
        </div>
      </template>
      <div v-else class="dim empty">暂无交易记录，在分析页或持仓页点击买入/卖出即可模拟下单</div>
      <div class="btns"><button class="primary" @click="$emit('close')">关闭</button></div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api'

defineEmits(['close'])
const trades = ref([])
const loading = ref(true)

onMounted(async () => {
  try {
    trades.value = await api.trades()
  } finally {
    loading.value = false
  }
})

const buys = computed(() => trades.value.filter((t) => t.side === 'buy').length)
const sells = computed(() => trades.value.filter((t) => t.side === 'sell' && t.realized_pnl != null))

const stats = computed(() => {
  const rs = sells.value.map((t) => t.realized_pnl)
  const realized = rs.reduce((a, b) => a + b, 0) + 0.0
  const wins = rs.filter((x) => x > 0).length
  return {
    realized,
    wins,
    winRate: rs.length ? Math.round((wins / rs.length) * 100) : 0,
    best: rs.length ? Math.max(...rs) : 0,
    worst: rs.length ? Math.min(...rs) : 0,
  }
})

function vsAdvice(t) {
  const c = t.context
  if (!c) return { text: '—', cls: 'neutral' }
  const a = c.advice_action || ''
  const buy = t.side === 'buy'
  if (/建仓|加仓|补仓/.test(a)) return buy ? { text: '✓ 跟随建议', cls: 'follow' } : { text: '✗ 逆建议', cls: 'against' }
  if (/减仓|止损|清仓|离场|回避/.test(a)) return !buy ? { text: '✓ 跟随建议', cls: 'follow' } : { text: '✗ 逆建议', cls: 'against' }
  if (/持有/.test(a)) return buy ? { text: '✓ 看多加仓', cls: 'follow' } : { text: '○ 主动离场', cls: 'neutral' }
  return { text: '○ 中性操作', cls: 'neutral' } // 观望等方向不明建议
}

function fmt(v) {
  const x = v + 0
  return (x > 0 ? '+' : '') + x.toLocaleString('zh-CN', { maximumFractionDigits: 0 }) + ' 元'
}
function cls(v) {
  const x = (v ?? 0) + 0
  if (x > 0) return 'up'
  if (x < 0) return 'down'
  return 'dim'
}
</script>

<style scoped>
.mask {
  position: fixed; inset: 0; z-index: 200;
  background: rgba(5, 8, 14, 0.66);
  display: flex; align-items: center; justify-content: center;
  padding: 12px;
}
.modal {
  width: 100%; max-width: 940px; min-width: 0;
  max-height: 86vh; overflow-y: auto;
}
.stats { display: flex; gap: 12px; margin-bottom: 12px; flex-wrap: wrap; }
.stat {
  flex: 1 1 150px; min-width: 0;
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 12px;
}
.stat .label { font-size: 11px; }
.stat b { font-size: 16px; font-family: Menlo, monospace; }
.stat .sub { font-size: 11px; margin-left: 6px; }
.small { font-size: 11px; }
.small-badge { font-size: 11px; padding: 2px 8px; }
.follow { background: rgba(74,168,255,0.15); color: var(--accent); }
.against { background: rgba(240,161,62,0.15); color: var(--warn); }
.neutral { background: rgba(139,150,168,0.15); color: var(--text-dim); }
.tip { font-size: 11px; margin-top: 10px; line-height: 1.6; }
.empty { padding: 30px; text-align: center; }
.btns { display: flex; justify-content: flex-end; margin-top: 12px; }

@media (max-width: 760px) {
  .hide-sm { display: none; }
  .stats { gap: 8px; }
  .stat { flex: 1 1 44%; padding: 7px 10px; }
  .stat b { font-size: 14px; }
}
</style>
