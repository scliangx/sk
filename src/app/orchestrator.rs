//! 核心业务编排器
//!
//! 负责协调各领域模块完成用户操作。
//! 每个公开方法对应一个 CLI 命令的完整业务流程。

use crate::domain::config::metadata::MetadataManager;
use crate::domain::config::model::{Server, ServerStatus};
use crate::domain::config::parser::SshConfigParser;
use crate::domain::config::writer::SshConfigWriter;
use crate::error::{SkError, SkResult};
use std::path::PathBuf;

/// 应用编排器
///
/// 持有各个领域模块的引用，协调完成用户请求。
pub struct Orchestrator;

impl Orchestrator {
    /// 添加服务器配置
    ///
    /// # 参数
    /// - name: 服务器别名
    /// - host: IP 地址或域名
    /// - user: 登录用户名
    /// - port: SSH 端口
    /// - identity_file: 可选的密钥文件路径
    /// - force: 是否强制覆盖已有配置
    ///
    /// # 流程
    /// 1. 参数校验
    /// 2. 冲突检测（如果 force 则跳过）
    /// 3. 写入 SSH config
    /// 4. 保存元数据
    ///
    /// # 返回
    /// 创建的 Server 实例
    pub fn add_server(
        name: &str,
        host: &str,
        user: &str,
        port: u16,
        identity_file: Option<&str>,
        force: bool,
    ) -> SkResult<Server> {
        // 参数校验
        if name.is_empty() {
            return Err(SkError::InvalidArgument("Server name cannot be empty".to_string()));
        }
        if host.is_empty() {
            return Err(SkError::InvalidArgument("Host address cannot be empty".to_string()));
        }
        if user.is_empty() {
            return Err(SkError::InvalidArgument("Username cannot be empty".to_string()));
        }
        if port == 0 {
            return Err(SkError::InvalidArgument(
                "Port must be between 1 and 65535".to_string(),
            ));
        }

        // 构建 Server 实例
        let mut server = Server::new(name.to_string(), host.to_string(), user.to_string());
        server.port = port;

        if let Some(key_path) = identity_file {
            server.identity_file = Some(PathBuf::from(key_path));
        }

        // 冲突检测
        let writer = SshConfigWriter::default_path()?;
        if writer.exists(name)? && !force {
            return Err(SkError::Config(format!(
                "Server '{}' already exists. Use --force to overwrite.",
                name
            )));
        }

        // 写入 SSH config
        if writer.exists(name)? {
            writer.update(name, &server)?;
        } else {
            writer.append(&server)?;
        }

        // 保存元数据
        let mut metadata = MetadataManager::load_default()?;
        metadata.upsert_server(name, false, "none");
        metadata.save()?;

        Ok(server)
    }

    /// 删除服务器配置
    ///
    /// # 参数
    /// - name: 服务器名称
    /// - delete_keys: 是否同时删除密钥文件
    ///
    /// # 返回
    /// (是否删除成功, 是否删除了密钥文件)
    pub fn remove_server(name: &str, delete_keys: bool) -> SkResult<(bool, bool)> {
        let writer = SshConfigWriter::default_path()?;
        let config_removed = writer.remove(name)?;

        // 删除密钥文件
        let mut keys_deleted = false;
        if delete_keys {
            keys_deleted = Self::delete_key_files(name);
        }

        // 删除密码存储
        let pm = crate::domain::password::store::PasswordManager::new();
        let _ = pm.delete(name);

        // 删除元数据（无论 config 中是否存在都要清理）
        let mut metadata = MetadataManager::load_default()?;
        metadata.remove_server(name);
        metadata.save()?;

        Ok((config_removed, keys_deleted))
    }

    /// 删除指定服务器的密钥文件
    fn delete_key_files(name: &str) -> bool {
        let mut deleted = false;

        // 删除私钥
        if let Ok(private_key) = crate::infra::fs::server_key_path(name) {
            if private_key.exists() {
                if std::fs::remove_file(&private_key).is_ok() {
                    deleted = true;
                }
            }
        }

        // 删除公钥
        if let Ok(public_key) = crate::infra::fs::server_pubkey_path(name) {
            if public_key.exists() {
                let _ = std::fs::remove_file(&public_key);
                deleted = true;
            }
        }

        deleted
    }

