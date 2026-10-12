//! 走势预测：支撑/压力位识别 + 基于综合评分与 ATR 的目标区间推算。

use crate::models::IndicatorSeries;
use crate::models::{round2, Bar, Forecast, PriceLevel, TargetBand};

/// 支撑/压力去重容差（相对价格 1%）
const LEVEL_TOL: f64 = 0.01;

pub fn forecast(bars: &[Bar], ind: &IndicatorSeries, total: f64) -> Forecast {
    let i = bars.len() - 1;
    let close = bars[i].close;
    let atr = sv(&ind.atr14, i).unwrap_or_else(|| avg_range(bars, i, 14));

    let supports = pick_supports(bars, ind, i, close, atr);
    let resistances = pick_resistances(bars, ind, i, close, atr);
    let targets = target_bands(close, atr, total);

    Forecast {
        supports,
        resistances,
        targets,
        note: "以上支撑/压力位由近期高低点、均线与布林轨道聚合识别，目标区间按 ATR 波动率与综合评分推算，均为技术面参考，不构成投资建议。".into(),
    }
}

fn pick_supports(bars: &[Bar], ind: &IndicatorSeries, i: usize, close: f64, atr: f64) -> Vec<PriceLevel> {
    let mut cands: Vec<PriceLevel> = Vec::new();
    let push = |cands: &mut Vec<PriceLevel>, price: f64, source: &str| {
        cands.push(PriceLevel { price: round2(price), source: source.into() })
    };
    if bars.len() >= 20 {
        push(&mut cands, min_low(bars, i - 19, i), "近20日低点");
    }
    if bars.len() >= 60 {
        push(&mut cands, min_low(bars, i - 59, i), "近60日低点");
    }
    if let Some(m) = sv(&ind.ma20, i) {
        push(&mut cands, m, "20日均线");
    }
    if let Some(m) = sv(&ind.ma60, i) {
        push(&mut cands, m, "60日均线");
    }
    if let Some(l) = sv(&ind.boll_lower, i) {
        push(&mut cands, l, "布林下轨");
    }
    let below: Vec<PriceLevel> = cands
        .into_iter()
        .filter(|p| p.price <= close * 0.998 && p.price > 0.0)
        .collect();
    let mut out = dedupe(below);
    out.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());
    out.truncate(3);
    if out.is_empty() {
        let p = round2((close - 2.0 * atr).max(0.01));
        out.push(PriceLevel { price: p, source: "ATR 波动推算".into() });
    }
    out
}

fn pick_resistances(bars: &[Bar], ind: &IndicatorSeries, i: usize, close: f64, atr: f64) -> Vec<PriceLevel> {
    let mut cands: Vec<PriceLevel> = Vec::new();
    let mut push = |cands: &mut Vec<PriceLevel>, price: f64, source: &str| {
        cands.push(PriceLevel { price: round2(price), source: source.into() })
    };
    if bars.len() >= 20 {
        push(&mut cands, max_high(bars, i - 19, i), "近20日高点");
    }
    if bars.len() >= 60 {
        push(&mut cands, max_high(bars, i - 59, i), "近60日高点");
    }
    if bars.len() >= 120 {
        // 更早的前高（排除最近20日）
        push(&mut cands, max_high(bars, i - 119, i - 20), "半年内前高");
    }
    if let Some(u) = sv(&ind.boll_upper, i) {
        push(&mut cands, u, "布林上轨");
    }
    let above: Vec<PriceLevel> = cands
        .into_iter()
        .filter(|p| p.price >= close * 1.002)
        .collect();
    let mut out = dedupe(above);
    out.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());
    out.truncate(3);
    if out.is_empty() {
        let p = round2(close + 2.0 * atr);
        out.push(PriceLevel { price: p, source: "ATR 波动推算".into() });
    }
    out
}

/// 按评分方向 + ATR 波动率推算未来 5/10/20 日区间
fn target_bands(close: f64, atr: f64, total: f64) -> Vec<TargetBand> {
    let mut out = Vec::new();
    for days in [5u32, 10, 20] {
        let sigma = atr * (days as f64).sqrt();
        let confidence = if total.abs() >= 40.0 {
            "较高"
        } else if total.abs() >= 20.0 {
            "中等"
        } else {
            "较低"
        };
        let (low, mid, high) = if total.abs() < 15.0 {
            (close - 0.7 * sigma, close, close + 0.7 * sigma)
        } else {
            let drift = (total / 100.0) * 0.6 * sigma;
            (close + drift - 0.7 * sigma, close + drift, close + drift + 0.7 * sigma)
        };
        out.push(TargetBand {
            days,
            low: round2(low.max(0.01)),
            mid: round2(mid),
            high: round2(high),
            confidence: confidence.into(),
        });
    }
    out
}

/// 相邻 1% 内的价位视为同一水平，保留先出现的
fn dedupe(mut levels: Vec<PriceLevel>) -> Vec<PriceLevel> {
    levels.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());
    let mut out: Vec<PriceLevel> = Vec::new();
    for p in levels {
        let dup = out
            .iter()
            .any(|q: &PriceLevel| (p.price - q.price).abs() <= q.price * LEVEL_TOL);
        if !dup {
            out.push(p);
        }
    }
    out
}

fn min_low(bars: &[Bar], from: usize, to: usize) -> f64 {
    bars[from..=to].iter().map(|b| b.low).fold(f64::MAX, f64::min)
}

fn max_high(bars: &[Bar], from: usize, to: usize) -> f64 {
    bars[from..=to].iter().map(|b| b.high).fold(f64::MIN, f64::max)
}

fn avg_range(bars: &[Bar], i: usize, n: usize) -> f64 {
    let from = i.saturating_sub(n - 1);
    let cnt = (i - from + 1) as f64;
    bars[from..=i].iter().map(|b| b.high - b.low).sum::<f64>() / cnt
}

fn sv(s: &[Option<f64>], i: usize) -> Option<f64> {
    s.get(i).copied().flatten()
}
