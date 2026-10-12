//! 操作建议引擎：综合评分 + 关键价位 + 持仓成本 + 风险偏好 → 可执行的操作建议。
//!
//! 已持仓时按「盈亏状态（浮盈/成本附近/浅套/深套）× 趋势评级」矩阵给出结论，
//! 并给出具体止损价、止盈/减仓位与补仓测算。

use crate::config::{Risk, TrailingMa};
use crate::models::{
    round2, Advice, Bar, Forecast, IndicatorSeries, KeyLevel, PositionMath, RiskParams,
    ScoreResult,
};

pub struct PositionView {
    pub cost: f64,
    pub shares: f64,
}

const DISCLAIMER: &str = "以上建议由技术指标规则自动生成，仅供参考，不构成任何投资建议。股市有风险，投资需谨慎。";

#[derive(PartialEq, Debug)]
enum Pnl {
    /// 浮盈 ≥ 5%
    Profit,
    /// 成本附近 ±5%
    Near,
    /// 浅套 -15% ~ -5%
    ShallowLoss,
    /// 深套 ≤ -15%
    DeepLoss,
}

pub fn advise(
    score: &ScoreResult,
    fc: &Forecast,
    bars: &[Bar],
    ind: &IndicatorSeries,
    pos: Option<PositionView>,
    risk: Risk,
) -> Advice {
    let i = bars.len() - 1;
    let close = bars[i].close;
    let level = score.rating_level;
    let s1 = fc.supports.first().map(|p| p.price);
    let r1 = fc.resistances.first().map(|p| p.price);
    let r2 = fc.resistances.get(1).map(|p| p.price);
    let trailing = trailing_price(risk.trailing_ma(), ind, i);
    let sl_pct = risk.stop_loss_pct();

    let mut advice = match pos {
        None => advise_new(score, level, close, s1, r1, r2, risk, sl_pct),
        Some(p) => {
            let pnl = profit_bucket(p.cost, close);
            advise_holding(score, level, close, p, pnl, s1, r1, r2, trailing, risk, sl_pct, i, bars)
        }
    };
    advice.risk_params = risk_params(risk);
    advice.disclaimer = DISCLAIMER.into();
    advice
}

fn risk_params(risk: Risk) -> RiskParams {
    RiskParams {
        label: risk.label().into(),
        stop_loss_pct: risk.stop_loss_pct(),
        trailing_ma: risk.trailing_ma().label().into(),
        first_build_pct: risk.first_build_pct(),
        add_ratio: risk.add_ratio(),
        cut_ratio: risk.cut_ratio(),
    }
}

// ---------- 未持仓 ----------

#[allow(clippy::too_many_arguments)]
fn advise_new(
    score: &ScoreResult,
    level: i32,
    close: f64,
    s1: Option<f64>,
    r1: Option<f64>,
    r2: Option<f64>,
    risk: Risk,
    sl_pct: f64,
) -> Advice {
    let mut advice = Advice {
        has_position: false,
        action: String::new(),
        headline: String::new(),
        reasons: Vec::new(),
        key_levels: Vec::new(),
        stop_loss: None,
        add_math: None,
        risk_params: risk_params(risk),
        disclaimer: String::new(),
    };
    match level {
        1 | 2 => {
            let support = s1.unwrap_or(close * 0.95);
            let stop = (support * 0.97).max(close * (1.0 - sl_pct));
            advice.action = if level == 2 { "积极建仓".into() } else { "逢低建仓".into() };
            advice.headline = if level == 2 {
                "技术面强烈看多，可按纪律分批建仓".into()
            } else {
                "技术面看多，可分批低吸建仓".into()
            };
            advice.reasons = vec![
                format!("当前评分 {}（{}），多头因素占优", score.total as i32, score.rating),
                format!("建议在第一支撑 {} 附近分批低吸，避免追高", support),
                format!("首次建仓建议不超过计划仓位的 {}%，站稳支撑后逐步加仓", (risk.first_build_pct() * 100.0) as i32),
                format!("跌破止损价 {} 或评分转负即离场", round2(stop)),
            ];
            advice.key_levels = vec![
                KeyLevel {
                    label: "买入参考区".into(),
                    price: round2(support * 0.99),
                    price2: Some(round2(support * 1.02)),
                    note: "第一支撑附近低吸".into(),
                },
                KeyLevel { label: "止损价".into(), price: round2(stop), price2: None, note: "跌破止损离场".into() },
            ];
            if let Some(r) = r1 {
                advice.key_levels.push(KeyLevel { label: "第一目标位".into(), price: r, price2: None, note: "到达可部分止盈".into() });
            }
            if let Some(r) = r2 {
                advice.key_levels.push(KeyLevel { label: "第二目标位".into(), price: r, price2: None, note: "趋势延续目标".into() });
            }
            advice.stop_loss = Some(round2(stop));
        }
        0 => {
            advice.action = "观望".into();
            advice.headline = "多空拉锯、方向未明，等待企稳信号再介入".into();
            advice.reasons = vec![
                format!("当前评分 {}（{}），方向信号不明确", score.total as i32, score.rating),
                "两种介入方式：回踩支撑企稳（缩量止跌+收复5日线）或放量突破压力位".into(),
            ];
            if let Some(s) = s1 {
                advice.key_levels.push(KeyLevel { label: "下方支撑".into(), price: s, price2: None, note: "回踩不破可试探性建仓".into() });
            }
            if let Some(r) = r1 {
                advice.key_levels.push(KeyLevel { label: "突破介入点".into(), price: r, price2: None, note: "放量突破可右侧介入".into() });
            }
        }
        _ => {
            advice.action = "回避".into();
            advice.headline = "技术面偏空，暂不建议参与，等待止跌信号".into();
            advice.reasons = vec![
                format!("当前评分 {}（{}），空头因素占优", score.total as i32, score.rating),
                "出现放量收复20日线、MACD 金叉等右侧信号前保持观望".into(),
            ];
            if let Some(s) = s1 {
                advice.key_levels.push(KeyLevel { label: "下方支撑".into(), price: s, price2: None, note: "跌破则进一步走弱".into() });
            }
            if let Some(r) = r1 {
                advice.key_levels.push(KeyLevel { label: "上方压力".into(), price: r, price2: None, note: "收复压力位才具备转多条件".into() });
            }
        }
    }
    advice
}

