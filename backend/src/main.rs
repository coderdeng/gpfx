//! 股票分析后端：axum 服务入口。

mod advisor;
mod ai;
mod config;
mod data_source;
mod forecast;
mod indicators;
mod models;
mod positions;
mod scoring;
mod session;
mod trades;
mod users;
mod watchlist;

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use data_source::{parse_code, DataSource};
use models::*;
use positions::PositionStore;
use serde_json::json;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use trades::TradeStore;
use users::UserStore;
use watchlist::WatchStore;

struct AppState {
    ds: DataSource,
    users: UserStore,
    /// 每个用户的持仓/流水存储，按需加载缓存
    stores: Mutex<HashMap<String, (Arc<Mutex<PositionStore>>, Arc<Mutex<TradeStore>>)>>,
    /// 每个用户的观察列表，按需加载缓存
    watches: Mutex<HashMap<String, Arc<Mutex<WatchStore>>>>,
    /// 注册邀请码（设置 REGISTER_INVITE 后启用；None = 开放注册）
    invite_code: Option<String>,
    /// 登录/注册失败限流
    login_guard: users::LoginGuard,
    data_dir: PathBuf,
}

impl AppState {
    /// 取（或创建）某用户的存储对
    async fn user_stores(
        &self,
        user: &str,
    ) -> (Arc<Mutex<PositionStore>>, Arc<Mutex<TradeStore>>) {
        let mut m = self.stores.lock().await;
        m.entry(user.to_string())
            .or_insert_with(|| {
                (
                    Arc::new(Mutex::new(PositionStore::load_for(&self.data_dir, user))),
                    Arc::new(Mutex::new(TradeStore::load_for(&self.data_dir, user))),
                )
            })
            .clone()
    }

    /// 取（或创建）某用户的观察列表
    async fn user_watch(&self, user: &str) -> Arc<Mutex<WatchStore>> {
        let mut m = self.watches.lock().await;
        m.entry(user.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(WatchStore::load_for(&self.data_dir, user))))
            .clone()
    }
}

/// 从 Authorization: Bearer <token> 解析用户；无效返回 401
fn auth_user(st: &AppState, headers: &axum::http::HeaderMap) -> Result<String, ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");
    if token.is_empty() {
        return Err(err(StatusCode::UNAUTHORIZED, "未登录"));
    }
    match st.users.verify_token(token) {
        Some(u) => Ok(u),
        None => Err(err(StatusCode::UNAUTHORIZED, "登录已过期，请重新登录")),
    }
}

type ApiError = (StatusCode, Json<serde_json::Value>);

fn err(status: StatusCode, msg: &str) -> ApiError {
    (status, Json(json!({ "error": msg })))
}

