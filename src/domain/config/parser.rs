//! SSH Config 解析器
//!
//! 解析 ~/.ssh/config 文件，提取 Host 块中的配置信息。
//!
//! 支持的指令：
//! - Host: 块起始标记
//! - HostName: 目标主机名/IP
//! - User: 登录用户名
//! - Port: SSH 端口
//! - IdentityFile: 密钥文件路径
//! - StrictHostKeyChecking: 识别但不处理
//!
//! 不支持的指令（跳过并记录警告）：
//! - Match, Include, ProxyJump, ProxyCommand 等复杂指令

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::domain::config::model::Server;
use crate::error::{SkError, SkResult};
use crate::infra::fs;

/// 解析过程中产生的警告信息
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParseWarning {
    pub line: usize,
    pub directive: String,
    pub message: String,
}

/// SSH Config 解析器
///
/// 负责将 `~/.ssh/config` 文本内容解析为结构化的 `Server` 列表。
#[derive(Debug)]
pub struct SshConfigParser {
    /// SSH config 文件路径
    path: PathBuf,
}

impl SshConfigParser {
    /// 使用默认路径（~/.ssh/config）创建解析器
    #[allow(dead_code)]
    pub fn default_path() -> SkResult<Self> {
        Ok(Self {
            path: fs::ssh_config_path()?,
        })
    }

    /// 使用指定路径创建解析器（主要用于测试）
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 从字符串内容解析所有 Host 块
    ///
    /// 用于测试场景，直接接收配置内容字符串。
    pub fn parse_str(&self, content: &str) -> (Vec<Server>, Vec<ParseWarning>) {
        let mut servers = Vec::new();
        let mut warnings = Vec::new();
        let mut current_host: Option<HostBuilder> = None;

        for (line_idx, raw_line) in content.lines().enumerate() {
            let line_num = line_idx + 1;
            let trimmed = raw_line.trim();

            // 跳过空行和注释行
            if trimmed.is_empty() || trimmed.starts_with('#') {
                // 如果当前有待提交的 Host 块，提交它
                if let Some(host) = current_host.take() {
                    if let Some(server) = host.build() {
                        servers.push(server);
                    }
                }
                continue;
            }

            // 将一行拆分为指令和参数
            let (keyword, value) = match split_directive(trimmed) {
                Some(kv) => kv,
                None => {
                    warnings.push(ParseWarning {
                        line: line_num,
                        directive: trimmed.to_string(),
                        message: "Unrecognized line format".to_string(),
                    });
                    continue;
                }
            };

            // 处理 Host 关键字（开始新的块）
            if keyword.eq_ignore_ascii_case("host") {
                // 如果有上一个块，先提交
                if let Some(host) = current_host.take() {
                    if let Some(server) = host.build() {
                        servers.push(server);
                    }
                }

                // 跳过通配符 Host 模式
                if value.contains('*') || value.contains('?') {
                    warnings.push(ParseWarning {
                        line: line_num,
                        directive: format!("Host {}", value),
                        message: "Wildcard Host patterns not yet supported, skipped".to_string(),
                    });
                    current_host = None;
                    continue;
                }

                // 多 Host 名（空格分隔），取第一个作为名称
                let first_name = value.split_whitespace().next().unwrap_or(value);
                current_host = Some(HostBuilder::new(first_name.to_string(), line_num));
                continue;
            }

            // 处理其他指令
            if let Some(ref mut host) = current_host {
                match keyword.to_lowercase().as_str() {
                    "hostname" => host.hostname = Some(value.to_string()),
                    "user" => host.user = Some(value.to_string()),
                    "port" => {
                        match value.parse::<u16>() {
                            Ok(p) if p > 0 => host.port = Some(p),
                            _ => {
                                warnings.push(ParseWarning {
                                    line: line_num,
                                    directive: format!("Port {}", value),
                                    message: "Invalid port number, ignored".to_string(),
                                });
                            }
                        }
                    }
                    "identityfile" => {
                        // 展开 ~ 为用户 HOME 目录
                        let expanded = expand_tilde(value);
                        host.identity_file = Some(PathBuf::from(expanded));
                    }
                    "stricthostkeychecking" => {
                        // 识别但不处理
                        host.attributes
                            .insert("StrictHostKeyChecking".to_string(), value.to_string());
                    }
                    _ => {
                        // 不支持的关键字，记录警告
                        warnings.push(ParseWarning {
                            line: line_num,
                            directive: format!("{} {}", keyword, value),
                            message: format!("Unsupported keyword '{}', skipped", keyword),
                        });
                    }
                }
            }
            // 如果在块外部遇到指令，忽略
        }

        // 文件末尾提交最后一个 Host 块
        if let Some(host) = current_host.take() {
            if let Some(server) = host.build() {
                servers.push(server);
            }
        }

        (servers, warnings)
    }

