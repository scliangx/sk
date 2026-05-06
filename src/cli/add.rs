//! add 命令：添加 SSH 服务器配置
//!
//! 添加前必须验证连接可达性。支持两种认证方式：
//! - 密码认证：sk add <name> -H <host> -u <user> -p <password>
//! - 密钥认证：sk add <name> -H <host> -u <user> -i <keyfile>
//! - 生成密钥：sk add <name> -H <host> -u <user> -k  (交互式输入密码后推送)

use colored::Colorize;
use std::net::TcpStream;
use std::time::Duration;

use crate::app::orchestrator::Orchestrator;
use crate::app::transaction::Transaction;
use crate::cli::args::OutputFormat;
use crate::domain::config::model::SecretString;
use crate::domain::key::generator::KeyGenerator;
use crate::domain::password::store::PasswordManager;
use crate::domain::ssh::push::KeyPusher;
use crate::error::{SkError, SkResult};
use crate::ui::interactive::Interactive;
use crate::ui::output::{Output, OutputMode};
use crate::ui::progress;

/// 连接验证结果
#[derive(Debug)]
enum VerifyResult {
    /// 连接成功
    Ok,
    /// 连接失败（含错误信息）
    Failed(String),
}

pub fn run(
    name: &str, host: &str, user: &str, port: u16,
    password: Option<&str>, identity_file: Option<&str>,
    with_key: bool, force: bool,
    output_format: OutputFormat, verbose: bool,
) -> SkResult<()> {
    let output = Output::new(OutputMode::from(output_format), verbose);

    // === 参数校验 ===
    if name.trim().is_empty() {
        return Err(SkError::InvalidArgument("Server name cannot be empty".into()));
    }
    if name.contains(char::is_whitespace) {
        return Err(SkError::InvalidArgument("Server name cannot contain spaces".into()));
    }
    if host.trim().is_empty() {
        return Err(SkError::InvalidArgument("Host address cannot be empty".into()));
    }
    if user.trim().is_empty() {
        return Err(SkError::InvalidArgument("Username cannot be empty".into()));
    }
    if port == 0 {
        return Err(SkError::InvalidArgument("Port must be 1-65535".into()));
    }

    // === 获取密码 / 密钥 ===
    let password_secret = if with_key {
        output.info("Setting up password-less key-based login...");
        Interactive::read_password(&format!("Enter password for {}@{}:", user, host))
    } else if let Some(pass) = password {
        if !output.is_json() {
            eprintln!("⚠ Warning: Password on command line is visible in shell history.");
        }
        Some(SecretString::new(pass.to_string()))
    } else if identity_file.is_none() {
        // 没有密码也没有密钥 → 交互式询问密码
        output.info("No credentials provided. Enter password (or Ctrl+C to skip):");
        Interactive::read_password(&format!("Password for {}@{} (optional):", user, host))
    } else {
        None
    };

    // === 连接验证 ===
    if !with_key {
        // -k 模式跳过验证（密钥还没生成）
        let spinner = progress::create_spinner(&format!(
            "Verifying connection to {}@{}:{}...",
            user, host, port
        ));

        let verify = verify_connectivity(host, port, user, password_secret.as_ref(), identity_file);

        match verify {
            VerifyResult::Ok => {
                spinner.finish_with_message(format!(
                    "✅ Connection to {}@{}:{} verified.",
                    user, host, port
                ));
            }
            VerifyResult::Failed(err) => {
                spinner.finish_with_message(format!(
                    "❌ Connection failed: {}", err
                ));
                output.error(&format!(
                    "Cannot connect to {}@{}:{} — {}", user, host, port, err
                ));

                if !force && !output.is_json() {
                    if !Interactive::confirm("Connection failed. Add this server anyway?") {
                        output.info("Add cancelled.");
                        return Ok(());
                    }
                    output.warn("Adding server despite connection failure.");
                } else if force {
                    output.warn("Adding server despite connection failure (force mode).");
                } else {
                    // JSON 模式下直接拒绝
                    return Err(SkError::Network(format!(
                        "Connection to {}:{} failed: {}", host, port, err
                    )));
                }
            }
        }
    }

    // === 生成密钥（-k 模式） ===
    let mut tx = Transaction::begin();
    let key_path = if with_key {
        let spinner = progress::create_spinner("Generating ED25519 key pair...");
        let key_pair = KeyGenerator::generate_and_save(name)?;
        spinner.finish_and_clear();
        let kp = format!("~/.sk/keys/{}_key", name);
        output.info(&format!("Key generated: {}", kp));

        let name_owned = name.to_string();
        tx.on_rollback(move || { let _ = KeyGenerator::delete_keys(&name_owned); Ok(()) });

        // 推送公钥（ssh2 协议 + 密码认证）
        if let Some(ref secret) = password_secret {
            let spinner = progress::create_spinner("Pushing public key to server...");
            let srv = crate::domain::config::model::Server::new(
                name.to_string(), host.to_string(), user.to_string(),
            ).with_port(port);

            match KeyPusher::push(&srv, &key_pair.authorized_key, Some(secret)) {
                Ok(()) => {
                    spinner.finish_with_message("✅ Public key pushed.");
                    output.info("Key installed on remote server.");
                }
                Err(e) => {
                    spinner.finish_with_message("❌ Key push failed!");
                    output.warn(&format!("Key push failed: {}. Falling back to password mode.", e));
                }
            }
        } else {
            output.info("No password — skipping key push.");
        }
        Some(kp)
    } else {
        identity_file.map(|f| f.to_string())
    };

    // === 写入 SSH config ===
    output.info(&format!("Saving server '{}'...", name));
    let server = Orchestrator::add_server(name, host, user, port, key_path.as_deref(), force)?;

    let name_owned = name.to_string();
    tx.on_rollback(move || {
        let w = crate::domain::config::writer::SshConfigWriter::default_path()?;
        let _ = w.remove(&name_owned);
        Ok(())
    });

    // === 存储密码 ===
    if let Some(ref secret) = password_secret {
        if !secret.is_empty() {
            let pm = PasswordManager::new();
            pm.save(name, secret)?;
            let name_owned = name.to_string();
            tx.on_rollback(move || { let _ = PasswordManager::new().delete(&name_owned); Ok(()) });

            let mut meta = crate::domain::config::metadata::MetadataManager::load_default()?;
            meta.upsert_server(name, true, pm.backend_name());
            meta.save()?;
        }
    }

    tx.commit();

    // === 输出 ===
    let has_credential = password_secret.is_some() || key_path.is_some();
    output.add_success(&server, has_credential);

    if !output.is_json() {
        if with_key {
            println!("  {}  {}", "sk".bold(), name);
            println!("  {}  Key-based login.", "→".dimmed());
        } else if password_secret.is_some() {
            println!("  {}  {}", "sk".bold(), name);
            println!("  {}  Auto-login with stored password.", "→".dimmed());
        } else if key_path.is_some() {
            println!("  {}  {}", "sk".bold(), name);
            println!("  {}  Key-based login.", "→".dimmed());
        }
    }

    Ok(())
}

