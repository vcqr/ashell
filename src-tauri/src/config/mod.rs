pub mod crypto;

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// 应用配置（启动后注入到全局状态）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// API 监听地址（实际绑定后回填，包含真实端口）
    pub api_addr: String,
    /// 鉴权 Token（随启动随机生成，前端需通过 invoke 获取）
    pub token: String,
    /// 数据库文件路径
    pub db_path: PathBuf,
    /// AES-GCM 加密密钥（32 字节，存放在 ~/.ashell/secret.key）
    pub crypto_key: [u8; 32],
}

static GLOBAL_CONFIG: OnceLock<AppConfig> = OnceLock::new();

/// 返回 ~/.ashell 目录，若不存在则自动创建
pub fn app_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("无法获取用户家目录"))?;
    let dir = home.join(".ashell");
    if !dir.exists() {
        fs::create_dir_all(&dir).with_context(|| format!("创建目录失败: {:?}", dir))?;
    }
    Ok(dir)
}

/// 返回 ~/.ashell/icons 目录（用户自定义主机图标），若不存在则自动创建
pub fn icons_dir() -> Result<PathBuf> {
    let dir = app_dir()?.join("icons");
    if !dir.exists() {
        fs::create_dir_all(&dir).with_context(|| format!("创建图标目录失败: {:?}", dir))?;
    }
    Ok(dir)
}

/// 返回 ~/.ashell/ai 目录（AI sidecar 工作目录），若不存在则自动创建
pub fn ai_dir() -> Result<PathBuf> {
    let dir = app_dir()?.join("ai");
    if !dir.exists() {
        fs::create_dir_all(&dir).with_context(|| format!("创建 AI 工作目录失败: {:?}", dir))?;
    }
    Ok(dir)
}

/// 返回 ~/.ashell/wallpaper 目录（窗口背景壁纸），若不存在则自动创建
pub fn wallpaper_dir() -> Result<PathBuf> {
    let dir = app_dir()?.join("wallpaper");
    if !dir.exists() {
        fs::create_dir_all(&dir).with_context(|| format!("创建壁纸目录失败: {:?}", dir))?;
    }
    Ok(dir)
}

/// 加载或生成本机加密密钥
fn load_or_generate_crypto_key() -> Result<[u8; 32]> {
    let path = app_dir()?.join("secret.key");
    if path.exists() {
        let raw = fs::read(&path).with_context(|| format!("读取 {:?} 失败", path))?;
        if raw.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&raw);
            return Ok(key);
        }
        // 密钥文件损坏：备份旧文件后重新生成，避免已有密文永久不可恢复时至少保留原始数据
        log::error!(
            "secret.key 长度异常（{} 字节，期望 32），已备份为 secret.key.bak 并重新生成。 \
             此前加密的凭证将无法解密，如需恢复请手动还原 secret.key.bak",
            raw.len()
        );
        let bak = app_dir()?.join("secret.key.bak");
        let _ = fs::rename(&path, &bak);
    }
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    fs::write(&path, key).with_context(|| format!("写入 {:?} 失败", path))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(key)
}

/// 生成随机 Token
fn generate_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

/// 初始化配置（暂未绑定端口，绑定后再回填 api_addr）
pub fn init() -> Result<AppConfig> {
    let dir = app_dir()?;
    let db_path = dir.join("ashell.db");
    let crypto_key = load_or_generate_crypto_key()?;
    let token = generate_token();
    // 主动确保 icons 目录存在
    let _ = icons_dir()?;

    Ok(AppConfig {
        api_addr: String::new(),
        token,
        db_path,
        crypto_key,
    })
}

/// 设置全局配置（仅允许设置一次）
pub fn set_global(cfg: AppConfig) {
    let _ = GLOBAL_CONFIG.set(cfg);
}

/// 获取全局配置
#[allow(dead_code)]
pub fn global() -> Option<&'static AppConfig> {
    GLOBAL_CONFIG.get()
}
