//! 多因子评分引擎：六大因子各自打分（-100~+100），加权合成综合分。
//!
//! 因子与权重：趋势 25% / 动量(MACD) 20% / 超买超卖(RSI·KDJ) 15% /
//! 量价 15% / 布林·波动 10% / K线形态 15%。

use crate::config;
use crate::models::{Bar, FactorScore, Financials, IndicatorSeries, ScoreResult, Valuation};

pub fn score(bars: &[Bar], ind: &IndicatorSeries) -> ScoreResult {
    let i = bars.len() - 1;
    let factors = vec![
        factor_trend(bars, ind, i),
        factor_momentum(ind, i),
        factor_oscillator(bars, ind, i),
        factor_volume(bars, ind, i),
        factor_boll(bars, ind, i),
        factor_pattern(bars, i),
    ];
    let total: f64 = factors.iter().map(|f| f.score * f.weight).sum();
    let total = clamp_score(total);
    let (rating, rating_level) = rating_of(total);
    ScoreResult {
        tech_total: total,
        fundamental_adj: 0.0,
        fundamental_notes: Vec::new(),
        total,
        rating: rating.to_string(),
        rating_level,
        factors,
    }
}

/// 基本面调整：估值（PE/PB）+ 财报（成长/盈利质量）的透明加减分，
/// 独立于六因子权重，便于解释与调整。加分封顶 +15，扣分封底 -30。
pub fn fundamental_adjustment(v: &Valuation, f: Option<&Financials>) -> (f64, Vec<String>) {
    let (mut adj, mut notes) = valuation_adjustment(v);
    match f {
        Some(fin) => {
            let (a, n) = growth_adjustment(fin);
            adj += a;
            notes.extend(n);
        }
        None => notes.push("暂无财报数据，仅按估值调整".into()),
    }
    (adj.clamp(-30.0, 15.0), notes)
}

/// 估值部分（PE/PB）
fn valuation_adjustment(v: &Valuation) -> (f64, Vec<String>) {
    let mut adj: f64 = 0.0;
    let mut notes = Vec::new();
    match v.pe {
        Some(pe) if pe < 0.0 => {
            adj -= 15.0;
            notes.push("当前亏损（PE<0），缺乏盈利支撑".into());
        }
        Some(pe) if pe > 100.0 => {
            adj -= 10.0;
            notes.push(format!("市盈率 {pe}，估值偏高，透支较多预期"));
        }
        Some(pe) if pe < 15.0 => {
            adj += 8.0;
            notes.push(format!("市盈率 {pe}，估值偏低"));
        }
        Some(pe) => notes.push(format!("市盈率 {pe}，估值中等")),
        None => notes.push("市盈率数据缺失".into()),
    }
    match v.pb {
        Some(pb) if pb >= 0.0 && pb < 0.8 => {
            adj += 5.0;
            notes.push(format!("市净率 {pb}，破净状态"));
        }
        Some(pb) if pb > 8.0 => {
            adj -= 5.0;
            notes.push(format!("市净率 {pb}，资产溢价较高"));
        }
        _ => {}
    }
    (adj, notes)
}

