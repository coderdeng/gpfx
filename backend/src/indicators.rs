//! 技术指标计算（纯函数，公式透明）。
//!
//! 全部序列与输入 bars 等长，热身期返回 None。

use crate::models::{Bar, IndicatorSeries};

type Series = Vec<Option<f64>>;

/// 简单移动平均
pub fn sma(v: &[f64], n: usize) -> Series {
    let mut out: Series = vec![None; v.len()];
    if n == 0 {
        return out;
    }
    let mut sum = 0.0;
    for (i, &x) in v.iter().enumerate() {
        sum += x;
        if i >= n {
            sum -= v[i - n];
        }
        if i + 1 >= n {
            out[i] = Some(sum / n as f64);
        }
    }
    out
}

/// EMA 内部实现：返回原始数组（NAN 表示未热身）与首个有效下标
fn ema_raw(v: &[f64], n: usize) -> (Vec<f64>, usize) {
    let mut out = vec![f64::NAN; v.len()];
    let none = usize::MAX;
    if n == 0 || v.len() < n {
        return (out, none);
    }
    let seed: f64 = v[..n].iter().sum::<f64>() / n as f64;
    out[n - 1] = seed;
    let k = 2.0 / (n as f64 + 1.0);
    for i in n..v.len() {
        out[i] = v[i] * k + out[i - 1] * (1.0 - k);
    }
    (out, n - 1)
}

pub fn ema(v: &[f64], n: usize) -> Series {
    let (raw, _) = ema_raw(v, n);
    raw.into_iter()
        .map(|x| if x.is_nan() { None } else { Some(x) })
        .collect()
}

/// MACD(12,26,9)，柱体 = 2*(DIF-DEA)（国内惯例）
pub fn macd(close: &[f64]) -> (Series, Series, Series) {
    let n = close.len();
    let nan_vec = || vec![f64::NAN; n];
    let (e12, _) = ema_raw(close, 12);
    let (e26, v26) = ema_raw(close, 26);
    if v26 == usize::MAX {
        return (nan_vec().iter().map(none_if_nan).collect(),
                nan_vec().iter().map(none_if_nan).collect(),
                nan_vec().iter().map(none_if_nan).collect());
    }
    let mut dif_raw = vec![f64::NAN; n];
    for i in v26..n {
        dif_raw[i] = e12[i] - e26[i];
    }
    // DEA：对 DIF 从其首个有效点起做 9 日 EMA
    let mut dea_raw = vec![f64::NAN; n];
    let (dea_seg, _) = ema_raw(&dif_raw[v26..], 9);
    for (j, &val) in dea_seg.iter().enumerate() {
        if !val.is_nan() {
            dea_raw[v26 + j] = val;
        }
    }
    let mut hist_raw = vec![f64::NAN; n];
    for i in 0..n {
        if !dif_raw[i].is_nan() && !dea_raw[i].is_nan() {
            hist_raw[i] = (dif_raw[i] - dea_raw[i]) * 2.0;
        }
    }
    (
        to_series(dif_raw),
        to_series(dea_raw),
        to_series(hist_raw),
    )
}

/// RSI（Wilder 平滑，国内软件惯例）
pub fn rsi(close: &[f64], n: usize) -> Series {
    let len = close.len();
    let mut out: Series = vec![None; len];
    if len <= n {
        return out;
    }
    let (mut ag, mut al) = (0.0, 0.0);
    for i in 1..=n {
        let d = close[i] - close[i - 1];
        if d > 0.0 { ag += d; } else { al -= d; }
    }
    ag /= n as f64;
    al /= n as f64;
    out[n] = Some(rsi_val(ag, al));
    let nf = n as f64;
    for i in n + 1..len {
        let d = close[i] - close[i - 1];
        ag = (ag * (nf - 1.0) + d.max(0.0)) / nf;
        al = (al * (nf - 1.0) + (-d).max(0.0)) / nf;
        out[i] = Some(rsi_val(ag, al));
    }
    out
}

fn rsi_val(ag: f64, al: f64) -> f64 {
    if ag + al <= 1e-12 {
        50.0
    } else {
        100.0 * ag / (ag + al)
    }
}

/// KDJ(9,3,3)
pub fn kdj(high: &[f64], low: &[f64], close: &[f64], n: usize) -> (Series, Series, Series) {
    let len = close.len();
    let (mut ks, mut ds, mut js): (Series, Series, Series) =
        (vec![None; len], vec![None; len], vec![None; len]);
    let (mut pk, mut pd) = (50.0, 50.0);
    for i in 0..len {
        if i + 1 < n {
            continue;
        }
        let hh = high[i + 1 - n..=i].iter().cloned().fold(f64::MIN, f64::max);
        let ll = low[i + 1 - n..=i].iter().cloned().fold(f64::MAX, f64::min);
        let rsv = if (hh - ll).abs() < 1e-12 {
            50.0
        } else {
            (close[i] - ll) / (hh - ll) * 100.0
        };
        let k = pk * 2.0 / 3.0 + rsv / 3.0;
        let d = pd * 2.0 / 3.0 + k / 3.0;
        ks[i] = Some(k);
        ds[i] = Some(d);
        js[i] = Some(3.0 * k - 2.0 * d);
        pk = k;
        pd = d;
    }
    (ks, ds, js)
}