    /// 解析文件，返回 Server 列表
    pub fn parse(&self) -> SkResult<Vec<Server>> {
        let content = match fs::read_file(&self.path) {
            Ok(c) => c,
            Err(SkError::Config(_)) => {
                // 文件不存在，返回空列表
                return Ok(Vec::new());
            }
            Err(e) => return Err(e),
        };

        let (servers, _warnings) = self.parse_str(&content);
        // TODO: 后续版本通过 verbose 标志输出 warnings
        Ok(servers)
    }

    /// 查找指定名称的 Host 块
    pub fn find_host(&self, name: &str) -> SkResult<Option<Server>> {
        let servers = self.parse()?;
        Ok(servers.into_iter().find(|s| s.name == name))
    }

    /// 获取解析器关联的文件路径
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Host 块构建器（解析过程的中间状态）
#[derive(Debug)]
struct HostBuilder {
    name: String,
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<PathBuf>,
    attributes: HashMap<String, String>,
}

impl HostBuilder {
    fn new(name: String, _start_line: usize) -> Self {
        Self {
            name,
            hostname: None,
            user: None,
            port: None,
            identity_file: None,
            attributes: HashMap::new(),
        }
    }

    /// 构建 Server 实例
    ///
    /// 只有当 HostName 和 User 都存在时才返回 Some，
    /// 否则认为该 Host 块不完整。
    fn build(self) -> Option<Server> {
        let hostname = self.hostname?;
        let user = self.user?;

        let mut server = Server::new(self.name, hostname, user);
        server.port = self.port.unwrap_or(22);
        server.identity_file = self.identity_file;

        Some(server)
    }
}

/// 将一行 SSH config 指令拆分为关键字和值
///
/// 支持格式：
/// - "Keyword value"
/// - "Keyword=value"
fn split_directive(line: &str) -> Option<(&str, &str)> {
    // 尝试按第一个空白字符拆分
    if let Some(pos) = line.find(char::is_whitespace) {
        let keyword = &line[..pos];
        let value = line[pos..].trim();
        if !keyword.is_empty() {
            return Some((keyword, value));
        }
    }

    // 尝试按 = 拆分
    if let Some(pos) = line.find('=') {
        let keyword = &line[..pos];
        let value = line[pos + 1..].trim();
        if !keyword.is_empty() {
            return Some((keyword, value));
        }
    }

    None
}

/// 展开路径中的 ~ 为用户 HOME 目录
pub fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy().to_string();
            return path.replacen('~', &home_str, 1);
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SSH config 样例 1: 基本配置
    const BASIC_CONFIG: &str = r#"
Host myserver
    HostName 192.168.1.100
    User root
    Port 22

Host webserver
    HostName example.com
    User admin
    Port 2222
    IdentityFile ~/.ssh/webserver_key
"#;

    /// SSH config 样例 2: 包含通配符和注释
    const ADVANCED_CONFIG: &str = r#"
# 默认配置
Host *.example.com
    User default

# 生产服务器
Host prod
    HostName 10.0.0.1
    User admin
    Port 22
    IdentityFile ~/.ssh/prod_key
    StrictHostKeyChecking accept-new

# 开发服务器
Host dev
    HostName dev.example.com
    User developer
    Port 2222
"#;

    /// SSH config 样例 3: 包含不支持的关键字
    const COMPLEX_CONFIG: &str = r#"
Host gateway
    HostName jump.example.com
    User jumpuser
    Port 22
    ProxyJump none
    Match host *.internal
        User internal
"#;

    /// SSH config 样例 4: 多 Host 名
    const MULTI_HOST_CONFIG: &str = "# 多 Host 名配置\nHost server1 server2 server3\n    HostName 10.0.0.1\n    User admin\n";

    /// SSH config 样例 5: 空内容和仅注释
    const EMPTY_CONFIG: &str = "# Nothing here yet\n\n# Just comments\n";

    // ---- 解析测试 ----

    #[test]
    fn test_parse_basic_config() {
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, warnings) = parser.parse_str(BASIC_CONFIG);

        assert_eq!(servers.len(), 2);
        assert!(warnings.is_empty());

        let myserver = &servers[0];
        assert_eq!(myserver.name, "myserver");
        assert_eq!(myserver.host, "192.168.1.100");
        assert_eq!(myserver.user, "root");
        assert_eq!(myserver.port, 22);