/// 财报部分（营收/净利增速、ROE、毛利率）
fn growth_adjustment(f: &Financials) -> (f64, Vec<String>) {
    let mut adj: f64 = 0.0;
    let mut notes = Vec::new();
    match f.revenue_yoy {
        Some(r) if r >= 20.0 => {
            adj += 6.0;
            notes.push(format!("营收同比 +{r:.1}%，处于高增长"));
        }
        Some(r) if r >= 10.0 => {
            adj += 3.0;
            notes.push(format!("营收同比 +{r:.1}%，稳健增长"));
        }
        Some(r) if r < -20.0 => {
            adj -= 12.0;
            notes.push(format!("营收同比 {r:.1}%，大幅下滑"));
        }
        Some(r) if r < 0.0 => {
            adj -= 8.0;
            notes.push(format!("营收同比 {r:.1}%，负增长"));
        }
        _ => {}
    }
    match f.profit_yoy {
        Some(p) if p >= 20.0 => {
            adj += 6.0;
            notes.push(format!("归母净利同比 +{p:.1}%，利润高增长"));
        }
        Some(p) if p >= 10.0 => {
            adj += 3.0;
            notes.push(format!("归母净利同比 +{p:.1}%，利润稳健增长"));
        }
        Some(p) if p < -30.0 => {
            adj -= 12.0;
            notes.push(format!("归母净利同比 {p:.1}%，利润大幅恶化"));
        }
        Some(p) if p < 0.0 => {
            adj -= 8.0;
            notes.push(format!("归母净利同比 {p:.1}%，利润下滑"));
        }
        _ => {}
    }
    match f.roe {
        Some(r) if r >= 15.0 => {
            adj += 6.0;
            notes.push(format!("ROE {r:.1}%，盈利能力优秀"));
        }
        Some(r) if r >= 10.0 => {
            adj += 3.0;
            notes.push(format!("ROE {r:.1}%，盈利能力良好"));
        }
        Some(r) if r < 0.0 => {
            adj -= 8.0;
            notes.push("ROE 为负，股东回报恶化".into());
        }
        Some(r) if r < 5.0 => {
            adj -= 5.0;
            notes.push(format!("ROE {r:.1}%，盈利能力偏弱"));
        }
        _ => {}
    }
    match f.gross_margin {
        Some(g) if g >= 40.0 => {
            adj += 3.0;
            notes.push(format!("毛利率 {g:.1}%，定价权较强"));
        }
        Some(g) if g < 15.0 => {
            adj -= 2.0;
            notes.push(format!("毛利率 {g:.1}%，偏低"));
        }
        _ => {}
    }
    (adj, notes)
}

/// 将基本面调整叠加到技术面评分，并重算评级
pub fn apply_fundamental(
    score: &mut ScoreResult,
    valuation: Option<&Valuation>,
    financials: Option<&Financials>,
) {
    let tech = score.total;
    let (adj, notes) = match (valuation, financials) {
        (Some(v), f) => fundamental_adjustment(v, f),
        (None, Some(fin)) => {
            let (a, n) = growth_adjustment(fin);
            (a, n)
        }
        _ => (0.0, vec!["暂无基本面数据，仅按技术面评分".into()]),
    };
    let total = clamp_score(tech + adj);
    let (rating, rating_level) = rating_of(total);
    score.tech_total = tech;
    score.fundamental_adj = adj;
    score.fundamental_notes = notes;
    score.total = total;
    score.rating = rating.to_string();
    score.rating_level = rating_level;
}

pub fn clamp_score(x: f64) -> f64 {
    x.clamp(-100.0, 100.0)
}

pub fn rating_of(total: f64) -> (&'static str, i32) {
    if total >= config::RATING_STRONG_BULL {
        ("强烈看多", 2)
    } else if total >= config::RATING_BULL {
        ("看多", 1)
    } else if total > config::RATING_BEAR {
        ("震荡", 0)
    } else if total > config::RATING_STRONG_BEAR {
        ("看空", -1)
    } else {
        ("强烈看空", -2)
    }
}

fn clamp(x: f64) -> f64 {
    x.clamp(-100.0, 100.0)
}

fn sv(s: &[Option<f64>], i: usize) -> Option<f64> {
    s.get(i).copied().flatten()
}

// ---------- 趋势因子 ----------

