//! 数据层：腾讯行情接口（主源）+ 东方财富（备源）+ 当日本地缓存。
//!
//! - 日线（前复权）：腾讯 `fqkline/get`，响应内嵌 `qt` 实时行情用于补充
//!   最新成交额/换手率；东财 `push2his` 作为备用源
//! - 搜索：东财 searchapi 联想接口，支持代码 / 中文名 / 拼音缩写

use crate::models::{round2, Bar, Financials, StockHit, Valuation};
use chrono::{Datelike, Local};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TENCENT_KLINE_URL: &str = "https://web.ifzq.gtimg.cn/appstock/app/fqkline/get";
const TENCENT_QUOTE_URL: &str = "https://qt.gtimg.cn/q";
const EM_F10_URL: &str = "https://datacenter.eastmoney.com/securities/api/data/v1/get";
const EM_KLINE_URL: &str = "https://push2his.eastmoney.com/api/qt/stock/kline/get";
const SUGGEST_URL: &str = "https://searchapi.eastmoney.com/api/suggest/get";
const SUGGEST_TOKEN: &str = "D43BF722C8E33BDC906FB84D85E326E8";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// 拉取的日线根数（约 15 个月，满足 MA60 与 120 日回看）
const BARS_LIMIT: u32 = 320;

pub struct DataSource {
    http: reqwest::Client,
    cache_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
struct EmKlineResp {
    data: Option<EmKlineData>,
}

#[derive(Debug, Deserialize)]
struct EmKlineData {
    name: Option<String>,
    klines: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SuggestResp {
    #[serde(rename = "QuotationCodeTable")]
    table: Option<SuggestTable>,
}

#[derive(Debug, Deserialize)]
struct SuggestTable {
    #[serde(default)]
    Data: Option<Vec<SuggestItem>>,
}

#[derive(Debug, Deserialize)]
struct SuggestItem {
    Code: String,
    Name: String,
    SecurityTypeName: String,
    QuoteID: String,
    Classify: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedF10 {
    fetched: String,
    data: Financials,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedKline {
    /// 缓存写入时刻 "YYYY-MM-DD HH:MM"
    fetched: String,
    name: String,
    bars: Vec<Bar>,
}

/// K线缓存时效：按「缓存内容是否完整」判断，而非只看当前时刻。
/// - 盘中（交易日 09:15~15:35）：只接受同日盘内 10 分钟内的抓取
/// - 盘外：要求缓存抓取于最近一次已完成的收盘（15:35）之后——
///   否则盘中抓的快照会把盘中价当收盘价一直用到当晚（昨日盘外的旧缓存同理过期）
fn kline_cache_fresh(fetched: &str, now: chrono::DateTime<Local>) -> bool {
    let Ok(t) = chrono::NaiveDateTime::parse_from_str(fetched, "%Y-%m-%d %H:%M") else {
        // 旧格式（仅日期）：只要日期相同就视为有效，避免格式迁移期全部失效
        return fetched == now.format("%Y-%m-%d").to_string();
    };
    let t = t.and_local_timezone(Local).single().unwrap_or(now);
    if t > now {
        return true; // 时钟回拨，保守视为有效
    }
    let in_window = |x: chrono::DateTime<Local>| {
        let hm = x.time();
        (chrono::NaiveTime::from_hms(9, 15, 0)..=chrono::NaiveTime::from_hms(15, 35, 0))
            .contains(&hm)
    };
    let weekday = now.weekday().num_days_from_monday(); // 0=周一
    if weekday < 5 && in_window(now) {
        // 盘中：10 分钟内的同日盘内抓取才有效（保证含今日实时 bar）
        t.date_naive() == now.date_naive()
            && in_window(t)
            && now - t < chrono::Duration::minutes(10)
    } else {
        t >= last_completed_close(now) // 盘外：缓存须晚于最近一次收盘
    }
}

/// 最近一个已完成收盘的时刻（工作日 15:35；简化模型，未排除节假日）
fn last_completed_close(now: chrono::DateTime<Local>) -> chrono::DateTime<Local> {
    let mut day = now.date_naive();
    loop {
        let close = day
            .and_hms_opt(15, 35, 0)
            .and_then(|n| n.and_local_timezone(Local).single())
            .unwrap_or(now);
        if close <= now && close.weekday().num_days_from_monday() < 5 {
            return close;
        }
        day = day.pred_opt().unwrap_or(day);
    }
}

impl DataSource {
    pub fn new(cache_dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(cache_dir)
            .map_err(|e| format!("创建缓存目录失败: {e}"))?;
        let http = reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))?;
        Ok(Self { http, cache_dir: cache_dir.to_path_buf() })
    }

