//! SSH Config 写入器
//!
//! 负责向 ~/.ssh/config 中安全地追加、删除、更新 Host 块。
//!
//! 设计要点：
//! - 保留用户手动编辑的内容（注释、空行、非 sk 管理的 Host 块）
//! - sk 管理的块有明确的标记注释
//! - 写入前获取文件锁，防止并发写入冲突
//! - 使用临时文件 + rename 实现原子写入

use std::path::{Path, PathBuf};

use crate::domain::config::model::Server;
use crate::domain::config::parser::SshConfigParser;
use crate::error::SkResult;
use crate::infra::fs;

/// sk 管理标记 — 写入到 Host 块的首行注释中
const SK_MANAGED_MARKER: &str = "# sk managed";

/// SSH Config 写入器（legacy，用于 export --to-ssh）
#[allow(dead_code)]
#[derive(Debug)]
pub struct SshConfigWriter {
    /// SSH config 文件路径
    path: PathBuf,
}

#[allow(dead_code)]
impl SshConfigWriter {
    /// 使用默认路径（~/.ssh/config）创建写入器
    pub fn default_path() -> SkResult<Self> {
        Ok(Self {
            path: fs::ssh_config_path()?,
        })
    }

    /// 使用指定路径创建写入器（主要用于测试）
    #[allow(dead_code)]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 生成 sk 管理的 Host 块文本
    fn format_host_block(server: &Server) -> String {
        let now = chrono::Local::now();
        let mut block = format!(
            "{}: {} (added {})\nHost {}\n    HostName {}\n    User {}\n",
            SK_MANAGED_MARKER,
            server.name,
            now.format("%Y-%m-%d"),
            server.name,
            server.host,
            server.user,
        );

        if server.port != 22 {
            block.push_str(&format!("    Port {}\n", server.port));
        }

        if let Some(ref key_path) = server.identity_file {
            block.push_str(&format!("    IdentityFile {}\n", key_path.display()));
        }

        block.push_str("    StrictHostKeyChecking accept-new\n");
        block
    }

    /// 追加一个 Host 块到 config 文件
    ///
    /// 如果 config 文件不存在，自动创建。
    /// 使用文件锁 + 原子写入确保安全。
    pub fn append(&self, server: &Server) -> SkResult<()> {
        // 确保 SSH 目录存在
        fs::init_sk_env()?;

        // 获取文件锁
        let lock = fs::FileLock::new(&self.path);
        lock.acquire()?;

        // 读取现有内容
        let existing = if self.path.exists() {
            fs::read_file(&self.path).unwrap_or_default()
        } else {
            String::new()
        };

        // 拼接新内容
        let new_block = Self::format_host_block(server);
        let new_content = if existing.is_empty() || existing.ends_with('\n') {
            format!("{}{}\n", existing, new_block)
        } else {
            format!("{}\n{}\n", existing, new_block)
        };

        // 原子写入
        fs::atomic_write(&self.path, &new_content)?;

        // 锁在 drop 时自动释放
        Ok(())
    }

    /// 删除指定名称的 Host 块
    ///
    /// 只删除 sk 管理的块（带有 `# sk managed: <name>` 标记的块）。
    /// 非 sk 管理的块不会被修改。
    pub fn remove(&self, name: &str) -> SkResult<bool> {
        // 如果文件不存在，返回 false
        if !self.path.exists() {
            return Ok(false);
        }

        let lock = fs::FileLock::new(&self.path);
        lock.acquire()?;

        let content = fs::read_file(&self.path).unwrap_or_default();
        let marker = format!("# sk managed: {}", name);

        // 检查是否存在该标记
        if !content.contains(&marker) {
            return Ok(false);
        }

        // 移除块：从标记注释开始到下一个 Host 之前结束
        let lines: Vec<&str> = content.lines().collect();
        let mut result: Vec<&str> = Vec::new();
        let mut in_removing_block = false;
        let mut found = false;

        for line in &lines {
            let trimmed = line.trim();

            // 检测 sk 管理标记
            if trimmed.starts_with(&marker) {
                in_removing_block = true;
                found = true;
                continue;
            }

            if in_removing_block {
                // 遇到下一个 Host 块时结束跳过，并添加一个空行分隔
                if trimmed.starts_with("Host ") && !trimmed.starts_with(&format!("Host {}", name))
                {
                    in_removing_block = false;
                    // 确保分隔空行
                    if !result.is_empty() && !result.last().unwrap().is_empty() {
                        result.push("");
                    }
                    result.push(line);
                }
                // 其他行：跳过（属于被删除的块）
                continue;
            }

            result.push(line);
        }

        if !found {
            return Ok(false);
        }

        // 重新组合内容
        let new_content = if result.is_empty() {
            String::new()
        } else {
            // 清理末尾多余的空行
            let mut content = result.join("\n");
            while content.ends_with("\n\n\n") {
                content.pop();
            }
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content
        };

        fs::atomic_write(&self.path, &new_content)?;
        Ok(true)
    }

    /// 更新指定名称的 Host 块
    ///
    /// 先删除旧块，再追加新块。
    pub fn update(&self, name: &str, server: &Server) -> SkResult<bool> {
        let removed = self.remove(name)?;
        if !removed {
            return Ok(false);
        }
        self.append(server)?;
        Ok(true)
    }

    /// 检查指定名称的 Host 块是否已存在
    pub fn exists(&self, name: &str) -> SkResult<bool> {
        let parser = SshConfigParser::new(self.path.clone());
        match parser.find_host(name) {
            Ok(Some(_)) => Ok(true),
            _ => Ok(false),
        }
    }

