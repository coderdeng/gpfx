//! 观察列表：按用户 JSON 持久化，记录添加时的基准价用于计算此后涨跌。

use chrono::Local;
use std::path::PathBuf;

/// 单只观察标的
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WatchItem {
    pub code: String,
    pub name: String,
    /// 添加时刻 "YYYY-MM-DD HH:MM"
    pub added_at: String,
    /// 添加时最近一根日K的日期
    pub base_date: String,
    /// 添加时最近一根日K的收盘价（盘内添加即为当时最新价）
    pub base_close: f64,
}

/// 观察列表容量上限，防止无限增长
pub const MAX_ITEMS: usize = 50;

pub struct WatchStore {
    path: PathBuf,
    items: Vec<WatchItem>,
}

impl WatchStore {
    /// 按用户加载：data/users/<user>/watchlist.json
    pub fn load_for(data_dir: &std::path::Path, user: &str) -> Self {
        let user_dir = data_dir.join("users").join(user);
        let _ = std::fs::create_dir_all(&user_dir);
        Self::load(user_dir.join("watchlist.json"))
    }

    pub fn load(path: PathBuf) -> Self {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(items) = serde_json::from_str::<Vec<WatchItem>>(&text) {
                return Self { path, items };
            }
        }
        Self { path, items: Vec::new() }
    }

    pub fn list(&self) -> Vec<WatchItem> {
        self.items.clone()
    }

    pub fn contains(&self, code: &str) -> bool {
        self.items.iter().any(|i| i.code == code)
    }

    /// 添加：基准取添加时最近一根日K（盘内即当时最新价）
    pub fn add(
        &mut self,
        code: &str,
        name: &str,
        base_date: &str,
        base_close: f64,
    ) -> Result<WatchItem, String> {
        if self.contains(code) {
            return Err("该股票已在观察列表中".into());
        }
        if self.items.len() >= MAX_ITEMS {
            return Err(format!("观察列表最多容纳 {} 只", MAX_ITEMS));
        }
        if !(base_close > 0.0) {
            return Err("行情基准价无效，无法添加".into());
        }
        let item = WatchItem {
            code: code.to_string(),
            name: if name.trim().is_empty() { code.to_string() } else { name.trim().to_string() },
            added_at: Local::now().format("%Y-%m-%d %H:%M").to_string(),
            base_date: base_date.to_string(),
            base_close,
        };
        self.items.push(item.clone());
        self.persist()?;
        Ok(item)
    }

    pub fn remove(&mut self, code: &str) -> Result<(), String> {
        let before = self.items.len();
        self.items.retain(|i| i.code != code);
        if self.items.len() == before {
            return Err("该股票不在观察列表中".into());
        }
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        let s = serde_json::to_string_pretty(&self.items).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, s).map_err(|e| format!("写入观察列表文件失败: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("test_watchlist_{}_{tag}.json", std::process::id()))
    }

    #[test]
    fn test_add_dedup_remove_persist() {
        let path = tmp_path("a");
        let _ = std::fs::remove_file(&path);
        let mut st = WatchStore::load(path.clone());

        let it = st.add("002202", "金风科技", "2026-09-15", 18.82).unwrap();
        assert_eq!(it.base_close, 18.82);
        assert_eq!(it.base_date, "2026-09-15");
        assert!(st.contains("002202"));
        // 重复添加被拒
        assert!(st.add("002202", "金风科技", "2026-09-15", 18.82).is_err());
        // 空名称回退为代码
        let it2 = st.add("600519", "  ", "2026-09-15", 1400.0).unwrap();
        assert_eq!(it2.name, "600519");
        // 无效基准价被拒
        assert!(st.add("300750", "宁德时代", "2026-09-15", 0.0).is_err());

        st.remove("002202").unwrap();
        assert!(!st.contains("002202"));
        assert!(st.remove("002202").is_err());
        assert_eq!(st.list().len(), 1);

        // 持久化：重新加载内容仍在
        let st2 = WatchStore::load(path.clone());
        assert_eq!(st2.list().len(), 1);
        assert_eq!(st2.list()[0].code, "600519");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_capacity_limit() {
        let path = tmp_path("b");
        let _ = std::fs::remove_file(&path);
        let mut st = WatchStore::load(path.clone());
        for i in 0..MAX_ITEMS {
            st.add(&format!("{i:06}"), &format!("股{i}"), "2026-09-15", 10.0).unwrap();
        }
        assert!(st.add("999999", "超限股", "2026-09-15", 10.0).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
