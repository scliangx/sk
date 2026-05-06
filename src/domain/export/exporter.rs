//! 配置导出器
//!
//! 将 sk 管理的服务器配置导出为 YAML 或 JSON 格式。
//! 敏感信息（密码）不会包含在导出文件中。

use serde::Serialize;

use crate::error::{SkError, SkResult};

/// 导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Yaml,
    Json,
}

/// 导出条目
#[derive(Debug, Serialize)]
pub struct ExportEntry {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_stored: Option<bool>,
}

/// 导出清单
#[derive(Debug, Serialize)]
pub struct ExportManifest {
    pub version: String,
    pub exported_at: String,
    pub server_count: usize,
    pub servers: Vec<ExportEntry>,
}

/// 配置导出器
pub struct Exporter;

impl Exporter {
    /// 导出所有服务器配置
    pub fn export_all(format: ExportFormat) -> SkResult<String> {
        let servers = crate::domain::config::store::load_all()?;
        let metadata = crate::domain::config::metadata::MetadataManager::load_default()?;

        let entries: Vec<ExportEntry> = servers
            .iter()
            .map(|s| {
                let meta = metadata.get_server(&s.name);
                ExportEntry {
                    name: s.name.clone(),
                    host: s.host.clone(),
                    port: s.port,
                    user: s.user.clone(),
                    identity_file: s.identity_file.as_ref().map(|p| p.display().to_string()),
                    password_stored: meta.map(|m| m.password_stored),
                }
            })
            .collect();

        let manifest = ExportManifest {
            version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: chrono::Local::now().to_rfc3339(),
            server_count: entries.len(),
            servers: entries,
        };

        match format {
            ExportFormat::Json => serde_json::to_string_pretty(&manifest)
                .map_err(|e| SkError::Config(format!("JSON serialize: {}", e))),
            ExportFormat::Yaml => serde_yaml::to_string(&manifest)
                .map_err(|e| SkError::Config(format!("YAML serialize: {}", e))),
        }
    }

    /// 导出到文件
    pub fn export_to_file(path: &std::path::Path, format: ExportFormat) -> SkResult<()> {
        let content = Self::export_all(format)?;
        crate::infra::fs::atomic_write(path, &content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_json() {
        let result = Exporter::export_all(ExportFormat::Json);
        // 可能为空（如果没有配置的服务器），但不应出错
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("\"version\""));
        assert!(content.contains("\"servers\""));
    }

    #[test]
    fn test_export_yaml() {
        let result = Exporter::export_all(ExportFormat::Yaml);
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("version:"));
        assert!(content.contains("servers:"));
    }
}
