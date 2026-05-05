//! SSH 配置核心数据模型
//!
//! 定义了 sk 工具中所有核心数据结构：
//! - Server: 服务器配置
//! - ServerStatus: 配置状态（用于 list 展示）
//! - ConnectionTestResult: 连接测试结果
//! - AddServerContext: 添加服务器的完整上下文

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 服务器配置（核心数据模型）
///
/// 定义一个 SSH 连接的所有配置信息，
/// 可直接映射到 ~/.ssh/config 的 Host 块。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Server {
    /// 别名，用于标识和 `ssh <name>` 连接
    pub name: String,
    /// IP 地址或域名
    pub host: String,
    /// SSH 端口，默认 22
    #[serde(default = "default_port")]
    pub port: u16,
    /// 登录用户名
    pub user: String,
    /// 密钥文件路径（相对或绝对路径）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<PathBuf>,
    /// 是否在钥匙串中存储了密码
    #[serde(default)]
    pub password_stored: bool,
    /// 添加时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Local>>,
}

fn default_port() -> u16 {
    22
}

impl Server {
    /// 创建新的 Server 实例，端口默认 22
    pub fn new(name: String, host: String, user: String) -> Self {
        Self {
            name,
            host,
            port: 22,
            user,
            identity_file: None,
            password_stored: false,
            created_at: Some(Local::now()),
        }
    }

    /// 设置自定义端口
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// 设置密钥文件路径
    #[allow(dead_code)]
    pub fn with_identity_file(mut self, path: PathBuf) -> Self {
        self.identity_file = Some(path);
        self
    }

    /// 检查端口是否有效
    #[allow(dead_code)]
    pub fn is_port_valid(&self) -> bool {
        (1..=65535).contains(&self.port)
    }

    /// 获取 SSH 连接字符串 `user@host:port`
    pub fn connection_string(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.user, self.host)
        } else {
            format!("{}@{}:{}", self.user, self.host, self.port)
        }
    }
}

/// 服务器配置状态（用于 list 命令展示）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    /// 已配置免密登录（IdentityFile 存在且密钥文件在磁盘上存在）
    KeyConfigured(PathBuf),
    /// 有密码存储但无密钥
    PasswordStored,
    /// 仅配置了连接信息，无密钥无密码
    Bare,
}

impl ServerStatus {
    /// 返回状态的图标表示
    pub fn icon(&self) -> &str {
        match self {
            Self::KeyConfigured(_) => "🔑",
            Self::PasswordStored => "🔒",
            Self::Bare => "📝",
        }
    }

    /// 返回状态的文字描述
    pub fn description(&self) -> &str {
        match self {
            Self::KeyConfigured(_) => "Key configured",
            Self::PasswordStored => "Password stored",
            Self::Bare => "Basic info only",
        }
    }
}

/// 连接测试结果
///
/// 分两层测试：TCP 层和 SSH 认证层。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    /// TCP 连接是否成功
    pub tcp_ok: bool,
    /// TCP 连接延迟（毫秒）
    pub tcp_latency_ms: u64,
    /// SSH 认证是否成功
    pub auth_ok: bool,
    /// 使用的认证方式（publickey / password / none）
    #[serde(default)]
    pub auth_method: String,
    /// 总体连接延迟（毫秒）
    pub total_latency_ms: u64,
    /// 错误信息（如果有）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ConnectionTestResult {
    /// 创建成功的测试结果
    pub fn success(tcp_latency_ms: u64, total_latency_ms: u64, auth_method: &str) -> Self {
        Self {
            tcp_ok: true,
            tcp_latency_ms,
            auth_ok: true,
            auth_method: auth_method.to_string(),
            total_latency_ms,
            error: None,
        }
    }

    /// 创建 TCP 失败的测试结果
    pub fn tcp_failed(error: String) -> Self {
        Self {
            tcp_ok: false,
            tcp_latency_ms: 0,
            auth_ok: false,
            auth_method: String::new(),
            total_latency_ms: 0,
            error: Some(error),
        }
    }

    /// 创建认证失败的测试结果
    pub fn auth_failed(tcp_latency_ms: u64, total_latency_ms: u64, error: String) -> Self {
        Self {
            tcp_ok: true,
            tcp_latency_ms,
            auth_ok: false,
            auth_method: String::new(),
            total_latency_ms,
            error: Some(error),
        }
    }

    /// 是否完全成功
    pub fn is_ok(&self) -> bool {
        self.tcp_ok && self.auth_ok
    }
}

// ============================================================================
// SecretString — 密码安全包装
// ============================================================================

