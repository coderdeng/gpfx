//! 用户账号存储与认证：用户名 + 密码（PBKDF2-HMAC-SHA256 十万次迭代），token 会话。
//!
//! 安全机制：
//! - 密码存储：标准 PBKDF2-HMAC-SHA256（10 万次迭代，自实现无第三方依赖）
//! - token：用户名 + 过期时间 + HMAC 签名（无状态，重启不失效，90 天有效）；
//!   签名密钥优先取 STOCK_APP_SECRET，否则首次运行自动生成 data/app_secret（0600）
//! - LoginGuard：按 IP 的登录失败限流（10 分钟内失败 5 次锁 10 分钟）

use chrono::{Duration, Local};
use std::{
    collections::HashMap,
    io::Read,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration as StdDuration, Instant},
};

/// 密码哈希迭代次数（OWASP 建议 PBKDF2-SHA256 ≥ 60 万，个人应用取 10 万平衡登录耗时）
pub const PBKDF2_ITERS: u32 = 100_000;

// ---------- SHA-256 / HMAC / PBKDF2（自实现，无第三方依赖） ----------

const SHA256_IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 压缩函数：处理一个 64 字节块（标准实现）
fn sha256_compress(h: &mut [u32; 8], block: &[u8; 64]) {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
        (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    h[0] = h[0].wrapping_add(a);
    h[1] = h[1].wrapping_add(b);
    h[2] = h[2].wrapping_add(c);
    h[3] = h[3].wrapping_add(d);
    h[4] = h[4].wrapping_add(e);
    h[5] = h[5].wrapping_add(f);
    h[6] = h[6].wrapping_add(g);
    h[7] = h[7].wrapping_add(hh);
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = SHA256_IV;
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut block = [0u8; 64];
        block.copy_from_slice(chunk);
        sha256_compress(&mut h, &block);
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// HMAC-SHA256（仅在登录/注册/token 等低频路径调用）
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = ipad.to_vec();
    inner.extend_from_slice(msg);
    let ih = sha256(&inner);
    let mut outer = opad.to_vec();
    outer.extend_from_slice(&ih);
    sha256(&outer)
}

/// HMAC 的 ipad/opad 中间态（密钥不变时预计算一次，供 PBKDF2 热循环复用）
fn hmac_midstates(key: &[u8]) -> ([u32; 8], [u32; 8]) {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut istate = SHA256_IV;
    sha256_compress(&mut istate, &ipad);
    let mut ostate = SHA256_IV;
    sha256_compress(&mut ostate, &opad);
    (istate, ostate)
}

/// 用中间态完成一次 HMAC，要求 msg ≤ 55 字节（单块收尾；PBKDF2 循环内恒为 32 字节）
fn hmac_block(istate: &[u32; 8], ostate: &[u32; 8], msg: &[u8]) -> [u8; 32] {
    debug_assert!(msg.len() <= 55);
    let mut block = [0u8; 64];
    block[..msg.len()].copy_from_slice(msg);
    block[msg.len()] = 0x80;
    let bits = ((64u64 + msg.len() as u64) * 8).to_be_bytes();
    block[56..].copy_from_slice(&bits);
    let mut h = *istate;
    sha256_compress(&mut h, &block);
    let mut inner = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        inner[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    let mut block2 = [0u8; 64];
    block2[..32].copy_from_slice(&inner);
    block2[32] = 0x80;
    block2[56..].copy_from_slice(&((64u64 + 32) * 8).to_be_bytes());
    let mut h2 = *ostate;
    sha256_compress(&mut h2, &block2);
    let mut out = [0u8; 32];
    for (i, v) in h2.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// 标准 PBKDF2-HMAC-SHA256 单块派生（RFC 8018）
fn pbkdf2_sha256(password: &[u8], salt: &[u8], iters: u32) -> [u8; 32] {
    assert!(iters >= 1, "PBKDF2 至少迭代 1 次");
    let (istate, ostate) = hmac_midstates(password);
    // U1 = HMAC(P, S || INT_32_BE(1))
    let mut msg = Vec::with_capacity(salt.len() + 4);
    msg.extend_from_slice(salt);
    msg.extend_from_slice(&1u32.to_be_bytes());
    let mut u = if msg.len() <= 55 {
        hmac_block(&istate, &ostate, &msg)
    } else {
        hmac_sha256(password, &msg)
    };
    let mut out = u;
    for _ in 1..iters {
        u = hmac_block(&istate, &ostate, &u);
        for i in 0..32 {
            out[i] ^= u[i];
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------- 用户存储 ----------

#[derive(Clone)]
struct User {
    /// 密码哈希（hex）
    pass_hash: String,
    salt: String,
    created: String,
    /// KDF 迭代次数（注册时固定为 PBKDF2_ITERS，预留未来上调空间）
    iters: u32,
}

/// 线程安全的用户存储（内部 std 锁；方法均为同步纯内存操作）
#[derive(Clone)]
pub struct UserStore {
    inner: Arc<std::sync::Mutex<Inner>>,
}

struct Inner {
    users: HashMap<String, User>,
    path: Option<PathBuf>,
    /// token 签名密钥（启动时解析一次）
    secret: String,
}

impl UserStore {
    pub fn load(path: &Path) -> Self {
        let mut users = HashMap::new();
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(obj) = v.as_object() {
                    for (name, u) in obj {
                        users.insert(
                            name.clone(),
                            User {
                                pass_hash: u["pass_hash"].as_str().unwrap_or("").into(),
                                salt: u["salt"].as_str().unwrap_or("").into(),
                                created: u["created"].as_str().unwrap_or("").into(),
                                iters: u["iters"].as_u64().unwrap_or(0) as u32,
                            },
                        );
                    }
                }
            }
        }
        let secret = resolve_secret(path.parent());
        Self {
            inner: Arc::new(std::sync::Mutex::new(Inner {
                users,
                path: Some(path.to_path_buf()),
                secret,
            })),
        }
    }

    pub fn exists(&self, name: &str) -> bool {
        self.inner.lock().unwrap().users.contains_key(name)
    }

    pub fn list_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.inner.lock().unwrap().users.keys().cloned().collect();
        v.sort();
        v
    }

    /// 注册新用户
    pub fn register(&self, name: &str, password: &str) -> Result<(), String> {
        if name.len() < 2 || name.len() > 24 {
            return Err("用户名长度需在 2~24 字符".into());
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || !c.is_ascii()) {
            return Err("用户名仅支持字母/数字/下划线/连字符/中文".into());
        }
        if password.len() < 4 {
            return Err("密码至少 4 位".into());
        }
        let mut inner = self.inner.lock().unwrap();
        if inner.users.contains_key(name) {
            return Err("用户名已被注册".into());
        }
        let salt = format!(
            "{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as u64
                ^ (name.len() as u64) << 32
        );
        let hash = hex(&pbkdf2_sha256(password.as_bytes(), salt.as_bytes(), PBKDF2_ITERS));
        inner.users.insert(
            name.to_string(),
            User {
                pass_hash: hash,
                salt,
                created: Local::now().format("%Y-%m-%d").to_string(),
                iters: PBKDF2_ITERS,
            },
        );
        persist(&inner)
    }

    pub fn verify(&self, name: &str, password: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        let Some(u) = inner.users.get(name) else { return false };
        hex(&pbkdf2_sha256(
            password.as_bytes(),
            u.salt.as_bytes(),
            u.iters,
        )) == u.pass_hash
    }

    /// 会话 token：用户名 + 过期时间 + 签名（无状态，重启不失效，90 天有效）
    pub fn token_for(&self, name: &str) -> String {
        let exp = (Local::now() + Duration::days(90)).timestamp();
        let secret = self.token_secret(name, exp);
        let sig = hex(&hmac_sha256(name.as_bytes(), secret.as_bytes()));
        format!("{}.{}.{}", name, exp, &sig[..16])
    }

    pub fn verify_token(&self, token: &str) -> Option<String> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        let (name, exp, sig) = (parts[0], parts[1], parts[2]);
        let exp: i64 = exp.parse().ok()?;
        if Local::now().timestamp() > exp {
            return None;
        }
        let secret = self.token_secret(name, exp);
        let expect = hex(&hmac_sha256(name.as_bytes(), secret.as_bytes()));
        if &expect[..16] == sig && self.exists(name) {
            Some(name.to_string())
        } else {
            None
        }
    }

    fn token_secret(&self, name: &str, exp: i64) -> String {
        format!("{}|{}|{}", name, exp, self.inner.lock().unwrap().secret)
    }
}

fn persist(inner: &Inner) -> Result<(), String> {
    let Some(path) = &inner.path else { return Ok(()) };
    let mut out = serde_json::Map::new();
    for (name, u) in &inner.users {
        out.insert(
            name.clone(),
            serde_json::json!({
                "pass_hash": u.pass_hash,
                "salt": u.salt,
                "created": u.created,
                "iters": u.iters,
            }),
        );
    }
    std::fs::write(path, serde_json::to_string_pretty(&out).unwrap_or_default())
        .map_err(|e| format!("写入用户文件失败: {e}"))
}

/// token 签名密钥：环境变量 STOCK_APP_SECRET 优先；
/// 否则首次运行生成 32 字节随机密钥存入 data/app_secret（0600），重启后 token 不失效
fn resolve_secret(data_dir: Option<&Path>) -> String {
    if let Ok(s) = std::env::var("STOCK_APP_SECRET") {
        if !s.trim().is_empty() {
            return s.trim().to_string();
        }
    }
    const DEFAULT: &str = "stock-analysis-local-secret";
    let Some(dir) = data_dir else { return DEFAULT.into() };
    let path = dir.join("app_secret");
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let mut buf = [0u8; 32];
    let rnd_ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_ok();
    if !rnd_ok {
        // /dev/urandom 不可用时的退化来源（个人设备足够，且下不为例）
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        buf = [n.to_be_bytes(), n.to_be_bytes(), n.to_be_bytes(), n.to_be_bytes()]
            .concat()
            .try_into()
            .unwrap();
    }
    let s = hex(&buf);
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
        {
            let _ = f.write_all(s.as_bytes());
        }
    }
    #[cfg(not(unix))]
    let _ = std::fs::write(&path, &s);
    s
}

// ---------- 登录失败限流 ----------

/// 窗口内最大失败次数
const LOGIN_MAX_FAILS: u32 = 5;
/// 失败计数窗口
const LOGIN_WINDOW: StdDuration = StdDuration::from_secs(600);
/// 达到上限后的锁定时长
const LOGIN_LOCK: StdDuration = StdDuration::from_secs(600);

/// 按 IP 的登录失败限流：10 分钟内失败 5 次锁 10 分钟，成功登录即清零
#[derive(Clone, Default)]
pub struct LoginGuard {
    inner: Arc<std::sync::Mutex<HashMap<IpAddr, Attempt>>>,
}

#[derive(Default)]
struct Attempt {
    count: u32,
    last: Option<Instant>,
    locked_until: Option<Instant>,
}

impl LoginGuard {
    /// 请求进入前检查；被锁定时返回带剩余秒数的错误信息
    pub fn check(&self, ip: IpAddr) -> Result<(), String> {
        let mut m = self.inner.lock().unwrap();
        Self::prune(&mut m);
        if let Some(a) = m.get(&ip) {
            if let Some(lu) = a.locked_until {
                let now = Instant::now();
                if lu > now {
                    return Err(format!(
                        "尝试次数过多，账号已临时锁定，请 {} 秒后再试",
                        (lu - now).as_secs() + 1
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn fail(&self, ip: IpAddr) {
        let mut m = self.inner.lock().unwrap();
        let now = Instant::now();
        let a = m.entry(ip).or_default();
        if a.last.is_some_and(|t| now.duration_since(t) > LOGIN_WINDOW) {
            a.count = 0;
        }
        a.count += 1;
        a.last = Some(now);
        if a.count >= LOGIN_MAX_FAILS {
            a.locked_until = Some(now + LOGIN_LOCK);
        }
    }

    pub fn success(&self, ip: IpAddr) {
        self.inner.lock().unwrap().remove(&ip);
    }

    /// 防止长期运行下 map 无限增长
    fn prune(m: &mut HashMap<IpAddr, Attempt>) {
        if m.len() < 4096 {
            return;
        }
        let now = Instant::now();
        m.retain(|_, a| {
            a.locked_until.is_some_and(|lu| lu > now)
                || a.last.is_some_and(|t| now.duration_since(t) <= LOGIN_WINDOW)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("users_test3_{}_{tag}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn test_sha256_vectors() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_hmac_vector() {
        assert_eq!(
            hex(&hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog")),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn test_pbkdf2_vectors() {
        // RFC 7914 / 公开测试向量：PBKDF2-HMAC-SHA256(P="password", S="salt")
        assert_eq!(
            hex(&pbkdf2_sha256(b"password", b"salt", 1)),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
        assert_eq!(
            hex(&pbkdf2_sha256(b"password", b"salt", 2)),
            "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"
        );
    }

    #[test]
    fn test_register_verify_token() {
        let dir = tmp_dir("a");
        let path = dir.join("users.json");
        let mut s = UserStore::load(&path);
        s.register("yihai", "pass1234").unwrap();
        assert!(s.exists("yihai"));
        assert!(!s.verify("yihai", "wrong"));
        assert!(s.verify("yihai", "pass1234"));
        // 重复注册拒绝
        assert!(s.register("yihai", "x").is_err());
        // token 往返
        let t = s.token_for("yihai");
        assert_eq!(s.verify_token(&t), Some("yihai".into()));
        // 篡改 token 拒绝
        assert_eq!(s.verify_token(&format!("{t}x")), None);
        // 新注册用户应为 10 万次迭代格式
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["yihai"]["iters"].as_u64(), Some(PBKDF2_ITERS as u64));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_token_survives_reload() {
        let dir = tmp_dir("b");
        let path = dir.join("users.json");
        let mut s = UserStore::load(&path);
        s.register("alice", "abcd").unwrap();
        let t = s.token_for("alice");
        drop(s);
        let s2 = UserStore::load(&path);
        assert_eq!(s2.verify_token(&t), Some("alice".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_login_guard_locks_after_failures() {
        let g = LoginGuard::default();
        let ip: IpAddr = "203.0.113.9".parse().unwrap();
        for _ in 0..LOGIN_MAX_FAILS {
            assert!(g.check(ip).is_ok());
            g.fail(ip);
        }
        // 达到上限即锁定
        assert!(g.check(ip).is_err());
        // 其他 IP 不受影响
        let other: IpAddr = "198.51.100.7".parse().unwrap();
        assert!(g.check(other).is_ok());
        // 成功登录清零
        g.success(ip);
        assert!(g.check(ip).is_ok());
    }
}