    /// 拉取日线（缓存命中则直接读盘），返回 (股票名, bars)
    pub async fn kline(&self, code: &str) -> Result<(String, Vec<Bar>), String> {
        let cache_file = self.cache_dir.join(format!("kline_{code}.json"));
        let now = Local::now();
        if let Ok(text) = std::fs::read_to_string(&cache_file) {
            if let Ok(cached) = serde_json::from_str::<CachedKline>(&text) {
                if kline_cache_fresh(&cached.fetched, now) && !cached.bars.is_empty() {
                    return Ok((cached.name, cached.bars));
                }
            }
        }

        // 主源：腾讯
        match self.fetch_tencent(code).await {
            Ok((name, bars)) => {
                self.save_cache(&cache_file, &now, &name, &bars);
                Ok((name, bars))
            }
            Err(t_err) => {
                // 备源：东方财富
                let secid = secid_of(code).ok_or_else(|| format!("不支持的股票代码 {code}"))?;
                let (name, bars) = self
                    .fetch_eastmoney(&secid, code)
                    .await
                    .map_err(|e| format!("腾讯源失败({t_err})；东财源失败({e})"))?;
                self.save_cache(&cache_file, &now, &name, &bars);
                Ok((name, bars))
            }
        }
    }

    /// 股票搜索：代码 / 名称 / 拼音缩写
    pub async fn search(&self, query: &str) -> Result<Vec<StockHit>, String> {
        let url = format!(
            "{SUGGEST_URL}?input={}&type=14&token={SUGGEST_TOKEN}&count=10",
            urlencoded(query)
        );
        let text = self.get_with_retry(&url).await?;
        let resp: SuggestResp =
            serde_json::from_str(&text).map_err(|e| format!("解析搜索响应失败: {e}"))?;
        let mut hits = Vec::new();
        if let Some(items) = resp.table.and_then(|t| t.Data) {
            for it in items {
                if it.Classify != "AStock" {
                    continue; // 只保留沪深京A股
                }
                let market = match it.SecurityTypeName.as_str() {
                    "沪A" => "SH",
                    "深A" => "SZ",
                    _ => "BJ",
                };
                hits.push(StockHit {
                    code: it.Code,
                    name: it.Name,
                    market: market.to_string(),
                    secid: it.QuoteID,
                });
            }
        }
        Ok(hits)
    }

    // ---------- 腾讯源 ----------

    async fn fetch_tencent(&self, code: &str) -> Result<(String, Vec<Bar>), String> {
        let symbol = tencent_symbol(code);
        let url =
            format!("{TENCENT_KLINE_URL}?param={symbol},day,,,{BARS_LIMIT},qfq");
        let text = self.get_with_retry(&url).await?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("解析腾讯K线失败: {e}"))?;
        let data = v
            .pointer("/data")
            .and_then(|d| d.get(&symbol))
            .ok_or("腾讯K线响应缺少数据")?;
        let rows = data
            .get(if data.get("qfqday").is_some() { "qfqday" } else { "day" })
            .and_then(|x| x.as_array())
            .ok_or("腾讯K线数组为空（可能已退市或长期停牌）")?;

