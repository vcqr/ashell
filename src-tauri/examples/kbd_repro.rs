//! 复现 ashell 2FA keyboard-interactive 序列,定位 RecvError:
//! 阶段1 = 弹窗前连接(密码自动应答,遇验证码中止并丢弃连接)
//! 阶段2 = 重连带完整重放(ashell 弹窗收集后的实际行为)
//!
//! 用法:RUST_LOG=russh=debug cargo run --example kbd_repro -- <addr> <port> <user> <password> <totp_code>

use std::sync::Arc;

use russh::client::{
    self, Handler, KeyboardInteractiveAuthResponse,
};

struct H;

impl Handler for H {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

async fn connect(addr: &str, port: u16) -> client::Handle<H> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(120)),
        ..Default::default()
    });
    client::connect(config, (addr, port), H)
        .await
        .expect("connect")
}

fn is_password(p: &str) -> bool {
    let t = p.to_lowercase();
    t.contains("password") || t.contains("passwd")
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let addr = args.next().expect("addr");
    let port: u16 = args.next().expect("port").parse().expect("port num");
    let user = args.next().expect("user");
    let pass = args.next().expect("password");
    let code = args.next().expect("totp code");

    // ===== 阶段1:弹窗前的连接 =====
    let mut h1 = connect(&addr, port).await;
    let r = h1
        .authenticate_password(&user, &pass)
        .await
        .expect("p1 password");
    println!("[p1] password success={}", r.success());
    let mut resp = h1
        .authenticate_keyboard_interactive_start(&user, None::<String>)
        .await
        .expect("p1 kbd start");
    loop {
        match resp {
            KeyboardInteractiveAuthResponse::Success => {
                println!("[p1] SUCCESS");
                break;
            }
            KeyboardInteractiveAuthResponse::Failure {
                remaining_methods, ..
            } => {
                println!("[p1] FAILURE remaining={remaining_methods:?}");
                break;
            }
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let ps: Vec<_> = prompts
                    .iter()
                    .map(|p| (p.prompt.clone(), p.echo))
                    .collect();
                println!("[p1] InfoRequest {ps:?}");
                if prompts.iter().any(|p| !is_password(&p.prompt)) {
                    println!("[p1] 存在无法就地应答的提示,丢弃连接(模拟弹窗中止)");
                    break;
                }
                let answers: Vec<String> = prompts
                    .iter()
                    .map(|p| if is_password(&p.prompt) { pass.clone() } else { String::new() })
                    .collect();
                resp = h1
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .expect("p1 respond");
            }
        }
    }
    drop(h1);
    println!("[p1] 连接已丢弃\n");

    // ===== 阶段2:重连带完整重放 =====
    let mut h2 = connect(&addr, port).await;
    let r = h2
        .authenticate_password(&user, &pass)
        .await
        .expect("p2 password");
    println!("[p2] password success={}", r.success());
    let replay = vec![pass.clone(), code.clone()];
    let mut idx = 0usize;
    let mut resp = h2
        .authenticate_keyboard_interactive_start(&user, None::<String>)
        .await
        .expect("p2 kbd start");
    loop {
        match resp {
            KeyboardInteractiveAuthResponse::Success => {
                println!("[p2] SUCCESS ✅");
                break;
            }
            KeyboardInteractiveAuthResponse::Failure {
                remaining_methods,
                partial_success,
            } => {
                println!("[p2] FAILURE remaining={remaining_methods:?} partial={partial_success}");
                break;
            }
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let ps: Vec<_> = prompts
                    .iter()
                    .map(|p| (p.prompt.clone(), p.echo))
                    .collect();
                println!("[p2] InfoRequest {ps:?}");
                let answers: Vec<String> = prompts
                    .iter()
                    .map(|_| {
                        idx += 1;
                        replay.get(idx - 1).cloned().unwrap_or_default()
                    })
                    .collect();
                println!("[p2] respond with {answers:?}");
                resp = match h2
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        println!("[p2] respond 出错: {e}");
                        return;
                    }
                };
            }
        }
    }
}