#[tokio::main]
async fn main() {
    let data_dir = PathBuf::from("data");
    let ds = DataSource::new(&data_dir).expect("初始化数据源失败");
    let users = UserStore::load(&data_dir.join("users.json"));
    let invite_code = config::env_or_dotenv(&data_dir, "REGISTER_INVITE").filter(|s| !s.is_empty());
    let state = Arc::new(AppState {
        ds,
        users,
        stores: Mutex::new(HashMap::new()),
        watches: Mutex::new(HashMap::new()),
        invite_code,
        login_guard: users::LoginGuard::default(),
        data_dir: data_dir.clone(),
    });
    let invite_enabled = state.invite_code.is_some();

    let static_dir = PathBuf::from("../frontend/dist");
    let app = Router::new()
        .route("/api/auth/register", post(auth_register))
        .route("/api/auth/login", post(auth_login))
        .route("/api/auth/me", get(auth_me))
        .route("/api/search", get(search))
        .route("/api/analysis/{code}", get(analysis))
        .route("/api/session/{code}", get(session_report))
        .route("/api/ai/{code}", get(ai_advice))
        .route("/api/ai/{code}/log", get(ai_call_log))
        .route("/api/positions", get(positions_list).post(positions_add))
        .route("/api/positions/{id}", put(positions_update).delete(positions_delete))
        .route("/api/trades", get(trades_list).post(trades_add))
        .route("/api/portfolio", get(portfolio))
        .route("/api/watchlist", get(watchlist_list).post(watchlist_add))
        .route("/api/watchlist/{code}", delete(watchlist_remove))
        .layer(CorsLayer::permissive())
        .with_state(state);
    let app = if static_dir.exists() {
        app.fallback_service(
            ServeDir::new(&static_dir).fallback(ServeFile::new(static_dir.join("index.html"))),
        )
    } else {
        app
    };

    // 0.0.0.0 以便局域网/组网内的手机访问；公网暴露前建议配置 REGISTER_INVITE 并套 HTTPS
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8787")
        .await
        .expect("绑定 0.0.0.0:8787 失败");
    println!("✅ 股票分析后端已启动: http://127.0.0.1:8787（局域网可访问）");
    println!("🔒 注册邀请码: {}", if invite_enabled { "已启用（新用户注册需携带）" } else { "未设置，开放注册" });
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

// ---------- 认证 ----------

#[derive(serde::Deserialize)]
struct AuthInput {
    username: String,
    password: String,
    /// 注册邀请码（仅当后端设置 REGISTER_INVITE 时必须携带且一致）
    #[serde(default)]
    invite: String,
}

/// 客户端真实 IP：仅当 TCP 对端是本机回环（即经本机 Caddy 反代）时采信
/// X-Forwarded-For，其余情况直接用对端地址（不可伪造）
fn client_ip(headers: &axum::http::HeaderMap, peer: SocketAddr) -> IpAddr {
    if peer.ip().is_loopback() {
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.trim().parse::<IpAddr>().ok())
        {
            return ip;
        }
    }
    peer.ip()
}

async fn auth_register(
    State(st): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(input): Json<AuthInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let ip = client_ip(&headers, peer);
    if let Err(msg) = st.login_guard.check(ip) {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, &msg));
    }
    if let Some(code) = &st.invite_code {
        if input.invite.trim() != code {
            st.login_guard.fail(ip); // 防止邀请码被穷举
            return Err(err(StatusCode::FORBIDDEN, "邀请码错误"));
        }
    }
    let name = input.username.trim().to_string();
    st.users
        .register(&name, &input.password)
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e))?;
    let token = st.users.token_for(&name);
    Ok((StatusCode::CREATED, Json(json!({ "user": name, "token": token }))))
}

async fn auth_login(
    State(st): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(input): Json<AuthInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ip = client_ip(&headers, peer);
    if let Err(msg) = st.login_guard.check(ip) {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, &msg));
    }
    let name = input.username.trim().to_string();
    if !st.users.verify(&name, &input.password) {
        st.login_guard.fail(ip);
        return Err(err(StatusCode::UNAUTHORIZED, "用户名或密码错误"));
    }
    st.login_guard.success(ip);
    let token = st.users.token_for(&name);
    Ok(Json(json!({ "user": name, "token": token })))
}

async fn auth_me(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = auth_user(&st, &headers)?;
    Ok(Json(json!({ "user": user })))
}

// ---------- 搜索 ----------

async fn search(
    State(st): State<Arc<AppState>>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let query = q.get("q").map(|s| s.trim().to_string()).unwrap_or_default();
    if query.is_empty() {
        return Ok(Json(json!([])));
    }
    let hits = st.ds.search(&query).await.map_err(|e| err(StatusCode::BAD_GATEWAY, &e))?;
    Ok(Json(json!(hits)))
}

// ---------- 个股分析 ----------