fn factor_trend(bars: &[Bar], ind: &IndicatorSeries, i: usize) -> FactorScore {
    let mut sc = 0.0;
    let mut reasons = Vec::new();
    let close = bars[i].close;
    let (ma5, ma10, ma20, ma60) =
        (sv(&ind.ma5, i), sv(&ind.ma10, i), sv(&ind.ma20, i), sv(&ind.ma60, i));

    if let Some(m20) = ma20 {
        if close > m20 { sc += 20.0; reasons.push("收盘价站上20日均线".into()); }
        else { sc -= 20.0; reasons.push("收盘价位于20日均线下方".into()); }
    }
    if let Some(m60) = ma60 {
        if close > m60 { sc += 15.0; reasons.push("收盘价站上60日均线".into()); }
        else { sc -= 15.0; reasons.push("收盘价位于60日均线下方".into()); }
    }
    if let (Some(a), Some(b)) = (ma5, ma10) {
        if a > b { sc += 15.0; reasons.push("短期均线(5>10)向上".into()); }
        else { sc -= 15.0; reasons.push("短期均线(5<10)走弱".into()); }
    }
    if let (Some(a), Some(b)) = (ma10, ma20) {
        if a > b { sc += 15.0; reasons.push("中期均线(10>20)向上".into()); }
        else { sc -= 15.0; reasons.push("中期均线(10<20)向下".into()); }
    }
    if let (Some(a), Some(b)) = (ma20, ma60) {
        if a > b { sc += 15.0; reasons.push("20日线在60日线上方，中期趋势偏多".into()); }
        else { sc -= 15.0; reasons.push("20日线在60日线下方，中期趋势偏空".into()); }
    }
    if bars.len() >= 25 {
        let recent_low = min_low(bars, i - 9, i);
        let prev_low = min_low(bars, i - 19, i - 10);
        if recent_low > prev_low { sc += 20.0; reasons.push("低点抬高，上升结构完好".into()); }
        else if recent_low < prev_low { sc -= 20.0; reasons.push("低点下移，下降结构压制".into()); }
    }
    if let (Some(a), Some(b), Some(c), Some(d)) = (ma5, ma10, ma20, ma60) {
        if a > b && b > c && c > d { reasons.push("均线呈多头排列".into()); }
        if a < b && b < c && c < d { reasons.push("均线呈空头排列".into()); }
    }
    FactorScore { name: "趋势".into(), score: clamp(sc), weight: config::W_TREND, reasons }
}

fn min_low(bars: &[Bar], from: usize, to: usize) -> f64 {
    bars[from..=to].iter().map(|b| b.low).fold(f64::MAX, f64::min)
}

fn max_high(bars: &[Bar], from: usize, to: usize) -> f64 {
    bars[from..=to].iter().map(|b| b.high).fold(f64::MIN, f64::max)
}

// ---------- 动量因子（MACD） ----------

fn factor_momentum(ind: &IndicatorSeries, i: usize) -> FactorScore {
    let mut sc = 0.0;
    let mut reasons = Vec::new();
    let (dif, dea, hist, hist_prev) = (
        sv(&ind.dif, i),
        sv(&ind.dea, i),
        sv(&ind.hist, i),
        i.checked_sub(1).and_then(|j| sv(&ind.hist, j)),
    );
    match (dif, dea, hist, hist_prev) {
        (Some(d), Some(e), Some(h), Some(hp)) => {
            if d > e {
                sc += 30.0;
                reasons.push("MACD 处于金叉状态（DIF>DEA）".into());
            } else {
                sc -= 30.0;
                reasons.push("MACD 处于死叉状态（DIF<DEA）".into());
            }
            if h > hp {
                sc += 15.0;
                if d > e { reasons.push("红柱放大，多头动能增强".into()); }
                else { reasons.push("绿柱收窄，空头动能减弱".into()); }
            } else {
                sc -= 15.0;
                if d > e { reasons.push("红柱收窄，多头动能减弱".into()); }
                else { reasons.push("绿柱放大，空头动能增强".into()); }
            }
            if d > 0.0 {
                sc += 25.0;
                reasons.push("DIF 位于零轴上方，属多头市场".into());
            } else {
                sc -= 25.0;
                reasons.push("DIF 位于零轴下方，属空头市场".into());
            }
            // 近 3 日新金叉/死叉加分
            let start = i.saturating_sub(2);
            for j in start..=i {
                if j == 0 { break; }
                if let (Some(d0), Some(e0), Some(d1), Some(e1)) =
                    (sv(&ind.dif, j - 1), sv(&ind.dea, j - 1), sv(&ind.dif, j), sv(&ind.dea, j))
                {
                    if d0 <= e0 && d1 > e1 {
                        sc += 20.0;
                        reasons.push("MACD 近日刚刚金叉，信号新鲜".into());
                        break;
                    }
                    if d0 >= e0 && d1 < e1 {
                        sc -= 20.0;
                        reasons.push("MACD 近日刚刚死叉，警惕走弱".into());
                        break;
                    }
                }
            }
        }
        _ => reasons.push("MACD 数据不足".into()),
    }
    FactorScore { name: "动量".into(), score: clamp(sc), weight: config::W_MOMENTUM, reasons }
}

