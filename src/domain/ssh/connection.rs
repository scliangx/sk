//! TCP 连接 + SSH 握手测试模块
//!
//! 分两层测试服务器连接状态：
//! 1. TCP Socket 连接（超时可配置）
//! 2. SSH 协议握手 + 认证尝试
//!
//! 支持三种认证方式（按优先级）：
//! 1. 公钥认证（使用 IdentityFile）
//! 2. 密码认证（如果存储了密码）
//! 3. 无认证（仅测试 TCP/SSH 可达性）

use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::domain::config::model::{ConnectionTestResult, Server};

/// 默认 TCP 连接超时（秒）
const DEFAULT_TCP_TIMEOUT_SECS: u64 = 10;

/// Keyboard-interactive 认证回调
struct SimplePrompt {
    password: String,
}
impl ssh2::KeyboardInteractivePrompt for SimplePrompt {
    fn prompt(&mut self, _username: &str, _instructions: &str, _prompts: &[ssh2::Prompt]) -> Vec<String> {
        vec![self.password.clone()]
    }
}

/// SSH 连接测试器
pub struct SshConnectionTester;

impl SshConnectionTester {
    /// 测试服务器连接
    ///
    /// # 参数
    /// - server: 服务器配置
    /// - timeout_secs: TCP 连接超时（秒）
    ///
    /// # 返回
    /// ConnectionTestResult 包含两层测试的结果
    pub fn test(server: &Server, timeout_secs: Option<u64>) -> ConnectionTestResult {
        let timeout = timeout_secs.unwrap_or(DEFAULT_TCP_TIMEOUT_SECS);

        // 第一层：TCP 连接测试
        let tcp_start = Instant::now();

        let addr = match Self::resolve_address(server, timeout) {
            Ok(a) => a,
            Err(e) => {
                return ConnectionTestResult::tcp_failed(format!("DNS resolution failed: {}", e));
            }
        };

        let tcp_result = Self::test_tcp_connection(&addr, timeout);
        let tcp_latency_ms = tcp_start.elapsed().as_millis() as u64;

        if let Err(ref e) = tcp_result {
            return ConnectionTestResult::tcp_failed(format!(
                "TCP connection failed ({}ms): {}",
                tcp_latency_ms, e
            ));
        }

        let mut tcp_stream = tcp_result.unwrap();

        // 第二层：SSH 握手 + 认证测试
        let ssh_start = Instant::now();

        let ssh_result = Self::test_ssh_auth(&mut tcp_stream, server, timeout);
        let total_latency_ms = ssh_start.elapsed().as_millis() as u64;

        match ssh_result {
            Ok(auth_method) => {
                ConnectionTestResult::success(tcp_latency_ms, total_latency_ms, &auth_method)
            }
            Err(e) => ConnectionTestResult::auth_failed(tcp_latency_ms, total_latency_ms, e),
        }
    }

    /// DNS 解析并获取 Socket 地址
    fn resolve_address(server: &Server, _timeout_secs: u64) -> Result<SocketAddr, String> {
        let addr_str = format!("{}:{}", server.host, server.port);
        let mut addrs = addr_str
            .to_socket_addrs()
            .map_err(|e| format!("Unable to resolve address {}: {}", addr_str, e))?;

        addrs
            .next()
            .ok_or_else(|| format!("DNS resolution returned no results: {}", addr_str))
    }

    /// TCP 连接测试
    fn test_tcp_connection(
        addr: &SocketAddr,
        timeout_secs: u64,
    ) -> Result<TcpStream, String> {
        let timeout = Duration::from_secs(timeout_secs);
        TcpStream::connect_timeout(addr, timeout)
            .map_err(|e| format!("Connection timed out or was refused: {}", e))
    }