async fn analysis(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(code): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<AnalysisResponse>, ApiError> {
    let risk = config::Risk::from_query(q.get("risk").map(|s| s.as_str()));
    let code = parse_code(&code).ok_or_else(|| {
        err(StatusCode::BAD_REQUEST, "无效的股票代码（应为6位数字，可带 sh/sz 前缀）")
    })?;
    let (name, bars) = st
        .ds
        .kline(&code)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, &e))?;
    if bars.len() < 30 {
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "该股上市时间过短或行情数据不足，暂无法分析",
        ));
    }
    let ind = indicators::compute(&bars);
    let mut score = scoring::score(&bars, &ind);
    let valuation = st.ds.valuation(&code).await.ok();
    let financials = st.ds.financials(&code).await.ok();
    scoring::apply_fundamental(&mut score, valuation.as_ref(), financials.as_ref());
    let fc = forecast::forecast(&bars, &ind, score.total);
    // 已登录用户带上其持仓上下文（建议区分持仓口径），未登录按无持仓处理
    let user = auth_user(&st, &headers).ok();
    let pos = if let Some(u) = &user {
        let (positions, _) = st.user_stores(u).await;
        let pos_view = positions
            .lock()
            .await
            .find_by_code(&code)
            .map(|p| advisor::PositionView { cost: p.cost, shares: p.shares });
        pos_view
    } else {
        None
    };
    let (pos_cost, pos_shares) = pos
        .as_ref()
        .map(|p| (Some(p.cost), Some(p.shares)))
        .unwrap_or((None, None));
    let advice = advisor::advise(&score, &fc, &bars, &ind, pos, risk);
    let has_position = pos_cost.is_some();
    let last = bars.last().unwrap();
    Ok(Json(AnalysisResponse {
        code,
        name,
        snapshot: Snapshot {
            date: last.date.clone(),
            close: last.close,
            pct_chg: last.pct_chg,
            turnover: last.turnover,
            amount: last.amount,
        },
        bars,
        indicators: ind,
        score,
        forecast: fc,
        advice,
        has_position,
        position_cost: pos_cost,
        position_shares: pos_shares,
        valuation,
        financials,
    }))
}

// ---------- 当日盘面分析 ----------

async fn session_report(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(code): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<SessionReport>, ApiError> {
    let code = parse_code(&code).ok_or_else(|| {
        err(StatusCode::BAD_REQUEST, "无效的股票代码（应为6位数字，可带 sh/sz 前缀）")
    })?;
    let (name, bars) = st
        .ds
        .kline(&code)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, &e))?;
    if bars.len() < 30 {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "行情数据不足，无法做盘面分析"));
    }
    let _ = name;
    let ind = indicators::compute(&bars);
    let mut score = scoring::score(&bars, &ind);
    let valuation = st.ds.valuation(&code).await.ok();
    let financials = st.ds.financials(&code).await.ok();
    scoring::apply_fundamental(&mut score, valuation.as_ref(), financials.as_ref());
    let user_opt = auth_user(&st, &headers).ok();
    let has_position = if let Some(u) = &user_opt {
        let (positions, _) = st.user_stores(u).await;
        let owned = positions.lock().await.find_by_code(&code).is_some();
        owned
    } else {
        false
    };
    Ok(Json(session::analyze(&bars, &ind, &score, has_position)))
}

// ---------- AI 分析建议 ----------

async fn ai_advice(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(code): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<AiAdvice>, ApiError> {
    let code = parse_code(&code).ok_or_else(|| {
        err(StatusCode::BAD_REQUEST, "无效的股票代码（应为6位数字，可带 sh/sz 前缀）")
    })?;
    let risk = config::Risk::from_query(q.get("risk").map(|s| s.as_str()));
    let (name, bars) = st
        .ds
        .kline(&code)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, &e))?;
    if bars.len() < 30 {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "行情数据不足，无法做 AI 分析"));
    }
    let ind = indicators::compute(&bars);
    let mut score = scoring::score(&bars, &ind);
    let valuation = st.ds.valuation(&code).await.ok();
    let financials = st.ds.financials(&code).await.ok();
    scoring::apply_fundamental(&mut score, valuation.as_ref(), financials.as_ref());
    let sess = {
        let user = auth_user(&st, &headers).ok();
        let has_position = if let Some(u) = &user {
            let (positions, _) = st.user_stores(u).await;
            let owned = positions.lock().await.find_by_code(&code).is_some();
            owned
        } else {
            false
        };
        session::analyze(&bars, &ind, &score, has_position)
    };
    // 持仓上下文（未登录/未持仓则 None）
    let position = if let Some(u) = auth_user(&st, &headers).ok() {
        let (positions, _) = st.user_stores(&u).await;
        let ctx = positions
            .lock()
            .await
            .find_by_code(&code)
            .map(|p| (p.cost, p.shares));
        ctx
    } else {
        None
    };
    let cfg = ai::AiConfig::load(&st.data_dir);
    let advice = ai::advise(
        &code,
        &name,
        &bars,
        &score,
        valuation.as_ref(),
        financials.as_ref(),
        &sess,
        position,
        risk.label(),
        &cfg,
        &st.data_dir,
    )
    .await;
    Ok(Json(advice))
}

