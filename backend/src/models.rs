//! API 数据结构定义。

use serde::{Deserialize, Serialize};

/// 一根日K线
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bar {
    pub date: String,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    /// 成交量（手）
    pub volume: f64,
    /// 成交额（元）
    pub amount: f64,
    /// 换手率 %
    pub turnover: f64,
    /// 涨跌幅 %
    pub pct_chg: f64,
}

/// 搜索结果条目
#[derive(Debug, Clone, Serialize)]
pub struct StockHit {
    pub code: String,
    pub name: String,
    pub market: String,
    pub secid: String,
}

/// 技术指标序列（与 bars 等长，热身期为 null）
#[derive(Debug, Clone, Default, Serialize)]
pub struct IndicatorSeries {
    pub ma5: Vec<Option<f64>>,
    pub ma10: Vec<Option<f64>>,
    pub ma20: Vec<Option<f64>>,
    pub ma60: Vec<Option<f64>>,
    pub dif: Vec<Option<f64>>,
    pub dea: Vec<Option<f64>>,
    pub hist: Vec<Option<f64>>,
    pub rsi6: Vec<Option<f64>>,
    pub rsi12: Vec<Option<f64>>,
    pub rsi24: Vec<Option<f64>>,
    pub kdj_k: Vec<Option<f64>>,
    pub kdj_d: Vec<Option<f64>>,
    pub kdj_j: Vec<Option<f64>>,
    pub boll_mid: Vec<Option<f64>>,
    pub boll_upper: Vec<Option<f64>>,
    pub boll_lower: Vec<Option<f64>>,
    pub vol_ma5: Vec<Option<f64>>,
    pub vol_ma10: Vec<Option<f64>>,
    pub atr14: Vec<Option<f64>>,
}

/// 单因子评分
#[derive(Debug, Clone, Serialize)]
pub struct FactorScore {
    pub name: String,
    /// -100 ~ +100
    pub score: f64,
    pub weight: f64,
    pub reasons: Vec<String>,
}

/// 综合评分结果
#[derive(Debug, Clone, Serialize)]
pub struct ScoreResult {
    /// 技术面六因子加权分（叠加基本面调整前）
    pub tech_total: f64,
    /// 基本面调整分（基于估值快照的透明加减分）
    pub fundamental_adj: f64,
    pub fundamental_notes: Vec<String>,
    /// 最终综合分 = clamp(tech_total + fundamental_adj)
    pub total: f64,
    pub rating: String,
    /// 2 强烈看多 / 1 看多 / 0 震荡 / -1 看空 / -2 强烈看空
    pub rating_level: i32,
    pub factors: Vec<FactorScore>,
}

/// 基本面估值快照（腾讯行情实时接口）
#[derive(Debug, Clone, Serialize)]
pub struct Valuation {
    /// 市盈率 TTM（亏损为负值）
    pub pe: Option<f64>,
    /// 市净率
    pub pb: Option<f64>,
    /// 总市值（亿元）
    pub market_cap: Option<f64>,
    /// 流通市值（亿元）
    pub float_cap: Option<f64>,
}

/// 财报主要指标（东方财富 F10，最新报告期）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Financials {
    /// 报告期，如 2026-06-30
    pub report_date: String,
    /// 营业总收入同比 %
    pub revenue_yoy: Option<f64>,
    /// 归母净利润同比 %
    pub profit_yoy: Option<f64>,
    /// 净资产收益率（加权）%
    pub roe: Option<f64>,
    /// 销售毛利率 %
    pub gross_margin: Option<f64>,
    /// 基本每股收益（元）
    pub eps: Option<f64>,
    /// 归母净利润（亿元）
    pub net_profit: Option<f64>,
}

// ---------- 当日盘面分析 ----------

/// 盘面单条要点
#[derive(Debug, Clone, Serialize)]
pub struct SessionPoint {
    /// 多头 / 空头 / 中性
    pub side: String,
    pub text: String,
}

