import { store } from './store'

async function handle(resp) {
  if (resp.status === 401) {
    // 登录过期：清登录态并提示
    store.token = null
    store.user = null
    localStorage.removeItem('token')
    localStorage.removeItem('user')
    store.authExpired = true
    throw new Error('未登录或登录已过期')
  }
  if (!resp.ok) {
    let msg = `HTTP ${resp.status}`
    try {
      const j = await resp.json()
      if (j.error) msg = j.error
    } catch { /* ignore */ }
    throw new Error(msg)
  }
  return resp.json()
}

function authHeaders() {
  return store.token ? { Authorization: `Bearer ${store.token}` } : {}
}

function post(method) {
  return (url, body) =>
    fetch(url, {
      method,
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify(body),
    }).then(handle)
}

export const api = {
  register: post('POST'),
  login: post('POST'),
  search: (q) => fetch(`/api/search?q=${encodeURIComponent(q)}`).then(handle),
  analysis: (code, risk) =>
    fetch(`/api/analysis/${code}?risk=${risk}`, { headers: authHeaders() }).then(handle),
  session: (code) => fetch(`/api/session/${code}`, { headers: authHeaders() }).then(handle),
  ai: (code, risk) =>
    fetch(`/api/ai/${code}?risk=${risk}`, { headers: authHeaders() }).then(handle),
  aiLog: (code) =>
    fetch(`/api/ai/${code}/log`, { headers: authHeaders() }).then(handle),
  portfolio: (risk) =>
    fetch(`/api/portfolio?risk=${risk}`, { headers: authHeaders() }).then(handle),
  positions: () => fetch('/api/positions', { headers: authHeaders() }).then(handle),
  addPosition: post('POST'),
  updatePosition: post('PUT'),
  deletePosition: (id) =>
    fetch(`/api/positions/${id}`, { method: 'DELETE', headers: authHeaders() }).then(handle),
  trades: (code) =>
    fetch(code ? `/api/trades?code=${encodeURIComponent(code)}` : '/api/trades', {
      headers: authHeaders(),
    }).then(handle),
  addTrade: post('POST'),
  watchlist: () => fetch('/api/watchlist', { headers: authHeaders() }).then(handle),
  addWatch: post('POST'),
  removeWatch: (code) =>
    fetch(`/api/watchlist/${encodeURIComponent(code)}`, {
      method: 'DELETE',
      headers: authHeaders(),
    }).then(handle),
}