    /// 列出所有服务器
    ///
    /// # 参数
    /// - filter: 可选的名称过滤关键词
    ///
    /// # 返回
    /// (Server, ServerStatus) 列表
    pub fn list_servers(filter: Option<&str>) -> SkResult<Vec<(Server, ServerStatus)>> {
        let parser = SshConfigParser::default_path()?;
        let servers = parser.parse()?;
        let metadata = MetadataManager::load_default()?;

        let result: Vec<(Server, ServerStatus)> = servers
            .into_iter()
            .filter(|s| {
                if let Some(f) = filter {
                    s.name.to_lowercase().contains(&f.to_lowercase())
                } else {
                    true
                }
            })
            .map(|mut s| {
                // 从元数据中更新密码存储状态
                if let Some(meta) = metadata.get_server(&s.name) {
                    s.password_stored = meta.password_stored;
                }
                let status = Self::determine_status(&s);
                (s, status)
            })
            .collect();

        Ok(result)
    }

    /// 判断服务器的配置状态
    fn determine_status(server: &Server) -> ServerStatus {
        // 检查密钥文件是否存在
        if let Some(ref key_path) = server.identity_file {
            let path = if key_path.is_absolute() {
                key_path.clone()
            } else if key_path.starts_with("~/") {
                // 展开 ~
                if let Some(home) = dirs::home_dir() {
                    home.join(key_path.strip_prefix("~/").unwrap())
                } else {
                    key_path.clone()
                }
            } else {
                // 相对路径，假设在 ~/.ssh/ 下
                if let Ok(ssh_dir) = crate::infra::fs::ssh_dir() {
                    ssh_dir.join(key_path.as_path())
                } else {
                    key_path.clone()
                }
            };

            if path.exists() {
                return ServerStatus::KeyConfigured(path);
            }
        }

        if server.password_stored {
            ServerStatus::PasswordStored
        } else {
            ServerStatus::Bare
        }
    }

    /// 获取单个服务器的详细信息（含元数据合并）
    pub fn get_server(name: &str) -> SkResult<Option<Server>> {
        let parser = SshConfigParser::default_path()?;
        let mut server = match parser.find_host(name)? {
            Some(s) => s,
            None => return Ok(None),
        };

        // 合并元数据中的密码存储状态
        if let Ok(metadata) = MetadataManager::load_default() {
            if let Some(meta) = metadata.get_server(name) {
                server.password_stored = meta.password_stored;
            }
        }

        Ok(Some(server))
    }

    /// 检查服务器名称是否可用
    #[allow(dead_code)]
    pub fn check_name_available(name: &str) -> SkResult<bool> {
        let writer = SshConfigWriter::default_path()?;
        Ok(!writer.exists(name)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// 准备测试环境的临时 SSH config 文件
    ///
    /// 注意：测试中不会使用真实 ~/.ssh/config，
    /// 但 Orchestrator 方法使用 default_path() 会访问真实路径。
    /// 因此这些测试主要验证参数校验和状态判定逻辑。
    #[allow(dead_code)]
    fn setup_test_config(dir: &TempDir) -> PathBuf {
        let config_path = dir.path().join("test_config");
        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            "Host existing-server\n    HostName 10.0.0.1\n    User admin\n"
        )
        .unwrap();
        config_path
    }

    #[test]
    fn test_add_server_invalid_name() {
        let result = Orchestrator::add_server("", "host", "user", 22, None, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_server_invalid_host() {
        let result = Orchestrator::add_server("name", "", "user", 22, None, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_server_invalid_user() {
        let result = Orchestrator::add_server("name", "host", "", 22, None, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_server_invalid_port_zero() {
        let result = Orchestrator::add_server("name", "host", "user", 0, None, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_determine_status_bare() {
        let server = Server::new("test".into(), "host".into(), "user".into());
        let status = Orchestrator::determine_status(&server);
        assert_eq!(status, ServerStatus::Bare);
    }

    #[test]
    fn test_determine_status_key_configured() {
        let dir = TempDir::new().unwrap();
        let key_path = dir.path().join("test_key");
        // 创建空文件模拟密钥存在
        std::fs::write(&key_path, "fake-key").unwrap();

        let server = Server::new("test".into(), "host".into(), "user".into())
            .with_identity_file(key_path.clone());
        let status = Orchestrator::determine_status(&server);
        assert_eq!(status, ServerStatus::KeyConfigured(key_path));
    }

    #[test]
    fn test_determine_status_password_stored() {
        let mut server = Server::new("test".into(), "host".into(), "user".into());
        server.password_stored = true;
        let status = Orchestrator::determine_status(&server);
        assert_eq!(status, ServerStatus::PasswordStored);
    }

    #[test]
    fn test_check_name_available() {
        // 这个测试在不存在的名称上应该返回 true
        let result = Orchestrator::check_name_available("very-unique-name-12345");
        // 可能因为真实 SSH config 中有此名称而返回 false，也可能返回 Ok(true)
        assert!(result.is_ok());
    }
}