// ---------- 超买超卖因子（RSI / KDJ / 背离） ----------

#[derive(PartialEq)]
enum Div {
    Bull,
    Bear,
}

fn factor_oscillator(bars: &[Bar], ind: &IndicatorSeries, i: usize) -> FactorScore {
    let mut sc = 0.0;
    let mut reasons = Vec::new();
    if let Some(r6) = sv(&ind.rsi6, i) {
        if r6 < 20.0 { sc += 35.0; reasons.push("RSI6 严重超卖，技术性反弹概率大".into()); }
        else if r6 < 30.0 { sc += 20.0; reasons.push("RSI6 进入超卖区".into()); }
        else if r6 > 80.0 { sc -= 35.0; reasons.push("RSI6 严重超买，注意回调风险".into()); }
        else if r6 > 70.0 { sc -= 20.0; reasons.push("RSI6 进入超买区".into()); }
    }
    match divergence(bars, &ind.rsi6, i) {
        Some(Div::Bull) => { sc += 25.0; reasons.push("RSI 底背离：价创新低而指标未新低".into()); }
        Some(Div::Bear) => { sc -= 25.0; reasons.push("RSI 顶背离：价创新高而指标未新高".into()); }
        None => {}
    }
    if let (Some(k), Some(d), Some(j)) = (sv(&ind.kdj_k, i), sv(&ind.kdj_d, i), sv(&ind.kdj_j, i)) {
        if k >= d { sc += 15.0; reasons.push("KDJ 处于金叉状态（K≥D）".into()); }
        else { sc -= 15.0; reasons.push("KDJ 处于死叉状态（K<D）".into()); }
        if j < 0.0 { sc += 15.0; reasons.push("J 值超卖（<0）".into()); }
        else if j > 100.0 { sc -= 15.0; reasons.push("J 值超买（>100）".into()); }
    }
    FactorScore { name: "超买超卖".into(), score: clamp(sc), weight: config::W_OSCILLATOR, reasons }
}

fn is_swing_low(bars: &[Bar], j: usize, w: usize) -> bool {
    let lo = j.saturating_sub(w);
    let hi = (j + w).min(bars.len() - 1);
    let v = bars[j].low;
    bars[lo..=hi].iter().all(|b| b.low >= v)
}

fn is_swing_high(bars: &[Bar], j: usize, w: usize) -> bool {
    let lo = j.saturating_sub(w);
    let hi = (j + w).min(bars.len() - 1);
    let v = bars[j].high;
    bars[lo..=hi].iter().all(|b| b.high <= v)
}