        let webserver = &servers[1];
        assert_eq!(webserver.name, "webserver");
        assert_eq!(webserver.host, "example.com");
        assert_eq!(webserver.user, "admin");
        assert_eq!(webserver.port, 2222);
        assert!(webserver.identity_file.is_some());
    }

    #[test]
    fn test_parse_skips_wildcard_host() {
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, warnings) = parser.parse_str(ADVANCED_CONFIG);

        // 应该只解析 prod 和 dev，跳过 *.example.com
        assert_eq!(servers.len(), 2);
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"prod"));
        assert!(names.contains(&"dev"));
        assert!(!names.contains(&"*.example.com"));

        // 通配符应该产生警告
        assert!(warnings.iter().any(|w| w.directive.contains("*.example.com")));
    }

    #[test]
    fn test_parse_complex_config() {
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, warnings) = parser.parse_str(COMPLEX_CONFIG);

        // gateway 应该被解析
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "gateway");

        // ProxyJump 和 Match 会产生警告
        assert!(warnings.iter().any(|w| w.directive.contains("ProxyJump")));
        assert!(warnings.iter().any(|w| w.directive.contains("Match")));
    }

    #[test]
    fn test_parse_multi_host_names() {
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, _warnings) = parser.parse_str(MULTI_HOST_CONFIG);

        // 应该只取第一个名称
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "server1");
    }

    #[test]
    fn test_parse_empty_config() {
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, _warnings) = parser.parse_str(EMPTY_CONFIG);
        assert!(servers.is_empty());
    }

    #[test]
    fn test_parse_incomplete_host_block() {
        // Host 块缺少 HostName 不应生成 Server
        let config = "Host incomplete\n    User test\n";
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, _warnings) = parser.parse_str(config);
        assert!(servers.is_empty());
    }

    #[test]
    fn test_parse_default_port() {
        // 不指定 Port 时默认为 22
        let config = "Host server\n    HostName 1.2.3.4\n    User admin\n";
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, _warnings) = parser.parse_str(config);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].port, 22);
    }

    #[test]
    fn test_parse_invalid_port() {
        // 无效端口号应被忽略
        let config = "Host server\n    HostName 1.2.3.4\n    User admin\n    Port abc\n";
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, warnings) = parser.parse_str(config);
        assert_eq!(servers.len(), 1);
        assert!(warnings.iter().any(|w| w.directive.contains("Port")));
    }

    #[test]
    fn test_parse_port_zero() {
        // Port 0 无效
        let config = "Host server\n    HostName 1.2.3.4\n    User admin\n    Port 0\n";
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, warnings) = parser.parse_str(config);
        assert_eq!(servers.len(), 1);
        assert!(warnings.iter().any(|w| w.directive.contains("Port 0")));
    }

    #[test]
    fn test_split_directive_space() {
        let result = split_directive("HostName example.com");
        assert!(result.is_some());
        let (k, v) = result.unwrap();
        assert_eq!(k, "HostName");
        assert_eq!(v, "example.com");
    }

    #[test]
    fn test_split_directive_equals() {
        let result = split_directive("Port=2222");
        assert!(result.is_some());
        let (k, v) = result.unwrap();
        assert_eq!(k, "Port");
        assert_eq!(v, "2222");
    }

    #[test]
    fn test_split_directive_invalid() {
        assert!(split_directive("").is_none());
    }

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/.ssh/key");
        assert!(!expanded.starts_with('~'));
        assert!(expanded.ends_with(".ssh/key"));

        let no_tilde = expand_tilde("/absolute/path");
        assert_eq!(no_tilde, "/absolute/path");
    }

    #[test]
    fn test_parse_str_with_comments_in_block() {
        // 块内的注释应该结束当前块
        let config = "Host s1\n    HostName 1.1.1.1\n    User u1\n# comment\nHost s2\n    HostName 2.2.2.2\n    User u2\n";
        let parser = SshConfigParser::new(PathBuf::from("dummy"));
        let (servers, _) = parser.parse_str(config);
        assert_eq!(servers.len(), 2);
    }

    // ---- 文件解析集成测试 ----

    #[test]
    fn test_parse_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("config");

        let content = "Host test-server\n    HostName 10.0.0.1\n    User admin\n    Port 2222\n";
        std::fs::write(&config_path, content).unwrap();

        let parser = SshConfigParser::new(config_path);
        let servers = parser.parse().unwrap();

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "test-server");
        assert_eq!(servers[0].host, "10.0.0.1");
        assert_eq!(servers[0].port, 2222);
    }

    #[test]
    fn test_parse_file_not_found() {
        let parser = SshConfigParser::new(PathBuf::from("/nonexistent/ssh/config"));
        let servers = parser.parse().unwrap();
        assert!(servers.is_empty());
    }

    #[test]
    fn test_find_host() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("config");

        let content = "Host alpha\n    HostName 1.1.1.1\n    User a\nHost beta\n    HostName 2.2.2.2\n    User b\n";
        std::fs::write(&config_path, content).unwrap();

        let parser = SshConfigParser::new(config_path);
        let found = parser.find_host("beta").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().host, "2.2.2.2");

        let not_found = parser.find_host("gamma").unwrap();
        assert!(not_found.is_none());
    }
}
