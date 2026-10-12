//! 当日盘面分析：基于最新日线对当日走势做技术复盘，
//! 输出多空要点、K线/量能解读、关键位攻防与次日操作预案。

use crate::models::IndicatorSeries as IndSeries;
use crate::models::{round2, Bar, LevelBattle, NextDayPlan, ScoreResult, SessionPoint, SessionReport};
use crate::scoring::rating_of;

pub fn analyze(
    bars: &[Bar],
    ind: &IndSeries,
    score: &ScoreResult,
    has_position: bool,
) -> SessionReport {
    let i = bars.len() - 1;
    let today = &bars[i];
    let prev = &bars[i - 1];
    let close = today.close;

    // ---- K线形态解读 ----
    let body = (today.close - today.open).abs();
    let range = (today.high - today.low).max(1e-9);
    let upper_wick = today.high - today.open.max(today.close);
    let lower_wick = today.open.min(today.close) - today.low;
    let body_pct = body / prev.close * 100.0;
    let amp = today.pct_chg.abs() + (today.high - today.low) / prev.close * 100.0;

    let candle_shape = if body / range < 0.12 {
        "十字星（多空平衡）"
    } else if today.close >= today.open && lower_wick > body * 1.5 {
        "带长下影的阳线（探底回升）"
    } else if today.close >= today.open && upper_wick > body * 1.5 {
        "带长上影的阳线（冲高回落）"
    } else if today.close < today.open && upper_wick > body * 1.5 {
        "带长上影的阴线（反弹受阻）"
    } else if today.close < today.open && lower_wick > body * 1.5 {
        "带长下影的阴线（下跌抵抗）"
    } else if today.close >= today.open {
        "实体阳线"
    } else {
        "实体阴线"
    };
    let candle_desc = format!(
        "{}：实体 {:.1}%，振幅 {:.1}%，上影 {:.1}% / 下影 {:.1}%",
        candle_shape,
        body_pct,
        amp,
        upper_wick / prev.close * 100.0,
        lower_wick / prev.close * 100.0
    );

    // ---- 量能解读 ----
    let (volume_desc, vol_ratio) = volume_desc(bars, ind, i);

    // ---- 多空要点 ----
    let (bull, bear) = points(bars, ind, i, today, vol_ratio);

    // ---- 关键位攻防 ----
    let battles = level_battles(bars, ind, i, close, today);

    // ---- 多头强度（0~100）----
    let strength = strength_score(&bull, &bear, score);

    // ---- 盘面定性 ----
    let (rating, _) = rating_of(score.total);
    let headline = format!(
        "{}，{}，评分 {}（{}）",
        if today.pct_chg >= 3.0 {
            format!("放量大涨{:.1}%", today.pct_chg)
        } else if today.pct_chg >= 0.0 {
            format!("收涨{:.1}%", today.pct_chg)
        } else if today.pct_chg <= -3.0 {
            format!("大跌{:.1}%", today.pct_chg)
        } else {
            format!("收跌{:.1}%", today.pct_chg)
        },
        candle_shape,
        score.total as i32,
        rating
    );

    // ---- 次日预案 ----
    let plans = next_day_plans(bars, ind, i, score, has_position, &battles);

    SessionReport {
        date: today.date.clone(),
        headline,
        strength,
        bull_points: bull,
        bear_points: bear,
        candle_desc,
        volume_desc,
        battles,
        next_day_plans: plans,
    }
}

fn volume_desc(bars: &[Bar], ind: &IndSeries, i: usize) -> (String, f64) {
    let vol = bars[i].volume;
    let sv = |j: usize| ind.vol_ma5.get(j).copied().flatten();
    let base = if i >= 5 {
        bars[i - 5..i].iter().map(|b| b.volume).sum::<f64>() / 5.0
    } else {
        sv(i).unwrap_or(vol)
    };
    let ratio = if base > 1e-9 { vol / base } else { 1.0 };
    let desc = if ratio >= 2.5 {
        format!("显著放量（量比 {:.2}），成交极为活跃，注意高位换手风险", ratio)
    } else if ratio >= 1.5 {
        format!("放量（量比 {:.2}），资金参与度明显提升", ratio)
    } else if ratio >= 0.8 {
        format!("量能持平（量比 {:.2}），交投平稳", ratio)
    } else if ratio >= 0.5 {
        format!("缩量（量比 {:.2}），多空观望情绪浓", ratio)
    } else {
        format!("显著缩量（量比 {:.2}），交投清淡，变盘窗口临近", ratio)
    };
    (desc, ratio)
}