/// 布林带 (中轨, 上轨, 下轨)，标准差为总体口径
pub fn boll(close: &[f64], n: usize, k: f64) -> (Series, Series, Series) {
    let len = close.len();
    let mid = sma(close, n);
    let (mut upper, mut lower): (Series, Series) = (vec![None; len], vec![None; len]);
    for i in (n - 1)..len {
        if let Some(m) = mid[i] {
            let win = &close[i + 1 - n..=i];
            let var: f64 = win.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / n as f64;
            let sd = var.sqrt();
            upper[i] = Some(m + k * sd);
            lower[i] = Some(m - k * sd);
        }
    }
    (mid, upper, lower)
}

/// ATR（Wilder 平滑）
pub fn atr(high: &[f64], low: &[f64], close: &[f64], n: usize) -> Series {
    let len = close.len();
    let mut out: Series = vec![None; len];
    if len < n {
        return out;
    }
    let mut tr = vec![0.0; len];
    tr[0] = high[0] - low[0];
    for i in 1..len {
        tr[i] = (high[i] - low[i])
            .max((high[i] - close[i - 1]).abs())
            .max((low[i] - close[i - 1]).abs());
    }
    let mut a: f64 = tr[..n].iter().sum::<f64>() / n as f64;
    out[n - 1] = Some(a);
    let nf = n as f64;
    for i in n..len {
        a = (a * (nf - 1.0) + tr[i]) / nf;
        out[i] = Some(a);
    }
    out
}

fn none_if_nan(x: &f64) -> Option<f64> {
    if x.is_nan() { None } else { Some(*x) }
}

fn to_series(raw: Vec<f64>) -> Series {
    raw.iter().map(none_if_nan).collect()
}

/// 一次性计算全部指标
pub fn compute(bars: &[Bar]) -> IndicatorSeries {
    let close: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let high: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let low: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect();

    let (dif, dea, hist) = macd(&close);
    let (kdj_k, kdj_d, kdj_j) = kdj(&high, &low, &close, 9);
    let (boll_mid, boll_upper, boll_lower) = boll(&close, 20, 2.0);

    IndicatorSeries {
        ma5: sma(&close, 5),
        ma10: sma(&close, 10),
        ma20: sma(&close, 20),
        ma60: sma(&close, 60),
        dif,
        dea,
        hist,
        rsi6: rsi(&close, 6),
        rsi12: rsi(&close, 12),
        rsi24: rsi(&close, 24),
        kdj_k,
        kdj_d,
        kdj_j,
        boll_mid,
        boll_upper,
        boll_lower,
        vol_ma5: sma(&vol, 5),
        vol_ma10: sma(&vol, 10),
        atr14: atr(&high, &low, &close, 14),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series_f64(s: &Series) -> Vec<f64> {
        s.iter().filter_map(|x| *x).collect()
    }

    #[test]
    fn test_sma() {
        let v: Vec<f64> = (1..=10).map(|x| x as f64).collect();
        let s = sma(&v, 3);
        assert!(s[0].is_none() && s[1].is_none());
        assert_eq!(s[2], Some(2.0));
        assert_eq!(s[9], Some(9.0));
    }

    #[test]
    fn test_ema_constant() {
        let v = vec![10.0; 40];
        let s = series_f64(&ema(&v, 12));
        assert!(s.iter().all(|&x| (x - 10.0).abs() < 1e-9));
    }

    #[test]
    fn test_macd_constant() {
        let v = vec![10.0; 60];
        let (dif, dea, hist) = macd(&v);
        assert_eq!(dif[59], Some(0.0));
        assert_eq!(dea[59], Some(0.0));
        assert_eq!(hist[59], Some(0.0));
    }

    #[test]
    fn test_rsi_monotonic() {
        let up: Vec<f64> = (1..=40).map(|x| x as f64).collect();
        assert_eq!(rsi(&up, 6)[39], Some(100.0));
        let down: Vec<f64> = (1..=40).map(|x| 41.0 - x as f64).collect();
        assert_eq!(rsi(&down, 6)[39], Some(0.0));
    }

    #[test]
    fn test_kdj_bounded() {
        let n = 60;
        let high: Vec<f64> = (0..n).map(|i| 10.0 + (i % 7) as f64).collect();
        let low: Vec<f64> = high.iter().map(|x| x - 1.0).collect();
        let close: Vec<f64> = high.iter().map(|x| x - 0.4).collect();
        let (k, d, j) = kdj(&high, &low, &close, 9);
        for s in [&k, &d, &j] {
            for v in series_f64(s) {
                assert!((-30.0..=130.0).contains(&v), "KDJ 越界: {v}");
            }
        }
        assert!(k[0].is_none());
    }

    #[test]
    fn test_boll_known() {
        // 常数序列带宽为 0
        let v = vec![100.0; 30];
        let (mid, upper, lower) = boll(&v, 20, 2.0);
        assert_eq!(mid[29], Some(100.0));
        assert_eq!(upper[29], Some(100.0));
        assert_eq!(lower[29], Some(100.0));
    }

    #[test]
    fn test_atr_constant_range() {
        let n = 40;
        let high = vec![11.0; n];
        let low = vec![10.0; n];
        let close = vec![10.5; n];
        let s = series_f64(&atr(&high, &low, &close, 14));
        assert!(s.iter().all(|&x| (x - 1.0).abs() < 1e-9));
    }
}