/// 安全字符串包装类型
///
/// 使用 zeroize 的 Zeroizing 包装，确保内存中的敏感数据在 drop 时被自动清零。
/// - 不可 Clone（防止内存中残留多份副本）
/// - 不可 Display（防止意外打印到日志）
#[derive(Debug)]
pub struct SecretString {
    inner: zeroize::Zeroizing<String>,
}

impl SecretString {
    /// 从 String 创建 SecretString
    pub fn new(s: String) -> Self {
        Self {
            inner: zeroize::Zeroizing::new(s),
        }
    }

    /// 获取内部字符串的引用（仅在需要传递给 SSH 认证时使用）
    ///
    /// # 安全性
    /// 调用方必须确保此引用不会被打印、记录到日志、或额外克隆。
    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    /// 判断是否为空
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// SecretString 不支持 Clone、不支持 Display（安全设计）
// Drop 时 Zeroizing<String> 自动调用 zeroize() 清零内存

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Server 测试 ----

    #[test]
    fn test_server_new_default_port() {
        let s = Server::new("test".into(), "192.168.1.1".into(), "root".into());
        assert_eq!(s.name, "test");
        assert_eq!(s.host, "192.168.1.1");
        assert_eq!(s.port, 22);
        assert_eq!(s.user, "root");
        assert!(s.identity_file.is_none());
        assert!(!s.password_stored);
        assert!(s.created_at.is_some());
    }

    #[test]
    fn test_server_with_port() {
        let s = Server::new("test".into(), "example.com".into(), "admin".into())
            .with_port(2222);
        assert_eq!(s.port, 2222);
    }

    #[test]
    fn test_server_with_identity_file() {
        let key_path = PathBuf::from("/home/user/.ssh/test_key");
        let s = Server::new("test".into(), "host".into(), "user".into())
            .with_identity_file(key_path.clone());
        assert_eq!(s.identity_file, Some(key_path));
    }

    #[test]
    fn test_is_port_valid() {
        let s = Server::new("t".into(), "h".into(), "u".into());
        assert!(s.is_port_valid()); // default 22

        let s2 = s.clone().with_port(0);
        assert!(!s2.is_port_valid());

        // 65536 超出 u16 范围，编译器会阻止，无需额外测试

        let s3 = s.with_port(65535);
        assert!(s3.is_port_valid());
    }

    #[test]
    fn test_connection_string() {
        let s = Server::new("t".into(), "example.com".into(), "root".into());
        assert_eq!(s.connection_string(), "root@example.com");

        let s2 = s.with_port(2222);
        assert_eq!(s2.connection_string(), "root@example.com:2222");
    }

    #[test]
    fn test_server_serialize_deserialize() {
        let s = Server::new("prod".into(), "10.0.0.1".into(), "admin".into())
            .with_port(2222);
        let json = serde_json::to_string(&s).unwrap();
        let s2: Server = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }

    // ---- ServerStatus 测试 ----

    #[test]
    fn test_server_status_icon() {
        assert_eq!(ServerStatus::KeyConfigured(PathBuf::from("/tmp/key")).icon(), "🔑");
        assert_eq!(ServerStatus::PasswordStored.icon(), "🔒");
        assert_eq!(ServerStatus::Bare.icon(), "📝");
    }

    #[test]
    fn test_server_status_description() {
        assert_eq!(
            ServerStatus::KeyConfigured(PathBuf::from("/tmp/key")).description(),
            "Key configured"
        );
        assert_eq!(ServerStatus::PasswordStored.description(), "Password stored");
        assert_eq!(ServerStatus::Bare.description(), "Basic info only");
    }

    // ---- ConnectionTestResult 测试 ----

    #[test]
    fn test_connection_test_result_success() {
        let r = ConnectionTestResult::success(10, 150, "publickey");
        assert!(r.is_ok());
        assert_eq!(r.tcp_latency_ms, 10);
        assert_eq!(r.total_latency_ms, 150);
        assert_eq!(r.auth_method, "publickey");
    }

    #[test]
    fn test_connection_test_result_tcp_failed() {
        let r = ConnectionTestResult::tcp_failed("timeout".into());
        assert!(!r.is_ok());
        assert!(!r.tcp_ok);
        assert_eq!(r.error, Some("timeout".to_string()));
    }

    #[test]
    fn test_connection_test_result_auth_failed() {
        let r = ConnectionTestResult::auth_failed(5, 100, "bad key".into());
        assert!(!r.is_ok());
        assert!(r.tcp_ok);
        assert!(!r.auth_ok);
    }

    // ---- SecretString 测试 ----

    #[test]
    fn test_secret_string_new() {
        let s = SecretString::new("mypassword".into());
        assert!(!s.is_empty());
        assert_eq!(s.as_str(), "mypassword");
    }

    #[test]
    fn test_secret_string_empty() {
        let s = SecretString::new(String::new());
        assert!(s.is_empty());
    }
}
