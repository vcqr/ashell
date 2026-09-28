//! 登录失败限流（暴力破解防护）。
//!
//! 单用户令牌模型下，`--token` / `ASHELL_WEB_TOKEN` 允许自设弱口令，
//! `0.0.0.0` 部署若无限制可被在线穷举。策略：
//!
//! - 按来源 IP 记录窗口（15 分钟）内连续失败次数，达到上限触发锁定
//! - 锁定时长从 30s 起指数翻倍（封顶 30 分钟），持续攻击无法维持穷举速率
//! - 环回地址豁免锁定（桌面端本机调用与本地自测），仍记录失败日志
//!
//! 注意：经反代/Docker userland proxy 部署时所有连接呈现为代理 IP，
//! 此时退化为全局限流——合法用户仍可在锁定间隔后重试。

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// 窗口内允许的最大连续失败次数
const FAIL_LIMIT: u32 = 5;
/// 失败计数窗口
const FAIL_WINDOW: Duration = Duration::from_secs(15 * 60);
/// 首次锁定时长，之后每次翻倍
const LOCKOUT_BASE: Duration = Duration::from_secs(30);
/// 锁定时长上限
const LOCKOUT_MAX: Duration = Duration::from_secs(30 * 60);
/// 全表清理阈值：超过该条目数时顺手清理过期记录，防止无界增长
const SWEEP_THRESHOLD: usize = 4096;

struct Entry {
    /// 当前窗口内的失败次数
    count: u32,
    /// 窗口起点
    window_start: Instant,
    /// 锁定截止时刻；None = 未锁定
    locked_until: Option<Instant>,
    /// 已触发的锁定次数（决定下次锁定时长指数）
    lockout_level: u32,
}

impl Entry {
    fn new(now: Instant) -> Self {
        Self {
            count: 0,
            window_start: now,
            locked_until: None,
            lockout_level: 0,
        }
    }

    /// 窗口过期则重开新窗口
    fn ensure_window(&mut self, now: Instant) {
        if now.duration_since(self.window_start) >= FAIL_WINDOW {
            self.count = 0;
            self.window_start = now;
        }
    }

    /// 记录一次失败；触发锁定时返回锁定时长
    fn on_failure(&mut self, now: Instant) -> Option<Duration> {
        self.ensure_window(now);
        self.count += 1;
        if self.count < FAIL_LIMIT {
            return None;
        }
        // 指数翻倍：30s、60s、120s ... 封顶 LOCKOUT_MAX
        let level = self.lockout_level.saturating_add(1);
        let secs = LOCKOUT_BASE
            .as_secs()
            .saturating_mul(1u64 << (level - 1).min(16));
        let lockout = Duration::from_secs(secs).min(LOCKOUT_MAX);
        self.lockout_level = level;
        self.locked_until = Some(now.checked_add(lockout)?);
        self.count = 0;
        self.window_start = now;
        Some(lockout)
    }

    /// 命中锁定时返回剩余等待时长
    fn locked_remaining(&self, now: Instant) -> Option<Duration> {
        self.locked_until
            .and_then(|until| until.checked_duration_since(now))
            .filter(|d| !d.is_zero())
    }

    fn expired(&self, now: Instant) -> bool {
        self.locked_remaining(now).is_none()
            && now.duration_since(self.window_start) >= FAIL_WINDOW
    }
}

static REGISTRY: LazyLock<Mutex<HashMap<IpAddr, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn with_registry<T>(f: impl FnOnce(&mut HashMap<IpAddr, Entry>) -> T) -> T {
    let mut map = REGISTRY.lock().expect("login registry poisoned");
    let out = f(&mut map);
    if map.len() > SWEEP_THRESHOLD {
        let now = Instant::now();
        map.retain(|_, e| !e.expired(now));
    }
    out
}

/// 环回地址豁免锁定（桌面端本机调用与本地自测）
fn is_loopback(ip: IpAddr) -> bool {
    ip.is_loopback()
}

/// 该 IP 当前是否处于锁定中；返回剩余等待时长
pub fn check_locked(ip: IpAddr) -> Option<Duration> {
    if is_loopback(ip) {
        return None;
    }
    let now = Instant::now();
    with_registry(|map| map.get(&ip).and_then(|e| e.locked_remaining(now)))
}

/// 记录一次登录失败；触发锁定时返回锁定时长（供日志/响应提示）
pub fn record_failure(ip: IpAddr) -> Option<Duration> {
    let now = Instant::now();
    with_registry(|map| {
        let entry = map.entry(ip).or_insert_with(|| Entry::new(now));
        let lockout = if is_loopback(ip) { None } else { entry.on_failure(now) };
        if lockout.is_none() && entry.count == 0 && entry.locked_until.is_none() {
            // 未触发锁定且窗口内无计数：孤儿条目（环回豁免），避免堆积
            if entry.lockout_level == 0 {
                map.remove(&ip);
            }
        }
        lockout
    })
}

/// 登录成功，清除该 IP 的失败记录
pub fn record_success(ip: IpAddr) {
    with_registry(|map| {
        map.remove(&ip);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_below_limit_do_not_lock() {
        let mut e = Entry::new(Instant::now());
        for _ in 0..FAIL_LIMIT - 1 {
            assert!(e.on_failure(Instant::now()).is_none());
        }
        assert!(e.locked_remaining(Instant::now()).is_none());
    }

    #[test]
    fn hitting_limit_locks_with_base_duration() {
        let mut e = Entry::new(Instant::now());
        for _ in 0..FAIL_LIMIT - 1 {
            e.on_failure(Instant::now());
        }
        let lockout = e.on_failure(Instant::now()).expect("should lock");
        assert_eq!(lockout, LOCKOUT_BASE);
        let remaining = e.locked_remaining(Instant::now()).expect("locked now");
        assert!(remaining <= LOCKOUT_BASE && remaining > Duration::ZERO);
    }

    #[test]
    fn lockout_expires_and_next_lock_doubles() {
        let mut e = Entry::new(Instant::now());
        for _ in 0..FAIL_LIMIT {
            e.on_failure(Instant::now());
        }
        // 锁定期间视角：仍处于锁定（锁定窗口内不重置锁定）
        assert!(e.locked_remaining(Instant::now()).is_some());
        // 锁定过期后允许尝试
        let after = Instant::now() + LOCKOUT_BASE + Duration::from_secs(1);
        assert!(e.locked_remaining(after).is_none());
        // 再次达到上限：锁定时长翻倍
        let mut second = None;
        for _ in 0..FAIL_LIMIT {
            second = e.on_failure(after);
        }
        assert_eq!(second, Some(LOCKOUT_BASE * 2));
    }

    #[test]
    fn window_reset_clears_stale_failures() {
        let mut e = Entry::new(Instant::now());
        for _ in 0..FAIL_LIMIT - 1 {
            e.on_failure(Instant::now());
        }
        // 窗口过期后再失败：计数从 0 开始，不触发锁定
        let later = Instant::now() + FAIL_WINDOW + Duration::from_secs(1);
        assert!(e.on_failure(later).is_none());
    }

    #[test]
    fn record_failure_and_success_roundtrip() {
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        for _ in 0..FAIL_LIMIT {
            record_failure(ip);
        }
        assert!(check_locked(ip).is_some());
        record_success(ip);
        assert!(check_locked(ip).is_none());
    }

    #[test]
    fn loopback_is_exempt_from_lockout() {
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        for _ in 0..FAIL_LIMIT * 2 {
            record_failure(ip);
        }
        assert!(check_locked(ip).is_none());
    }
}
