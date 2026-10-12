//! 全局配置：因子权重与风险偏好参数。

use std::path::Path;

/// 读环境变量；未设置时回退读 backend/.env（KEY=VALUE 简易格式，与 ai.rs 同源）
pub fn env_or_dotenv(data_dir: &Path, key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key) {
        if !v.trim().is_empty() {
            return Some(v.trim().to_string());
        }
    }
    let env_file = data_dir.parent().unwrap_or(Path::new(".")).join(".env");
    let text = std::fs::read_to_string(env_file).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                let v = v.trim().trim_matches('"');
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// 多因子权重，合计 1.0
pub const W_TREND: f64 = 0.25;
pub const W_MOMENTUM: f64 = 0.20;
pub const W_OSCILLATOR: f64 = 0.15;
pub const W_VOLUME: f64 = 0.15;
pub const W_BOLL: f64 = 0.10;
pub const W_PATTERN: f64 = 0.15;

/// 综合评分（-100 ~ +100）对应评级阈值
pub const RATING_STRONG_BULL: f64 = 40.0;
pub const RATING_BULL: f64 = 15.0;
pub const RATING_BEAR: f64 = -15.0;
pub const RATING_STRONG_BEAR: f64 = -40.0;

// ---------- A股交易费用（按自己券商费率修改此处） ----------

/// 佣金费率（双边），万 5
pub const COMMISSION_RATE: f64 = 0.000_5;
/// 佣金单笔最低（元）
pub const COMMISSION_MIN: f64 = 5.0;
/// 印花税（仅卖出），2023-08 起为万 5
pub const STAMP_DUTY_RATE: f64 = 0.000_5;
/// 过户费（双边），万 0.1
pub const TRANSFER_FEE_RATE: f64 = 0.000_01;

/// 买入费用 = 佣金（含最低） + 过户费
pub fn buy_fee(amount: f64) -> f64 {
    (amount * COMMISSION_RATE).max(COMMISSION_MIN) + amount * TRANSFER_FEE_RATE
}

/// 卖出费用 = 佣金（含最低） + 印花税 + 过户费
pub fn sell_fee(amount: f64) -> f64 {
    (amount * COMMISSION_RATE).max(COMMISSION_MIN)
        + amount * STAMP_DUTY_RATE
        + amount * TRANSFER_FEE_RATE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    Conservative,
    Balanced,
    Aggressive,
}

impl Risk {
    pub fn from_query(s: Option<&str>) -> Risk {
        match s.unwrap_or("").to_ascii_lowercase().as_str() {
            "conservative" => Risk::Conservative,
            "aggressive" => Risk::Aggressive,
            _ => Risk::Balanced,
        }
    }

    /// 止损线（相对成本价）
    pub fn stop_loss_pct(self) -> f64 {
        match self {
            Risk::Conservative => 0.05,
            Risk::Balanced => 0.08,
            Risk::Aggressive => 0.12,
        }
    }

    /// 移动止盈参考均线（跌破即减仓）
    pub fn trailing_ma(self) -> TrailingMa {
        match self {
            Risk::Conservative => TrailingMa::Ma10,
            Risk::Balanced => TrailingMa::Ma20,
            Risk::Aggressive => TrailingMa::Ma60,
        }
    }

    /// 首次建仓占计划仓位的比例
    pub fn first_build_pct(self) -> f64 {
        match self {
            Risk::Conservative => 0.20,
            Risk::Balanced => 0.30,
            Risk::Aggressive => 0.40,
        }
    }

    /// 补仓比例（相对现有股数）
    pub fn add_ratio(self) -> f64 {
        match self {
            Risk::Conservative => 0.30,
            Risk::Balanced => 0.50,
            Risk::Aggressive => 0.70,
        }
    }

    /// 反弹减仓比例（越保守减得越多）
    pub fn cut_ratio(self) -> f64 {
        match self {
            Risk::Conservative => 0.50,
            Risk::Balanced => 0.35,
            Risk::Aggressive => 0.20,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Risk::Conservative => "保守",
            Risk::Balanced => "稳健",
            Risk::Aggressive => "激进",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TrailingMa {
    Ma10,
    Ma20,
    Ma60,
}

impl TrailingMa {
    pub fn label(self) -> &'static str {
        match self {
            TrailingMa::Ma10 => "10日均线",
            TrailingMa::Ma20 => "20日均线",
            TrailingMa::Ma60 => "60日均线",
        }
    }
}