#[allow(clippy::too_many_arguments)]
fn points(
    bars: &[Bar],
    ind: &IndSeries,
    i: usize,
    today: &Bar,
    vol_ratio: f64,
) -> (Vec<SessionPoint>, Vec<SessionPoint>) {
    let mut bull = Vec::new();
    let mut bear = Vec::new();
    let close = today.close;
    let sv = |s: &[Option<f64>], j: usize| s.get(j).copied().flatten();

    // 涨跌与位置
    if today.pct_chg >= 1.0 {
        bull.push(SessionPoint { side: "多头".into(), text: format!("当日上涨 {:.1}%", today.pct_chg) });
    } else if today.pct_chg <= -1.0 {
        bear.push(SessionPoint { side: "空头".into(), text: format!("当日下跌 {:.1}%", today.pct_chg) });
    }
    for (name, s) in [("5日线", &ind.ma5), ("20日线", &ind.ma20), ("60日线", &ind.ma60)] {
        if let Some(m) = sv(s, i) {
            if close > m {
                bull.push(SessionPoint { side: "多头".into(), text: format!("收盘站上{}（{}）", name, round2(m)) });
            } else {
                bear.push(SessionPoint { side: "空头".into(), text: format!("收盘受制于{}（{}）", name, round2(m)) });
            }
        }
    }
    // 量价配合
    if today.pct_chg >= 0.0 && vol_ratio >= 1.5 {
        bull.push(SessionPoint { side: "多头".into(), text: "放量上涨，资金流入态度明确".into() });
    } else if today.pct_chg < 0.0 && vol_ratio >= 1.5 {
        bear.push(SessionPoint { side: "空头".into(), text: "放量下跌，抛压真实沉重".into() });
    } else if today.pct_chg < 0.0 && vol_ratio < 0.8 {
        bull.push(SessionPoint { side: "多头".into(), text: "缩量回调，恐慌抛压有限".into() });
    } else if today.pct_chg >= 0.0 && vol_ratio < 0.8 {
        bear.push(SessionPoint { side: "空头".into(), text: "缩量上涨，追涨意愿不足".into() });
    }
    // 影线
    let body = (today.close - today.open).abs().max(1e-9);
    let lower = today.open.min(today.close) - today.low;
    let upper = today.high - today.open.max(today.close);
    if lower > body * 1.5 {
        bull.push(SessionPoint { side: "多头".into(), text: "长下影承接，低位有资金护盘".into() });
    }
    if upper > body * 1.5 {
        bear.push(SessionPoint { side: "空头".into(), text: "长上影压制，高位抛盘明显".into() });
    }
    // MACD 与 KDJ 状态
    if let (Some(d), Some(e)) = (sv(&ind.dif, i), sv(&ind.dea, i)) {
        if d > e {
            bull.push(SessionPoint { side: "多头".into(), text: "MACD 金叉状态，动能偏多".into() });
        } else {
            bear.push(SessionPoint { side: "空头".into(), text: "MACD 死叉状态，动能偏空".into() });
        }
    }
    if let Some(r6) = sv(&ind.rsi6, i) {
        if r6 < 25.0 {
            bull.push(SessionPoint { side: "多头".into(), text: format!("RSI6 仅 {:.0}，短线超卖随时反抽", r6) });
        } else if r6 > 80.0 {
            bear.push(SessionPoint { side: "空头".into(), text: format!("RSI6 高达 {:.0}，短线超买注意回踩", r6) });
        }
    }
    // 连续形态
    if i >= 3 {
        if bars[i - 2..=i].iter().all(|b| b.close > b.open) {
            bull.push(SessionPoint { side: "多头".into(), text: "三连阳，短线节奏偏强".into() });
        } else if bars[i - 2..=i].iter().all(|b| b.close < b.open) {
            bear.push(SessionPoint { side: "空头".into(), text: "三连阴，短线节奏偏弱".into() });
        }
    }
    (bull, bear)
}

