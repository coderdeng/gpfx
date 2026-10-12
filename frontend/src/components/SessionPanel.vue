<template>
  <div class="panel session">
    <div class="head">
      <h3>当日盘面 · {{ report.date }}</h3>
      <div class="gauge-row">
        <span class="dim">多头强度</span>
        <div class="bar">
          <div class="fill" :style="{ width: report.strength + '%' }"></div>
          <div class="mid"></div>
        </div>
        <b :class="strengthCls">{{ report.strength }}</b>
      </div>
    </div>
    <div class="headline">{{ report.headline }}</div>

    <div class="cols">
      <div class="col">
        <div class="subtitle up">▲ 多方要点（{{ report.bull_points.length }}）</div>
        <ul class="pts">
          <li v-for="(p, i) in report.bull_points" :key="'b' + i">{{ p.text }}</li>
          <li v-if="!report.bull_points.length" class="dim">无</li>
        </ul>
      </div>
      <div class="col">
        <div class="subtitle down">▼ 空方要点（{{ report.bear_points.length }}）</div>
        <ul class="pts">
          <li v-for="(p, i) in report.bear_points" :key="'s' + i">{{ p.text }}</li>
          <li v-if="!report.bear_points.length" class="dim">无</li>
        </ul>
      </div>
    </div>

    <div class="descs">
      <div>🕯 {{ report.candle_desc }}</div>
      <div>📊 {{ report.volume_desc }}</div>
    </div>

    <div class="subtitle" style="margin-top: 12px">关键位攻防</div>
    <div class="table-wrap">
      <table>
        <thead><tr><th>关键位</th><th>今日结果</th><th>距收盘</th></tr></thead>
        <tbody>
          <tr v-for="b in report.battles" :key="b.label">
            <td>{{ b.label }}</td>
            <td><span :class="battleCls(b.result)">{{ b.result }}</span></td>
            <td class="dim">{{ b.gap_pct > 0 ? '+' : '' }}{{ b.gap_pct }}%</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div class="subtitle" style="margin-top: 12px">次日预案</div>
    <div class="plans">
      <div v-for="(p, i) in report.next_day_plans" :key="i" class="plan">
        <div class="plan-head">
          <span class="scenario">{{ p.scenario }}</span>
          <span class="badge neutral action-badge">{{ p.action }}</span>
        </div>
        <div class="dim detail">{{ p.detail }}</div>
      </div>
    </div>
    <div class="dim note">盘面分析基于最新日线自动生成，预案为技术推演，仅供参考，不构成投资建议。</div>
  </div>
</template>

<script setup>
import { computed } from 'vue'

const props = defineProps({ report: { type: Object, required: true } })

const strengthCls = computed(() => {
  const s = props.report.strength
  if (s >= 65) return 'up'
  if (s <= 35) return 'down'
  return 'dim'
})

function battleCls(r) {
  if (/守住|突破/.test(r)) return 'up'
  if (/跌破|承压/.test(r)) return 'down'
  return 'dim'
}
</script>

<style scoped>
.head { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
.gauge-row { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.bar {
  position: relative; width: 120px; height: 8px;
  background: #232b3d; border-radius: 4px;
}
.fill { height: 100%; border-radius: 4px; background: linear-gradient(90deg, var(--down), var(--up)); }
.mid { position: absolute; left: 50%; top: -2px; bottom: -2px; width: 1px; background: #3a4457; }
.headline { font-size: 15px; font-weight: 600; margin: 8px 0 10px; }
.cols { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; margin-bottom: 10px; }
.subtitle { font-size: 12px; font-weight: 600; margin-bottom: 4px; }
.pts { list-style: none; margin: 0; padding: 0; font-size: 12px; line-height: 1.7; }
.pts li::before { content: '· '; }
.descs { font-size: 12px; line-height: 1.8; border-top: 1px dashed var(--border); padding-top: 8px; }
.plans { display: flex; flex-direction: column; gap: 8px; }
.plan {
  background: var(--panel-2); border: 1px solid var(--border);
  border-radius: 8px; padding: 8px 12px;
}
.plan-head { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.scenario { font-size: 13px; font-weight: 600; }
.action-badge { margin-left: auto; }
.detail { font-size: 12px; line-height: 1.6; margin-top: 4px; }
.note { font-size: 11px; margin-top: 10px; }

@media (max-width: 760px) {
  .cols { grid-template-columns: 1fr; }
}
</style>
