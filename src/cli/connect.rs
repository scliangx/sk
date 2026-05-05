//! connect 命令：SSH 登录到服务器（原生 SSH 协议实现）
//!
//! 使用 ssh2 (libssh2) 实现标准 SSH 协议连接，不依赖外部 ssh 二进制。
//! 支持三种认证方式：密钥 / 存储密码 / 交互式密码

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::app::orchestrator::Orchestrator;
use crate::cli::args::OutputFormat;
use crate::domain::password::store::PasswordManager;
use crate::error::{SkError, SkResult};
use crate::ui::interactive::Interactive;
use crate::ui::output::{Output, OutputMode};

/// 执行连接（支持 server name 和 user@host[:port] 格式）
pub fn run(
    target: Option<&str>,
    output_format: OutputFormat,
    verbose: bool,
) -> SkResult<()> {
    let output = Output::new(OutputMode::from(output_format), verbose);

    let target = match target {
        Some(t) => t.to_string(),
        None => {
            // 无参数 → 交互式选择服务器
            let servers = Orchestrator::list_servers(None)?;
            if servers.is_empty() {
                output.warn("No servers configured. Use 'sk add' to add a server.");
                return Ok(());
            }
            let items: Vec<String> = servers.iter().map(|(s, status)| {
                format!("{}  ({}@{}:{}) [{}]", s.name, s.user, s.host, s.port, status.description())
            }).collect();
            match Interactive::select_item("Select a server to connect:", &items) {
                Some(idx) => servers[idx].0.name.clone(),
                None => { output.info("Cancelled."); return Ok(()); }
            }
        }
    };

    // 解析 target：可能是 server name 或 user@host[:port]
    let server = resolve_target(&target, &output)?;

    // 获取密码
    let password = if server.password_stored {
        match PasswordManager::new().get(&server.name) {
            Ok(secret) => Some(secret),
            Err(e) => {
                output.error(&format!("Stored password unavailable: {}", e));
                output.info("The encrypted password could not be decrypted — possibly moved from another machine.");
                // 降级到交互式输入
                Interactive::read_password(&format!("Password for {}@{}:", server.user, server.host))
            }
        }
    } else if server.identity_file.is_none() {
        Interactive::read_password(&format!("Password for {}@{}:", server.user, server.host))
    } else {
        None
    };

    output.info(&format!("Connecting to {}...", server.connection_string()));

    // 原生 SSH 协议连接
    native_ssh_session(&server, password.as_ref())?;

    // 记录连接
    if let Ok(mut meta) = crate::domain::config::metadata::MetadataManager::load_default() {
        meta.record_connection(&server.name);
        let _ = meta.save();
    }

    Ok(())
}

/// 解析连接目标：优先查找已配置的服务器，否则按 user@host[:port] 解析
fn resolve_target(
    target: &str,
    output: &Output,
) -> SkResult<crate::domain::config::model::Server> {
    // 1. 尝试作为服务器名称查找
    if let Ok(Some(server)) = Orchestrator::get_server(target) {
        return Ok(server);
    }

    // 2. 尝试解析 user@host[:port] 格式
    if target.contains('@') {
        let (user, host_part) = target.split_once('@').unwrap();
        let (host, port) = if let Some((h, p)) = host_part.split_once(':') {
            (h.to_string(), p.parse::<u16>().unwrap_or(22))
        } else {
            (host_part.to_string(), 22)
        };

        if user.is_empty() || host.is_empty() {
            return Err(SkError::InvalidArgument(
                "Invalid format. Use: sk <name> or sk <user@host>".into(),
            ));
        }

        output.info(&format!("Connecting to {}@{}:{}...", user, host, port));
        return Ok(crate::domain::config::model::Server::new(
            format!("adhoc-{}", target),
            host,
            user.to_string(),
        )
        .with_port(port));
    }

    // 3. 无法解析
    Err(SkError::Config(format!(
        "Server '{}' not found. Use 'sk list' to see configured servers, or 'sk <user@host>' for ad-hoc connection.",
        target
    )))
}

/// 原生 SSH 协议会话（ssh2 + libssh2）
fn native_ssh_session(
    server: &crate::domain::config::model::Server,
    password: Option<&crate::domain::config::model::SecretString>,
) -> SkResult<()> {
    // 1. TCP 连接
    let addr = format!("{}:{}", server.host, server.port);
    let tcp = TcpStream::connect_timeout(
        &addr.parse().map_err(|e| SkError::Network(format!("Bad address: {}", e)))?,
        Duration::from_secs(10),
    )
    .map_err(|e| SkError::Network(format!("Cannot connect to {}: {}", addr, e)))?;

    // 2. SSH handshake
    let mut session =
        ssh2::Session::new().map_err(|e| SkError::Internal(format!("SSH error: {}", e)))?;
    session.set_tcp_stream(tcp);
    session
        .handshake()
        .map_err(|e| SkError::Network(format!("SSH handshake failed: {}", e)))?;

    // 3. 认证
    auth_session(&mut session, server, password)?;

    // 4. 打开 channel + PTY + shell
    let mut channel = session
        .channel_session()
        .map_err(|e| SkError::Internal(format!("Channel error: {}", e)))?;

    channel
        .request_pty("xterm-256color", None, Some((80, 24, 0, 0)))
        .map_err(|e| SkError::Internal(format!("PTY error: {}", e)))?;

    channel
        .shell()
        .map_err(|e| SkError::Internal(format!("Shell error: {}", e)))?;

    session.set_blocking(false);

    // 5. 终端原始模式 + I/O 转发
    let _raw = RawTerminal::enable();

    run_interactive_session(&mut channel)?;

    let _ = channel.send_eof();
    let _ = channel.wait_close();
    Ok(())
}