fn level_battles(
    bars: &[Bar],
    ind: &IndSeries,
    i: usize,
    close: f64,
    today: &Bar,
) -> Vec<LevelBattle> {
    let sv = |s: &[Option<f64>], j: usize| s.get(j).copied().flatten();
    let mut cands: Vec<(String, f64)> = Vec::new();
    if let Some(m) = sv(&ind.ma20, i) {
        cands.push(("20日线".into(), m));
    }
    if let Some(m) = sv(&ind.ma60, i) {
        cands.push(("60日线".into(), m));
    }
    if i >= 20 {
        cands.push(("近20日高点".into(), bars[i - 19..=i].iter().map(|b| b.high).fold(f64::MIN, f64::max)));
        cands.push(("近20日低点".into(), bars[i - 19..=i].iter().map(|b| b.low).fold(f64::MAX, f64::min)));
    }
    if let (Some(u), Some(l)) = (sv(&ind.boll_upper, i), sv(&ind.boll_lower, i)) {
        cands.push(("布林上轨".into(), u));
        cands.push(("布林下轨".into(), l));
    }
    // 去重（1.5% 内视为同一水平）并按 |距收盘| 排序取近的
    cands.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let mut uniq: Vec<(String, f64)> = Vec::new();
    for (name, p) in cands {
        if !uniq.iter().any(|(_, q)| (p - q).abs() <= q * 0.015) {
            uniq.push((name, p));
        }
    }
    uniq.sort_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap());
    uniq.truncate(6);

    uniq.into_iter()
        .map(|(name, p)| {
            let label = format!("{name} {}", round2(p));
            let gap = (p - close) / close * 100.0;
            let intraday_touch = today.low <= p && today.high >= p;
            let result = if intraday_touch {
                if p <= today.open {
                    if close >= p { "守住" } else { "跌破" }
                } else if close >= p {
                    "突破"
                } else {
                    "承压"
                }
            } else if gap > 0.0 {
                "未触及（上方）"
            } else {
                "未触及（下方）"
            };
            LevelBattle { label, price: round2(p), result: result.into(), gap_pct: round2(gap) }
        })
        .collect()
}

fn strength_score(bull: &[SessionPoint], bear: &[SessionPoint], score: &ScoreResult) -> u32 {
    let raw = 50.0 + (bull.len() as f64 - bear.len() as f64) * 6.0 + score.total * 0.2;
    raw.clamp(0.0, 100.0) as u32
}

fn next_day_plans(
    bars: &[Bar],
    ind: &IndSeries,
    i: usize,
    score: &ScoreResult,
    has_position: bool,
    battles: &[LevelBattle],
) -> Vec<NextDayPlan> {
    let close = bars[i].close;
    let atr = ind
        .atr14
        .get(i)
        .copied()
        .flatten()
        .unwrap_or_else(|| (bars[i].high - bars[i].low).max(close * 0.01));
    let sv = |s: &[Option<f64>], j: usize| s.get(j).copied().flatten();
    let ma5 = sv(&ind.ma5, i);
    let support = battles
        .iter()
        .filter(|b| b.price < close * 0.999)
        .max_by(|a, b| a.price.partial_cmp(&b.price).unwrap())
        .map(|b| b.price)
        .unwrap_or(close - 2.0 * atr);
    let resist = battles
        .iter()
        .filter(|b| b.price > close * 1.001)
        .min_by(|a, b| a.price.partial_cmp(&b.price).unwrap())
        .map(|b| b.price)
        .unwrap_or(close + 2.0 * atr);
    let pos_tag = if has_position { "（持仓者）" } else { "（未持仓者）" };
    let anchor = if score.total >= 0.0 { ma5.unwrap_or(close) } else { close };
    let anchor_name = if score.total >= 0.0 { "5日线" } else { "今日收盘价" };

    let mut plans = vec![
        NextDayPlan {
            scenario: format!("强势延续：站稳 {}（{}）上方", round2(anchor), anchor_name),
            action: if score.total >= 0.0 { "顺势持有/加仓" } else { "反弹减仓" }.into(),
            detail: format!(
                "{}开盘不破 {} 且量能延续，则按当前评分（{}）方向操作；未持仓者回踩不破可轻仓试多",
                pos_tag,
                round2(anchor),
                score.total as i32
            ),
        },
        NextDayPlan {
            scenario: format!("回踩支撑：回落至 {} 附近", round2(support)),
            action: if score.total >= -15.0 { "支撑企稳低吸 / 持仓者补仓摊薄" } else { "破位减仓，不接飞刀" }.into(),
            detail: format!(
                "缩量回踩不破可低吸（止损 {}）；放量跌破则离场观望，等重新收复",
                round2(support * 0.97)
            ),
        },
        NextDayPlan {
            scenario: format!("突破压力：放量站上 {}", round2(resist)),
            action: if score.total >= 15.0 { "突破跟随，注意回踩确认" } else { "等回踩确认再介入" }.into(),
            detail: format!(
                "放量突破 {} 后回落不破该位（±1%）视为有效突破；假突破（次日跌回）则止损",
                round2(resist)
            ),
        },
    ];
    // ATR 提示
    plans.insert(
        1,
        NextDayPlan {
            scenario: "中性震荡".into(),
            action: "区间内不动".into(),
            detail: format!(
                "正常波动幅度约 ±{:.1}%（ATR {:.2}），在 {} ~ {} 之间震荡属正常，不必频繁操作",
                atr / close * 100.0,
                round2(atr),
                round2(close - atr),
                round2(close + atr)
            ),
        },
    );
    plans
}
