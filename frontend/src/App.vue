<template>
  <!-- 未登录：登录/注册页 -->
  <LoginView v-if="!store.user" />

  <div v-else class="app">
    <header class="topbar">
      <div class="brand" @click="switchView('analysis')">📈 股票技术分析</div>
      <nav>
        <button :class="{ primary: store.view === 'analysis' }" @click="switchView('analysis')">个股分析</button>
        <button :class="{ primary: store.view === 'portfolio' }" @click="switchView('portfolio')">我的持仓</button>
        <button :class="{ primary: store.view === 'watchlist' }" @click="switchView('watchlist')">观察列表</button>
      </nav>
      <div class="risk">
        <span class="dim">风格</span>
        <select :value="store.risk" @change="setRisk($event.target.value)">
          <option value="conservative">保守</option>
          <option value="balanced">稳健</option>
          <option value="aggressive">激进</option>
        </select>
      </div>
      <div class="user">
        <span class="uname">👤 {{ store.user }}</span>
        <button class="logout" @click="doLogout">退出</button>
      </div>
    </header>
    <main class="main">
      <AnalysisView v-if="store.view === 'analysis'" />
      <PortfolioView v-else-if="store.view === 'portfolio'" />
      <WatchlistView v-else />
    </main>
    <footer class="footer dim">
      数据来源：腾讯/东方财富公开行情接口（前复权日线）· 技术分析结果仅供参考，不构成投资建议
    </footer>
  </div>
</template>

<script setup>
import { store, setRisk, logout } from './store'
import AnalysisView from './views/AnalysisView.vue'
import PortfolioView from './views/PortfolioView.vue'
import WatchlistView from './views/WatchlistView.vue'
import LoginView from './components/LoginView.vue'

function switchView(v) {
  store.view = v
}

function doLogout() {
  logout()
  store.view = 'analysis'
}
</script>

<style scoped>
.app { display: flex; flex-direction: column; min-height: 100vh; }
.topbar {
  display: flex;
  align-items: center;
  gap: 20px;
  padding: 10px 20px;
  border-bottom: 1px solid var(--border);
  background: var(--panel);
  position: sticky;
  top: 0;
  z-index: 10;
  flex-wrap: wrap;
}
.brand { font-size: 16px; font-weight: 700; cursor: pointer; user-select: none; }
nav { display: flex; gap: 8px; flex: 1; }
nav button { white-space: nowrap; }
.risk { display: flex; align-items: center; gap: 6px; }
.user { display: flex; align-items: center; gap: 8px; }
.uname { font-size: 13px; color: var(--text-dim); }
.logout { font-size: 12px; padding: 4px 10px; }
.main { flex: 1; padding: 14px 20px; max-width: 1500px; width: 100%; margin: 0 auto; }
.footer { text-align: center; padding: 12px; font-size: 12px; }

@media (max-width: 640px) {
  .topbar { gap: 8px 12px; padding: 8px 12px; }
  .brand { font-size: 14px; }
  nav { order: 3; width: 100%; }
  nav button { padding: 6px 14px; font-size: 12px; }
  .user { margin-left: auto; }
}
</style>