/// 交互式会话 I/O 转发（使用 crossterm 事件驱动输入，避免 stdin 缓冲问题）
fn run_interactive_session(channel: &mut ssh2::Channel) -> SkResult<()> {
    use crossterm::event::{self, Event, KeyEventKind};
    let mut stdout = io::stdout();
    let mut channel_buf = [0u8; 8192];

    loop {
        // channel → stdout
        match channel.read(&mut channel_buf) {
            Ok(0) => {
                if channel.eof() { break; }
            }
            Ok(n) => {
                let _ = stdout.write_all(&channel_buf[..n]);
                let _ = stdout.flush();
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }

        // keyboard event → channel（crossterm 事件驱动，无缓冲）
        if event::poll(Duration::from_millis(5)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Release { continue; }

                let mut send = [0u8; 16];
                let n = key_event_to_bytes(&key, &mut send);
                if n > 0 {
                    match channel.write_all(&send[..n]) {
                        Ok(()) => {}
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                        Err(_) => break,
                    }
                }
            }
        }

        if channel.eof() { break; }
    }

    Ok(())
}

/// 将 crossterm KeyEvent 转换为终端字节序列
fn key_event_to_bytes(key: &crossterm::event::KeyEvent, buf: &mut [u8]) -> usize {
    use crossterm::event::{KeyCode, KeyModifiers};

    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && ('a'..='z').contains(&c) {
                buf[0] = c as u8 - b'a' + 1;
                1
            } else {
                let s = c.encode_utf8(buf);
                s.len()
            }
        }
        KeyCode::Enter => { buf[0] = b'\r'; 1 }
        KeyCode::Backspace => { buf[0] = 0x7f; 1 }
        KeyCode::Tab => { buf[0] = b'\t'; 1 }
        KeyCode::Esc => { buf[0] = 0x1b; 1 }
        KeyCode::Up => { buf[..3].copy_from_slice(b"\x1b[A"); 3 }
        KeyCode::Down => { buf[..3].copy_from_slice(b"\x1b[B"); 3 }
        KeyCode::Right => { buf[..3].copy_from_slice(b"\x1b[C"); 3 }
        KeyCode::Left => { buf[..3].copy_from_slice(b"\x1b[D"); 3 }
        KeyCode::Delete => { buf[..4].copy_from_slice(b"\x1b[3~"); 4 }
        KeyCode::Home => { buf[..3].copy_from_slice(b"\x1b[H"); 3 }
        KeyCode::End => { buf[..3].copy_from_slice(b"\x1b[F"); 3 }
        KeyCode::F(n) if n <= 12 => {
            let seq = match n {
                1 => b"\x1bOP", 2 => b"\x1bOQ", 3 => b"\x1bOR", 4 => b"\x1bOS",
                _ => return 0,
            };
            let len = seq.len();
            buf[..len].copy_from_slice(seq);
            len
        }
        _ => 0,
    }
}

/// Keyboard-interactive 回调
struct SimplePrompt {
    password: String,
}

impl ssh2::KeyboardInteractivePrompt for SimplePrompt {
    fn prompt(&mut self, _username: &str, _instructions: &str, _prompts: &[ssh2::Prompt]) -> Vec<String> {
        vec![self.password.clone()]
    }
}

/// SSH 认证
fn auth_session(
    session: &mut ssh2::Session,
    server: &crate::domain::config::model::Server,
    password: Option<&crate::domain::config::model::SecretString>,
) -> SkResult<()> {
    // 1. 密钥认证
    if let Some(ref identity_file) = server.identity_file {
        let key_path =
            crate::domain::config::parser::expand_tilde(&identity_file.to_string_lossy());
        if std::path::Path::new(&key_path).exists() {
            if session
                .userauth_pubkey_file(&server.user, None, std::path::Path::new(&key_path), None)
                .is_ok()
                && session.authenticated()
            {
                return Ok(());
            }
        }
    }

    // 2. 密码认证
    if let Some(secret) = password {
        let pass = secret.as_str();
        // 先尝试 password 方式
        match session.userauth_password(&server.user, pass) {
            Ok(()) if session.authenticated() => return Ok(()),
            Ok(_) => {}
            Err(e) => {
                // password auth failed, try keyboard-interactive
                let mut prompt = SimplePrompt { password: pass.to_string() };
                match session.userauth_keyboard_interactive(&server.user, &mut prompt) {
                    Ok(()) if session.authenticated() => return Ok(()),
                    Ok(_) => {}
                    Err(e2) => {
                        return Err(SkError::Auth(format!(
                            "Password auth: {}, Keyboard-interactive: {}",
                            e, e2
                        )));
                    }
                }
            }
        }
    }

    // 3. ssh-agent
    if session.userauth_agent(&server.user).is_ok() && session.authenticated() {
        return Ok(());
    }

    Err(SkError::Auth(format!(
        "Authentication failed for '{}'@'{}'",
        server.user, server.host
    )))
}

// ============================================================================
// 终端原始模式（跨平台）
// ============================================================================

struct RawTerminal;

impl RawTerminal {
    fn enable() -> Self {
        let _ = crossterm::terminal::enable_raw_mode();
        Self
    }
}

impl Drop for RawTerminal {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_nonexistent() {
        // 不存在的服务器且非 user@host 格式 → 返回错误
        assert!(run(Some("noexist-xyz"), OutputFormat::Text, false).is_err());
    }

    #[test]
    fn test_connect_user_at_host_parse() {
        // user@host 格式能解析，TCP 不可达直接返回 Error
        let result = run(Some("test@192.0.2.1:22"), OutputFormat::Text, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_connect_no_args() {
        assert!(run(None, OutputFormat::Text, false).is_ok());
    }
}