/// 关键位攻防结果
#[derive(Debug, Clone, Serialize)]
pub struct LevelBattle {
    /// 如 "第一支撑 1263.01"
    pub label: String,
    pub price: f64,
    /// "守住" | "跌破" | "突破" | "承压" | "未触及"
    pub result: String,
    /// 距离当日收盘的百分比
    pub gap_pct: f64,
}

/// 次日操作预案
#[derive(Debug, Clone, Serialize)]
pub struct NextDayPlan {
    pub scenario: String,
    pub action: String,
    pub detail: String,
}

/// AI 分析建议结果
#[derive(Debug, Clone, Serialize)]
pub struct AiAdvice {
    /// 是否已配置 AI 服务
    pub configured: bool,
    /// 使用的模型（未配置时为空）
    pub model: String,
    /// AI 文本建议（markdown）
    pub content: String,
    /// 生成时间（缓存命中时为首次生成时间）
    pub generated_at: String,
    /// 是否来自缓存
    pub cached: bool,
}

/// 当日盘面分析结果
#[derive(Debug, Clone, Serialize)]
pub struct SessionReport {
    /// 当日日期
    pub date: String,
    /// 盘面定性，如 "放量长阳收复20日线"
    pub headline: String,
    /// 0~100 当日多头强度
    pub strength: u32,
    /// 多方要点
    pub bull_points: Vec<SessionPoint>,
    /// 空方要点
    pub bear_points: Vec<SessionPoint>,
    /// K线形态描述（实体、影线、振幅）
    pub candle_desc: String,
    /// 量能描述（量比、与5日均量对比）
    pub volume_desc: String,
    /// 关键位攻防
    pub battles: Vec<LevelBattle>,
    /// 次日预案（最多 3 条）
    pub next_day_plans: Vec<NextDayPlan>,
}

/// 关键价位（支撑/压力）
#[derive(Debug, Clone, Serialize)]
pub struct PriceLevel {
    pub price: f64,
    pub source: String,
}

/// 未来 N 日目标区间
#[derive(Debug, Clone, Serialize)]
pub struct TargetBand {
    pub days: u32,
    pub low: f64,
    pub mid: f64,
    pub high: f64,
    pub confidence: String,
}

/// 走势预测
#[derive(Debug, Clone, Serialize)]
pub struct Forecast {
    pub supports: Vec<PriceLevel>,
    pub resistances: Vec<PriceLevel>,
    pub targets: Vec<TargetBand>,
    pub note: String,
}

/// 建议中的关键价位
#[derive(Debug, Clone, Serialize)]
pub struct KeyLevel {
    pub label: String,
    pub price: f64,
    pub price2: Option<f64>,
    pub note: String,
}

/// 加/补仓测算
#[derive(Debug, Clone, Serialize)]
pub struct PositionMath {
    pub add_price: f64,
    pub add_shares: f64,
    pub new_cost: f64,
    pub new_shares: f64,
    pub note: String,
}

/// 当前风险风格参数（前端展示 + 透明化建议依据）
#[derive(Debug, Clone, Serialize)]
pub struct RiskParams {
    pub label: String,
    /// 止损线（相对成本）
    pub stop_loss_pct: f64,
    /// 移动止盈参考均线
    pub trailing_ma: String,
    /// 首次建仓占计划仓位比例
    pub first_build_pct: f64,
    /// 补仓比例
    pub add_ratio: f64,
    /// 反弹减仓比例
    pub cut_ratio: f64,
}

/// 操作建议
#[derive(Debug, Clone, Serialize)]
pub struct Advice {
    pub has_position: bool,
    pub action: String,
    pub headline: String,
    pub reasons: Vec<String>,
    pub key_levels: Vec<KeyLevel>,
    pub stop_loss: Option<f64>,
    pub add_math: Option<PositionMath>,
    pub risk_params: RiskParams,
    pub disclaimer: String,
}

/// 最新行情快照
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub date: String,
    pub close: f64,
    pub pct_chg: f64,
    pub turnover: f64,
    pub amount: f64,
}