/// 最近一次该股票 AI 调用的完整 prompt 与响应（透明化）
async fn ai_call_log(
    State(st): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let code = parse_code(&code)
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "无效的股票代码"))?;
    let log = ai::last_call_log(&st.data_dir, &code)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "该股票尚无 AI 调用记录"))?;
    Ok(Json(serde_json::to_value(log).unwrap_or_default()))
}

// ---------- 持仓管理 ----------

async fn positions_list(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = auth_user(&st, &headers)?;
    let (positions, _) = st.user_stores(&user).await;
    let items = positions.lock().await.list();
    Ok(Json(json!(items)))
}

async fn positions_add(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(input): Json<PositionInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let user = auth_user(&st, &headers)?;
    let (positions, _) = st.user_stores(&user).await;
    let p = positions
        .lock()
        .await
        .add(input)
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e))?;
    Ok((StatusCode::CREATED, Json(json!(p))))
}

async fn positions_update(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<u64>,
    Json(input): Json<PositionInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = auth_user(&st, &headers)?;
    let (positions, _) = st.user_stores(&user).await;
    let p = positions
        .lock()
        .await
        .update(id, input)
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e))?;
    Ok(Json(json!(p)))
}

async fn positions_delete(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<u64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = auth_user(&st, &headers)?;
    let (positions, _) = st.user_stores(&user).await;
    positions
        .lock()
        .await
        .remove(id)
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e))?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- 交易记录 ----------

async fn trades_list(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = auth_user(&st, &headers)?;
    let (_, trades) = st.user_stores(&user).await;
    let ts = trades.lock().await;
    let items = match q.get("code").map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(c) => ts.for_code(c),
        None => ts.list(),
    };
    Ok(Json(json!(items)))
}

/// 下单时点的技术面快照（复盘用），失败不影响下单
async fn trade_context(
    st: &AppState,
    user: &str,
    code: &str,
    risk: config::Risk,
) -> Option<TradeContext> {
    let (_, bars) = st.ds.kline(code).await.ok()?;
    if bars.len() < 30 {
        return None;
    }
    let ind = indicators::compute(&bars);
    let mut score = scoring::score(&bars, &ind);
    let valuation = st.ds.valuation(code).await.ok();
    let financials = st.ds.financials(code).await.ok();
    scoring::apply_fundamental(&mut score, valuation.as_ref(), financials.as_ref());
    let fc = forecast::forecast(&bars, &ind, score.total);
    let (positions, _) = st.user_stores(user).await;
    let pos = positions
        .lock()
        .await
        .find_by_code(code)
        .map(|p| advisor::PositionView { cost: p.cost, shares: p.shares });
    let advice = advisor::advise(&score, &fc, &bars, &ind, pos, risk);
    Some(TradeContext {
        score: round2(score.total),
        rating: score.rating,
        advice_action: advice.action,
    })
}

