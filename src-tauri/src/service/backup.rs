use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use s3::creds::Credentials;
use s3::{Bucket, Region};
use sha2::Sha256;
use sqlx::{Column, Row, TypeInfo};

use crate::config;
use crate::errors::{AppError, AppResult};
use crate::models::DbPool;

const MANIFEST_FILE: &str = "manifest.json";
const PBKDF2_ITERATIONS: u32 = 100_000;
const SALT_LEN: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub path_prefix: String,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            bucket: String::new(),
            region: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            path_prefix: "ashell/".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct BackupFile {
    version: u32,
    timestamp: String,
    app_version: String,
    data: BackupData,
}

#[derive(Serialize, Deserialize)]
struct BackupData {
    groups: Vec<serde_json::Value>,
    hosts: Vec<serde_json::Value>,
    ai_providers: Vec<serde_json::Value>,
    ai_engines: Vec<serde_json::Value>,
    app_settings: Vec<serde_json::Value>,
    quick_phrases: Vec<serde_json::Value>,
    command_templates: Vec<serde_json::Value>,
    command_history: Vec<String>,
}

#[derive(Serialize)]
pub struct BackupItem {
    pub key: String,
    pub timestamp: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    backups: Vec<ManifestEntry>,
}

#[derive(Serialize, Deserialize)]
struct ManifestEntry {
    key: String,
    timestamp: String,
    size: u64,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            backups: Vec::new(),
        }
    }
}

// ── Password-based encryption ──

fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);
    key
}

/// Encrypt plaintext with a user-provided password.
/// Output is a JSON string: { v, salt, nonce, data }
fn encrypt_with_password(plaintext: &str, password: &str) -> AppResult<String> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let key = derive_key(password, &salt);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| AppError::Crypto(format!("aes encrypt: {e}")))?;

    Ok(serde_json::json!({
        "v": 1,
        "salt": B64.encode(&salt),
        "nonce": B64.encode(&nonce_bytes),
        "data": B64.encode(&ciphertext),
    })
    .to_string())
}

/// Decrypt a password-encrypted JSON string back to plaintext.
fn decrypt_with_password(encrypted: &str, password: &str) -> AppResult<String> {
    let wrapper: serde_json::Value = serde_json::from_str(encrypted)
        .map_err(|e| AppError::Internal(format!("parse encrypted wrapper: {e}")))?;

    let salt = B64
        .decode(wrapper.get("salt").and_then(|v| v.as_str()).unwrap_or(""))
        .map_err(|e| AppError::Internal(format!("decode salt: {e}")))?;
    let nonce_bytes = B64
        .decode(
            wrapper
                .get("nonce")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        )
        .map_err(|e| AppError::Internal(format!("decode nonce: {e}")))?;
    let ciphertext = B64
        .decode(
            wrapper
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        )
        .map_err(|e| AppError::Internal(format!("decode ciphertext: {e}")))?;

    let key = derive_key(password, &salt);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| AppError::Crypto(format!("aes decrypt (wrong password?): {e}")))?;

    String::from_utf8(plaintext).map_err(|e| AppError::Internal(format!("utf8: {e}")))
}

// ── Credential decrypt/encrypt ──

/// Decrypt credential fields in backup data using the local crypto key.
fn decrypt_credentials(data: &mut BackupData, crypto_key: &[u8; 32]) -> AppResult<()> {
    for host in &mut data.hosts {
        if let Some(obj) = host.as_object_mut() {
            decrypt_field(obj, "password", crypto_key)?;
            decrypt_field(obj, "private_key", crypto_key)?;
        }
    }
    for provider in &mut data.ai_providers {
        if let Some(obj) = provider.as_object_mut() {
            decrypt_field(obj, "api_key", crypto_key)?;
        }
    }
    Ok(())
}

/// Encrypt credential fields in backup data using the local crypto key.
fn encrypt_credentials(data: &mut BackupData, crypto_key: &[u8; 32]) -> AppResult<()> {
    for host in &mut data.hosts {
        if let Some(obj) = host.as_object_mut() {
            encrypt_field(obj, "password", crypto_key);
            encrypt_field(obj, "private_key", crypto_key);
        }
    }
    for provider in &mut data.ai_providers {
        if let Some(obj) = provider.as_object_mut() {
            encrypt_field(obj, "api_key", crypto_key);
        }
    }
    Ok(())
}

