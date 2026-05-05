//! 公钥推送到远程服务器（ssh2 原生协议实现）
//!
//! 通过 SSH 协议连接远程服务器，执行远程命令将公钥追加到 authorized_keys。
//! 支持密码认证和 ssh-agent 认证，不依赖外部 ssh 二进制。

use std::io::Read;
use std::net::TcpStream;

use crate::domain::config::model::{SecretString, Server};
use crate::error::{SkError, SkResult};

/// SSH 公钥推送器
pub struct KeyPusher;

impl KeyPusher {
    /// 推送公钥到远程服务器
    ///
    /// 使用 ssh2 原生协议执行远程命令：
    /// `mkdir -p ~/.ssh && chmod 700 ~/.ssh && echo '<key>' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys`
    ///
    /// # 认证方式（按优先级）
    /// 1. 密码认证（如果提供了 password）
    /// 2. ssh-agent 认证
    pub fn push(server: &Server, public_key: &str, password: Option<&SecretString>) -> SkResult<()> {
        let addr = format!("{}:{}", server.host, server.port);
        let tcp = TcpStream::connect(&addr).map_err(|e| {
            SkError::Network(format!("Cannot connect to {}: {}", addr, e))
        })?;

        let mut session = ssh2::Session::new()
            .map_err(|e| SkError::Internal(format!("SSH session error: {}", e)))?;

        session.set_tcp_stream(tcp);
        session
            .handshake()
            .map_err(|e| SkError::Network(format!("SSH handshake failed: {}", e)))?;

        // 认证
        Self::authenticate(&mut session, server, password)?;

        // 执行远程命令
        let commands = format!(
            "mkdir -p ~/.ssh && chmod 700 ~/.ssh && echo '{}' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys",
            public_key
        );

        let mut channel = session
            .channel_session()
            .map_err(|e| SkError::Internal(format!("Channel open failed: {}", e)))?;

        channel
            .exec(&commands)
            .map_err(|e| SkError::Network(format!("Remote command failed: {}", e)))?;

        let mut output = String::new();
        channel
            .read_to_string(&mut output)
            .map_err(|e| SkError::Internal(format!("Read remote output failed: {}", e)))?;

        channel
            .wait_close()
            .map_err(|e| SkError::Network(format!("Remote command error: {}", e)))?;

        Ok(())
    }

    /// SSH 认证
    fn authenticate(
        session: &mut ssh2::Session,
        server: &Server,
        password: Option<&SecretString>,
    ) -> SkResult<()> {
        // 1. 密码认证
        if let Some(secret) = password {
            if session.userauth_password(&server.user, secret.as_str()).is_ok()
                && session.authenticated()
            {
                return Ok(());
            }
        }

        // 2. ssh-agent
        if session.userauth_agent(&server.user).is_ok() && session.authenticated() {
            return Ok(());
        }

        Err(SkError::Auth(format!(
            "Authentication failed for '{}'@'{}'. Provide a password or ensure ssh-agent is running.",
            server.user, server.host
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_without_server() {
        let server = Server::new("t".into(), "127.0.0.1".into(), "nobody".into()).with_port(19999);
        let result = KeyPusher::push(&server, "ssh-ed25519 AAAAtest key", None);
        assert!(result.is_err());
    }
}
