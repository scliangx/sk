//! sk 元数据存储
//!
//! 管理 `~/.ssh/sk.yaml` 文件，存储 SSH config 之外的需要补充信息：
//! - 服务器创建时间
//! - 密码是否已存储
//! - 密码存储后端类型
//! - 最近连接时间
//!
//! 这些信息不会写入 `~/.ssh/config`，保持 config 文件干净兼容。

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{SkError, SkResult};
use crate::infra::fs;

/// sk 元数据文件顶层结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkMetadata {
    /// 元数据格式版本
    pub version: u8,
    /// 最后修改时间
    pub last_modified: DateTime<Local>,
    /// 各服务器的元数据（key = 服务器名称）
    #[serde(default)]
    pub servers: HashMap<String, ServerMetadata>,
}

/// 单个服务器的元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMetadata {
    /// 创建时间
    pub created_at: DateTime<Local>,
    /// 是否存储了密码
    #[serde(default)]
    pub password_stored: bool,
    /// 密码存储后端（keychain / encrypted-file / none）
    #[serde(default)]
    pub password_backend: String,
    /// 最近一次连接时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connected: Option<DateTime<Local>>,
}

/// 元数据管理器
///
/// 负责 sk.yaml 文件的读写操作。
#[derive(Debug)]
pub struct MetadataManager {
    /// 元数据文件路径
    path: PathBuf,
    /// 内存中的元数据缓存
    metadata: SkMetadata,
}

impl MetadataManager {
    /// 使用默认路径创建并加载元数据
    pub fn load_default() -> SkResult<Self> {
        Self::load(&fs::sk_metadata_path()?)
    }

    /// 从指定路径加载元数据
    pub fn load(path: &Path) -> SkResult<Self> {
        let metadata = if path.exists() {
            let content = fs::read_file(path)?;
            serde_yaml::from_str(&content)
                .map_err(|e| SkError::Config(format!("Metadata file format error: {}", e)))?
        } else {
            // 文件不存在，创建空的元数据
            SkMetadata {
                version: 1,
                last_modified: Local::now(),
                servers: HashMap::new(),
            }
        };

        Ok(Self {
            path: path.to_path_buf(),
            metadata,
        })
    }

    /// 添加或更新服务器元数据
    pub fn upsert_server(&mut self, name: &str, password_stored: bool, password_backend: &str) {
        self.metadata.servers.insert(
            name.to_string(),
            ServerMetadata {
                created_at: Local::now(),
                password_stored,
                password_backend: password_backend.to_string(),
                last_connected: None,
            },
        );
    }

    /// 获取服务器元数据
    pub fn get_server(&self, name: &str) -> Option<&ServerMetadata> {
        self.metadata.servers.get(name)
    }

    /// 删除服务器元数据
    pub fn remove_server(&mut self, name: &str) -> bool {
        self.metadata.servers.remove(name).is_some()
    }

    /// 更新最近连接时间
    pub fn record_connection(&mut self, name: &str) {
        if let Some(meta) = self.metadata.servers.get_mut(name) {
            meta.last_connected = Some(Local::now());
        }
    }

    /// 列出所有服务器名称
    #[allow(dead_code)]
    pub fn server_names(&self) -> Vec<&String> {
        self.metadata.servers.keys().collect()
    }

    /// 检查服务器是否存在
    #[allow(dead_code)]
    pub fn contains(&self, name: &str) -> bool {
        self.metadata.servers.contains_key(name)
    }

    /// 保存元数据到磁盘
    pub fn save(&mut self) -> SkResult<()> {
        self.metadata.last_modified = Local::now();

        let content = serde_yaml::to_string(&self.metadata).map_err(|e| SkError::FileWrite {
            path: self.path.clone(),
            reason: format!("Failed to serialize metadata: {}", e),
        })?;

        fs::atomic_write(&self.path, &content)?;
        Ok(())
    }

    /// 获取元数据路径
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 获取服务器总数
    #[allow(dead_code)]
    pub fn server_count(&self) -> usize {
        self.metadata.servers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_metadata_is_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sk.yaml");

        let manager = MetadataManager::load(&path).unwrap();
        assert_eq!(manager.server_count(), 0);
        assert_eq!(manager.metadata.version, 1);
    }

    #[test]
    fn test_upsert_and_get_server() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sk.yaml");

        let mut manager = MetadataManager::load(&path).unwrap();
        manager.upsert_server("prod", true, "keychain");

        let meta = manager.get_server("prod").unwrap();
        assert!(meta.password_stored);
        assert_eq!(meta.password_backend, "keychain");
    }

    #[test]
    fn test_remove_server() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sk.yaml");

        let mut manager = MetadataManager::load(&path).unwrap();
        manager.upsert_server("prod", true, "keychain");
        assert!(manager.contains("prod"));

        let removed = manager.remove_server("prod");
        assert!(removed);
        assert!(!manager.contains("prod"));
    }

    #[test]
    fn test_record_connection() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sk.yaml");

        let mut manager = MetadataManager::load(&path).unwrap();
        manager.upsert_server("prod", false, "none");

        // 初始无连接记录
        assert!(manager.get_server("prod").unwrap().last_connected.is_none());

        manager.record_connection("prod");
        assert!(manager.get_server("prod").unwrap().last_connected.is_some());
    }

    #[test]
    fn test_save_and_reload() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sk.yaml");

        {
            let mut manager = MetadataManager::load(&path).unwrap();
            manager.upsert_server("server-a", true, "keychain");
            manager.upsert_server("server-b", false, "none");
            manager.save().unwrap();
        }

        // 重新加载
        let manager2 = MetadataManager::load(&path).unwrap();
        assert_eq!(manager2.server_count(), 2);
        assert!(manager2.contains("server-a"));
        assert!(manager2.contains("server-b"));

        let meta_a = manager2.get_server("server-a").unwrap();
        assert!(meta_a.password_stored);
        assert_eq!(meta_a.password_backend, "keychain");
    }

    #[test]
    fn test_server_names() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sk.yaml");

        let mut manager = MetadataManager::load(&path).unwrap();
        manager.upsert_server("alpha", false, "none");
        manager.upsert_server("beta", false, "none");

        let mut names: Vec<&str> = manager.server_names().iter().map(|s| s.as_str()).collect();
        names.sort();

        assert_eq!(names, vec!["alpha", "beta"]);
    }
}
