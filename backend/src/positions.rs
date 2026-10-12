//! 持仓管理：JSON 文件持久化。

use crate::models::{Position, PositionInput};
use chrono::Local;
use std::path::PathBuf;

pub struct PositionStore {
    path: PathBuf,
    items: Vec<Position>,
    next_id: u64,
}

impl PositionStore {
    /// 按用户加载：data/users/<user>/positions.json
    pub fn load_for(data_dir: &std::path::Path, user: &str) -> Self {
        let user_dir = data_dir.join("users").join(user);
        let _ = std::fs::create_dir_all(&user_dir);
        Self::load(user_dir.join("positions.json"))
    }

    /// 兼容旧路径（data/positions.json）
    pub fn load(path: PathBuf) -> Self {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(items) = serde_json::from_str::<Vec<Position>>(&text) {
                let next_id = items.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                return Self { path, items, next_id };
            }
        }
        Self { path, items: Vec::new(), next_id: 1 }
    }

    pub fn list(&self) -> Vec<Position> {
        self.items.clone()
    }

    pub fn find_by_code(&self, code: &str) -> Option<Position> {
        self.items.iter().find(|p| p.code == code).cloned()
    }

    pub fn add(&mut self, input: PositionInput) -> Result<Position, String> {
        validate(&input)?;
        let pos = Position {
            id: self.next_id,
            code: input.code,
            name: input.name,
            cost: input.cost,
            shares: input.shares,
            created: Local::now().format("%Y-%m-%d %H:%M").to_string(),
        };
        self.next_id += 1;
        self.items.push(pos.clone());
        self.persist()?;
        Ok(pos)
    }

    pub fn update(&mut self, id: u64, input: PositionInput) -> Result<Position, String> {
        validate(&input)?;
        let p = self
            .items
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| "持仓不存在".to_string())?;
        p.code = input.code;
        p.name = input.name;
        p.cost = input.cost;
        p.shares = input.shares;
        let out = p.clone();
        self.persist()?;
        Ok(out)
    }

    pub fn remove(&mut self, id: u64) -> Result<(), String> {
        let before = self.items.len();
        self.items.retain(|p| p.id != id);
        if self.items.len() == before {
            return Err("持仓不存在".into());
        }
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        let s = serde_json::to_string_pretty(&self.items).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, s).map_err(|e| format!("写入持仓文件失败: {e}"))
    }
}

fn validate(input: &PositionInput) -> Result<(), String> {
    if input.code.trim().is_empty() {
        return Err("股票代码不能为空".into());
    }
    if !(input.cost > 0.0) {
        return Err("成本价必须大于 0".into());
    }
    if !(input.shares > 0.0) {
        return Err("持股数量必须大于 0".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_accounting() {
        let path = std::env::temp_dir().join("test_pos_fee.json");
        let _ = std::fs::remove_file(&path);
        let mut store = PositionStore::load(path.clone());

        // 买入 100股@10，费用 10 → 成本含费 (1000+10)/100 = 10.10
        let p = store.buy("tst001", "测试股", 10.0, 100.0, 10.0).unwrap();
        assert!((p.cost - 10.10).abs() < 1e-9);

        // 再买 100股@20 无费用 → (10.10×100 + 2000)/200 = 15.05
        let p2 = store.buy("tst001", "测试股", 20.0, 100.0, 0.0).unwrap();
        assert!((p2.cost - 15.05).abs() < 1e-9);

        // 卖出 100股@20，费用 15 → (20−15.05)×100 − 15 = 480
        let o = store.sell("tst001", 20.0, 100.0, 15.0).unwrap();
        assert!((o.realized - 480.0).abs() < 1e-9);
        assert!(!o.closed);
        // 摊薄口径：卖出净回款回冲剩余成本 → (15.05×200 − (20×100−15)) / 100 = 10.25
        let p = store.find_by_code("tst001").unwrap();
        assert!((p.cost - 10.25).abs() < 1e-9);

        // 清仓 → 持仓移除
        let o2 = store.sell("tst001", 20.0, 100.0, 0.0).unwrap();
        assert!(o2.closed);
        assert!(store.list().is_empty());
        let _ = std::fs::remove_file(&path);
    }
}

/// 卖出结果
pub struct SellOutcome {
    pub name: String,
    /// 按当时持仓成本扣费后的净盈亏
    pub realized: f64,
    /// 本次卖出费用
    pub fee: f64,
    /// 是否已清仓（持仓被移除）
    pub closed: bool,
}

impl PositionStore {
    /// 买入：费用计入持仓成本（摊薄口径）；已持有则加权摊薄，未持有则新建
    pub fn buy(&mut self, code: &str, name: &str, price: f64, shares: f64, fee: f64) -> Result<Position, String> {
        if !(price > 0.0) || !(shares > 0.0) {
            return Err("成交价与股数必须大于 0".into());
        }
        if let Some(p) = self.items.iter_mut().find(|p| p.code == code) {
            let new_shares = p.shares + shares;
            p.cost = (p.cost * p.shares + price * shares + fee) / new_shares;
            p.shares = new_shares;
            let out = p.clone();
            self.persist()?;
            Ok(out)
        } else {
            self.add(PositionInput {
                code: code.to_string(),
                name: name.to_string(),
                cost: (price * shares + fee) / shares,
                shares,
            })
        }
    }

    /// 卖出：盈亏 = (卖价 − 成本) × 股数 − 卖出费用；清仓则移除持仓。
    /// 成本按券商摊薄口径回冲：卖出净回款（成交额−费用）冲减剩余持仓总成本，
    /// 盈利摊低成本、亏损抬高成本。
    pub fn sell(&mut self, code: &str, price: f64, shares: f64, fee: f64) -> Result<SellOutcome, String> {
        if !(price > 0.0) || !(shares > 0.0) {
            return Err("成交价与股数必须大于 0".into());
        }
        let idx = self
            .items
            .iter()
            .position(|p| p.code == code)
            .ok_or_else(|| "未持有该股，无法卖出".to_string())?;
        let p = &mut self.items[idx];
        if shares > p.shares + 1e-9 {
            return Err(format!("卖出股数超过持有量（当前持有 {} 股）", p.shares as i64));
        }
        let realized = (price - p.cost) * shares - fee;
        let remaining = p.shares - shares;
        let closed = remaining <= 1e-9;
        if !closed {
            p.cost = (p.cost * p.shares - (price * shares - fee)) / remaining;
        }
        let name = p.name.clone();
        p.shares = remaining;
        if closed {
            self.items.remove(idx);
        }
        self.persist()?;
        Ok(SellOutcome { name, realized, fee, closed })
    }
}
