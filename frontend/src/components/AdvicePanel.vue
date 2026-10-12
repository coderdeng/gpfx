<template>
  <div class="panel advice" :class="tone">
    <div class="head">
      <h3>操作建议</h3>
      <span class="badge" :class="badgeCls">{{ advice.action }}</span>
      <span v-if="advice.has_position" class="pos-tag">已持仓</span>
    </div>
    <div class="headline">{{ advice.headline }}</div>
    <ul class="reasons">
      <li v-for="(r, i) in advice.reasons" :key="i">{{ r }}</li>
    </ul>

    <div v-if="advice.key_levels?.length" class="levels">
      <div
        v-for="k in advice.key_levels"
        :key="k.label"
        class="level"
      >
        <span class="label" :class="levelCls(k.label)">{{ k.label }}</span>
        <span class="price">
          {{ k.price }}<template v-if="k.price2"> ~ {{ k.price2 }}</template>
        </span>
        <span class="dim note">{{ k.note }}</span>
      </div>
    </div>

    <div v-if="advice.add_math" class="add-math">
      💰 {{ advice.add_math.note }}：以 {{ advice.add_math.add_price }} 补仓
      {{ advice.add_math.add_shares }} 股 → 新成本
      <b class="up">{{ advice.add_math.new_cost }}</b>
      （合计 {{ advice.add_math.new_shares }} 股）
    </div>

    <div class="trade-row">
      <button class="primary" @click="$emit('trade', 'buy')">
        {{ advice.has_position ? '买入 / 补仓' : '模拟买入建仓' }}
      </button>
      <button v-if="advice.has_position" class="sell-btn" @click="$emit('trade', 'sell')">
        卖出 / 减仓
      </button>
      <span class="dim hint">模拟记账，按建议价位或自定义成交</span>
    </div>

    <div class="risk-strip">
      <span class="rs-label">{{ advice.risk_params?.label || '稳健' }}风格参数</span>
      <span>止损线 {{ pct(advice.risk_params?.stop_loss_pct) }}</span>
      <span>移动止盈 {{ advice.risk_params?.trailing_ma || '20日均线' }}</span>
      <span>建仓 ≤{{ pct(advice.risk_params?.first_build_pct) }}</span>
      <span>补仓 {{ pct(advice.risk_params?.add_ratio) }}</span>
      <span>反弹减仓 {{ pct(advice.risk_params?.cut_ratio) }}</span>
    </div>

    <div class="disclaimer dim">{{ advice.disclaimer }}</div>
  </div>
</template>

<script setup>
import { computed } from 'vue'

const props = defineProps({
  advice: { type: Object, required: true },
  close: { type: Number, required: true },
})

defineEmits(['trade'])

const tone = computed(() => (badgeCls.value === 'bear' ? '' : ''))

const badgeCls = computed(() => {
  const a = props.advice.action || ''
  if (/止损|离场|回避/.test(a)) return 'strong-bear'
  if (/减仓/.test(a)) return 'bear'
  if (/加仓|补仓|建仓/.test(a)) return 'bull'
  if (/持有/.test(a)) return 'bull'
  return 'neutral' // 观望等
})

function levelCls(label) {
  if (/止损|清仓|离场|保本/.test(label)) return 'lv-stop'
  if (/压力|目标|止盈/.test(label)) return 'up'
  if (/支撑|买入|加仓|补仓|低吸/.test(label)) return 'down'
  return 'dim'
}

function pct(v) {
  if (v == null) return '—'
  return `${Math.round(v * 100)}%`
}
</script>

<style scoped>
.advice { border-top: 3px solid var(--text-dim); }
.head { display: flex; align-items: center; gap: 10px; }
.head h3 { margin: 0; }
.pos-tag { font-size: 11px; color: var(--warn); border: 1px solid var(--warn); padding: 1px 6px; border-radius: 4px; }
.headline { font-size: 15px; font-weight: 600; margin: 10px 0 8px; }
.reasons { margin: 0 0 12px; padding-left: 18px; line-height: 1.8; }
.levels { display: flex; flex-direction: column; gap: 6px; margin-bottom: 10px; }
.level { display: flex; align-items: baseline; gap: 10px; font-size: 13px; }
.label { min-width: 84px; font-weight: 600; }
.price { font-family: Menlo, monospace; font-weight: 700; color: var(--text); }
.note { font-size: 12px; }
.lv-stop { color: #ff6b6b; }
.risk-strip {
  display: flex; flex-wrap: wrap; gap: 4px 14px;
  background: rgba(74,168,255,0.07);
  border: 1px solid rgba(74,168,255,0.25);
  border-radius: 8px;
  padding: 7px 12px;
  font-size: 12px;
  color: var(--text-dim);
  margin-bottom: 10px;
}
.rs-label { color: var(--accent); font-weight: 600; }
.trade-row { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
.trade-row .hint { font-size: 11px; }
.sell-btn { background: var(--down); border-color: var(--down); color: #fff; }
.add-math {
  background: rgba(245, 215, 110, 0.08);
  border: 1px solid rgba(245, 215, 110, 0.3);
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 13px;
  margin-bottom: 10px;
}
.disclaimer { font-size: 11px; line-height: 1.5; border-top: 1px dashed var(--border); padding-top: 8px; }
</style>