/// 验证连接：TCP + SSH 握手 + 认证
///
/// 优先使用密码认证，其次密钥认证。
fn verify_connectivity(
    host: &str, port: u16, user: &str,
    password: Option<&SecretString>,
    identity_file: Option<&str>,
) -> VerifyResult {
    let addr = format!("{}:{}", host, port);

    // TCP 层
    let tcp = match TcpStream::connect_timeout(
        &addr.parse().unwrap(),
        Duration::from_secs(5),
    ) {
        Ok(t) => t,
        Err(e) => return VerifyResult::Failed(format!("TCP connect failed: {}", e)),
    };

    // SSH 层
    let mut session = match ssh2::Session::new() {
        Ok(s) => s,
        Err(e) => return VerifyResult::Failed(format!("SSH init failed: {}", e)),
    };

    session.set_tcp_stream(tcp);
    if let Err(e) = session.handshake() {
        return VerifyResult::Failed(format!("SSH handshake failed: {}", e));
    }

    // 认证：密码优先
    if let Some(secret) = password {
        if session.userauth_password(user, secret.as_str()).is_ok() && session.authenticated() {
            return VerifyResult::Ok;
        }
        return VerifyResult::Failed("Password authentication failed".into());
    }

    // 密钥认证
    if let Some(key_path) = identity_file {
        let expanded = crate::domain::config::parser::expand_tilde(key_path);
        let p = std::path::Path::new(&expanded);
        if p.exists() {
            if session.userauth_pubkey_file(user, None, p, None).is_ok() && session.authenticated() {
                return VerifyResult::Ok;
            }
            return VerifyResult::Failed("Key authentication failed".into());
        }
        return VerifyResult::Failed(format!("Key file not found: {}", key_path));
    }

    // 尝试 agent
    if session.userauth_agent(user).is_ok() && session.authenticated() {
        return VerifyResult::Ok;
    }

    // 无凭据但 SSH 可达
    VerifyResult::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(name: &str, host: &str, user: &str) -> SkResult<()> {
        run(name, host, user, 22, None, None, false, false, OutputFormat::Text, false)
    }

    #[test]
    fn test_add_empty_name() {
        assert!(run_test("", "h", "u").is_err());
    }

    #[test]
    fn test_add_whitespace_name() {
        assert!(run_test("a b", "h", "u").is_err());
    }

    #[test]
    fn test_add_empty_host() {
        assert!(run_test("n", "", "u").is_err());
    }

    #[test]
    fn test_add_empty_user() {
        assert!(run_test("n", "h", "").is_err());
    }

    #[test]
    fn test_verify_unreachable() {
        let r = verify_connectivity("192.0.2.1", 22, "root", None, None);
        assert!(matches!(r, VerifyResult::Failed(_)));
    }

    #[test]
    fn test_verify_refused() {
        let r = verify_connectivity("127.0.0.1", 19999, "nobody", None, None);
        assert!(matches!(r, VerifyResult::Failed(_)));
    }
}