// ---------- 已持仓 ----------

#[allow(clippy::too_many_arguments)]
fn advise_holding(
    score: &ScoreResult,
    level: i32,
    close: f64,
    pos: PositionView,
    pnl: Pnl,
    s1: Option<f64>,
    r1: Option<f64>,
    r2: Option<f64>,
    trailing: Option<f64>,
    risk: Risk,
    sl_pct: f64,
    i: usize,
    bars: &[Bar],
) -> Advice {
    let profit_pct = (close - pos.cost) / pos.cost * 100.0;
    let pnl_text = if profit_pct >= 0.0 {
        format!("浮盈 {:.1}%", profit_pct)
    } else {
        format!("浮亏 {:.1}%", profit_pct.abs())
    };
    let mut advice = Advice {
        has_position: true,
        action: String::new(),
        headline: String::new(),
        reasons: Vec::new(),
        key_levels: Vec::new(),
        stop_loss: None,
        add_math: None,
        risk_params: risk_params(risk),
        disclaimer: String::new(),
    };
    let support = s1.unwrap_or(close * 0.95);
    let res1 = r1.unwrap_or(close * 1.05);
    // 移动止盈均线的提前减仓纪律（风格相关，用于浅套等分支）
    let trailing_cut = trailing
        .filter(|&t| t < close)
        .map(|t| {
            format!(
                "【{}风格】收盘跌破{}（≈{}）先减 {:.0}% 仓位",
                risk.label(),
                risk.trailing_ma().label(),
                round2(t),
                risk.cut_ratio() * 100.0
            )
        });

    match pnl {
        Pnl::Profit => {
            let trail = trailing.unwrap_or(pos.cost);
            match level {
                1 | 2 => {
                    advice.action = "持有".into();
                    advice.headline = format!(
                        "多头趋势完好、当前{}，继续持有并跟随{}移动止盈",
                        pnl_text, risk.trailing_ma().label()
                    );
                    advice.reasons = vec![
                        format!("当前评分 {}（{}），趋势仍向上", score.total as i32, score.rating),
                        format!("收盘跌破{}（≈{}）则减仓锁定利润", risk.trailing_ma().label(), round2(trail)),
                        format!("第一压力位 {} 附近可部分止盈，或持股等待突破", res1),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "移动止盈位".into(), price: round2(trail.max(pos.cost)), price2: None, note: "收盘跌破即减仓".into() },
                        KeyLevel { label: "第一压力位".into(), price: res1, price2: None, note: "可部分止盈".into() },
                    ];
                    if let Some(r) = r2 {
                        advice.key_levels.push(KeyLevel { label: "第二压力位".into(), price: r, price2: None, note: "趋势延续目标".into() });
                    }
                    advice.stop_loss = Some(round2(trail.max(pos.cost)));
                }
                0 => {
                    advice.action = "持有观察".into();
                    advice.headline = format!("当前{}但上行动能减弱，压力位附近部分止盈", pnl_text);
                    advice.reasons = vec![
                        format!("当前评分 {}（{}），建议落袋为安一部分", score.total as i32, score.rating),
                        format!("上方 {} 附近减仓 {:.0}%", res1, risk.cut_ratio() * 100.0),
                        format!("若回吐至成本价 {} 必须离场，不让盈利变亏损", round2(pos.cost)),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "部分止盈位".into(), price: res1, price2: None, note: format!("到位减仓 {:.0}%", risk.cut_ratio() * 100.0) },
                        KeyLevel { label: "保本止损位".into(), price: round2(pos.cost), price2: None, note: "回吐至成本价必须离场".into() },
                    ];
                    advice.stop_loss = Some(round2(pos.cost));
                }
                _ => {
                    let cut = trail.max(pos.cost);
                    advice.action = "减仓".into();
                    advice.headline = format!("趋势转弱、当前{}，逢反弹减仓保住利润", pnl_text);
                    advice.reasons = vec![
                        format!("当前评分 {}（{}），空头因素抬头", score.total as i32, score.rating),
                        format!("【{}风格】先减仓 {:.0}% 控制回撤", risk.label(), risk.cut_ratio() * 100.0),
                        format!("跌破 {} 全部了结，利润落袋", round2(cut)),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "反弹减仓区".into(), price: res1, price2: Some(close), note: "逢高降低仓位".into() },
                        KeyLevel { label: "清仓线".into(), price: round2(cut), price2: None, note: "跌破全部离场".into() },
                    ];
                    advice.stop_loss = Some(round2(cut));
                }
            }
        }
        Pnl::Near => {
            match level {
                1 | 2 => {
                    advice.action = "持有/加仓".into();
                    advice.headline = "趋势向上且成本附近，持股待涨，回踩支撑可小幅加仓".into();
                    advice.reasons = vec![
                        format!("当前评分 {}（{}），方向向上", score.total as i32, score.rating),
                        format!("回踩 {} 企稳可补仓，摊低持仓成本", support),
                        format!("跌破 {} 一并止损离场", round2(support * 0.97)),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "加仓参考区".into(), price: round2(support * 0.99), price2: Some(round2(support * 1.01)), note: "企稳后再补，不追高".into() },
                        KeyLevel { label: "止损价".into(), price: round2((support * 0.97).max(pos.cost * (1.0 - sl_pct))), price2: None, note: "跌破止损".into() },
                        KeyLevel { label: "第一目标位".into(), price: res1, price2: None, note: "到位可部分止盈".into() },
                    ];
                    advice.add_math = Some(add_math(&pos, support, risk, &mut advice.reasons));
                    advice.stop_loss = Some(round2((support * 0.97).max(pos.cost * (1.0 - sl_pct))));
                }
                0 => {
                    advice.action = "持有".into();
                    advice.headline = "成本附近震荡，持有观察：破位止损、到位减仓".into();
                    advice.reasons = vec![
                        format!("当前评分 {}（{}），多空均衡", score.total as i32, score.rating),
                        format!("跌破 {} 止损，防深套", round2((support * 0.97).max(pos.cost * (1.0 - sl_pct)))),
                        format!("反弹至 {} 可部分止盈", res1),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "止损价".into(), price: round2((support * 0.97).max(pos.cost * (1.0 - sl_pct))), price2: None, note: "跌破止损".into() },
                        KeyLevel { label: "第一压力位".into(), price: res1, price2: None, note: "到位部分止盈".into() },
                    ];
                    advice.stop_loss = Some(round2((support * 0.97).max(pos.cost * (1.0 - sl_pct))));
                }
                _ => {
                    advice.action = "减仓".into();
                    advice.headline = "转弱信号出现，先减仓控制风险、防止深套".into();
                    advice.reasons = vec![
                        format!("当前评分 {}（{}），建议降低仓位", score.total as i32, score.rating),
                        format!("反弹至 {} 先减仓 {:.0}%", res1, risk.cut_ratio() * 100.0),
                        format!("跌破 {} 全部止损", round2(pos.cost * (1.0 - sl_pct))),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "反弹减仓位".into(), price: res1, price2: None, note: "逢高减仓".into() },
                        KeyLevel { label: "止损价".into(), price: round2(pos.cost * (1.0 - sl_pct)), price2: None, note: "跌破清仓".into() },
                    ];
                    advice.stop_loss = Some(round2(pos.cost * (1.0 - sl_pct)));
                }
            }
        }
        Pnl::ShallowLoss => {
            match level {
                1 | 2 => {
                    advice.action = "持有/补仓".into();
                    advice.headline = "中期趋势仍多，浅套可耐心持有，回踩支撑企稳可补仓摊低成本".into();
                    advice.reasons = vec![
                        format!("当前{}，评分 {}（{}），趋势未破坏", pnl_text, score.total as i32, score.rating),
                        format!("回踩 {} 企稳可补仓，摊低成本后更容易回本", support),
                        "补仓仅限轻仓者，且破位必须严格止损".into(),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "补仓参考区".into(), price: round2(support * 0.99), price2: Some(round2(support * 1.01)), note: "企稳再补".into() },
                        KeyLevel { label: "补仓止损线".into(), price: round2(support * 0.97), price2: None, note: "破位全部止损".into() },
                        KeyLevel { label: "第一目标位".into(), price: res1, price2: None, note: "解套减仓位".into() },
                    ];
                    advice.add_math = Some(add_math(&pos, support, risk, &mut advice.reasons));
                    if let Some(s) = &trailing_cut {
                        advice.reasons.push(s.clone());
                    }
                    if let Some(t) = trailing.filter(|&t| t < close) {
                        advice.key_levels.push(KeyLevel {
                            label: "先减仓线".into(),
                            price: round2(t),
                            price2: None,
                            note: format!("跌破{}先减 {:.0}%，其余按支撑纪律", risk.trailing_ma().label(), risk.cut_ratio() * 100.0),
                        });
                    }
                    advice.stop_loss = Some(round2(support * 0.97));
                }
                0 => {
                    advice.action = "持有观察".into();
                    advice.headline = "浅套+震荡：反弹至压力位先减部分仓位，破支撑止损".into();
                    advice.reasons = vec![
                        format!("当前{}，评分 {}（{}），方向不明", pnl_text, score.total as i32, score.rating),
                        format!("【{}风格】反弹至 {} 减仓 {:.0}% 降低风险", risk.label(), res1, risk.cut_ratio() * 100.0),
                        format!("跌破 {} 止损离场，避免演变为深套", round2(support * 0.97)),
                    ];
                    if let Some(s) = &trailing_cut {
                        advice.reasons.push(s.clone());
                    }
                    advice.key_levels = vec![
                        KeyLevel { label: "反弹减仓位".into(), price: res1, price2: None, note: "逢高减仓".into() },
                        KeyLevel { label: "止损价".into(), price: round2(support * 0.97), price2: None, note: "破位止损".into() },
                    ];
                    advice.stop_loss = Some(round2(support * 0.97));
                }
                _ => {
                    advice.action = "逢反弹减仓".into();
                    advice.headline = "浅套且趋势偏空，反弹减仓为主，破位止损不硬扛".into();
                    advice.reasons = vec![
                        format!("当前{}，评分 {}（{}），空头占优", pnl_text, score.total as i32, score.rating),
                        format!("【{}风格】利用反弹至 {} 的机会先减 {:.0}%", risk.label(), res1, risk.cut_ratio() * 100.0),
                        format!("跌破 {} 全部止损离场", round2(support * 0.97)),
                    ];
                    if let Some(s) = &trailing_cut {
                        advice.reasons.push(s.clone());
                    }
                    advice.key_levels = vec![
                        KeyLevel { label: "反弹减仓区".into(), price: res1, price2: Some(close), note: "逢高降低仓位".into() },
                        KeyLevel { label: "离场线".into(), price: round2(support * 0.97), price2: None, note: "跌破全部止损".into() },
                    ];
                    advice.stop_loss = Some(round2(support * 0.97));
                }
            }
        }
        Pnl::DeepLoss => {
            let floor = low_60(bars, i) * 0.98;
            match level {
                1 | 2 => {
                    advice.action = "持有/反弹减仓".into();
                    advice.headline = "深套但出现企稳做多信号，反弹至压力位先减仓，勿急于补仓".into();
                    advice.reasons = vec![
                        format!("当前{}，评分 {}（{}），出现修复迹象", pnl_text, score.total as i32, score.rating),
                        format!("【{}风格】反弹至 {} 先减 {:.0}%，降低风险敞口", risk.label(), res1, risk.cut_ratio() * 100.0),
                        "深套补仓需要极大确定性，仅当放量收复20日线且逻辑未变时小仓位参与".into(),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "反弹减仓位".into(), price: res1, price2: None, note: "逢高先减".into() },
                        KeyLevel { label: "破位离场线".into(), price: round2(floor), price2: None, note: "创新低则止损离场".into() },
                    ];
                    advice.stop_loss = Some(round2(floor));
                }
                0 => {
                    advice.action = "逢反弹减仓".into();
                    advice.headline = "深套+震荡：利用反弹逐步降低仓位，等待明确方向".into();
                    advice.reasons = vec![
                        format!("当前{}，评分 {}（{}），弱势整理", pnl_text, score.total as i32, score.rating),
                        format!("【{}风格】反弹至 {} 先减 {:.0}%，再视情况分批处理", risk.label(), res1, risk.cut_ratio() * 100.0),
                        format!("跌破 {}（阶段新低）止损离场", round2(floor)),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "反弹减仓位".into(), price: res1, price2: None, note: "逢高减仓".into() },
                        KeyLevel { label: "离场线".into(), price: round2(floor), price2: None, note: "创新低止损".into() },
                    ];
                    advice.stop_loss = Some(round2(floor));
                }
                _ => {
                    advice.action = "止损离场".into();
                    advice.headline = "深套且趋势走坏，建议分批止损，避免亏损继续扩大".into();
                    advice.reasons = vec![
                        format!("当前{}，评分 {}（{}），空头趋势明确", pnl_text, score.total as i32, score.rating),
                        "资金效率优先：止损后等右侧信号再介入，远好于深套硬扛".into(),
                        format!("可分 2~3 批执行，反弹即减，{} 下方清仓", support),
                    ];
                    advice.key_levels = vec![
                        KeyLevel { label: "清仓线".into(), price: round2(support * 0.98), price2: None, note: "跌破全部离场".into() },
                        KeyLevel { label: "反弹减仓区".into(), price: res1, price2: Some(close), note: "分批执行".into() },
                    ];
                    advice.stop_loss = Some(round2(support * 0.98));
                }
            }
        }
    }
    // 追加评分依据（取最强因子的理由）
    let mut reasons = top_factor_reasons(score);
    advice.reasons.append(&mut reasons);
    advice
}

