#!/bin/bash
# 一键启动：后端 (Rust/axum, :8787) + 前端 (Vite dev, :5173)
set -e
cd "$(dirname "$0")"

echo "==> 启动后端 (127.0.0.1:8787)..."
(cd backend && cargo run) &
BACK_PID=$!

echo "==> 启动前端 (127.0.0.1:5173)..."
cd frontend
[ -d node_modules ] || npm install
npm run dev &
FRONT_PID=$!

trap "kill $BACK_PID $FRONT_PID 2>/dev/null" EXIT
echo ""
echo "✅ 就绪后访问: http://127.0.0.1:5173  (后端 API: http://127.0.0.1:8787)"
echo "   Ctrl+C 退出"
wait