fn decrypt_field(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    crypto_key: &[u8; 32],
) -> AppResult<()> {
    if let Some(serde_json::Value::String(s)) = obj.get(field) {
        if !s.is_empty() {
            let decrypted = config::crypto::decrypt(crypto_key, s)?;
            obj.insert(field.to_string(), serde_json::Value::String(decrypted));
        }
    }
    Ok(())
}

fn encrypt_field(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    crypto_key: &[u8; 32],
) {
    if let Some(serde_json::Value::String(s)) = obj.get(field).cloned() {
        if !s.is_empty() {
            if let Ok(encrypted) = config::crypto::encrypt(crypto_key, &s) {
                obj.insert(field.to_string(), serde_json::Value::String(encrypted));
            }
        }
    }
}

// ── Config persistence ──

fn config_path() -> AppResult<PathBuf> {
    Ok(config::app_dir()?.join("backup.json"))
}

pub fn load_config() -> AppResult<BackupConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(BackupConfig::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    let cfg: BackupConfig = serde_json::from_str(&raw)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(cfg)
}

pub fn save_config(cfg: &BackupConfig) -> AppResult<()> {
    let path = config_path()?;
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    std::fs::write(&path, json)?;
    Ok(())
}

// ── S3 helpers ──

fn create_bucket(cfg: &BackupConfig) -> AppResult<Box<Bucket>> {
    let endpoint = if cfg.endpoint.is_empty() {
        "https://s3.amazonaws.com".to_string()
    } else {
        cfg.endpoint.clone()
    };
    let region_str = if cfg.region.is_empty() {
        "us-east-1".to_string()
    } else {
        cfg.region.clone()
    };
    let region = Region::Custom {
        region: region_str,
        endpoint,
    };
    let creds = Credentials::new(
        Some(&cfg.access_key),
        Some(&cfg.secret_key),
        None,
        None,
        None,
    )
    .map_err(|e| AppError::Internal(format!("s3 credentials: {e}")))?;
    let bucket = Bucket::new(&cfg.bucket, region, creds)
        .map_err(|e| AppError::Internal(format!("s3 bucket: {e}")))?
        .with_path_style();
    Ok(bucket)
}

fn manifest_key(cfg: &BackupConfig) -> String {
    format!("{}{}", cfg.path_prefix, MANIFEST_FILE)
}

async fn load_manifest(bucket: &Bucket, cfg: &BackupConfig) -> AppResult<Manifest> {
    let key = manifest_key(cfg);
    let response = bucket
        .get_object(&key)
        .await
        .map_err(|e| AppError::Internal(format!("s3 get_object: {e}")))?;

    if response.status_code() == 404 || response.status_code() == 400 {
        return Ok(Manifest::default());
    }
    if response.status_code() >= 300 {
        return Err(AppError::Internal(format!(
            "s3 get_object status: {}",
            response.status_code()
        )));
    }

    serde_json::from_slice(response.as_slice())
        .map_err(|e| AppError::Internal(format!("parse manifest: {e}")))
}

async fn save_manifest(
    bucket: &Bucket,
    cfg: &BackupConfig,
    manifest: &Manifest,
) -> AppResult<()> {
    let key = manifest_key(cfg);
    let json = serde_json::to_vec(manifest)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let response = bucket
        .put_object(&key, json.as_slice())
        .await
        .map_err(|e| AppError::Internal(format!("s3 put_object: {e}")))?;

    if response.status_code() >= 300 {
        return Err(AppError::Internal(format!(
            "s3 put_object status: {}",
            response.status_code()
        )));
    }
    Ok(())
}

pub async fn test_connection(cfg: &BackupConfig) -> AppResult<()> {
    if cfg.bucket.is_empty() || cfg.access_key.is_empty() {
        return Err(AppError::Internal("bucket and access key are required".into()));
    }
    let bucket = create_bucket(cfg)?;
    let key = manifest_key(cfg);
    let response = bucket
        .get_object(&key)
        .await
        .map_err(|e| AppError::Internal(format!("s3 get_object: {e}")))?;

    match response.status_code() {
        200 | 404 => Ok(()),
        code => Err(AppError::Internal(format!(
            "s3 test_connection status: {}",
            code
        ))),
    }
}

// ── Table dump/restore ──