/// /api/analysis/{code} 全量响应
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResponse {
    pub code: String,
    pub name: String,
    pub snapshot: Snapshot,
    pub bars: Vec<Bar>,
    pub indicators: IndicatorSeries,
    pub score: ScoreResult,
    pub forecast: Forecast,
    pub advice: Advice,
    /// 当前是否持有该股（来自持仓管理）
    pub has_position: bool,
    /// 持仓成本价（未持仓为 null）
    pub position_cost: Option<f64>,
    /// 持股数量（未持仓为 null）
    pub position_shares: Option<f64>,
    /// 基本面估值快照
    pub valuation: Option<Valuation>,
    /// 财报主要指标
    pub financials: Option<Financials>,
}

// ---------- 持仓 ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: u64,
    pub code: String,
    pub name: String,
    pub cost: f64,
    pub shares: f64,
    pub created: String,
}

#[derive(Debug, Deserialize)]
pub struct PositionInput {
    pub code: String,
    pub name: String,
    pub cost: f64,
    pub shares: f64,
}

/// 组合页单行
#[derive(Debug, Clone, Serialize)]
pub struct PortfolioRow {
    pub position: Position,
    pub close: f64,
    pub market_value: f64,
    pub profit: f64,
    pub profit_pct: f64,
    pub weight_pct: f64,
    pub score: ScoreResult,
    pub action: String,
    pub headline: String,
    pub stop_loss: Option<f64>,
    /// 该股历史卖出已实现盈亏
    pub realized_pnl: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioResponse {
    pub rows: Vec<PortfolioRow>,
    pub total_value: f64,
    pub total_cost: f64,
    pub total_profit: f64,
    pub total_profit_pct: f64,
    /// 全部历史卖出已实现盈亏合计
    pub total_realized: f64,
    pub warnings: Vec<String>,
    pub generated_at: String,
}

// ---------- 交易记录 ----------

/// 下单时点的技术面快照（用于复盘对照）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeContext {
    /// 下单时综合评分
    pub score: f64,
    /// 下单时评级
    pub rating: String,
    /// 下单时系统给出的操作建议
    pub advice_action: String,
}

/// 一笔买入/卖出操作（模拟记账）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: u64,
    pub code: String,
    pub name: String,
    /// "buy" | "sell"
    pub side: String,
    pub price: f64,
    pub shares: f64,
    /// 本次交易费用（买入=佣金+过户费；卖出=佣金+印花税+过户费）
    #[serde(default)]
    pub fee: f64,
    /// 卖出时按当时持仓成本实现的盈亏（已扣卖出费用）
    pub realized_pnl: Option<f64>,
    pub note: Option<String>,
    /// 下单时点的技术面快照
    pub context: Option<TradeContext>,
    pub created: String,
}

#[derive(Debug, Deserialize)]
pub struct TradeInput {
    pub code: String,
    pub name: Option<String>,
    pub side: String,
    pub price: f64,
    pub shares: f64,
    pub note: Option<String>,
    /// 风险风格（用于计算下单时点建议）
    pub risk: Option<String>,
}

// ---------- 观察列表 ----------

/// 观察列表添加入参
#[derive(Debug, Deserialize)]
pub struct WatchInput {
    pub code: String,
    #[serde(default)]
    pub name: String,
}

/// 观察列表行（含实时涨跌）
#[derive(Debug, Clone, Serialize)]
pub struct WatchRow {
    pub code: String,
    pub name: String,
    /// 添加时刻 "YYYY-MM-DD HH:MM"
    pub added_at: String,
    /// 基准日（添加时最近一根日K的日期）
    pub base_date: String,
    /// 基准价（盘内添加为添加当时最新价，跨日后取基准日收盘价）
    pub base_close: f64,
    /// 最新一根K线
    pub last_date: String,
    pub last_close: f64,
    /// 最新一日涨跌幅 %
    pub pct_today: f64,
    /// 自添加以来累计涨跌幅 %
    pub pct_since_add: f64,
    /// 自添加以来每日收盘价（画走势用）
    pub closes_since: Vec<f64>,
}

/// 保留 2 位小数
pub fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
