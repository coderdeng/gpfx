// 全局共享状态
import { reactive } from 'vue'

export const store = reactive({
  risk: localStorage.getItem('risk') || 'balanced',
  view: 'analysis', // analysis | portfolio | watchlist
  analysisCode: null,
  // 登录态：token 与用户名
  token: localStorage.getItem('token') || null,
  user: localStorage.getItem('user') || null,
  // 401 时置 true，App 显示登录页
  authExpired: false,
  // 交易后递增，触发分析页/持仓页刷新
  tradeVersion: 0,
})

export function setRisk(r) {
  store.risk = r
  localStorage.setItem('risk', r)
}

export function setAuth(user, token) {
  store.user = user
  store.token = token
  localStorage.setItem('user', user)
  localStorage.setItem('token', token)
}

export function logout() {
  store.user = null
  store.token = null
  localStorage.removeItem('user')
  localStorage.removeItem('token')
}

export function bumpTrades() {
  store.tradeVersion++
}