    /// 获取写入器关联的文件路径
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// 创建测试用的写入器和临时 config 文件
    fn setup_test_writer(dir: &TempDir) -> (SshConfigWriter, PathBuf) {
        let config_path = dir.path().join("config");
        let writer = SshConfigWriter::new(config_path.clone());
        (writer, config_path)
    }

    /// 创建测试用的 Server 实例
    fn test_server(name: &str) -> Server {
        Server::new(
            name.to_string(),
            "192.168.1.1".to_string(),
            "root".to_string(),
        )
    }

    #[test]
    fn test_append_new_server() {
        let dir = TempDir::new().unwrap();
        let (writer, config_path) = setup_test_writer(&dir);

        let server = test_server("prod");
        writer.append(&server).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("# sk managed: prod"));
        assert!(content.contains("Host prod"));
        assert!(content.contains("HostName 192.168.1.1"));
        assert!(content.contains("User root"));
    }

    #[test]
    fn test_append_with_custom_port_and_key() {
        let dir = TempDir::new().unwrap();
        let (writer, config_path) = setup_test_writer(&dir);

        let server = test_server("db")
            .with_port(5432)
            .with_identity_file(PathBuf::from("~/.ssh/sk_keys/db_key"));

        writer.append(&server).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Port 5432"));
        assert!(content.contains("IdentityFile"));
        assert!(content.contains("StrictHostKeyChecking accept-new"));
    }

    #[test]
    fn test_append_does_not_include_default_port() {
        let dir = TempDir::new().unwrap();
        let (writer, config_path) = setup_test_writer(&dir);

        let server = test_server("web"); // port defaults to 22
        writer.append(&server).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        // Port 22 是默认值，不应显式写入
        assert!(!content.contains("Port 22"));
    }

    #[test]
    fn test_append_preserves_existing_content() {
        let dir = TempDir::new().unwrap();
        let (writer, config_path) = setup_test_writer(&dir);

        // 先写入一些手动配置
        std::fs::write(
            &config_path,
            "Host manual-server\n    HostName 1.1.1.1\n    User root\n\n",
        )
        .unwrap();

        // 再通过 writer 追加
        let server = test_server("auto-server");
        writer.append(&server).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Host manual-server"));
        assert!(content.contains("# sk managed: auto-server"));
    }

    #[test]
    fn test_remove_existing_server() {
        let dir = TempDir::new().unwrap();
        let (writer, config_path) = setup_test_writer(&dir);

        let server = test_server("temp-server");
        writer.append(&server).unwrap();
        assert!(config_path.exists());

        let removed = writer.remove("temp-server").unwrap();
        assert!(removed);

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("temp-server"));
    }

    #[test]
    fn test_remove_nonexistent_server() {
        let dir = TempDir::new().unwrap();
        let (writer, _) = setup_test_writer(&dir);

        let removed = writer.remove("nonexistent").unwrap();
        assert!(!removed);
    }

    #[test]
    fn test_remove_only_removes_sk_managed() {
        let dir = TempDir::new().unwrap();
        let (writer, config_path) = setup_test_writer(&dir);

        // 写入一个手动配置和一个 sk 管理的配置
        let initial = "Host manual\n    HostName 1.1.1.1\n    User admin\n\n# sk managed: managed-svr (added 2026-01-01)\nHost managed-svr\n    HostName 2.2.2.2\n    User root\n    StrictHostKeyChecking accept-new\n\n";
        std::fs::write(&config_path, initial).unwrap();

        // 删除 sk 管理的配置
        let removed = writer.remove("managed-svr").unwrap();
        assert!(removed);

        let content = std::fs::read_to_string(&config_path).unwrap();
        // 手动配置应该保留
        assert!(content.contains("Host manual"));
        assert!(content.contains("User admin"));
        // sk 管理的配置应该被删除
        assert!(!content.contains("managed-svr"));
    }

    #[test]
    fn test_update_existing_server() {
        let dir = TempDir::new().unwrap();
        let (writer, config_path) = setup_test_writer(&dir);

        let original = test_server("updatable");
        writer.append(&original).unwrap();

        let updated = test_server("updatable")
            .with_port(2222)
            .with_identity_file(PathBuf::from("~/.ssh/sk_keys/updatable_key"));

        let result = writer.update("updatable", &updated).unwrap();
        assert!(result);

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Port 2222"));
        assert!(content.contains("IdentityFile"));
    }

    #[test]
    fn test_exists() {
        let dir = TempDir::new().unwrap();
        let (writer, _config_path) = setup_test_writer(&dir);

        let server = test_server("check-me");
        writer.append(&server).unwrap();

        assert!(writer.exists("check-me").unwrap());
        assert!(!writer.exists("not-there").unwrap());
    }

    #[test]
    fn test_format_host_block_default_port() {
        let server = test_server("default");
        let block = SshConfigWriter::format_host_block(&server);

        assert!(block.contains("# sk managed: default"));
        assert!(block.contains("Host default"));
        assert!(block.contains("HostName 192.168.1.1"));
        assert!(block.contains("User root"));
        assert!(!block.contains("Port 22"));
        assert!(block.contains("StrictHostKeyChecking accept-new"));
    }

    #[test]
    fn test_format_host_block_non_default_port() {
        let server = test_server("custom").with_port(2222);
        let block = SshConfigWriter::format_host_block(&server);
        assert!(block.contains("Port 2222"));
    }

    #[test]
    fn test_append_to_empty_file() {
        let dir = TempDir::new().unwrap();
        let (writer, _) = setup_test_writer(&dir);
        // 文件不存在时应自动创建
        let server = test_server("first");
        writer.append(&server).unwrap();
        assert!(writer.exists("first").unwrap());
    }
}
