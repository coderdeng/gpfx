<template>
  <div class="login-wrap">
    <div class="login-card panel">
      <div class="logo">📈 股票技术分析</div>
      <div class="dim sub">{{ isRegister ? '创建新账号' : '登录以查看你的持仓与交易' }}</div>

      <label>用户名
        <input v-model="username" placeholder="如 yihai" @keydown.enter="submit" />
      </label>
      <label>密码
        <input v-model="password" type="password" placeholder="至少 4 位" @keydown.enter="submit" />
      </label>
      <label v-if="isRegister">邀请码
        <input v-model="invite" placeholder="后端开启注册邀请码时必填" @keydown.enter="submit" />
      </label>
      <div v-if="error" class="error-box">{{ error }}</div>
      <button class="primary submit" :disabled="busy || !username || !password" @click="submit">
        {{ busy ? '请稍候…' : isRegister ? '注册并登录' : '登录' }}
      </button>
      <div class="switch dim" @click="isRegister = !isRegister; error = ''">
        {{ isRegister ? '已有账号？去登录' : '没有账号？注册一个' }}
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref } from 'vue'
import { api } from '../api'
import { store, setAuth } from '../store'

const username = ref('')
const password = ref('')
const invite = ref('')
const isRegister = ref(false)
const error = ref('')
const busy = ref(false)

async function submit() {
  if (busy.value || !username.value || !password.value) return
  busy.value = true
  error.value = ''
  try {
    const url = isRegister.value ? '/api/auth/register' : '/api/auth/login'
    const res = await api[isRegister.value ? 'register' : 'login'](url, {
      username: username.value.trim(),
      password: password.value,
      invite: invite.value.trim(),
    })
    setAuth(res.user, res.token)
    store.authExpired = false
  } catch (e) {
    error.value = e.message
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
.login-wrap {
  min-height: 100vh;
  display: flex; align-items: center; justify-content: center;
  padding: 16px;
}
.login-card { width: 360px; max-width: 94vw; }
.logo { font-size: 18px; font-weight: 700; margin-bottom: 4px; }
.sub { margin-bottom: 18px; }
label { display: flex; flex-direction: column; gap: 5px; margin-bottom: 12px; color: var(--text-dim); font-size: 13px; }
label input { width: 100%; padding: 9px 12px; font-size: 14px; }
.submit { width: 100%; padding: 10px; font-size: 14px; margin-top: 4px; }
.switch { text-align: center; margin-top: 14px; cursor: pointer; font-size: 13px; }
.switch:hover { color: var(--accent); }
</style>
