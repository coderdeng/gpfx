//! AI 分析建议：将结构化技术数据打包发给大模型（OpenAI 兼容接口），
//! 获取文字解读与建议。
//!
//! 配置（环境变量或 backend/.env）：
//!   AI_API_KEY   — 必填，如 sk-xxx
//!   AI_BASE_URL  — 可选，默认 https://api.openai.com/v1（兼容任何 OpenAI 格式接口）
//!   AI_MODEL     — 可选，默认 gpt-4o-mini
//!
//! 未配置 AI_API_KEY 时返回 configured=false，前端显示提示。

use crate::models::{AiAdvice, Bar, ScoreResult, SessionReport, Valuation, Financials};
use chrono::Local;
use std::path::Path;
use std::time::Duration;

pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

/// 一次 AI 调用的完整记录（日志文件 + 前端展示用）
#[derive(Debug, Clone, serde::Serialize)]
pub struct AiCallLog {
    pub code: String,
    pub name: String,
    pub model: String,
    /// 完整 user prompt
    pub prompt: String,
    /// 模型返回原文
    pub response: String,
    /// prompt 字符数
    pub prompt_chars: usize,
    pub created_at: String,
    pub cached: bool,
}

impl AiConfig {
    /// 从环境变量与 backend/.env 读取配置
    pub fn load(data_dir: &Path) -> Self {
        // .env 简易解析（KEY=VALUE），不引入第三方依赖
        let env_file = data_dir.parent().unwrap_or(Path::new(".")).join(".env");
        let mut vals: Vec<(String, String)> = Vec::new();
        if let Ok(text) = std::fs::read_to_string(&env_file) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    vals.push((k.trim().to_string(), v.trim().trim_matches('"').to_string()));
                }
            }
        }
        let get = |k: &str| -> Option<String> {
            std::env::var(k).ok().filter(|s| !s.is_empty()).or_else(|| {
                vals.iter()
                    .find(|(kk, _)| kk == k)
                    .map(|(_, v)| v.clone())
                    .filter(|s| !s.is_empty())
            })
        };
        Self {
            api_key: get("AI_API_KEY").unwrap_or_default(),
            base_url: get("AI_BASE_URL").unwrap_or_else(|| "https://api.openai.com/v1".into()),
            model: get("AI_MODEL").unwrap_or_else(|| "gpt-4o-mini".into()),
        }
    }

    pub fn configured(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// 生成 AI 建议。同一股票同一数据签名当日缓存，避免重复消耗 token。
pub async fn advise(
    st_code: &str,
    name: &str,
    bars: &[Bar],
    score: &ScoreResult,
    valuation: Option<&Valuation>,
    financials: Option<&Financials>,
    session: &SessionReport,
    position: Option<(f64, f64)>, // (cost, shares)
    risk_label: &str,
    cfg: &AiConfig,
    cache_dir: &Path,
) -> AiAdvice {
    let now = Local::now();
    if !cfg.configured() {
        return AiAdvice {
            configured: false,
            model: String::new(),
            content: "尚未配置 AI 服务。请在 backend/.env 中设置 AI_API_KEY（可选 AI_BASE_URL / AI_MODEL），然后重启后端。".into(),
            generated_at: String::new(),
            cached: false,
        };
    }

    // 缓存键：代码 + 最新K线日期 + 收盘价 + 评分（数据变了就重新请求）
    let last = bars.last().unwrap();
    let sig = format!(
        "{}|{}|{}|{:.2}",
        st_code, last.date, last.close, score.total
    );
    let cache_file = cache_dir.join(format!("ai_{}.json", st_code));
    if let Ok(text) = std::fs::read_to_string(&cache_file) {
        if let Ok(c) = serde_json::from_str::<serde_json::Value>(&text) {
            if c["sig"] == serde_json::json!(sig) && c["model"] == serde_json::json!(cfg.model) {
                return AiAdvice {
                    configured: true,
                    model: cfg.model.clone(),
                    content: c["content"].as_str().unwrap_or_default().into(),
                    generated_at: c["generated_at"].as_str().unwrap_or_default().into(),
                    cached: true,
                };
            }
        }
    }

    let prompt = build_prompt(
        st_code, name, bars, score, valuation, financials, session, position, risk_label,
    );
    let prompt_chars = prompt.chars().count();
    let (content, cached, created) = match call_openai(cfg, &prompt).await {
        Ok(c) => (c, false, now.format("%Y-%m-%d %H:%M").to_string()),
        Err(e) => {
            return AiAdvice {
                configured: true,
                model: cfg.model.clone(),
                content: format!("⚠️ AI 请求失败：{e}\n\n请检查 AI_API_KEY / AI_BASE_URL / AI_MODEL 配置与网络。"),
                generated_at: now.format("%Y-%m-%d %H:%M").to_string(),
                cached: false,
            }
        }
    };
    // 记录完整调用日志（追加到 data/ai_log.jsonl，含 prompt 与响应）
    let log_entry = serde_json::json!({
        "code": st_code, "name": name, "model": cfg.model,
        "created_at": created, "prompt_chars": prompt_chars,
        "prompt": prompt, "response": content,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(cache_dir.join("ai_log.jsonl"))
    {
        use std::io::Write;
        let _ = writeln!(f, "{log_entry}");
    }
    let generated_at = created;
    let _ = serde_json::to_string_pretty(&serde_json::json!({
        "sig": sig, "model": cfg.model, "generated_at": generated_at, "content": content,
    }))
    .map(|j| std::fs::write(&cache_file, j));
    AiAdvice { configured: true, model: cfg.model.clone(), content, generated_at, cached: false }
}

/// 读取最近一次该股票的完整调用日志（prompt + 响应）
pub fn last_call_log(cache_dir: &Path, code: &str) -> Option<AiCallLog> {
    let text = std::fs::read_to_string(cache_dir.join("ai_log.jsonl")).ok()?;
    let mut last: Option<AiCallLog> = None;
    for line in text.lines().rev() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v["code"] == serde_json::json!(code) {
                last = Some(AiCallLog {
                    code: v["code"].as_str()?.to_string(),
                    name: v["name"].as_str()?.to_string(),
                    model: v["model"].as_str()?.to_string(),
                    prompt: v["prompt"].as_str()?.to_string(),
                    response: v["response"].as_str()?.to_string(),
                    prompt_chars: v["prompt_chars"].as_u64()? as usize,
                    created_at: v["created_at"].as_str()?.to_string(),
                    cached: true,
                });
                break;
            }
        }
    }
    last
}

#[allow(clippy::too_many_arguments)]
fn build_prompt(
    code: &str,
    name: &str,
    bars: &[Bar],
    score: &ScoreResult,
    valuation: Option<&Valuation>,
    financials: Option<&Financials>,
    session: &SessionReport,
    position: Option<(f64, f64)>,
    risk_label: &str,
) -> String {
    let i = bars.len() - 1;
    let b = &bars[i];
    let p = &bars[i - 1];
    let recent: Vec<String> = bars[i.saturating_sub(9)..=i]
        .iter()
        .map(|x| {
            format!(
                "{} 开{} 收{} 高{} 低{} 量{} 涨跌{:.2}%",
                x.date, x.open, x.close, x.high, x.low, x.volume, x.pct_chg
            )
        })
        .collect();

    let mut s = format!(
        "你是严谨的A股技术分析助手。基于下面系统已算好的数据做解读与操作建议，不要编造数据。\
用中文 markdown 输出，结构：1)一句话结论 2)技术面解读 3)基本面解读 4)风险提示 5)操作建议（分持仓者/未持仓者）。\
语言克制、观点明确、给具体价位。技术分析仅供参考不构成投资建议。\n\n\
【标的】{name}（{code}） 最新交易日 {date} 收盘 {close:.2}（{pchg:+.2}%），风险风格：{risk}\n\
【综合评分】{total:.1}（{rating}）；技术面 {tech:.1} + 基本面调整 {adj:.1}\n\
【六因子】\n",
        name = name, code = code, date = b.date, close = b.close, pchg = b.pct_chg,
        risk = risk_label, total = score.total, rating = score.rating,
        tech = score.tech_total, adj = score.fundamental_adj
    );
    for f in &score.factors {
        s.push_str(&format!(
            "- {}（权重{:.0}%）：{:.0}。{}\n",
            f.name,
            f.weight * 100.0,
            f.score,
            f.reasons.join("；")
        ));
    }
    s.push_str(&format!(
        "\n【当日K线】开 {:.2} 收 {:.2} 高 {:.2} 低 {:.2} 量 {:.0}（昨收 {:.2}）\n",
        b.open, b.close, b.high, b.low, b.volume, p.close
    ));
    s.push_str(&format!("【量能】{}\n", session.volume_desc));
    s.push_str(&format!("【形态】{}\n", session.candle_desc));
    s.push_str(&format!(
        "\n【关键位攻防】\n{}",
        session
            .battles
            .iter()
            .map(|x| format!("- {}：{}（距收 {:.1}%）", x.label, x.result, x.gap_pct))
            .collect::<Vec<_>>()
            .join("\n")
    ));
    s.push_str(&format!(
        "\n【多空要点】多方：{}；空方：{}\n",
        session.bull_points.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join("；"),
        session.bear_points.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join("；")
    ));
    if let Some(v) = valuation {
        s.push_str(&format!(
            "\n【估值】PE {} / PB {} / 总市值 {}亿\n",
            fmt_opt(v.pe),
            fmt_opt(v.pb),
            fmt_opt(v.market_cap)
        ));
    }
    if let Some(f) = financials {
        s.push_str(&format!(
            "【财报】报告期 {}：营收同比 {}%，归母净利同比 {}%，ROE {}%，毛利率 {}\n",
            f.report_date,
            fmt_pct(f.revenue_yoy),
            fmt_pct(f.profit_yoy),
            fmt_pct(f.roe),
            f.gross_margin.map(|g| format!("{g:.1}")).unwrap_or_else(|| "—".into())
        ));
    }
    match position {
        Some((cost, shares)) => s.push_str(&format!(
            "【用户持仓】持有 {} 股，成本 {:.2}（浮盈 {:.1}%）\n",
            shares,
            cost,
            (b.close - cost) / cost * 100.0
        )),
        None => s.push_str("【用户持仓】未持有\n"),
    }
    s.push_str(&format!("\n【近10日行情】\n{}\n", recent.join("\n")));
    s
}

/// Option 数值 → "—" 或保留 2 位小数
fn fmt_opt(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "—".into())
}

/// 百分比数值 → "+1.78%" 形式
fn fmt_pct(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:+.1}"),
        None => "—".into(),
    }
}

async fn call_openai(cfg: &AiConfig, prompt: &str) -> Result<String, String> {    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| e.to_string())?;
    let body = serde_json::json!({
        "model": cfg.model,
        "messages": [
            { "role": "system", "content": "你是专业的A股技术分析助手，基于给定数据做客观分析，输出中文 markdown。" },
            { "role": "user", "content": prompt }
        ],
        "temperature": 0.4
    });
    let resp = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", &text[..text.len().min(300)]));
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {e}"))?;
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("响应缺少 choices[0].message.content：{}", &text[..text.len().min(300)]))
}