/// RSI 背离检测（近 50 日内最近两个摆动低点/高点）
fn divergence(bars: &[Bar], rsi6: &[Option<f64>], i: usize) -> Option<Div> {
    if bars.len() < 40 {
        return None;
    }
    let start = i.saturating_sub(50).max(3);
    let (mut lows, mut highs) = (Vec::new(), Vec::new());
    for j in start..i.saturating_sub(1) {
        if is_swing_low(bars, j, 2) { lows.push(j); }
        if is_swing_high(bars, j, 2) { highs.push(j); }
    }
    if lows.len() >= 2 {
        let a = lows[lows.len() - 2];
        let b = lows[lows.len() - 1];
        if b > i.saturating_sub(20) {
            if let (Some(ra), Some(rb)) = (sv(rsi6, a), sv(rsi6, b)) {
                if bars[b].low < bars[a].low && rb > ra + 3.0 {
                    return Some(Div::Bull);
                }
            }
        }
    }
    if highs.len() >= 2 {
        let a = highs[highs.len() - 2];
        let b = highs[highs.len() - 1];
        if b > i.saturating_sub(20) {
            if let (Some(ra), Some(rb)) = (sv(rsi6, a), sv(rsi6, b)) {
                if bars[b].high > bars[a].high && rb < ra - 3.0 {
                    return Some(Div::Bear);
                }
            }
        }
    }
    None
}

// ---------- 量价因子 ----------

fn factor_volume(bars: &[Bar], ind: &IndicatorSeries, i: usize) -> FactorScore {
    let mut sc = 0.0;
    let mut reasons = Vec::new();
    let vol = bars[i].volume;
    let vma5 = sv(&ind.vol_ma5, i);
    if let Some(v5) = vma5 {
        if v5 > 1e-9 {
            let ratio = vol / v5;
            let up = bars[i].pct_chg > 0.0;
            if up {
                if ratio >= 1.5 { sc += 40.0; reasons.push("放量上涨，资金流入积极".into()); }
                else if ratio >= 0.9 { sc += 18.0; reasons.push("量能平稳上涨".into()); }
                else { sc += 5.0; reasons.push("缩量上涨，追涨动能一般".into()); }
            } else {
                if ratio >= 1.5 { sc -= 40.0; reasons.push("放量下跌，抛压沉重".into()); }
                else if ratio >= 0.9 { sc -= 18.0; reasons.push("量能平稳下跌".into()); }
                else { sc += 12.0; reasons.push("缩量回调，抛压有限".into()); }
            }
        }
    }
    if i >= 10 {
        if let (Some(v_now), Some(v_prev)) = (sv(&ind.vol_ma5, i), sv(&ind.vol_ma5, i - 5)) {
            let chg5: f64 = bars[i - 4..=i].iter().map(|b| b.pct_chg).sum();
            if chg5 > 0.0 && v_now > v_prev * 1.05 {
                sc += 15.0;
                reasons.push("近5日量价齐升".into());
            } else if chg5 < 0.0 && v_now > v_prev * 1.05 {
                sc -= 15.0;
                reasons.push("近5日下跌放量，量价背离".into());
            } else if chg5 < 0.0 && v_now < v_prev * 0.9 {
                sc += 8.0;
                reasons.push("近5日缩量整理，抛压趋弱".into());
            }
        }
    }
    FactorScore { name: "量价".into(), score: clamp(sc), weight: config::W_VOLUME, reasons }
}

// ---------- 布林/波动因子 ----------

fn factor_boll(bars: &[Bar], ind: &IndicatorSeries, i: usize) -> FactorScore {
    let mut sc = 0.0;
    let mut reasons = Vec::new();
    let close = bars[i].close;
    let (mid, up, low) =
        (sv(&ind.boll_mid, i), sv(&ind.boll_upper, i), sv(&ind.boll_lower, i));
    if let (Some(m), Some(u), Some(l)) = (mid, up, low) {
        if close > m {
            sc += 30.0;
            reasons.push("股价运行于布林中轨上方，强势区".into());
        } else {
            sc -= 30.0;
            reasons.push("股价运行于布林中轨下方，弱势区".into());
        }
        if close > u {
            sc -= 15.0;
            reasons.push("触及布林上轨，短线超买需防回踩".into());
        } else if close < l {
            sc += 15.0;
            reasons.push("触及布林下轨，短线超卖有反弹需求".into());
        }
        // 带宽挤压：与近 120 日最小带宽比较
        let bw = (u - l) / m;
        if bw > 0.0 {
            let from = i.saturating_sub(120).max(19);
            let min_bw = (from..=i)
                .filter_map(|j| match (sv(&ind.boll_upper, j), sv(&ind.boll_lower, j), sv(&ind.boll_mid, j)) {
                    (Some(u2), Some(l2), Some(m2)) if m2 > 0.0 => Some((u2 - l2) / m2),
                    _ => None,
                })
                .fold(f64::MAX, f64::min);
            if min_bw < f64::MAX && bw <= min_bw * 1.25 {
                reasons.push("布林带宽收窄至阶段低位，变盘窗口临近".into());
            }
        }
    } else {
        reasons.push("布林数据不足".into());
    }
    FactorScore { name: "布林波动".into(), score: clamp(sc), weight: config::W_BOLL, reasons }
}