async fn dump_table(pool: &DbPool, table: &str) -> AppResult<Vec<serde_json::Value>> {
    let sql = format!("SELECT * FROM {}", table);
    let rows = sqlx::query(&sql).fetch_all(pool).await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let mut obj = serde_json::Map::new();
        for col in row.columns() {
            let name = col.name();
            let type_name = col.type_info().name();
            let val = match type_name {
                "TEXT" => {
                    let v: Option<String> = row.try_get(name).unwrap_or(None);
                    v.map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null)
                }
                "INTEGER" => {
                    let v: Option<i64> = row.try_get(name).unwrap_or(None);
                    v.map(|n| serde_json::Value::Number(n.into()))
                        .unwrap_or(serde_json::Value::Null)
                }
                "REAL" => {
                    let v: Option<f64> = row.try_get(name).unwrap_or(None);
                    v.and_then(|n| {
                        serde_json::Number::from_f64(n).map(serde_json::Value::Number)
                    })
                    .unwrap_or(serde_json::Value::Null)
                }
                _ => {
                    if let Ok(v) = row.try_get::<Option<String>, _>(name) {
                        v.map(serde_json::Value::String)
                            .unwrap_or(serde_json::Value::Null)
                    } else if let Ok(v) = row.try_get::<Option<i64>, _>(name) {
                        v.map(|n| serde_json::Value::Number(n.into()))
                            .unwrap_or(serde_json::Value::Null)
                    } else {
                        serde_json::Value::Null
                    }
                }
            };
            obj.insert(name.to_string(), val);
        }
        result.push(serde_json::Value::Object(obj));
    }
    Ok(result)
}

async fn build_backup_data(
    pool: &DbPool,
    command_history: Vec<String>,
) -> AppResult<BackupData> {
    Ok(BackupData {
        groups: dump_table(pool, "groups").await?,
        hosts: dump_table(pool, "hosts").await?,
        ai_providers: dump_table(pool, "ai_providers").await?,
        ai_engines: dump_table(pool, "ai_engines").await?,
        app_settings: dump_table(pool, "app_settings").await?,
        quick_phrases: dump_table(pool, "quick_phrases").await?,
        command_templates: dump_table(pool, "command_templates").await?,
        command_history,
    })
}

async fn restore_table_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    table: &str,
    rows: &[serde_json::Value],
) -> AppResult<()> {
    sqlx::query(&format!("DELETE FROM {}", table))
        .execute(&mut **tx)
        .await?;

    for row in rows {
        let obj = row
            .as_object()
            .ok_or_else(|| AppError::Internal("backup row is not an object".into()))?;

        let columns: Vec<String> = obj.keys().cloned().collect();
        let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            columns.join(", "),
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&sql);
        for col in &columns {
            let val = &obj[col];
            match val {
                serde_json::Value::String(s) => {
                    query = query.bind(s);
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        query = query.bind(i);
                    } else if let Some(f) = n.as_f64() {
                        query = query.bind(f);
                    } else {
                        query = query.bind(None::<i64>);
                    }
                }
                serde_json::Value::Bool(b) => {
                    query = query.bind(*b as i64);
                }
                serde_json::Value::Null => {
                    query = query.bind(None::<String>);
                }
                _ => {
                    query = query.bind(val.to_string());
                }
            }
        }
        query.execute(&mut **tx).await?;
    }

    Ok(())
}

// ── Public API ──

pub async fn create_backup(
    pool: &DbPool,
    crypto_key: &[u8; 32],
    cfg: &BackupConfig,
    command_history: Vec<String>,
    password: String,
) -> AppResult<String> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H%M%SZ").to_string();

    let mut data = build_backup_data(pool, command_history).await?;
    decrypt_credentials(&mut data, crypto_key)?;

    let backup = BackupFile {
        version: 1,
        timestamp: timestamp.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        data,
    };

    let json = serde_json::to_string_pretty(&backup)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let encrypted = encrypt_with_password(&json, &password)?;
    let key = format!("{}ashell-backup-{}.json", cfg.path_prefix, timestamp);
    let size = encrypted.len() as u64;

    let bucket = create_bucket(cfg)?;
    let response = bucket
        .put_object(&key, encrypted.as_bytes())
        .await
        .map_err(|e| AppError::Internal(format!("s3 put_object: {e}")))?;

    if response.status_code() >= 300 {
        return Err(AppError::Internal(format!(
            "s3 put_object status: {}",
            response.status_code()
        )));
    }

    let mut manifest = load_manifest(&bucket, cfg).await?;
    manifest.backups.push(ManifestEntry {
        key: key.clone(),
        timestamp: timestamp.clone(),
        size,
    });
    manifest
        .backups
        .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    save_manifest(&bucket, cfg, &manifest).await?;

    Ok(key)
}