fn profit_bucket(cost: f64, close: f64) -> Pnl {
    let pct = (close - cost) / cost * 100.0;
    if pct >= 5.0 {
        Pnl::Profit
    } else if pct >= -5.0 {
        Pnl::Near
    } else if pct >= -15.0 {
        Pnl::ShallowLoss
    } else {
        Pnl::DeepLoss
    }
}

fn trailing_price(ma: TrailingMa, ind: &IndicatorSeries, i: usize) -> Option<f64> {
    let s = match ma {
        TrailingMa::Ma10 => &ind.ma10,
        TrailingMa::Ma20 => &ind.ma20,
        TrailingMa::Ma60 => &ind.ma60,
    };
    s.get(i).copied().flatten()
}

fn low_60(bars: &[Bar], i: usize) -> f64 {
    let from = i.saturating_sub(59);
    bars[from..=i].iter().map(|b| b.low).fold(f64::MAX, f64::min)
}

/// 补仓测算：按风险偏好比例在支撑位附近补仓后的新成本
fn add_math(pos: &PositionView, price: f64, risk: Risk, reasons: &mut Vec<String>) -> PositionMath {
    let add_shares = (pos.shares * risk.add_ratio() / 100.0).round() * 100.0;
    let new_shares = pos.shares + add_shares;
    let new_cost = (pos.cost * pos.shares + price * add_shares) / new_shares;
    reasons.push(format!(
        "若在 {} 附近补仓 {} 股，持仓成本将由 {} 降至 {}",
        round2(price),
        add_shares as i64,
        round2(pos.cost),
        round2(new_cost)
    ));
    PositionMath {
        add_price: round2(price),
        add_shares,
        new_cost: round2(new_cost),
        new_shares,
        note: format!("按{}风格补仓 {:.0}% 仓位测算", risk.label(), risk.add_ratio() * 100.0),
    }
}

fn top_factor_reasons(score: &ScoreResult) -> Vec<String> {
    let mut fs = score.factors.clone();
    fs.sort_by(|a, b| b.score.abs().partial_cmp(&a.score.abs()).unwrap());
    fs.iter()
        .filter(|f| f.score.abs() >= 20.0)
        .filter_map(|f| f.reasons.first().map(|r| format!("【{}】{}", f.name, r)))
        .take(3)
        .collect()
}