// ---------- K线形态因子 ----------

fn factor_pattern(bars: &[Bar], i: usize) -> FactorScore {
    let mut sc = 0.0;
    let mut reasons = Vec::new();
    if i < 3 {
        return FactorScore { name: "K线形态".into(), score: 0.0, weight: config::W_PATTERN, reasons };
    }
    let b = &bars[i];
    let p = &bars[i - 1];
    let body = (b.close - b.open).abs();
    let range = b.high - b.low;
    if range > 1e-9 {
        let lower = b.open.min(b.close) - b.low;
        let upper = b.high - b.open.max(b.close);
        if lower >= body * 2.0 && lower >= range * 0.5 {
            sc += 20.0;
            reasons.push("长下影线，下方承接有力".into());
        }
        if upper >= body * 2.0 && upper >= range * 0.5 {
            sc -= 20.0;
            reasons.push("长上影线，上方抛压明显".into());
        }
    }
    let p_body = (p.close - p.open).abs();
    if p.close < p.open && b.close > b.open && b.close >= p.open && b.open <= p.close && body > p_body {
        sc += 25.0;
        reasons.push("阳线吞没前日阴线，短线反转信号".into());
    }
    if p.close > p.open && b.close < b.open && b.open >= p.close && b.close <= p.open && body > p_body {
        sc -= 25.0;
        reasons.push("阴线吞没前日阳线，短线转弱信号".into());
    }
    if i >= 3 {
        let up3 = bars[i - 2..=i].iter().all(|x| x.close > x.open);
        let dn3 = bars[i - 2..=i].iter().all(|x| x.close < x.open);
        if up3 {
            sc += 15.0;
            reasons.push("三连阳，短线走强".into());
        } else if dn3 {
            sc -= 15.0;
            reasons.push("三连阴，短线承压".into());
        }
    }
    if b.low > p.high * 1.004 {
        sc += 18.0;
        reasons.push("向上跳空，缺口构成支撑".into());
    } else if b.high < p.low * 0.996 {
        sc -= 18.0;
        reasons.push("向下跳空，缺口构成压制".into());
    }
    if b.pct_chg >= 5.0 {
        sc += 10.0;
        reasons.push("单日大涨（≥5%）".into());
    } else if b.pct_chg <= -5.0 {
        sc -= 10.0;
        reasons.push("单日大跌（≤-5%）".into());
    }
    FactorScore { name: "K线形态".into(), score: clamp(sc), weight: config::W_PATTERN, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators;

    fn synth(deltas: &[f64]) -> Vec<Bar> {
        let mut bars = Vec::with_capacity(deltas.len());
        let mut close = 10.0;
        for (k, &d) in deltas.iter().enumerate() {
            let open = close;
            close += d;
            bars.push(Bar {
                date: format!("d{k}"),
                open,
                close,
                high: close.max(open) + 0.1,
                low: close.min(open) - 0.1,
                volume: 1000.0 + k as f64,
                amount: 0.0,
                turnover: 1.0,
                pct_chg: d / open * 100.0,
            });
        }
        bars
    }

    #[test]
    fn test_uptrend_scores_positive() {
        let deltas: Vec<f64> = std::iter::repeat(0.15).take(130).collect();
        let bars = synth(&deltas);
        let ind = indicators::compute(&bars);
        let r = score(&bars, &ind);
        assert!(r.total > 15.0, "上升行情综合分应看多，实际 {} ({})", r.total, r.rating);
    }

    #[test]
    fn test_downtrend_scores_negative() {
        let deltas: Vec<f64> = std::iter::repeat(-0.15).take(130).collect();
        let bars = synth(&deltas);
        let ind = indicators::compute(&bars);
        let r = score(&bars, &ind);
        assert!(r.total < -15.0, "下跌行情综合分应看空，实际 {} ({})", r.total, r.rating);
    }

    #[test]
    fn test_no_panic_on_short_data() {
        let bars = synth(&[0.1, -0.1, 0.2, 0.05, -0.05]);
        let ind = indicators::compute(&bars);
        let r = score(&bars, &ind); // 不应 panic
        assert!(r.total.abs() <= 100.0);
    }

    #[test]
    fn test_rating_thresholds() {
        assert_eq!(rating_of(50.0), ("强烈看多", 2));
        assert_eq!(rating_of(20.0), ("看多", 1));
        assert_eq!(rating_of(0.0), ("震荡", 0));
        assert_eq!(rating_of(-20.0), ("看空", -1));
        assert_eq!(rating_of(-50.0), ("强烈看空", -2));
    }

    #[test]
    fn test_fundamental_adjustment() {
        let loss = Valuation { pe: Some(-2.0), pb: Some(2.0), market_cap: None, float_cap: None };
        let (adj, notes) = valuation_adjustment(&loss);
        assert!((adj + 15.0).abs() < 1e-9);
        assert!(notes[0].contains("亏损"));

        // 低估值 + 破净：+8 +5 = +13
        let cheap = Valuation { pe: Some(8.0), pb: Some(0.6), market_cap: None, float_cap: None };
        assert!((valuation_adjustment(&cheap).0 - 13.0).abs() < 1e-9);

        // 高估值：-10 -5 = -15
        let pricey = Valuation { pe: Some(150.0), pb: Some(10.0), market_cap: None, float_cap: None };
        assert!((valuation_adjustment(&pricey).0 + 15.0).abs() < 1e-9);

        // 中等估值：0
        let mid = Valuation { pe: Some(30.0), pb: Some(3.0), market_cap: None, float_cap: None };
        assert!(valuation_adjustment(&mid).0.abs() < 1e-9);
    }

    #[test]
    fn test_growth_adjustment() {
        let strong = Financials {
            report_date: "2026-06-30".into(),
            revenue_yoy: Some(35.0),
            profit_yoy: Some(40.0),
            roe: Some(18.0),
            gross_margin: Some(50.0),
            eps: None,
            net_profit: None,
        };
        // +6 +6 +6 +3 = +21（封顶在组合口径 fundamental_adjustment 中执行）
        assert!((growth_adjustment(&strong).0 - 21.0).abs() < 1e-9);

        let weak = Financials {
            report_date: "2026-06-30".into(),
            revenue_yoy: Some(-25.0),
            profit_yoy: Some(-40.0),
            roe: Some(-2.0),
            gross_margin: Some(10.0),
            eps: None,
            net_profit: None,
        };
        // -12 -12 -8 -2 = -34（封底在组合口径中执行）
        assert!((growth_adjustment(&weak).0 + 34.0).abs() < 1e-9);

        // 组合：估值中等 0 + 财报高增 21 → 封顶 15
        let mid_v = Valuation { pe: Some(30.0), pb: Some(3.0), market_cap: None, float_cap: None };
        let (adj, _) = fundamental_adjustment(&mid_v, Some(&strong));
        assert!((adj - 15.0).abs() < 1e-9);
    }
}