async fn trades_add(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(input): Json<TradeInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let user = auth_user(&st, &headers)?;
    let side = input.side.to_ascii_lowercase();
    if side != "buy" && side != "sell" {
        return Err(err(StatusCode::BAD_REQUEST, "side 必须为 buy 或 sell"));
    }
    if !(input.price > 0.0) || !(input.shares > 0.0) {
        return Err(err(StatusCode::BAD_REQUEST, "成交价与股数必须大于 0"));
    }
    let code = parse_code(&input.code)
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "无效的股票代码"))?;

    let risk = config::Risk::from_query(input.risk.as_deref());
    let context = trade_context(&st, &user, &code, risk).await;

    // 买入未传名称时，从行情缓存解析股票名称
    let mut name_hint = input.name.clone().unwrap_or_default();
    if name_hint.trim().is_empty() {
        if let Ok((n, _)) = st.ds.kline(&code).await {
            name_hint = n;
        }
    }
    if name_hint.trim().is_empty() {
        name_hint = code.clone();
    }

    // 交易费用：买入=佣金(≥5元)+过户费（计入成本）；卖出=佣金+印花税+过户费（冲减盈亏）
    let amount = input.price * input.shares;
    let fee = if side == "buy" {
        config::buy_fee(amount)
    } else {
        config::sell_fee(amount)
    };

    // 先改该用户的持仓（买入费用计入成本 / 卖出扣费后净盈亏，清仓则移除）
    let (positions, trades) = st.user_stores(&user).await;
    let (name, realized) = {
        let mut ps = positions.lock().await;
        match side.as_str() {
            "buy" => {
                let p = ps
                    .buy(&code, &name_hint, input.price, input.shares, fee)
                    .map_err(|e| err(StatusCode::BAD_REQUEST, &e))?;
                (p.name.clone(), None)
            }
            _ => {
                let o = ps
                    .sell(&code, input.price, input.shares, fee)
                    .map_err(|e| err(StatusCode::BAD_REQUEST, &e))?;
                (o.name, Some(o.realized))
            }
        }
    };
    // 再记该用户的流水（含费用与当时技术面快照）
    let mut ts = trades.lock().await;
    let t = ts
        .record(
            &code,
            &name,
            &side,
            input.price,
            input.shares,
            fee,
            realized,
            input.note,
            context,
        )
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e))?;
    Ok((StatusCode::CREATED, Json(json!(t))))
}

// ---------- 组合分析 ----------