        let mut bars: Vec<Bar> = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(arr) = row.as_array() else { continue };
            if arr.len() < 6 {
                continue;
            }
            // [日期, 开, 收, 高, 低, 量(手)]
            let num = |i: usize| -> Option<f64> {
                arr.get(i)?.as_str()?.parse::<f64>().ok()
            };
            let (Some(open), Some(close), Some(high), Some(low), Some(volume)) =
                (num(1), num(2), num(3), num(4), num(5))
            else {
                continue;
            };
            bars.push(Bar {
                date: arr[0].as_str().unwrap_or("").to_string(),
                open,
                close,
                high,
                low,
                volume,
                amount: 0.0,
                turnover: 0.0,
                pct_chg: 0.0,
            });
        }
        if bars.is_empty() {
            return Err("腾讯K线数据为空".into());
        }
        // 前复权口径下的涨跌幅
        for k in 1..bars.len() {
            if bars[k - 1].close > 1e-9 {
                bars[k].pct_chg =
                    (bars[k].close / bars[k - 1].close - 1.0) * 100.0;
            }
        }
        // 从内嵌 qt 实时行情补充最新一根的成交额/换手率
        let mut name = code.to_string();
        if let Some(qt) = data.pointer("/qt").and_then(|q| q.get(&symbol)).and_then(|q| q.as_array()) {
            if let Some(n) = qt.get(1).and_then(|x| x.as_str()) {
                name = n.to_string();
            }
            let qt_date = qt.get(30).and_then(|x| x.as_str()).unwrap_or("");
            let last = bars.last_mut().unwrap();
            if qt_date.starts_with(&last.date.replace('-', "")) {
                if let Some(a) = qt.get(37).and_then(|x| x.as_str()).and_then(|s| s.parse::<f64>().ok()) {
                    last.amount = a * 1e4; // 万元 -> 元
                }
                if let Some(t) = qt.get(38).and_then(|x| x.as_str()).and_then(|s| s.parse::<f64>().ok()) {
                    last.turnover = t;
                }
            }
        }
        Ok((name, bars))
    }

    /// 实时估值快照：市盈率 / 市净率 / 总市值 / 流通市值
    pub async fn valuation(&self, code: &str) -> Result<Valuation, String> {
        let symbol = tencent_symbol(code);
        let url = format!("{TENCENT_QUOTE_URL}={symbol}");
        let text = self.get_with_retry(&url).await?;
        let f: Vec<&str> = text.split('~').collect();
        let num = |i: usize| -> Option<f64> { f.get(i)?.trim().parse::<f64>().ok() };
        // 字段：39=市盈率TTM 44=流通市值(亿) 45=总市值(亿) 46=市净率
        Ok(Valuation {
            pe: num(39),
            pb: num(46),
            market_cap: num(45),
            float_cap: num(44),
        })
    }

    /// 财报主要指标（东财 F10，最新报告期；财报按季更新，当日缓存）
    pub async fn financials(&self, code: &str) -> Result<Financials, String> {
        let cache_file = self.cache_dir.join(format!("f10_{code}.json"));
        let today = Local::now().format("%Y-%m-%d").to_string();
        if let Ok(text) = std::fs::read_to_string(&cache_file) {
            if let Ok(c) = serde_json::from_str::<CachedF10>(&text) {
                if c.fetched == today {
                    return Ok(c.data);
                }
            }
        }

        let suffix = match code.chars().next() {
            Some('6') => "SH",
            Some('0') | Some('3') => "SZ",
            _ => "BJ",
        };
        let url = format!(
            "{EM_F10_URL}?reportName=RPT_F10_FINANCE_MAINFINADATA&columns=ALL&filter=(SECUCODE%3D%22{code}.{suffix}%22)&pageNumber=1&pageSize=1&sortTypes=-1&sortColumns=REPORT_DATE&source=HSF10&client=PC"
        );
        let text = self.get_with_retry(&url).await?;

        #[derive(Deserialize)]
        struct F10Resp {
            result: Option<F10Result>,
        }
        #[derive(Deserialize)]
        struct F10Result {
            #[serde(default)]
            data: Option<Vec<serde_json::Value>>,
        }
        let resp: F10Resp =
            serde_json::from_str(&text).map_err(|e| format!("解析财报响应失败: {e}"))?;
        let row = resp
            .result
            .and_then(|r| r.data)
            .and_then(|d| d.into_iter().next())
            .ok_or_else(|| "无财报数据（可能为次新股）".to_string())?;
        let num = |k: &str| row.get(k).and_then(|v| v.as_f64());
        let report_date = row
            .get("REPORT_DATE")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .split(' ')
            .next()
            .unwrap_or("")
            .to_string();
        let fin = Financials {
            report_date,
            revenue_yoy: num("TOTALOPERATEREVETZ"),
            profit_yoy: num("PARENTNETPROFITTZ"),
            roe: num("ROEJQ"),
            gross_margin: num("XSMLL"),
            eps: num("EPSJB"),
            net_profit: num("PARENTNETPROFIT").map(|x| round2(x / 1e8)),
        };
        if let Ok(json) = serde_json::to_string(&CachedF10 { fetched: today, data: fin.clone() }) {
            let _ = std::fs::write(&cache_file, json);
        }
        Ok(fin)
    }

