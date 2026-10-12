<template>
  <div class="mask" @click.self="$emit('close')">
    <div class="modal panel">
      <h3>模拟交易 · {{ name }}（{{ code }}）</h3>

      <div class="side-toggle">
        <button :class="{ 'side-buy': side === 'buy' }" @click="side = 'buy'">买入</button>
        <button :class="{ 'side-sell': side === 'sell' }" @click="side = 'sell'">卖出</button>
      </div>

      <div class="fields">
        <label>成交价 <input v-model.number="price" type="number" step="0.01" min="0" /></label>
        <label>股数 <input v-model.number="shares" type="number" step="100" min="0" /></label>
        <label>备注 <input v-model="note" placeholder="选填，如：按建议止损离场" /></label>
      </div>

      <div v-if="preview" class="preview">
        <template v-if="side === 'buy'">
          <div v-if="cost">
            成交后：持股 {{ preview.newShares }} 股，成本
            <b>{{ preview.newCost }}</b>（原 {{ cost }}，费用已计入）
          </div>
          <div v-else>当前未持有，成交后以 {{ preview.newCost }} 建立持仓 {{ shares }} 股（含费用）</div>
          <div class="dim">费用 ≈ {{ preview.fee }} 元（佣金 + 过户费）</div>
        </template>
        <template v-else>
          <div v-if="cost">
            成交后：剩余 {{ preview.remaining }} 股<span v-if="preview.remaining === 0">（清仓，将移除该持仓）</span>，
            净盈亏 <b :class="preview.realized >= 0 ? 'up' : 'down'">{{ fmt(preview.realized) }}</b>（已扣费）<span
              v-if="preview.remaining > 0"
            >，摊薄成本 <b>{{ preview.newCost }}</b></span>
          </div>
          <div v-else class="down">当前未持有该股，无法卖出</div>
          <div class="dim">费用 ≈ {{ preview.fee }} 元（佣金 + 印花税 + 过户费）</div>
        </template>
      </div>

      <div v-if="error" class="error-box">{{ error }}</div>

      <div class="btns">
        <button @click="$emit('close')">取消</button>
        <button class="primary" :disabled="!valid" @click="submit">
          确认{{ side === 'buy' ? '买入' : '卖出' }}
        </button>
      </div>
      <div class="dim tip">模拟记账：仅更新本地持仓与流水，不涉及真实资金</div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue'
import { api } from '../api'
import { store } from '../store'

const props = defineProps({
  code: { type: String, required: true },
  name: { type: String, required: true },
  cost: { type: Number, default: null },   // 持仓成本，未持有为 null
  shares: { type: Number, default: null }, // 持股数量
  close: { type: Number, required: true }, // 最新收盘价（默认成交价）
  side: { type: String, default: 'buy' },
  price0: { type: Number, default: null }, // 预填价格（如建议价位）
  shares0: { type: Number, default: null },// 预填股数（如建议补仓股数）
})
const emit = defineEmits(['close', 'done'])

const side = ref(props.side)
const price = ref(props.price0 ?? props.close)
const shares = ref(props.shares0 ?? null)
const note = ref('')
const error = ref('')

const valid = computed(() => price.value > 0 && shares.value > 0)

// A股费率（与后端 config.rs 保持一致，如券商不同请两处同步修改）
const COMMISSION = 0.0005 // 佣金 万5
const COMMISSION_MIN = 5 // 最低 5 元
const STAMP = 0.0005 // 印花税 万5（仅卖出）
const TRANSFER = 0.00001 // 过户费 万0.1
const buyFee = (amt) => Math.max(amt * COMMISSION, COMMISSION_MIN) + amt * TRANSFER
const sellFee = (amt) => Math.max(amt * COMMISSION, COMMISSION_MIN) + amt * STAMP + amt * TRANSFER

const preview = computed(() => {
  if (!valid.value) return null
  const amt = price.value * shares.value
  if (side.value === 'buy') {
    const fee = buyFee(amt)
    if (!props.cost) {
      return { fee: fee.toFixed(2), newShares: shares.value, newCost: ((amt + fee) / shares.value).toFixed(2) }
    }
    const ns = props.shares + shares.value
    const nc = (props.cost * props.shares + amt + fee) / ns
    return { fee: fee.toFixed(2), newShares: ns, newCost: nc.toFixed(2) }
  }
  if (!props.cost) return null
  const fee = sellFee(amt)
  const remaining = Math.max(0, props.shares - shares.value)
  const realized = (price.value - props.cost) * shares.value - fee
  // 摊薄口径：卖出净回款冲减剩余持仓成本（与后端 positions.rs 保持一致）
  const newCost =
    remaining > 0 ? (props.cost * props.shares - (amt - fee)) / remaining : 0
  return { fee: fee.toFixed(2), remaining, realized, newCost: newCost.toFixed(2) }
})

function fmt(v) {
  const s = (v >= 0 ? '+' : '') + v.toFixed(0)
  return s + ' 元'
}

async function submit() {
  error.value = ''
  try {
    await api.addTrade('/api/trades', {
      code: props.code,
      name: props.name,
      side: side.value,
      price: price.value,
      shares: shares.value,
      note: note.value.trim() || null,
      risk: store.risk,
    })
    emit('done')
    emit('close')
  } catch (e) {
    error.value = e.message
  }
}
</script>

<style scoped>
.mask {
  position: fixed; inset: 0; z-index: 200;
  background: rgba(5, 8, 14, 0.66);
  display: flex; align-items: center; justify-content: center;
}
.modal { width: 400px; max-width: 92vw; }
.side-toggle { display: flex; gap: 8px; margin-bottom: 12px; }
.side-toggle button { flex: 1; padding: 8px; font-size: 14px; }
.side-buy { background: var(--up) !important; border-color: var(--up) !important; color: #fff !important; }
.side-sell { background: var(--down) !important; border-color: var(--down) !important; color: #fff !important; }
.fields { display: flex; flex-direction: column; gap: 10px; margin-bottom: 12px; }
.fields label { display: flex; align-items: center; justify-content: space-between; gap: 10px; color: var(--text-dim); }
.fields input { width: 220px; }
.preview {
  background: var(--panel-2);
  border: 1px dashed var(--border);
  border-radius: 8px;
  padding: 10px 12px;
  font-size: 13px;
  line-height: 1.6;
  margin-bottom: 12px;
}
.btns { display: flex; gap: 10px; justify-content: flex-end; }
.tip { font-size: 11px; margin-top: 10px; text-align: right; }
</style>