async fn portfolio(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<PortfolioResponse>, ApiError> {
    let user = auth_user(&st, &headers)?;
    let risk = config::Risk::from_query(q.get("risk").map(|s| s.as_str()));
    let (positions, trades) = st.user_stores(&user).await;
    let items = positions.lock().await.list();

    struct Draft {
        pos: Position,
        close: f64,
        score: ScoreResult,
        advice: Advice,
    }
    let (mut drafts, mut warnings) = (Vec::new(), Vec::new());
    for p in items {
        let Some(code) = parse_code(&p.code) else {
            warnings.push(format!("{} 代码无效，已跳过", p.code));
            continue;
        };
        match st.ds.kline(&code).await {
            Ok((_, bars)) if bars.len() >= 30 => {
                let ind = indicators::compute(&bars);
                let mut sc = scoring::score(&bars, &ind);
                let val = st.ds.valuation(&code).await.ok();
                let fin = st.ds.financials(&code).await.ok();
                scoring::apply_fundamental(&mut sc, val.as_ref(), fin.as_ref());
                let fc = forecast::forecast(&bars, &ind, sc.total);
                let advice = advisor::advise(
                    &sc,
                    &fc,
                    &bars,
                    &ind,
                    Some(advisor::PositionView { cost: p.cost, shares: p.shares }),
                    risk,
                );
                let close = bars.last().unwrap().close;
                drafts.push(Draft { pos: p, close, score: sc, advice });
            }
            Ok(_) => warnings.push(format!("{} 行情数据不足，已跳过", p.code)),
            Err(e) => warnings.push(format!("{} 获取行情失败: {}", p.code, e)),
        }
    }

    let total_value: f64 = drafts.iter().map(|d| d.pos.shares * d.close).sum();
    let total_cost: f64 = drafts.iter().map(|d| d.pos.shares * d.pos.cost).sum();
    // 归一化 -0.0（空组合时 sum 可能产生负零）
    let (total_value, total_cost) = (total_value + 0.0, total_cost + 0.0);
    let ts = trades.lock().await;
    let mut rows: Vec<PortfolioRow> = drafts
        .into_iter()
        .map(|d| {
            let realized = ts.realized_for_code(&d.pos.code);
            let mv = d.pos.shares * d.close;
            let cost = d.pos.shares * d.pos.cost;
            PortfolioRow {
                weight_pct: if total_value > 0.0 { mv / total_value * 100.0 } else { 0.0 },
                position: d.pos,
                close: d.close,
                market_value: mv,
                profit: mv - cost,
                profit_pct: if cost > 0.0 { (mv - cost) / cost * 100.0 } else { 0.0 },
                score: d.score,
                action: d.advice.action,
                headline: d.advice.headline,
                stop_loss: d.advice.stop_loss,
                realized_pnl: realized,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.market_value.partial_cmp(&a.market_value).unwrap());

    Ok(Json(PortfolioResponse {
        total_value,
        total_cost,
        total_profit: total_value - total_cost,
        total_profit_pct: if total_cost > 0.0 {
            (total_value - total_cost) / total_cost * 100.0
        } else {
            0.0
        },
        total_realized: ts.total_realized(),
        rows,
        warnings,
        generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    }))
}

// ---------- 观察列表 ----------

async fn watchlist_list(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<WatchRow>>, ApiError> {
    let user = auth_user(&st, &headers)?;
    let watch = st.user_watch(&user).await;
    let items = watch.lock().await.list();

    let mut rows = Vec::with_capacity(items.len());
    for it in items {
        let (live_name, bars) = match st.ds.kline(&it.code).await {
            Ok((n, b)) => (Some(n), if b.is_empty() { None } else { Some(b) }),
            Err(_) => (None, None),
        };
        let name = live_name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| it.name.clone());
        let (last_date, last_close, pct_today, closes_since, base_close) = match &bars {
            Some(bs) => {
                let last = bs.last().unwrap();
                // 基准价：基准日就是最新一根（当日添加）→ 用添加时记录的价格；
                // 已跨日 → 改取基准日收盘价，与前复权序列保持一致（规避除权假涨跌）
                let base = if last.date == it.base_date {
                    it.base_close
                } else {
                    bs.iter()
                        .find(|b| b.date == it.base_date)
                        .map(|b| b.close)
                        .unwrap_or(it.base_close)
                };
                let closes = bs
                    .iter()
                    .filter(|b| b.date.as_str() >= it.base_date.as_str())
                    .map(|b| b.close)
                    .collect();
                (last.date.clone(), last.close, last.pct_chg, closes, base)
            }
            // 行情缺失（停牌/退市等）：降级为只显示记录的基准信息
            None => (it.base_date.clone(), it.base_close, 0.0, Vec::new(), it.base_close),
        };
        let pct_since_add = if base_close > 1e-9 {
            (last_close / base_close - 1.0) * 100.0
        } else {
            0.0
        };
        rows.push(WatchRow {
            code: it.code,
            name,
            added_at: it.added_at,
            base_date: it.base_date,
            base_close: round2(base_close),
            last_date,
            last_close,
            pct_today,
            pct_since_add,
            closes_since,
        });
    }
    Ok(Json(rows))
}

async fn watchlist_add(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(input): Json<WatchInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let user = auth_user(&st, &headers)?;
    let code =
        parse_code(&input.code).ok_or_else(|| err(StatusCode::BAD_REQUEST, "无效的股票代码"))?;
    // 基准取添加时最近一根日K（盘内即为当时最新价）
    let (name, bars) = st
        .ds
        .kline(&code)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, &e))?;
    let last = bars
        .last()
        .ok_or_else(|| err(StatusCode::BAD_GATEWAY, "行情数据为空，无法记录基准价"))?;
    let watch = st.user_watch(&user).await;
    let item = watch
        .lock()
        .await
        .add(&code, &name, &last.date, last.close)
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e))?;
    Ok((StatusCode::CREATED, Json(json!(item))))
}

async fn watchlist_remove(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = auth_user(&st, &headers)?;
    let code =
        parse_code(&code).ok_or_else(|| err(StatusCode::BAD_REQUEST, "无效的股票代码"))?;
    let watch = st.user_watch(&user).await;
    watch
        .lock()
        .await
        .remove(&code)
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e))?;
    Ok(Json(json!({ "ok": true })))
}