    // ---------- 东财源 ----------

    async fn fetch_eastmoney(&self, secid: &str, code: &str) -> Result<(String, Vec<Bar>), String> {
        let beg = (Local::now() - chrono::Duration::days(620))
            .format("%Y%m%d")
            .to_string();
        let url = format!(
            "{EM_KLINE_URL}?secid={secid}&fields1=f1,f2,f3,f4,f5,f6&fields2=f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61&klt=101&fqt=1&beg={beg}&end=20500101"
        );
        let text = self.get_with_retry(&url).await?;
        let resp: EmKlineResp =
            serde_json::from_str(&text).map_err(|e| format!("解析东财K线失败: {e}"))?;
        let data = resp.data.ok_or_else(|| "接口未返回数据（代码可能不存在）".to_string())?;
        let mut bars: Vec<Bar> = Vec::with_capacity(data.klines.len());
        for line in &data.klines {
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 11 {
                continue;
            }
            // 日期,开,收,高,低,量(手),额,振幅,涨跌幅,涨跌额,换手率
            let (Ok(open), Ok(close), Ok(high), Ok(low), Ok(volume), Ok(amount)) = (
                f[1].parse::<f64>(), f[2].parse::<f64>(), f[3].parse::<f64>(),
                f[4].parse::<f64>(), f[5].parse::<f64>(), f[6].parse::<f64>(),
            ) else {
                continue;
            };
            bars.push(Bar {
                date: f[0].to_string(),
                open,
                close,
                high,
                low,
                volume,
                amount,
                turnover: f[10].parse::<f64>().unwrap_or(0.0),
                pct_chg: f[8].parse::<f64>().unwrap_or(0.0),
            });
        }
        if bars.is_empty() {
            return Err("东财K线数据为空（可能长期停牌）".into());
        }
        Ok((data.name.unwrap_or_else(|| code.to_string()), bars))
    }

    // ---------- 公共 ----------

    fn save_cache(&self, path: &Path, now: &chrono::DateTime<Local>, name: &str, bars: &[Bar]) {
        let fetched = now.format("%Y-%m-%d %H:%M").to_string();
        let cached = CachedKline { fetched, name: name.to_string(), bars: bars.to_vec() };
        if let Ok(json) = serde_json::to_string(&cached) {
            let _ = std::fs::write(path, json);
        }
    }

    async fn get_with_retry(&self, url: &str) -> Result<String, String> {
        let mut last_err = String::new();
        for _ in 0..2 {
            match self.http.get(url).send().await {
                Ok(resp) => match resp.text().await {
                    Ok(t) => return Ok(t),
                    Err(e) => last_err = format!("读取响应失败: {e}"),
                },
                Err(e) => last_err = format!("请求失败: {e}"),
            }
            tokio::time::sleep(Duration::from_millis(600)).await;
        }
        Err(last_err)
    }
}

/// 6位代码 -> 腾讯 symbol：6 开头沪市，0/3 深市，4/8/9 北交所
pub fn tencent_symbol(code: &str) -> String {
    let prefix = match code.chars().next() {
        Some('6') => "sh",
        Some('0') | Some('3') => "sz",
        _ => "bj",
    };
    format!("{prefix}{code}")
}

/// 6位代码 -> 东财 secid：6 开头沪市，0/3 深市，4/8/9 北交所
pub fn secid_of(code: &str) -> Option<String> {
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let c = code.chars().next()?;
    match c {
        '6' => Some(format!("1.{code}")),
        '0' | '3' | '4' | '8' | '9' => Some(format!("0.{code}")),
        _ => None,
    }
}

