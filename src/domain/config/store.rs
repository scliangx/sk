//! YAML 服务器存储 — 主数据源 (~/.sk/servers.yaml)
//!
//! 所有 sk 命令（add/remove/list/test/connect）使用此模块，
//! 不再直接读写 ~/.ssh/config。

use crate::domain::config::model::Server;
use crate::error::{SkError, SkResult};
use crate::infra::fs;

/// 加载所有服务器
pub fn load_all() -> SkResult<Vec<Server>> {
    let path = fs::sk_servers_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_file(&path)?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_yaml::from_str(&content)
        .map_err(|e| SkError::Config(format!("Failed to parse servers: {}", e)))
}

/// 保存所有服务器
fn save_all(servers: &[Server]) -> SkResult<()> {
    let content = serde_yaml::to_string(servers)
        .map_err(|e| SkError::Config(format!("Failed to serialize servers: {}", e)))?;
    fs::atomic_write(&fs::sk_servers_path()?, &content)
}

/// 添加服务器（重名则覆盖）
pub fn add(server: &Server) -> SkResult<()> {
    let mut servers = load_all()?;
    servers.retain(|s| s.name != server.name);
    servers.push(server.clone());
    save_all(&servers)
}

/// 删除服务器
pub fn remove(name: &str) -> SkResult<bool> {
    let mut servers = load_all()?;
    let len_before = servers.len();
    servers.retain(|s| s.name != name);
    if servers.len() == len_before {
        return Ok(false);
    }
    save_all(&servers)?;
    Ok(true)
}

/// 更新服务器
#[allow(dead_code)]
pub fn update(name: &str, server: &Server) -> SkResult<bool> {
    let mut servers = load_all()?;
    if let Some(existing) = servers.iter_mut().find(|s| s.name == name) {
        *existing = server.clone();
        save_all(&servers)?;
        return Ok(true);
    }
    Ok(false)
}

/// 查找服务器
pub fn find(name: &str) -> SkResult<Option<Server>> {
    Ok(load_all()?.into_iter().find(|s| s.name == name))
}

/// 检查服务器是否存在
pub fn exists(name: &str) -> SkResult<bool> {
    Ok(load_all()?.iter().any(|s| s.name == name))
}
