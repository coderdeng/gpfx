<template>
  <div class="panel ai-panel">
    <div class="head">
      <h3>🤖 AI 分析建议</h3>
      <div class="meta">
        <span v-if="advice?.model" class="dim model">{{ advice.model }}</span>
        <button class="refresh" :disabled="loading" @click="$emit('refresh')">
          {{ loading ? '分析中…' : advice?.cached ? '重新生成' : advice ? '刷新' : '生成 AI 分析' }}
        </button>
      </div>
    </div>

    <div v-if="loading && !advice" class="dim loading">AI 分析中，通常需要 5~20 秒…</div>

    <template v-if="advice">
      <div v-if="!advice.configured" class="dim notcfg">
        ⚙️ {{ advice.content }}
        <div class="cfg-hint">
          配置方法：在 <code>backend/.env</code> 写入
          <code>AI_API_KEY=你的key</code>（可选 <code>AI_BASE_URL</code> 指向任何 OpenAI 兼容接口、<code>AI_MODEL</code> 指定模型），重启后端生效。
        </div>
      </div>
      <div v-else class="md" v-html="rendered"></div>
      <div v-if="advice.configured" class="foot dim">
        生成时间 {{ advice.generated_at }}<template v-if="advice.cached">（缓存，数据变化后自动重新生成）</template>
        · AI 输出基于系统已算好的指标数据，仅供参考，不构成投资建议
        <button class="log-toggle" @click="toggleLog">
          {{ showLog ? '收起提示词 ▲' : '查看发送给 AI 的完整提示词 ▼' }}
        </button>
      </div>
      <div v-if="showLog" class="log-box">
        <div v-if="logLoading" class="dim">加载中…</div>
        <template v-else-if="log">
          <div class="log-meta dim">
            模型 <b>{{ log.model }}</b> · 提示词 <b>{{ log.prompt_chars }}</b> 字符 · 调用时间 {{ log.created_at }}
          </div>
          <pre class="prompt">{{ log.prompt }}</pre>
        </template>
        <div v-else class="dim">暂无调用记录（生成一次分析后即可查看）</div>
      </div>
    </template>
    <div v-else-if="!loading" class="dim loading">点击右上角按钮生成 AI 分析建议</div>
  </div>
</template>

<script setup>
import { computed, ref, watch } from 'vue'
import { api } from '../api'

const props = defineProps({
  advice: { type: Object, default: null },
  loading: { type: Boolean, default: false },
  code: { type: String, default: '' },
})
const emit = defineEmits(['refresh'])

const showLog = ref(false)
const log = ref(null)
const logLoading = ref(false)

async function toggleLog() {
  showLog.value = !showLog.value
  if (showLog.value && !log.value && props.code) {
    logLoading.value = true
    try {
      log.value = await api.aiLog(props.code)
    } catch {
      log.value = null
    } finally {
      logLoading.value = false
    }
  }
}
// 换股票后清空已加载的日志
watch(() => props.code, () => { log.value = null; showLog.value = false })

// 极简 markdown 渲染：标题/加粗/列表/换行（避免引入第三方依赖）
const rendered = computed(() => {
  if (!props.advice?.content) return ''
  const esc = (s) =>
    s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  return esc(props.advice.content)
    .split(/\n/)
    .map((line) => {
      const l = line.trim()
      if (/^#{1,4}\s/.test(l)) return `<h4>${inline(l.replace(/^#{1,4}\s/, ''))}</h4>`
      if (/^[-*]\s/.test(l)) return `<li>${inline(l.replace(/^[-*]\s/, ''))}</li>`
      if (/^\d+[.、]\s/.test(l)) return `<li>${inline(l.replace(/^\d+[.、]\s/, ''))}</li>`
      if (!l) return '<br/>'
      return `<p>${inline(l)}</p>`
    })
    .join('')
    .replace(/(<br\/>\s*){2,}/g, '<br/>')
    .replace(/<li>/g, '<ul style="margin:0;padding-left:18px"><li>')
    .replace(/<\/li>/g, '</li></ul>')
    .replace(/<\/ul><ul[^>]*>/g, '')

  function inline(s) {
    return s
      .replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>')
      .replace(/`([^`]+)`/g, '<code>$1</code>')
  }
})
</script>

<style scoped>
.ai-panel { border-top: 3px solid var(--accent); }
.head { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
.meta { display: flex; align-items: center; gap: 10px; }
.model { font-size: 11px; }
.refresh { font-size: 12px; padding: 4px 12px; }
.loading { padding: 16px; text-align: center; }
.notcfg { font-size: 13px; line-height: 1.8; }
.cfg-hint { margin-top: 6px; font-size: 12px; }
.cfg-hint code {
  background: var(--panel-2); border: 1px solid var(--border);
  border-radius: 4px; padding: 1px 5px; font-size: 11px;
}
.md { font-size: 13px; line-height: 1.9; }
.md :deep(h4) { margin: 10px 0 4px; color: var(--accent); }
.md :deep(p) { margin: 6px 0; }
.md :deep(code) {
  background: var(--panel-2); border: 1px solid var(--border);
  border-radius: 4px; padding: 0 5px; font-size: 12px;
}
.md :deep(ul) { margin: 4px 0; }
.foot { font-size: 11px; margin-top: 10px; border-top: 1px dashed var(--border); padding-top: 8px; }
.log-toggle {
  display: block; margin-top: 8px; font-size: 11px; padding: 3px 8px;
}
.log-box { margin-top: 8px; }
.log-meta { font-size: 12px; margin-bottom: 6px; }
.prompt {
  background: #0a0d13; border: 1px solid var(--border); border-radius: 8px;
  padding: 12px; font-size: 11px; line-height: 1.7; color: #9fb0c8;
  white-space: pre-wrap; word-break: break-all;
  max-height: 400px; overflow-y: auto;
  font-family: Menlo, Consolas, monospace;
}
</style>