/// 宽容解析用户输入：支持 600519 / sh600519 / 600519.SH 等
pub fn parse_code(input: &str) -> Option<String> {
    let s = input.trim().to_ascii_lowercase();
    let s = s
        .strip_prefix("sh")
        .or_else(|| s.strip_prefix("sz"))
        .or_else(|| s.strip_prefix("bj"))
        .unwrap_or(&s);
    let s = s.split('.').next().unwrap_or(s);
    let s = s.trim();
    if s.len() == 6 && s.chars().all(|c| c.is_ascii_digit()) {
        Some(s.to_string())
    } else {
        None
    }
}

fn urlencoded(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secid() {
        assert_eq!(secid_of("600519"), Some("1.600519".into()));
        assert_eq!(secid_of("000001"), Some("0.000001".into()));
        assert_eq!(secid_of("300750"), Some("0.300750".into()));
        assert_eq!(secid_of("920008"), Some("0.920008".into()));
        assert_eq!(secid_of("60051"), None);
    }

    #[test]
    fn test_tencent_symbol() {
        assert_eq!(tencent_symbol("600519"), "sh600519");
        assert_eq!(tencent_symbol("000001"), "sz000001");
        assert_eq!(tencent_symbol("300750"), "sz300750");
    }

    #[test]
    fn test_parse_code() {
        assert_eq!(parse_code("600519"), Some("600519".into()));
        assert_eq!(parse_code(" SH600519 "), Some("600519".into()));
        assert_eq!(parse_code("600519.SH"), Some("600519".into()));
        assert_eq!(parse_code("茅台"), None);
    }

    #[test]
    fn test_kline_cache_fresh() {
        let mk = |s: &str| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
                .unwrap()
                .and_local_timezone(Local)
                .single()
                .unwrap()
        };
        // 周一盘内：10 分钟外过期
        let now = mk("2026-09-14 10:30"); // 周一
        assert!(!kline_cache_fresh("2026-09-14 10:00", now));
        // 盘内：10 分钟内有效
        assert!(kline_cache_fresh("2026-09-14 10:25", now));
        // 盘前缓存带进盘中：过期
        assert!(!kline_cache_fresh("2026-09-14 09:05", now));
        // 当日收盘后：一直有效
        assert!(kline_cache_fresh("2026-09-14 15:40", mk("2026-09-14 23:00")));
        // 跨日且已开盘：过期（昨日缓存不能用于今日盘中）
        assert!(!kline_cache_fresh("2026-09-14 15:40", mk("2026-09-15 10:00")));
        // 跨日但仍在盘前（<9:15）：昨收数据本就是最新，视为有效
        assert!(kline_cache_fresh("2026-09-14 15:40", mk("2026-09-15 09:00")));
        // 周六缓存到周日：24 小时内有效
        assert!(kline_cache_fresh("2026-09-12 20:00", mk("2026-09-13 10:00")));
        // 节假日无法与工作日区分：盘外时段统一要求缓存抓取于最近收盘之后，
        // 节假日期间最多多拉取一次，之后当日缓存有效
        assert!(!kline_cache_fresh("2026-10-01 11:00", mk("2026-10-01 18:00")));
        assert!(kline_cache_fresh("2026-10-01 16:00", mk("2026-10-01 18:00")));
        // 回归：盘中抓的快照，当晚必须过期（不能把盘中价当收盘价用到深夜）
        assert!(!kline_cache_fresh("2026-09-15 14:53", mk("2026-09-15 22:05")));
        // 回归：昨晚盘外抓的缓存，今晚不能继续当最新数据用
        assert!(!kline_cache_fresh("2026-09-14 20:00", mk("2026-09-15 22:00")));
        // 回归：周五盘中抓的快照，周末不能当周五收盘用
        assert!(!kline_cache_fresh("2026-09-11 14:00", mk("2026-09-12 12:00")));
        // 旧格式（纯日期）：当日有效
        assert!(kline_cache_fresh("2026-09-14", mk("2026-09-14 20:00")));
    }
}
