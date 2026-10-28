//! 本地假 2FA SSH 服务端：keyboard-interactive 强制 TOTP（固定验证码 123456）。
//!
//! 用于端到端复现 ashell 终端的 2FA 连接流程：密码/公钥一律拒绝，仅开放
//! keyboard-interactive；InfoRequest 提示 "Verification code(TOTP): "（不回显），
//! 应答 123456 即认证成功。认证后提供最小 shell：欢迎语 + 提示符 + 回显；
//! exec 输出固定的 SHELL 探测结果（bash），sftp subsystem 返回失败。
//!
//! 用法：cargo run --example fake_2fa_server -- [port]（默认 2299）

use std::collections::HashMap;
use std::sync::Arc;

use russh::keys::PrivateKey;
use russh::server::{Auth, Msg, Response, Server as _, Session};
use russh::ChannelId;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const TOTP_CODE: &str = "123456";
/// SHELL_PROBE（echo "$SHELL"; readlink -f /bin/sh）的固定输出
const SHELL_PROBE_OUTPUT: &str = "/bin/bash\n/bin/bash\n";

fn kbd_only() -> Auth {
    Auth::Reject {
        proceed_with_methods: Some(russh::MethodSet::from(
            &[russh::MethodKind::KeyboardInteractive][..],
        )),
        partial_success: false,
    }
}

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2299);

    let config = Arc::new(russh::server::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
        auth_rejection_time: std::time::Duration::from_secs(0),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        keys: vec![PrivateKey::random(
            &mut rand::rng(),
            russh::keys::Algorithm::Ed25519,
        )
        .unwrap()],
        ..Default::default()
    });

    let socket = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
    log::info!("fake 2FA ssh server listening on 127.0.0.1:{port} (code={TOTP_CODE})");

    let mut sh = ServerState::default();
    let server = sh.run_on_socket(config, &socket);
    server.await.unwrap();
}

#[derive(Default, Clone)]
struct ServerState {
    id: usize,
    #[allow(dead_code)]
    clients: Arc<Mutex<HashMap<usize, (ChannelId, russh::server::Handle)>>>,
}

impl russh::server::Server for ServerState {
    type Handler = FakeServer;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self::Handler {
        FakeServer
    }
}

struct FakeServer;

impl russh::server::Handler for FakeServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _: &str, _: &str) -> Result<Auth, Self::Error> {
        log::info!("auth_password -> reject (kbd-interactive only)");
        Ok(kbd_only())
    }

    async fn auth_publickey(
        &mut self,
        _: &str,
        _: &russh::keys::ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        log::info!("auth_publickey -> reject (kbd-interactive only)");
        Ok(kbd_only())
    }

    async fn auth_none(&mut self, _: &str) -> Result<Auth, Self::Error> {
        Ok(kbd_only())
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        user: &str,
        _: &str,
        response: Option<Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        match response {
            None => {
                log::info!("kbd InfoRequest for {user}: prompt=Verification code(TOTP)");
                Ok(Auth::Partial {
                    name: "pam_google_authenticator".into(),
                    instructions: "Enter the TOTP code".into(),
                    prompts: std::borrow::Cow::Owned(vec![(
                        std::borrow::Cow::Borrowed("Verification code(TOTP): "),
                        false,
                    )]),
                })
            }
            Some(answers) => {
                let given: Vec<String> = answers
                    .map(|a| String::from_utf8_lossy(&a).to_string())
                    .collect();
                log::info!("kbd responses: {given:?}");
                if given.iter().any(|a| a.trim() == TOTP_CODE) {
                    log::info!("kbd auth ACCEPT for {user}");
                    Ok(Auth::Accept)
                } else {
                    log::info!("kbd auth reject for {user}");
                    Ok(kbd_only())
                }
            }
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: russh::Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        log::info!("channel_open_session #{}", channel.id());
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        col: u32,
        row: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        log::info!("pty_request #{channel} term={term} {col}x{row}");
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        log::info!("shell_request #{channel} -> welcome + prompt");
        session.channel_success(channel)?;
        session.data(channel, "Welcome to fake 2FA server\r\nuser@fake:~$ ")?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        log::info!("exec_request #{channel}: {}", String::from_utf8_lossy(data));
        session.channel_success(channel)?;
        session.data(channel, SHELL_PROBE_OUTPUT)?;
        session.eof(channel)?;
        session.exit_status_request(channel, 0)?;
        session.close(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        log::info!("subsystem_request #{channel}: {name} -> failure");
        session.channel_failure(channel)
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col: u32,
        row: u32,
        _: u32,
        _: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        log::info!("window_change #{channel} {col}x{row}");
        session.channel_success(channel)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // 简单回显 shell：原样返回用户输入
        session.data(channel, data.to_vec())?;
        Ok(())
    }
}
