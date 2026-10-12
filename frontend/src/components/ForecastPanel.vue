<template>
  <div class="panel forecast">
    <h3>走势预测（技术推算）</h3>
    <div class="cols">
      <div>
        <div class="subtitle down">▼ 下方支撑</div>
        <table>
          <thead><tr><th>价位</th><th>依据</th></tr></thead>
          <tbody>
            <tr v-for="p in forecast.supports" :key="'s' + p.price">
              <td class="down price">{{ p.price }}</td>
              <td class="dim">{{ p.source }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div>
        <div class="subtitle up">▲ 上方压力</div>
        <table>
          <thead><tr><th>价位</th><th>依据</th></tr></thead>
          <tbody>
            <tr v-for="p in forecast.resistances" :key="'r' + p.price">
              <td class="up price">{{ p.price }}</td>
              <td class="dim">{{ p.source }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
    <div class="subtitle" style="margin-top: 12px">目标区间（基于 ATR 波动率 + 综合评分）</div>
    <table>
      <thead><tr><th>周期</th><th>区间下沿</th><th>中值</th><th>区间上沿</th><th>置信度</th></tr></thead>
      <tbody>
        <tr v-for="t in forecast.targets" :key="t.days">
          <td>未来 {{ t.days }} 日</td>
          <td class="down">{{ t.low }}</td>
          <td class="dim">{{ t.mid }}</td>
          <td class="up">{{ t.high }}</td>
          <td class="dim">{{ t.confidence }}</td>
        </tr>
      </tbody>
    </table>
    <div class="note dim">{{ forecast.note }}</div>
  </div>
</template>

<script setup>
defineProps({ forecast: { type: Object, required: true } })
</script>

<style scoped>
.cols { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }

@media (max-width: 760px) {
  .cols { grid-template-columns: 1fr; }
}
.subtitle { font-size: 12px; font-weight: 600; margin-bottom: 4px; }
.price { font-family: Menlo, monospace; font-weight: 700; }
.note { font-size: 12px; margin-top: 10px; line-height: 1.6; }
</style>
