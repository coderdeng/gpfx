//! 交易记录（模拟盘买卖流水）：JSON 文件持久化。

use crate::models::{Trade, TradeContext};
use chrono::Local;
use std::path::PathBuf;

pub struct TradeStore {
    path: PathBuf,
    items: Vec<Trade>,
    next_id: u64,
}

impl TradeStore {
    /// 按用户加载：data/users/<user>/trades.json
    pub fn load_for(data_dir: &std::path::Path, user: &str) -> Self {
        let user_dir = data_dir.join("users").join(user);
        let _ = std::fs::create_dir_all(&user_dir);
        Self::load(user_dir.join("trades.json"))
    }

    pub fn load(path: PathBuf) -> Self {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(items) = serde_json::from_str::<Vec<Trade>>(&text) {
                let next_id = items.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                return Self { path, items, next_id };
            }
        }
        Self { path, items: Vec::new(), next_id: 1 }
    }

    /// 全部流水，最新在前
    pub fn list(&self) -> Vec<Trade> {
        let mut v = self.items.clone();
        v.reverse();
        v
    }

    pub fn for_code(&self, code: &str) -> Vec<Trade> {
        self.list().into_iter().filter(|t| t.code == code).collect()
    }

    /// 某只股票历史卖出已实现盈亏
    pub fn realized_for_code(&self, code: &str) -> f64 {
        self.items
            .iter()
            .filter(|t| t.code == code)
            .filter_map(|t| t.realized_pnl)
            .sum::<f64>()
            + 0.0 // 归一化 -0.0
    }

    pub fn total_realized(&self) -> f64 {
        self.items
            .iter()
            .filter_map(|t| t.realized_pnl)
            .sum::<f64>()
            + 0.0
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        code: &str,
        name: &str,
        side: &str,
        price: f64,
        shares: f64,
        fee: f64,
        realized_pnl: Option<f64>,
        note: Option<String>,
        context: Option<TradeContext>,
    ) -> Result<Trade, String> {
        let trade = Trade {
            id: self.next_id,
            code: code.to_string(),
            name: name.to_string(),
            side: side.to_string(),
            price,
            shares,
            fee,
            realized_pnl,
            note,
            context,
            created: Local::now().format("%Y-%m-%d %H:%M").to_string(),
        };
        self.next_id += 1;
        self.items.push(trade.clone());
        let s = serde_json::to_string_pretty(&self.items).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, s).map_err(|e| format!("写入交易记录失败: {e}"))?;
        Ok(trade)
    }
}
