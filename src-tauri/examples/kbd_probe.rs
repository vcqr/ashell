//! 诊断用探针：复现 ashell 的认证链（公钥 → keyboard-interactive），
//! 在收到 keyboard-interactive InfoRequest 时打印 name/instructions/prompts
//! 后直接断开，不发送任何应答（不消耗验证码尝试、不产生失败计数）。
//!
//! 用法：cargo run --example kbd_probe -- <addr> <port> <user> <key_path>

use std::sync::Arc;

use russh::client::{self, Handler, Session as ClientSession};
use russh::keys::{decode_secret_key, PrivateKeyWithHashAlg};

struct ProbeHandler;

impl Handler for ProbeHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // 诊断探针：信任服务器密钥
        Ok(true)
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut ClientSession,
    ) -> Result<(), Self::Error> {
        println!("[banner] {banner:?}");
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let addr = args.next().expect("addr");
    let port: u16 = args.next().expect("port").parse().expect("port num");
    let user = args.next().expect("user");
    let key_path = args.next().expect("key_path");

    let config = Arc::new(client::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(15)),
        ..Default::default()
    });

    let mut handle = client::connect(config, (addr.as_str(), port), ProbeHandler)
        .await
        .expect("connect");

    let key_str = std::fs::read_to_string(&key_path).expect("read key");
    let key = decode_secret_key(&key_str, None).expect("decode key");
    // 与主链路一致：RSA 按 server-sig-algs 选 rsa-sha2-*，避免落到 SHA-1 的
    // ssh-rsa（OpenSSH ≥ 8.8 默认禁用）；非 RSA 忽略该参数
    let hash_alg = if key.algorithm().is_rsa() {
        handle
            .best_supported_rsa_hash()
            .await
            .unwrap_or_default()
            .flatten()
    } else {
        None
    };
    let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg);

    let res = handle
        .authenticate_publickey(&user, key)
        .await
        .expect("pubkey");
    match &res {
        russh::client::AuthResult::Success => println!("[pubkey] SUCCESS"),
        russh::client::AuthResult::Failure {
            remaining_methods,
            partial_success,
        } => println!(
            "[pubkey] FAILURE partial_success={partial_success} remaining={remaining_methods:?}"
        ),
    }
    if res.success() {
        return;
    }

    println!("[kbd] start...");
    match handle
        .authenticate_keyboard_interactive_start(&user, None::<String>)
        .await
    {
        Ok(russh::client::KeyboardInteractiveAuthResponse::InfoRequest {
            name,
            instructions,
            prompts,
        }) => {
            println!("[kbd] InfoRequest name={name:?}");
            println!("[kbd] instructions={instructions:?}");
            for p in &prompts {
                println!("[kbd] prompt={:?} echo={}", p.prompt, p.echo);
            }
            println!("[kbd] 到此为止：不应答，直接断开");
        }
        Ok(other) => println!("[kbd] unexpected: {other:?}"),
        Err(e) => println!("[kbd] error: {e}"),
    }
    let _ = handle.disconnect(russh::Disconnect::ByApplication, "probe done", "en").await;
}