    /// SSH 握手 + 认证测试
    ///
    /// 优先尝试公钥认证，其次密码认证，最后无认证（仅测试可达性）。
    fn test_ssh_auth(
        tcp: &mut TcpStream,
        server: &Server,
        _timeout_secs: u64,
    ) -> Result<String, String> {
        // 确保 TCP_NODELAY 设置
        let _ = tcp.set_nodelay(true);

        #[cfg(feature = "ssh2-backend")]
        {
            let mut session =
                ssh2::Session::new().map_err(|e| format!("SSH session creation failed: {}", e))?;

            session.set_tcp_stream(tcp.try_clone().unwrap());
            session
                .handshake()
                .map_err(|e| format!("SSH handshake failed: {}", e))?;

            // 尝试使用 IdentityFile 进行公钥认证
            if let Some(ref key_path) = server.identity_file {
                // 展开密钥路径
                let expanded_path = crate::domain::config::parser::expand_tilde(
                    &key_path.to_string_lossy(),
                );
                let pubkey_path = format!("{}.pub", expanded_path);

                // 检查私钥存在
                let key_exists = std::path::Path::new(&expanded_path).exists();
                let pubkey_exists = std::path::Path::new(&pubkey_path).exists();

                if key_exists && pubkey_exists {
                    match session.userauth_pubkey_file(
                        &server.user,
                        Some(&std::path::PathBuf::from(&pubkey_path)),
                        &std::path::PathBuf::from(&expanded_path),
                        None,
                    ) {
                        Ok(_) => {
                            // 公钥认证成功
                            let _ = session.disconnect(None, "sk test complete", None);
                            return Ok("publickey".to_string());
                        }
                        Err(e) => {
                            // 公钥认证失败，记录但不返回错误（后续可能尝试其他方式）
                            if !server.password_stored {
                                return Err(format!("Public key authentication failed: {}", e));
                            }
                        }
                    }
                }
            }

            // 如果公钥认证不可用且没有密码，则报告可达性
            if !server.password_stored {
                let _ = session.disconnect(None, "sk reachability test", None);
                return Ok("none (reachable)".to_string());
            }

            // 密码认证：从密码存储读取并尝试认证
            let pm = crate::domain::password::store::PasswordManager::new();
            if let Ok(secret) = pm.get(&server.name) {
                let pass = secret.as_str();
                // 先尝试 password 方式
                if session.userauth_password(&server.user, pass).is_ok()
                    && session.authenticated()
                {
                    let _ = session.disconnect(None, "sk test complete", None);
                    return Ok("password".to_string());
                }
                // 再尝试 keyboard-interactive 方式
                let mut prompt = SimplePrompt { password: pass.to_string() };
                if session
                    .userauth_keyboard_interactive(&server.user, &mut prompt)
                    .is_ok()
                    && session.authenticated()
                {
                    let _ = session.disconnect(None, "sk test complete", None);
                    return Ok("password".to_string());
                }
            }

            let _ = session.disconnect(None, "sk test complete", None);
            return Err("Password stored but authentication failed".to_string());
        }

        #[cfg(not(feature = "ssh2-backend"))]
        {
            return Err("SSH authentication test requires the ssh2-backend feature".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_server() -> Server {
        Server::new(
            "test-server".to_string(),
            "127.0.0.1".to_string(),
            "root".to_string(),
        )
    }

    #[test]
    fn test_resolve_address_localhost() {
        let server = test_server();
        let addr = SshConnectionTester::resolve_address(&server, 10);
        assert!(addr.is_ok());
        let addr = addr.unwrap();
        assert_eq!(addr.port(), 22);
    }

    #[test]
    fn test_resolve_address_custom_port() {
        let server = test_server().with_port(2222);
        let addr = SshConnectionTester::resolve_address(&server, 10);
        assert!(addr.is_ok());
        assert_eq!(addr.unwrap().port(), 2222);
    }

    #[test]
    fn test_connection_test_unreachable_host() {
        let server = Server::new(
            "unreachable".to_string(),
            "192.0.2.1".to_string(), // TEST-NET-1 (RFC 5737), should be unreachable
            "root".to_string(),
        );
        let result = SshConnectionTester::test(&server, Some(2));
        assert!(!result.is_ok());
        assert!(result.error.is_some());
    }

    #[test]
    fn test_connection_test_refused_port() {
        let server = Server::new(
            "refused".to_string(),
            "127.0.0.1".to_string(),
            "root".to_string(),
        )
        .with_port(19999); // 极不可能有服务监听此端口
        let result = SshConnectionTester::test(&server, Some(2));
        assert!(!result.tcp_ok || !result.auth_ok);
    }

    #[test]
    fn test_expand_tilde() {
        let path = crate::domain::config::parser::expand_tilde("~/.ssh/key");
        assert!(!path.starts_with('~'));
        assert!(path.ends_with(".ssh/key"));
    }
}