pub async fn export_backup(
    pool: &DbPool,
    crypto_key: &[u8; 32],
    command_history: Vec<String>,
    password: String,
) -> AppResult<String> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H%M%SZ").to_string();

    let mut data = build_backup_data(pool, command_history).await?;
    decrypt_credentials(&mut data, crypto_key)?;

    let backup = BackupFile {
        version: 1,
        timestamp,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        data,
    };

    let json = serde_json::to_string_pretty(&backup)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    encrypt_with_password(&json, &password)
}

pub async fn list_backups(cfg: &BackupConfig) -> AppResult<Vec<BackupItem>> {
    if cfg.bucket.is_empty() || cfg.access_key.is_empty() {
        return Ok(Vec::new());
    }
    let bucket = create_bucket(cfg)?;
    let manifest = load_manifest(&bucket, cfg).await?;

    let mut items: Vec<BackupItem> = manifest
        .backups
        .into_iter()
        .map(|e| BackupItem {
            key: e.key,
            timestamp: e.timestamp,
            size: e.size,
        })
        .collect();

    items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(items)
}

pub async fn delete_backup(cfg: &BackupConfig, key: &str) -> AppResult<()> {
    let bucket = create_bucket(cfg)?;

    // Best-effort delete from S3 — some services/credentials return 403 on delete.
    // The manifest is the source of truth for the backup list, so we update it regardless.
    if let Err(e) = bucket.delete_object(key).await {
        eprintln!("[backup] s3 delete_object warning (non-fatal): {e}");
    }

    // Remove from manifest
    let mut manifest = load_manifest(&bucket, cfg).await?;
    manifest.backups.retain(|e| e.key != key);
    save_manifest(&bucket, cfg, &manifest).await?;

    Ok(())
}

pub async fn restore_backup(
    pool: &DbPool,
    crypto_key: &[u8; 32],
    cfg: &BackupConfig,
    key: &str,
    password: &str,
) -> AppResult<Vec<String>> {
    let bucket = create_bucket(cfg)?;
    let response = bucket
        .get_object(key)
        .await
        .map_err(|e| AppError::Internal(format!("s3 get_object: {e}")))?;

    if response.status_code() >= 300 {
        return Err(AppError::Internal(format!(
            "s3 get_object status: {}",
            response.status_code()
        )));
    }

    let encrypted = String::from_utf8(response.as_slice().to_vec())
        .map_err(|e| AppError::Internal(format!("utf8: {e}")))?;
    let json = decrypt_with_password(&encrypted, password)?;

    let mut backup: BackupFile = serde_json::from_str(&json)
        .map_err(|e| AppError::Internal(format!("parse backup json: {e}")))?;

    encrypt_credentials(&mut backup.data, crypto_key)?;

    let data = backup.data;
    let mut tx = pool.begin().await?;

    restore_table_tx(&mut tx, "groups", &data.groups).await?;
    restore_table_tx(&mut tx, "hosts", &data.hosts).await?;
    restore_table_tx(&mut tx, "ai_providers", &data.ai_providers).await?;
    restore_table_tx(&mut tx, "ai_engines", &data.ai_engines).await?;
    restore_table_tx(&mut tx, "app_settings", &data.app_settings).await?;
    restore_table_tx(&mut tx, "quick_phrases", &data.quick_phrases).await?;
    restore_table_tx(&mut tx, "command_templates", &data.command_templates).await?;

    tx.commit().await?;

    Ok(data.command_history)
}

pub async fn import_backup(
    pool: &DbPool,
    crypto_key: &[u8; 32],
    content: &str,
    password: &str,
) -> AppResult<Vec<String>> {
    let json = decrypt_with_password(content, password)?;

    let mut backup: BackupFile = serde_json::from_str(&json)
        .map_err(|e| AppError::Internal(format!("parse backup json: {e}")))?;

    encrypt_credentials(&mut backup.data, crypto_key)?;

    let data = backup.data;
    let mut tx = pool.begin().await?;

    restore_table_tx(&mut tx, "groups", &data.groups).await?;
    restore_table_tx(&mut tx, "hosts", &data.hosts).await?;
    restore_table_tx(&mut tx, "ai_providers", &data.ai_providers).await?;
    restore_table_tx(&mut tx, "ai_engines", &data.ai_engines).await?;
    restore_table_tx(&mut tx, "app_settings", &data.app_settings).await?;
    restore_table_tx(&mut tx, "quick_phrases", &data.quick_phrases).await?;
    restore_table_tx(&mut tx, "command_templates", &data.command_templates).await?;

    tx.commit().await?;

    Ok(data.command_history)
}
